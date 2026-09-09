"""Persist a Desktop task's execution contract without starting inference."""
import json
import os
from pathlib import Path
import time

CONTRACT = (
    'This is a disposable implementation worker. The first user request is the actual assignment. '
    'There is no initialization response to produce. Context compaction is not task completion. '
    'Before ending a context window, persist a concise private checkpoint with the current goal, '
    'completed changes and evidence, outstanding checks, latest parent corrections, publication '
    'restrictions and next action. After compaction, recover that checkpoint and the current '
    'assignment before continuing; do not repeat completed work or revive superseded instructions. '
    'A historical summary does not supersede later corrections or current repository instructions. '
    'Keep private context out of public issues, commits, logs and PRs. Before first publication '
    'of work derived from private context, obtain parent review of exact outgoing text, all new '
    'commits and the diff. Report review-ready work or an actionable blocker to the exact parent '
    'task ID with send_message_to_thread before ending the turn. If that tool is unavailable, '
    'persist the pending notification and explain the blocker; never claim delivery. '
    'The parent owns review, merge and archive. Do not merge or archive yourself.'
)


class Client:
    def __init__(self, proc, selector, deadline):
        self.proc, self.selector, self.deadline = proc, selector, deadline
        self.buffer = b''
        self.events = []
        self.thread_id = None
        self.turn_id = None

    def send(self, value):
        self.proc.stdin.write((json.dumps(value) + '\n').encode())
        self.proc.stdin.flush()

    def receive(self):
        while time.monotonic() < self.deadline:
            if b'\n' in self.buffer:
                line, self.buffer = self.buffer.split(b'\n', 1)
                if line:
                    return json.loads(line)
                continue
            if self.proc.poll() is not None:
                raise RuntimeError('Temporary app-server exited before preparation completed')
            if self.selector.select(min(1, max(0, self.deadline - time.monotonic()))):
                chunk = os.read(self.proc.stdout.fileno(), 65536)
                if not chunk:
                    raise RuntimeError('Temporary app-server closed its output')
                self.buffer += chunk
        raise TimeoutError('Task preparation deadline exceeded')

    def observe(self, message):
        if 'method' not in message:
            return
        if 'id' in message:
            self.send({'id': message['id'], 'error': {'code': -32601,
                       'message': 'Tools and approval requests are forbidden during preparation'}})
            raise ValueError('Preparation requested a tool or approval: ' + message['method'])
        params = message.get('params', {})
        if self.thread_id and params.get('threadId') == self.thread_id:
            if message['method'] == 'turn/started':
                self.turn_id = params['turn']['id']
            if message['method'] in ('item/started', 'item/completed'):
                item_type = params.get('item', {}).get('type')
                if item_type not in ('userMessage', 'agentMessage', 'reasoning'):
                    raise ValueError('Preparation attempted forbidden item: ' + str(item_type))
            self.events.append(message)

    def request(self, rid, method, params):
        self.send({'id': rid, 'method': method, 'params': params})
        while True:
            message = self.receive()
            if message.get('id') == rid and 'method' not in message:
                if 'error' in message:
                    raise ValueError(json.dumps(message['error']))
                return message['result']
            self.observe(message)

    def prepare(self, thread, rollout_path):
        self.thread_id = thread
        self.request(4, 'thread/inject_items', {'threadId': thread, 'items': [{
            'type': 'message', 'role': 'developer',
            'content': [{'type': 'input_text', 'text': CONTRACT}]}]})
        result = self.request(5, 'thread/read', {'threadId': thread, 'includeTurns': False})
        if result.get('thread', {}).get('id') != thread:
            raise ValueError('Persisted task read returned a different task')
        if self.turn_id is not None:
            raise ValueError('Task persistence unexpectedly started an inference turn')
        path = Path(rollout_path) if rollout_path else None
        if path is None or not path.is_file() or path.stat().st_size == 0:
            raise ValueError('Task contract saved but no persisted rollout file exists')
        found_contract = False
        with path.open() as stream:
            for line in stream:
                if not line.strip():
                    continue
                record = json.loads(line)
                item = record.get('payload', {})
                if (record.get('type') == 'response_item' and item.get('type') == 'message'
                        and item.get('role') == 'developer'):
                    found_contract |= any(part.get('text') == CONTRACT
                                          for part in item.get('content', []))
        if not found_contract:
            raise ValueError('Persisted rollout does not contain the exact execution contract')
        return None

    def interrupt(self, thread):
        if self.turn_id:
            try:
                self.send({'id': 99, 'method': 'turn/interrupt',
                           'params': {'threadId': thread, 'turnId': self.turn_id}})
            except (OSError, ValueError):
                pass
