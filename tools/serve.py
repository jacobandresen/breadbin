"""Serve docs/ for development: like `python3 -m http.server`, but every
response says Cache-Control: no-cache, so the browser asks again on each load
and never mixes an old module with a new one (the Studio's modules load
lazily, after the page, where even a hard reload can use the cache)."""
import functools
import http.server
import socket
import sys


class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cache-Control', 'no-cache')
        super().end_headers()


class DualStack(http.server.ThreadingHTTPServer):
    """Listens on IPv6 and IPv4 both: `localhost` is often ::1 first, and a
    browser that tries only that never reaches a server on 0.0.0.0."""
    address_family = socket.AF_INET6

    def server_bind(self):
        self.socket.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        super().server_bind()


if __name__ == '__main__':
    root = sys.argv[1] if len(sys.argv) > 1 else 'docs'
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 8064
    handler = functools.partial(NoCache, directory=root)
    print(f'Serving {root} on http://localhost:{port}/play/', flush=True)
    DualStack(('::', port), handler).serve_forever()
