#!/usr/bin/env python3
"""Local redirect/media endpoint for check-media.py --kind video --http.

Forward the chosen port with hdc rport tcp:18769 tcp:18769. The server binds
only localhost and serves the checked-in sample, without arbitrary file IO.
"""
import argparse
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class MediaServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, port):
        self.sample = Path(__file__).with_name("video").joinpath("sample.mp4").read_bytes()
        super().__init__(("127.0.0.1", port), MediaHandler)


class MediaHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/sample.mp4")
            self.send_header("Content-Length", "0")
            self.end_headers()
        elif self.path in ("/sample.mp4", "/slow"):
            self.send_response(200)
            self.send_header("Content-Type", "video/mp4")
            self.send_header("Content-Length", str(len(self.server.sample)))
            self.end_headers()
            if self.path == "/slow":
                try:
                    for start in range(0, len(self.server.sample), 1024):
                        self.wfile.write(self.server.sample[start:start + 1024])
                        self.wfile.flush()
                        time.sleep(0.05)
                except (BrokenPipeError, ConnectionResetError):
                    self.close_connection = True
                    self.log_message("obsolete slow download was cancelled")
            else:
                self.wfile.write(self.server.sample)
        elif self.path == "/oversized":
            self.send_response(200)
            self.send_header("Content-Length", str((64 << 20) + 1))
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
        else:
            self.send_error(404)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=18769)
    args = parser.parse_args()
    MediaServer(args.port).serve_forever()
