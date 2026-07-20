#!/usr/bin/env python3

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


PAGES = {
    "/a1": ("A1", "a1"),
    "/a2": ("A2", "a2"),
    "/b1": ("B1", "b1"),
    "/recovery": ("Recovery", "recovery"),
}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?", 1)[0]
        page = PAGES.get(path)
        if page is None:
            self.send_error(404)
            return
        title, marker = page
        body = f"""<!doctype html>
<html>
  <head><meta charset="utf-8"><title>{title}</title></head>
  <body data-marker="{marker}"><h1>{title}</h1></body>
</html>
""".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        return


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", required=True)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    Path(args.port_file).write_text(f"{server.server_port}\n")
    server.serve_forever()


if __name__ == "__main__":
    main()
