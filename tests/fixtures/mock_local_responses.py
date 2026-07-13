"""Mock CTOX local Responses runtime for integration tests.

Speaks the local Responses wire (one JSON request line in, NDJSON events
out) on a unix socket, plus a small HTTP endpoint for the llama-style
/tokenize preflight. The socket path and model tag are passed as argv so the
managed-backend supervisor can adopt this process by its ps command line.
"""
import socket, os, threading, sys, time

SOCK = sys.argv[1]
# argv: <socket-path> <model-tag-for-ps-visibility> <delay-file> <tokenizer-port>
DELAY_FILE = sys.argv[3] if len(sys.argv) > 3 else ""
TCP_PORT = int(sys.argv[4]) if len(sys.argv) > 4 else 0

SSE = (
    'event: response.created\n'
    'data: {"type":"response.created","response":{"id":"r1"}}\n\n'
    'event: response.output_item.done\n'
    'data: {"type":"response.output_item.done","item":{"type":"message","role":"assistant","id":"m1","content":[{"type":"output_text","text":"mock ok"}]}}\n\n'
    'event: response.completed\n'
    'data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":1,"input_tokens_details":null,"output_tokens":1,"output_tokens_details":null,"total_tokens":2}}}\n\n'
).encode()

def handle(conn):
    try:
        # Local Responses wire: ONE JSON request line in, NDJSON events out.
        buf = b""
        while b"\n" not in buf:
            chunk = conn.recv(65536)
            if not chunk:
                return
            buf += chunk
        print("MOCK HIT responses_create", flush=True)
        delay = 0.0
        try:
            with open(DELAY_FILE) as f:
                delay = float(f.read().strip())
        except Exception:
            pass
        if delay > 0:
            time.sleep(delay)
        events = [
            '{"type":"response.created","response":{"id":"r1"}}',
            '{"type":"response.output_item.done","item":{"type":"message","role":"assistant","id":"m1","content":[{"type":"output_text","text":"mock ok"}]}}',
            '{"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":1,"input_tokens_details":null,"output_tokens":1,"output_tokens_details":null,"total_tokens":2}}}',
        ]
        for e in events:
            conn.sendall(e.encode() + b"\n")
    finally:
        try:
            conn.close()
        except Exception:
            pass

def tcp_server(sock):
    while True:
        conn, _ = sock.accept()
        threading.Thread(target=handle_tcp, args=(conn,), daemon=True).start()

def handle_tcp(conn):
    try:
        buf = b""
        while b"\r\n\r\n" not in buf:
            chunk = conn.recv(65536)
            if not chunk:
                return
            buf += chunk
        head, _, rest = buf.partition(b"\r\n\r\n")
        headers = head.decode("latin1").split("\r\n")
        reqline = headers[0]
        clen = 0
        for h in headers[1:]:
            if h.lower().startswith("content-length:"):
                clen = int(h.split(":", 1)[1].strip())
        while len(rest) < clen:
            chunk = conn.recv(65536)
            if not chunk:
                break
            rest += chunk
        if "/tokenize" in reqline:
            body = b'{"tokens":[1,2,3,4]}'
        else:
            body = b'{"status":"ok"}'
        conn.sendall(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: " + str(len(body)).encode() + b"\r\nconnection: close\r\n\r\n" + body)
    finally:
        try:
            conn.close()
        except Exception:
            pass

_tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
_tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
_tcp.bind(("127.0.0.1", TCP_PORT))
_tcp.listen(64)
_tcp_port = _tcp.getsockname()[1]
threading.Thread(target=tcp_server, args=(_tcp,), daemon=True).start()
os.makedirs(os.path.dirname(SOCK), exist_ok=True)
with open(SOCK + ".http.json", "w") as f:
    f.write('{"base_url":"http://127.0.0.1:%d"}' % _tcp_port)

os.makedirs(os.path.dirname(SOCK), exist_ok=True)
try:
    os.unlink(SOCK)
except FileNotFoundError:
    pass
srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(SOCK)
srv.listen(64)
print("LISTENING", SOCK, flush=True)
while True:
    conn, _ = srv.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
