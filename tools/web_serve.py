#!/usr/bin/env python3
"""Serves web/www/ on http://localhost:8000 with the headers the browser build needs.

The game uses shared memory between the page and its worker, which browsers only
allow on "cross-origin isolated" pages. On GitHub Pages a service worker (coi-sw.js)
arranges that; this server just sends the two headers itself.

    web/build.sh && python3 tools/web_serve.py [port]
"""
import functools, http.server, os, sys

class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "www")
    handler = functools.partial(Handler, directory=root)
    handler.extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".js": "text/javascript", ".wasm": "application/wasm"}
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as httpd:
        print(f"Serving rusted-empire on http://localhost:{port}/")
        httpd.serve_forever()
