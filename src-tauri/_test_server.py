
from http.server import HTTPServer, BaseHTTPRequestHandler
import os

class H(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok")
    def log_message(self, fmt, *args):
        pass

srv = HTTPServer(("127.0.0.1", 8766), H)
import sys; print("started", flush=True)
srv.serve_forever()
