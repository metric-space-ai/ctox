"""Bounded READY-only first turn that materializes a Desktop rollout."""
import json
import os
from pathlib import Path
import time

PROMPT = ('This is only a task-persistence handshake. No implementation assignment has been given. '
          'Do not use any tools, inspect or change files, run commands, contact services, or delegate. '
          'Reply with exactly READY and finish this turn. Wait for the parent to send the real assignment later.')


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
        result = self.request(4, 'turn/start', {'threadId': thread,
                              'input': [{'type': 'text', 'text': PROMPT}]})
        self.turn_id = result['turn']['id']
        while True:
            for event in self.events:
                params = event['params']
                turn = params.get('turn', {})
                if event['method'] == 'turn/completed' and turn.get('id') == self.turn_id:
                    if turn.get('status') != 'completed' or turn.get('error'):
                        raise ValueError('Preparation turn did not complete successfully: ' + json.dumps(turn))
                    items = [e['params']['item'] for e in self.events
                             if e['method'] == 'item/completed' and
                             e['params'].get('turnId') == self.turn_id]
                    items += turn.get('items', [])
                    if any(i.get('type') not in ('userMessage', 'agentMessage', 'reasoning') for i in items):
                        raise ValueError('Preparation completed with a forbidden tool item')
                    replies = [i.get('text', '').strip() for i in items if i.get('type') == 'agentMessage']
                    if not replies or replies[-1] != 'READY':
                        raise ValueError('Preparation did not finish with the required READY reply')
                    path = Path(rollout_path) if rollout_path else None
                    if path is None or not path.is_file() or path.stat().st_size == 0:
                        raise ValueError('Preparation completed but no persisted rollout file exists')
                    return self.turn_id
            self.observe(self.receive())

    def interrupt(self, thread):
        if self.turn_id:
            try:
                self.send({'id': 99, 'method': 'turn/interrupt',
                           'params': {'threadId': thread, 'turnId': self.turn_id}})
            except (OSError, ValueError):
                pass
