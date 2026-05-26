# Side-finding (NOT a blocker) — B1 format pivot changes get_file_skeleton anchor display

- PR: C2-Dirac-B1. When: 2026-05-25. Per invariant #3.

## Finding
B1 pivots `AnchorStateManager` to store TOKENS (`Apple`) instead of full
`Apple§<hash6hex>` strings, so `get_anchors()` now returns tokens. The
out-of-scope tool `get_file_skeleton.rs:88-91` calls
`register_file_lines()` + `get_anchors()` and feeds the result to
`skeleton::generate_skeleton`, which renders `# ... §{anchor} ...`.

Consequence: get_file_skeleton's skeleton output anchor display changes from
`# ... §Apple§a1f89c ...` (pre-B1) to `# ... §Apple ...` (post-B1).

The B1 implementer's report claimed "legacy returns the old format because
nothing downstream consumed it" — that is INACCURATE: skeleton.rs DOES consume
get_anchors() output. The change is real.

## Severity: LOW (not a regression)
- `cargo build` clean; `cargo test agent::skeleton` 2/2 pass (the skeleton unit
  test passes its own anchors literally, so it's unaffected); get_file_skeleton
  has 0 dedicated tests → no test breaks.
- The new display (bare token `Apple`) is arguably MORE consistent with B1's
  anchored-edit model: the token `Apple` is exactly what `EditTool`'s anchored
  validator (`resolve_anchor_index`) resolves, so a skeleton-derived anchor now
  round-trips to an edit. Pre-B1's `Apple§hash` did not.

## Recommendation
Accept as a benign, consistency-improving side-effect of the in-scope format
pivot. If a future PR wants get_file_skeleton to show `<token>§<literal>` (full
B1 anchor form) for parity with read_file, that's a small follow-up — out of B1
scope. No action required for B1 merge.
