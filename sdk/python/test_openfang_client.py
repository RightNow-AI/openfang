"""Tests for the dependency-free REST client: python -m unittest discover."""

import io
import json
import threading
import unittest
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch
from urllib.error import HTTPError

from openfang_client import OpenFang, OpenFangError


@contextmanager
def streaming_server(write_response):
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            write_response(self.wfile)

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield OpenFang("http://127.0.0.1:{}".format(server.server_port))
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


class StreamingTests(unittest.TestCase):
    def test_event_arrives_before_response_finishes(self):
        finish_response = threading.Event()
        event_received = threading.Event()
        received = []
        errors = []

        def write_response(output):
            output.write(b'data: {"delta":"hello"}\n\n')
            output.flush()
            finish_response.wait(5)
            output.write(b"data: [DONE]\n\n")
            output.flush()

        with streaming_server(write_response) as client:
            events = client.agents.stream("test-agent", "Hello")

            def read_event():
                try:
                    received.append(next(events))
                    event_received.set()
                except Exception as error:
                    errors.append(error)

            reader = threading.Thread(target=read_event, daemon=True)
            reader.start()
            try:
                delivered_while_open = event_received.wait(2)
            finally:
                finish_response.set()
                reader.join(5)
                events.close()

        self.assertEqual(errors, [])
        self.assertTrue(delivered_while_open, "event waited for the response to end")
        self.assertEqual(received, [{"delta": "hello"}])

    def test_utf8_character_crossing_4096_byte_boundary(self):
        prefix = b'data: {"delta":"'
        text = "a" * (4095 - len(prefix)) + "\U0001f642"
        payload = (
            b"data: "
            + json.dumps({"delta": text}, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            + b"\n\n"
        )

        def write_response(output):
            output.write(payload)
            output.flush()

        with streaming_server(write_response) as client:
            self.assertEqual(list(client.agents.stream("test-agent", "Hello")), [{"delta": text}])

    def test_response_closes_after_done_marker(self):
        response = io.BytesIO(b'data: {"delta":"hello"}\n\ndata: [DONE]\n\n')
        with patch("openfang_client.urlopen", return_value=response):
            self.assertEqual(
                list(OpenFang("http://localhost").agents.stream("test-agent", "Hello")),
                [{"delta": "hello"}],
            )
        self.assertTrue(response.closed)

    def test_response_closes_when_consumer_stops(self):
        response = io.BytesIO(b'data: {"delta":"hello"}\n\ndata: {"delta":"world"}\n\n')
        with patch("openfang_client.urlopen", return_value=response):
            events = OpenFang("http://localhost").agents.stream("test-agent", "Hello")
            self.assertEqual(next(events), {"delta": "hello"})
            events.close()
        self.assertTrue(response.closed)

    def test_response_closes_on_decode_error(self):
        response = io.BytesIO(b"data: \xff\n\n")
        with patch("openfang_client.urlopen", return_value=response):
            with self.assertRaises(UnicodeDecodeError):
                list(OpenFang("http://localhost").agents.stream("test-agent", "Hello"))
        self.assertTrue(response.closed)

    def test_crlf_comments_and_non_json_data(self):
        response = io.BytesIO(
            b': keepalive\r\nevent: message\r\ndata: {"delta":"hello"}\r\n\r\ndata: plain text\r\n\r\n'
        )
        with patch("openfang_client.urlopen", return_value=response):
            self.assertEqual(
                list(OpenFang("http://localhost").agents.stream("test-agent", "Hello")),
                [{"delta": "hello"}, {"raw": "plain text"}],
            )
        self.assertTrue(response.closed)

    def test_http_error_preserves_status_and_body(self):
        error = HTTPError("http://localhost", 503, "Unavailable", {}, io.BytesIO(b"busy"))
        with patch("openfang_client.urlopen", side_effect=error):
            with self.assertRaises(OpenFangError) as caught:
                list(OpenFang("http://localhost").agents.stream("test-agent", "Hello"))
        self.assertEqual(caught.exception.status, 503)
        self.assertEqual(caught.exception.body, "busy")


if __name__ == "__main__":
    unittest.main()
