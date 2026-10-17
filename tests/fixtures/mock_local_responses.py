"""Mock CTOX local Responses runtime for integration tests.

Speaks the local Responses wire (one JSON request line in, NDJSON events
out) on a unix socket, plus a small HTTP endpoint for the llama-style
/tokenize preflight. The socket path and model tag are passed as argv so the
managed-backend supervisor can adopt this process by its ps command line.
"""
import socket, os, threading, sys, time, json

SOCK = sys.argv[1]
# argv: <socket-path> <model-tag-for-ps-visibility> <delay-file> <tokenizer-port>
DELAY_FILE = sys.argv[3] if len(sys.argv) > 3 else ""
TCP_PORT = int(sys.argv[4]) if len(sys.argv) > 4 else 0

# One canonical event sequence for both supported transports. A completed
# Responses object carries its output as well as the streamed item events.
MESSAGE = {"type": "message", "role": "assistant", "id": "m1", "status": "completed",
           "content": [{"type": "output_text", "text": "mock ok", "annotations": []}]}
RESPONSE = {"id": "r1", "object": "response", "created_at": 0, "status": "completed",
            "model": sys.argv[2], "error": None, "incomplete_details": None,
            "output": [MESSAGE], "usage": {"input_tokens": 1,
            "input_tokens_details": {"cached_tokens": 0}, "output_tokens": 1,
            "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 2}}
EVENTS = [
    {"type": "response.created", "response": dict(RESPONSE, status="in_progress", output=[])},
    {"type": "response.output_item.added", "output_index": 0,
     "item": dict(MESSAGE, status="in_progress", content=[])},
    {"type": "response.content_part.added", "item_id": "m1", "output_index": 0,
     "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}},
    {"type": "response.output_text.delta", "item_id": "m1", "output_index": 0,
     "content_index": 0, "delta": "mock ok"},
    {"type": "response.output_text.done", "item_id": "m1", "output_index": 0,
     "content_index": 0, "text": "mock ok"},
    {"type": "response.content_part.done", "item_id": "m1", "output_index": 0,
     "content_index": 0, "part": MESSAGE["content"][0]},
    {"type": "response.output_item.done", "output_index": 0, "item": MESSAGE},
    {"type": "response.completed", "response": RESPONSE},
]
def response_events(request):
    def nodes(value):
        if isinstance(value, dict):
            yield value
            for child in value.values():
                yield from nodes(child)
        elif isinstance(value, list):
            for child in value:
                yield from nodes(child)
        elif isinstance(value, str) and value.lstrip().startswith(("{", "[")):
            try:
                yield from nodes(json.loads(value))
            except json.JSONDecodeError:
                pass
    plan_tool = next((node["name"] for node in nodes(request)
                      if isinstance(node.get("name"), str)
                      and node["name"].split(".")[-1] == "update_plan"), None)
    history = json.dumps(request)
    if plan_tool is None or "fixture-plan-done" in history:
        return EVENTS
    stage = "done" if "fixture-plan-start" in history else "start"
    arguments = json.dumps({"explanation": "Prepare the deterministic test reply", "plan": [{"step": "Prepare deterministic fixture reply",
                                      "status": "completed" if stage == "done" else "in_progress"}]})
    item = {"type": "function_call", "id": "fc-" + stage,
            "call_id": "fixture-plan-" + stage, "name": plan_tool,
            "arguments": arguments, "status": "completed"}
    return [
        {"type": "response.created", "response": dict(RESPONSE, status="in_progress", output=[])},
        {"type": "response.output_item.added", "output_index": 0,
         "item": dict(item, arguments="", status="in_progress")},
        {"type": "response.function_call_arguments.delta", "item_id": item["id"],
         "output_index": 0, "delta": arguments},
        {"type": "response.function_call_arguments.done", "item_id": item["id"],
         "output_index": 0, "arguments": arguments},
        {"type": "response.output_item.done", "output_index": 0, "item": item},
        {"type": "response.completed", "response": dict(RESPONSE, output=[item])},
    ]

def sse(events):
    return "".join("event: " + e["type"] + "\ndata: " + json.dumps(e) + "\n\n" for e in events).encode()


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
        request = json.loads(buf.split(b"\n", 1)[0])
        for event in response_events(request):
            conn.sendall(json.dumps(event).encode() + b"\n")
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
        if "/responses" in reqline:
            # Managed local runtimes advertise an HTTP companion endpoint.
            # The daemon may choose it instead of the private NDJSON socket;
            # both paths must model the same turn and disconnect delay.
            print("MOCK HIT responses_create", flush=True)
            try:
                with open(DELAY_FILE) as delay_file:
                    delay = float(delay_file.read().strip())
            except Exception:
                delay = 0.0
            if delay > 0:
                time.sleep(delay)
            body = sse(response_events(json.loads(rest[:clen])))
            content_type = b"text/event-stream"
        elif "/tokenize" in reqline:
            body = b'{"tokens":[1,2,3,4]}'
            content_type = b"application/json"
        else:
            body = b'{"status":"ok"}'
            content_type = b"application/json"
        conn.sendall(b"HTTP/1.1 200 OK\r\ncontent-type: " + content_type + b"\r\ncontent-length: " + str(len(body)).encode() + b"\r\nconnection: close\r\n\r\n" + body)
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
