#!/usr/bin/env python3
import base64
import importlib.util
import pathlib
import unittest


SCRIPT_PATH = pathlib.Path(__file__).with_name("zammad_request.py")
SPEC = importlib.util.spec_from_file_location("zammad_request", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ZammadRequestTests(unittest.TestCase):
    def test_token_header_wins(self):
        args = type(
            "Args",
            (),
            {
                "token": "abc123",
                "user": "user@example.com",
                "password": "secret",
                "content_type": "application/json",
            },
        )()
        headers = MODULE.build_headers(args)
        self.assertEqual(headers["Authorization"], "Token token=abc123")

    def test_basic_auth_header(self):
        args = type(
            "Args",
            (),
            {
                "token": None,
                "user": "user@example.com",
                "password": "secret",
                "content_type": "application/json",
            },
        )()
        headers = MODULE.build_headers(args)
        expected = "Basic " + base64.b64encode(b"user@example.com:secret").decode("ascii")
        self.assertEqual(headers["Authorization"], expected)


if __name__ == "__main__":
    unittest.main()
