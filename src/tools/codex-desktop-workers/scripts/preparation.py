"""Persist a Desktop task's execution contract without starting inference."""
import json
import os
from pathlib import Path
import time

CONTRACT = "You are an implementation worker. The first user message is the real assignment; do not produce a READY or initialization reply. Solve the assigned problem and verify the result. Choose the implementation details yourself within the given boundaries. Write short, readable messages. Keep one task and PR for this assignment and its corrections. Before publishing work derived from private context, have the parent review the outgoing changes and text for confidential information. Keep a durable private checkpoint of the assignment, progress, corrections and next step; resume it after compaction. Report results or actionable blockers to the exact parent task ID using send_message_to_thread, then end the turn without polling. If delivery fails, preserve the pending notification. The parent owns review, merge and archive."


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
