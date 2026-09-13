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
        self.rollout.write_text(json.dumps({'type': 'response_item', 'payload': {
            'type': 'message', 'role': 'developer',
            'content': [{'type': 'input_text', 'text': preparation.CONTRACT}]}}) + '\n')
        self.proc = Mock(stdin=io.BytesIO())
        self.client = preparation.Client(self.proc, Mock(), time.monotonic() + 30)

    def replies(self, thread='thread'):
        return [{'id': 4, 'result': {}},
                {'id': 5, 'result': {'thread': {'id': thread}}}]

    def test_persists_contract_without_user_input_or_model_turn(self):
        self.client.receive = Mock(side_effect=self.replies())
        self.assertIsNone(self.client.prepare('thread', str(self.rollout)))
        requests = [json.loads(x) for x in self.proc.stdin.getvalue().splitlines()]
        self.assertEqual([x['method'] for x in requests], ['thread/inject_items', 'thread/read'])
        items = requests[0]['params']['items']
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]['role'], 'developer')
        self.assertNotIn('READY', items[0]['content'][0]['text'])
        self.assertNotIn('No implementation assignment', items[0]['content'][0]['text'])
        self.assertFalse(requests[1]['params']['includeTurns'])
        self.assertIsNone(self.client.turn_id)

    def test_rpc_failure_cannot_report_ready(self):
        self.client.receive = Mock(return_value={'id': 4, 'error': {'message': 'unsupported'}})
        with self.assertRaisesRegex(ValueError, 'unsupported'):
            self.client.prepare('thread', str(self.rollout))
        self.assertEqual(len(self.proc.stdin.getvalue().splitlines()), 1)

    def test_read_failure_cannot_report_ready(self):
        self.client.receive = Mock(side_effect=[self.replies()[0],
                                    {'id': 5, 'error': {'message': 'not persisted'}}])
        with self.assertRaisesRegex(ValueError, 'not persisted'):
            self.client.prepare('thread', str(self.rollout))

    def test_read_must_return_same_task(self):
        self.client.receive = Mock(side_effect=self.replies('other'))
        with self.assertRaisesRegex(ValueError, 'different task'):
            self.client.prepare('thread', str(self.rollout))

    def test_requires_nonempty_rollout_file(self):
        for path in [None, str(self.rollout.with_name('missing')), str(self.rollout)]:
            self.rollout.write_text('')
            self.client.receive = Mock(side_effect=self.replies())
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, 'no persisted rollout'):
                self.client.prepare('thread', path)

    def test_unexpected_inference_turn_is_rejected(self):
        events = [{'method': 'turn/started', 'params': {'threadId': 'thread',
                   'turn': {'id': 'unexpected'}}}] + self.replies()
        self.client.receive = Mock(side_effect=events)
        with self.assertRaisesRegex(ValueError, 'started an inference turn'):
            self.client.prepare('thread', str(self.rollout))
        self.client.interrupt('thread')
        last = json.loads(self.proc.stdin.getvalue().splitlines()[-1])
        self.assertEqual(last['method'], 'turn/interrupt')
        self.assertEqual(last['params']['turnId'], 'unexpected')

    def test_nonempty_file_without_exact_developer_contract_is_rejected(self):
        for record in [{'type': 'session_meta'}, {'type': 'response_item', 'payload': {
            'type': 'message', 'role': 'user',
            'content': [{'type': 'input_text', 'text': preparation.CONTRACT}]}},
            {'type': 'response_item', 'payload': {'type': 'message', 'role': 'developer',
             'content': [{'type': 'input_text', 'text': 'truncated contract'}]}}]:
            self.rollout.write_text(json.dumps(record) + '\n')
            self.client.receive = Mock(side_effect=self.replies())
            with self.subTest(record=record), self.assertRaisesRegex(ValueError, 'exact execution contract'):
                self.client.prepare('thread', str(self.rollout))

    def test_setup_tool_execution_is_rejected(self):
        self.client.receive = Mock(return_value={'method': 'item/started',
            'params': {'threadId': 'thread', 'item': {'type': 'commandExecution'}}})
        with self.assertRaisesRegex(ValueError, 'forbidden item'):
            self.client.prepare('thread', str(self.rollout))

    def test_incomplete_rpc_times_out(self):
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
