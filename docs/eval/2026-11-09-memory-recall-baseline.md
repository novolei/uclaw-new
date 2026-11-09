# Memory recall baseline (uclaw-new) — 2026-11-09

Harness: `src-tauri/examples/memory_recall_eval/` —
`cargo run --example memory_recall_eval -- --verbose --out report.json`.
It snapshots `~/.uclaw/uclaw.db` and `bucket_seal/chunks.db` with `VACUUM INTO`
(live data untouched — mtimes verified unchanged) and runs a JSONL golden set.
The golden set holds the user's personal facts, so it lives outside the repo at
`~/.uclaw/eval/memory_recall_golden.jsonl` (seeded from uclaw-pi's 37-question
set — same user); `golden.sample.jsonl` shows the format.

Golden set: 37 natural-language "do you remember…" questions — 27 Chinese, 10
English; 28 profile facts, 3 past episodes, 6 learned-skill lookups.

Data at run time: 747 graph nodes, 1181 bucket_seal chunks, 817 live
summaries (embedded with bge-small-en-v1.5, the app's default in-process
embedder). Space `default`; recall config defaults.

## Ranked recall (expected fact in top-k)

| group | n | backend | hit@1 | hit@3 | hit@5 | hit@10 | MRR | errors | p50 ms |
|---|---|---|---|---|---|---|---|---|---|
| all | 37 | graph_fts | 0.0% | 2.7% | 2.7% | 2.7% | 0.014 | 0 | 0.2 |
| all | 37 | graph_plan | 2.7% | 8.1% | 16.2% | 16.2% | 0.067 | 0 | 80.1 |
| all | 37 | bucket_seal_fts | 2.7% | 2.7% | 2.7% | 2.7% | 0.027 | 9 | 0.2 |
| all | 37 | bucket_seal_hybrid | 5.4% | 10.8% | 13.5% | 18.9% | 0.093 | 0 | 42.3 |
| lang=en | 10 | graph_fts | 0.0% | 0.0% | 0.0% | 0.0% | 0.000 | 0 | 0.1 |
| lang=en | 10 | graph_plan | 0.0% | 0.0% | 10.0% | 10.0% | 0.020 | 0 | 64.9 |
| lang=en | 10 | bucket_seal_fts | 0.0% | 0.0% | 0.0% | 0.0% | 0.000 | 8 | 0.1 |
| lang=en | 10 | bucket_seal_hybrid | 0.0% | 20.0% | 30.0% | 40.0% | 0.134 | 0 | 41.2 |
| lang=zh | 27 | graph_fts | 0.0% | 3.7% | 3.7% | 3.7% | 0.019 | 0 | 0.2 |
| lang=zh | 27 | graph_plan | 3.7% | 11.1% | 18.5% | 18.5% | 0.085 | 0 | 93.2 |
| lang=zh | 27 | bucket_seal_fts | 3.7% | 3.7% | 3.7% | 3.7% | 0.037 | 1 | 0.2 |
| lang=zh | 27 | bucket_seal_hybrid | 7.4% | 7.4% | 7.4% | 11.1% | 0.078 | 0 | 42.4 |

## What reaches the prompt

| group | n | stored anywhere | agent prompt hit | chat prompt hit | agent hit / stored | avg agent chars | avg chat chars |
|---|---|---|---|---|---|---|---|
| all | 37 | 89.2% | 13.5% | 10.8% | 15.2% | 13263 | 6317 |
| lang=en | 10 | 90.0% | 20.0% | 20.0% | 22.2% | 13730 | 6667 |
| lang=zh | 27 | 88.9% | 11.1% | 7.4% | 12.5% | 13090 | 6187 |

(Per-category rows omitted here; profile questions are the worst: agent
prompt hit 3.6% with 92.9% stored.)

## Root causes (verified)

1. **No segmentation for Chinese.** `memory_fts` is trigram (needs ≥ 3-char
   exact substrings) and `mem_tree_chunks_fts` is `porter unicode61` (a CJK run
   is one token). A natural Chinese question matches nothing in either; e.g.
   `MATCH` on a two-character Chinese place name returns 0 on both while `LIKE` finds it in 10 chunks.
2. **Raw question passed to `MATCH`.** Any `?` (and other FTS syntax) is a
   syntax error → empty: 9 of 37 `bucket_seal_fts` queries errored.
3. **English-only vectors over summaries only.** bge-small-en-v1.5 embeds
   bucket_seal summaries; chunks and graph facts have no vectors, and Chinese
   questions embed poorly.
4. **What does get recalled is not the fact.** 89.2% of answers are stored, but
   only 13.5% (agent) / 10.8% (chat) reach the prompt; the agent block averages
   13k chars, dominated by query-independent sections (boot, learned skills).

## Gate

Best ranked hit@5 = 16.2% (graph_plan) < 50% → proceed with the segmented
BM25 index (plan Task 3+). Bar to beat: hit@5 16.2%, agent prompt hit 13.5%,
chat prompt hit 10.8%.
