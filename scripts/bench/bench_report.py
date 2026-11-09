#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Turn-level latency + prefix-cache report from llm_bench_proxy.py recordings.

Joins the proxy log with the app's own `agent_messages` table (user-message
`created_at`, epoch ms — written when the user hits send) to split each turn into:

  pre_llm   user message persisted → first main LLM request reaches the proxy
  provider  request arrives → first streamed token (prefill + queueing at provider)
  e2e_ttft  user message persisted → first token        (what the user feels)
  turn      user message persisted → last main LLM request finished

and reports per-request prefix-cache behaviour:

  hit%      provider-reported prompt_cache_hit_tokens / prompt_tokens
  prefix%   share of this request's serialized messages that is byte-identical to the
            previous main request's (upper bound on what a prefix cache can reuse)
  same_msgs number of leading messages identical to the previous main request

"Main" requests are the agent loop's (they carry tools); everything else
(title generation, memory extraction, reflection …) is listed as auxiliary.

Usage:
    python3 bench_report.py --out ./bench-out --label uclaw-new --db ~/.uclaw/uclaw.db
    python3 bench_report.py --out ./bench-out --label uclaw-pi  --db ~/.uclaw-pi/uclaw.db
    python3 bench_report.py --out ./bench-out --compare uclaw-new uclaw-pi \
        --db-of uclaw-new=~/.uclaw/uclaw.db --db-of uclaw-pi=~/.uclaw-pi/uclaw.db
