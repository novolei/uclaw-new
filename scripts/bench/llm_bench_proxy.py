#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Recording reverse proxy for measuring agent LLM traffic (TTFT + prefix cache).

Point an app's provider Base URL at this proxy (e.g. http://127.0.0.1:18801/v1)
and it forwards every request to the real upstream (default DeepSeek) while
recording, per request:

  * t_arrive          — when the request reached the proxy (≈ app finished pre-LLM work)
  * t_first_byte      — first response bytes from upstream
  * t_first_token     — first SSE delta carrying content / reasoning / tool-call text
  * t_end             — response finished
  * usage             — prompt/completion tokens + DeepSeek prompt_cache_hit/miss_tokens
                        (or OpenAI prompt_tokens_details.cached_tokens)
  * request shape     — model, #messages, #tools, system-prompt bytes, total bytes

Full request bodies are saved next to the log so `bench_report.py` can compute
how much of each request's prefix is byte-identical to the previous one — the
condition for provider-side automatic prefix caching.

The Authorization header is forwarded but never written to disk.
Stdlib only.  Usage:

    python3 llm_bench_proxy.py --listen 18801 --label uclaw-new --out ./bench-out
"""
import argparse
import http.client
import json
import os
import ssl
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

HOP_BY_HOP = {
    "connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
    "te", "trailers", "transfer-encoding", "upgrade", "content-length", "host",
    "accept-encoding",
}


def now_ms() -> float:
    return time.time() * 1000.0


class State:
    def __init__(self, label: str, out_dir: str, upstream: str):
        self.label = label
        self.out_dir = os.path.join(out_dir, label)
        os.makedirs(self.out_dir, exist_ok=True)
        u = urlsplit(upstream)
        self.scheme = u.scheme
        self.host = u.hostname
        self.port = u.port or (443 if u.scheme == "https" else 80)
        self.lock = threading.Lock()
        self.seq = 0
        self.log_path = os.path.join(self.out_dir, "requests.jsonl")

    def next_seq(self) -> int:
        with self.lock:
            self.seq += 1
            return self.seq

    def append(self, rec: dict) -> None:
        with self.lock:
            with open(self.log_path, "a", encoding="utf-8") as f:
                f.write(json.dumps(rec, ensure_ascii=False) + "\n")


def summarize_request(body: bytes) -> dict:
    try:
        j = json.loads(body)
    except Exception:
        return {"parse_error": True, "bytes": len(body)}
    msgs = j.get("messages") or []
    sys_bytes = 0
    for m in msgs:
        if m.get("role") == "system":
            c = m.get("content")
            sys_bytes += len(json.dumps(c, ensure_ascii=False).encode()) if c is not None else 0
    return {
        "model": j.get("model"),
        "stream": bool(j.get("stream")),
        "n_messages": len(msgs),
        "n_tools": len(j.get("tools") or []),
        "system_bytes": sys_bytes,
        "bytes": len(body),
    }


def scan_sse_line(line: bytes, rec: dict, t: float) -> None:
    """Inspect one SSE `data:` line; record first-token time and usage."""
    if not line.startswith(b"data:"):
        return
    payload = line[5:].strip()
    if not payload or payload == b"[DONE]":
        return
    try:
        j = json.loads(payload)
    except Exception:
        return
    for ch in j.get("choices") or []:
        d = ch.get("delta") or {}
        if rec.get("t_first_token") is None and (
            d.get("content") or d.get("reasoning_content") or d.get("tool_calls")
        ):
            rec["t_first_token"] = t
            rec["first_token_kind"] = (
                "reasoning" if d.get("reasoning_content") else
                "tool_call" if d.get("tool_calls") else "content"
            )
        if rec.get("t_first_content") is None and d.get("content"):
            rec["t_first_content"] = t
    if j.get("usage"):
        rec["usage"] = j["usage"]


def make_handler(state: State):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *args):  # quiet
            pass

        def _forward(self, method: str) -> None:
            t_arrive = now_ms()
            length = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(length) if length else b""
            seq = state.next_seq()
            rec = {"seq": seq, "label": state.label, "method": method, "path": self.path,
                   "t_arrive": t_arrive}
            if body and self.path.rstrip("/").endswith("chat/completions"):
                rec.update(summarize_request(body))
                with open(os.path.join(state.out_dir, f"{seq:05d}.req.json"), "wb") as f:
                    f.write(body)

            headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP_BY_HOP}
            headers["Accept-Encoding"] = "identity"
            if state.scheme == "https":
                conn = http.client.HTTPSConnection(state.host, state.port, timeout=600,
                                                   context=ssl.create_default_context())
            else:
                conn = http.client.HTTPConnection(state.host, state.port, timeout=600)
            try:
                conn.request(method, self.path, body=body or None, headers=headers)
                resp = conn.getresponse()
                rec["status"] = resp.status
                self.send_response(resp.status)
                for k, v in resp.getheaders():
                    if k.lower() not in HOP_BY_HOP:
                        self.send_header(k, v)
                self.send_header("Transfer-Encoding", "chunked")
                self.send_header("Connection", "close")
                self.end_headers()

                buf = b""
                while True:
                    chunk = resp.read1(65536)
                    if not chunk:
                        break
                    t = now_ms()
                    if rec.get("t_first_byte") is None:
                        rec["t_first_byte"] = t
                    self.wfile.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                    self.wfile.flush()
                    buf += chunk
                    while b"\n" in buf:
                        line, buf = buf.split(b"\n", 1)
                        scan_sse_line(line.strip(), rec, t)
                if buf.strip():
                    scan_sse_line(buf.strip(), rec, now_ms())
                    if "usage" not in rec:  # non-streaming JSON body
                        try:
                            j = json.loads(buf)
                            if j.get("usage"):
                                rec["usage"] = j["usage"]
                        except Exception:
                            pass
                self.wfile.write(b"0\r\n\r\n")
                self.wfile.flush()
            except Exception as e:  # report upstream failure to the app
                rec["error"] = repr(e)
                try:
                    self.send_error(502, "bench proxy upstream error")
                except Exception:
                    pass
            finally:
                rec["t_end"] = now_ms()
                conn.close()
                state.append(rec)
                u = rec.get("usage") or {}
                ttft = (rec["t_first_token"] - t_arrive) if rec.get("t_first_token") else None
                print(f"[{state.label}] #{seq} {rec.get('status')} msgs={rec.get('n_messages')} "
                      f"bytes={rec.get('bytes')} ttft={ttft and round(ttft)}ms "
                      f"in={u.get('prompt_tokens')} hit={u.get('prompt_cache_hit_tokens')} "
                      f"miss={u.get('prompt_cache_miss_tokens')}", flush=True)

        def do_POST(self):
            self._forward("POST")

        def do_GET(self):
            self._forward("GET")

    return Handler


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--listen", type=int, required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--out", default="bench-out")
    ap.add_argument("--upstream", default="https://api.deepseek.com")
    a = ap.parse_args()
    state = State(a.label, a.out, a.upstream)
    srv = ThreadingHTTPServer(("127.0.0.1", a.listen), make_handler(state))
    print(f"[{a.label}] listening on http://127.0.0.1:{a.listen} → {a.upstream}  (log: {state.log_path})",
          flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
