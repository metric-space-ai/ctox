import io
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import Mock

import preparation


class PreparationProtocol(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.root.cleanup)
        self.rollout = Path(self.root.name) / 'rollout.jsonl'
        self.rollout.write_text('{"type":"session_meta"}\n')
        self.proc = Mock(stdin=io.BytesIO())
        self.client = preparation.Client(self.proc, Mock(), time.monotonic() + 90)

    def events(self, status='completed', text='READY', error=None):
        return [
            {'method': 'turn/started', 'params': {'threadId': 'thread', 'turn': {'id': 'turn'}}},
            {'method': 'item/completed', 'params': {'threadId': 'thread', 'turnId': 'turn',
              'item': {'type': 'agentMessage', 'text': text}}},
            {'method': 'turn/completed', 'params': {'threadId': 'thread',
              'turn': {'id': 'turn', 'status': status, 'error': error}}},
            {'id': 4, 'result': {'turn': {'id': 'turn'}}},
        ]

    def test_fast_completion_before_rpc_response_is_retained(self):
        self.client.receive = Mock(side_effect=self.events())
        self.assertEqual(self.client.prepare('thread', str(self.rollout)), 'turn')
        request = json.loads(self.proc.stdin.getvalue().splitlines()[0])
        self.assertNotIn('developerInstructions', request['params'])
        self.assertNotIn('model', request['params'])
        self.assertEqual(request['params']['input'][0]['text'], preparation.PROMPT)

    def test_failed_interrupted_or_error_turn_never_becomes_ready(self):
        for status, error in [('failed', None), ('interrupted', None), ('completed', {'message': 'quota'})]:
            with self.subTest(status=status, error=error):
                self.client.events = []
                self.client.receive = Mock(side_effect=self.events(status=status, error=error))
                with self.assertRaisesRegex(ValueError, 'did not complete successfully'):
                    self.client.prepare('thread', str(self.rollout))

    def test_completed_turn_requires_ready_reply(self):
        self.client.receive = Mock(side_effect=self.events(text='I started implementation'))
        with self.assertRaisesRegex(ValueError, 'required READY'):
            self.client.prepare('thread', str(self.rollout))

    def test_completed_turn_requires_nonempty_rollout_file(self):
        for path in [None, str(self.rollout.with_name('missing')), str(self.rollout)]:
            self.rollout.write_text('')
            self.client.events = []
            self.client.receive = Mock(side_effect=self.events())
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, 'no persisted rollout'):
                self.client.prepare('thread', path)

    def test_forbidden_tool_item_interrupts_preparation(self):
        events = self.events()
        events.insert(1, {'method': 'item/started', 'params': {'threadId': 'thread',
                         'turnId': 'turn', 'item': {'type': 'commandExecution'}}})
        self.client.receive = Mock(side_effect=events)
        with self.assertRaisesRegex(ValueError, 'forbidden item'):
            self.client.prepare('thread', str(self.rollout))
        self.client.interrupt('thread')
        last = json.loads(self.proc.stdin.getvalue().splitlines()[-1])
        self.assertEqual(last['method'], 'turn/interrupt')
        self.assertEqual(last['params'], {'threadId': 'thread', 'turnId': 'turn'})

    def test_incomplete_turn_times_out(self):
        self.client.deadline = time.monotonic() - 1
        with self.assertRaisesRegex(TimeoutError, 'deadline'):
            self.client.prepare('thread', str(self.rollout))

    def test_server_tool_request_is_rejected(self):
        self.client.receive = Mock(return_value={'id': 17, 'method': 'item/tool/call', 'params': {}})
        with self.assertRaisesRegex(ValueError, 'tool or approval'):
            self.client.prepare('thread', str(self.rollout))
        response = json.loads(self.proc.stdin.getvalue().splitlines()[-1])
        self.assertEqual(response['id'], 17)
        self.assertIn('error', response)


if __name__ == '__main__':
    unittest.main()