"""
import argparse
import json
import os
import sqlite3
import statistics as st


def load_requests(out_dir: str, label: str):
    path = os.path.join(out_dir, label, "requests.jsonl")
    recs = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            if r.get("path", "").rstrip("/").endswith("chat/completions") and r.get("status") == 200:
                recs.append(r)
    recs.sort(key=lambda r: r["t_arrive"])
    return recs


def load_body(out_dir: str, label: str, seq: int):
    p = os.path.join(out_dir, label, f"{seq:05d}.req.json")
    try:
        with open(p, "rb") as f:
            return json.loads(f.read())
    except Exception:
        return None


def user_turns(db_path: str, t_lo: float, t_hi: float):
    db = sqlite3.connect(os.path.expanduser(db_path))
    rows = db.execute(
        "SELECT created_at, session_id, substr(content,1,40) FROM agent_messages "
        "WHERE role='user' AND created_at BETWEEN ? AND ? ORDER BY created_at",
        (int(t_lo), int(t_hi)),
    ).fetchall()
    return [{"t": float(r[0]), "session": r[1], "text": r[2]} for r in rows]


def prefix_stats(prev, cur):
    if not prev or not cur:
        return None, None
    pm, cm = prev.get("messages") or [], cur.get("messages") or []
    same = 0
    for a, b in zip(pm, cm):
        if a == b:
            same += 1
        else:
            break
    ps = json.dumps(pm, ensure_ascii=False, sort_keys=True)
    cs = json.dumps(cm, ensure_ascii=False, sort_keys=True)
    n = 0
    for a, b in zip(ps, cs):
        if a != b:
            break
        n += 1
    tools_same = (prev.get("tools") == cur.get("tools"))
    return same, (100.0 * n / max(len(cs), 1), tools_same)


def pct(xs, p):
    xs = sorted(x for x in xs if x is not None)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def fmt(x, unit="ms"):
    return "-" if x is None else (f"{x/1000:.2f}s" if unit == "ms" else f"{x:.0f}{unit}")


def analyze(out_dir, label, db_path, verbose=True):
    reqs = load_requests(out_dir, label)
    if not reqs:
        print(f"[{label}] no requests recorded")
        return None
    main = [r for r in reqs if (r.get("n_tools") or 0) > 0]
    aux = [r for r in reqs if (r.get("n_tools") or 0) == 0]
    turns = user_turns(db_path, reqs[0]["t_arrive"] - 120_000, reqs[-1]["t_arrive"] + 1)

    # assign each main request to the latest user turn at or before it
    for t in turns:
        t["reqs"] = []
    for r in main:
        owner = None
        for t in turns:
            if t["t"] <= r["t_arrive"]:
                owner = t
        if owner:
            owner["reqs"].append(r)
    turns = [t for t in turns if t["reqs"]]

    prev_body = None
    rows = []
    for r in main:
        body = load_body(out_dir, label, r["seq"])
        same, pfx = prefix_stats(prev_body, body)
        prev_body = body
        u = r.get("usage") or {}
        pt = u.get("prompt_tokens")
        hit = u.get("prompt_cache_hit_tokens")
        if hit is None:
            hit = (u.get("prompt_tokens_details") or {}).get("cached_tokens")
        r["_hit_pct"] = (100.0 * hit / pt) if (pt and hit is not None) else None
        r["_prefix_pct"] = pfx[0] if pfx else None
        r["_tools_same"] = pfx[1] if pfx else None
        r["_same_msgs"] = same
        r["_provider_ttft"] = (r["t_first_token"] - r["t_arrive"]) if r.get("t_first_token") else None
        rows.append(r)

    if verbose:
        print(f"\n=== {label}: {len(turns)} turns, {len(main)} main requests, {len(aux)} auxiliary ===")
        print(f"{'turn':>4} {'pre_llm':>8} {'e2e_ttft':>9} {'turn':>8} {'calls':>5}  first-request: "
              f"{'prov_ttft':>9} {'in_tok':>7} {'hit%':>5} {'prefix%':>7} {'same_msgs':>9}  text")
    tsum = []
    for i, t in enumerate(turns, 1):
        f = t["reqs"][0]
        pre = f["t_arrive"] - t["t"]
        e2e = (f["t_first_token"] - t["t"]) if f.get("t_first_token") else None
        tt = t["reqs"][-1]["t_end"] - t["t"]
        tsum.append({"pre": pre, "e2e": e2e, "turn": tt, "calls": len(t["reqs"])})
        if verbose:
            u = f.get("usage") or {}
            print(f"{i:>4} {fmt(pre):>8} {fmt(e2e):>9} {fmt(tt):>8} {len(t['reqs']):>5}  "
                  f"{fmt(f['_provider_ttft']):>23} {str(u.get('prompt_tokens')):>7} "
                  f"{fmt(f['_hit_pct'], '%'):>5} {fmt(f['_prefix_pct'], '%'):>7} "
                  f"{str(f['_same_msgs']):>9}  {t['text']!r}")

    hits = [r["_hit_pct"] for r in rows if r["_hit_pct"] is not None]
    tot_in = sum((r.get("usage") or {}).get("prompt_tokens") or 0 for r in rows)
    tot_hit = sum(((r.get("usage") or {}).get("prompt_cache_hit_tokens")
                   or ((r.get("usage") or {}).get("prompt_tokens_details") or {}).get("cached_tokens") or 0)
                  for r in rows)
    summary = {
        "label": label,
        "turns": len(turns),
        "main_requests": len(main),
        "aux_requests": len(aux),
        "calls_per_turn": (len(main) / len(turns)) if turns else None,
        "pre_llm_median": st.median([x["pre"] for x in tsum]) if tsum else None,
        "e2e_ttft_median": st.median([x["e2e"] for x in tsum if x["e2e"]]) if tsum else None,
        "e2e_ttft_p90": pct([x["e2e"] for x in tsum], 0.9),
        "provider_ttft_median": st.median([r["_provider_ttft"] for r in rows if r["_provider_ttft"]]) if rows else None,
        "turn_median": st.median([x["turn"] for x in tsum]) if tsum else None,
        "avg_prompt_tokens": (tot_in / len(rows)) if rows else None,
        "cache_hit_weighted": (100.0 * tot_hit / tot_in) if tot_in else None,
        "cache_hit_median": st.median(hits) if hits else None,
        "tools_changed": sum(1 for r in rows if r["_tools_same"] is False),
    }
    if verbose:
        print("\n  summary:")
        for k, v in summary.items():
            if k == "label":
                continue
            if isinstance(v, float):
                v = fmt(v) if ("ttft" in k or "llm" in k or k.startswith("turn")) else f"{v:.1f}"
            print(f"    {k:<22} {v}")
        if aux:
            print(f"  auxiliary requests (no tools): {len(aux)} — models: "
                  f"{sorted(set(r.get('model') for r in aux))}")
    return summary


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="bench-out")
    ap.add_argument("--label")
    ap.add_argument("--db")
    ap.add_argument("--compare", nargs=2)
    ap.add_argument("--db-of", action="append", default=[], help="label=path")
    a = ap.parse_args()
    if a.compare:
        dbs = dict(x.split("=", 1) for x in a.db_of)
        sums = [analyze(a.out, lbl, dbs[lbl]) for lbl in a.compare]
        if all(sums):
            print(f"\n=== comparison ===\n{'metric':<22} {a.compare[0]:>14} {a.compare[1]:>14}")
            for k in sums[0]:
                if k == "label":
                    continue
                vals = []
                for s in sums:
                    v = s[k]
                    if isinstance(v, float):
                        v = fmt(v) if ("ttft" in k or "llm" in k or k.startswith("turn")) else f"{v:.1f}"
                    vals.append(str(v))
                print(f"{k:<22} {vals[0]:>14} {vals[1]:>14}")
    else:
        analyze(a.out, a.label, a.db)


if __name__ == "__main__":
    main()
