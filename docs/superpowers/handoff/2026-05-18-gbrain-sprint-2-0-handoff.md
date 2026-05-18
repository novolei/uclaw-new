# gbrain Sprint 2.0 — Mac-side commit + verify hand-off

Branch: continue on whatever current local branch (or create
`claude/sprint-gbrain-2-0`). Five files touched, all small + additive.

## What's in this commit

```
.gitignore                                    # +9 lines (bunembed + gbrain-source)
CLAUDE.md                                     # +6 lines (bootstrap + gotcha)
src-tauri/tauri.conf.json                     # +2 lines (resources)
scripts/setup-bun-runtime.sh                  # new — Bun binary fetcher
scripts/setup-gbrain-source.sh                # new — gbrain clone + bun install
```

Plus this handoff doc + `COMMIT_GBRAIN_SPRINT_2_0.txt`.

## Pre-flight

```bash
cd ~/Documents/uclaw
rm -f .git/index.lock          # if sandbox left one
git status                      # 3 modified + 2 untracked scripts + 2 docs
git diff --stat
```

## Commit (suggested)

```bash
# Core bundle pipeline (scripts + config)
git add scripts/setup-bun-runtime.sh \
        scripts/setup-gbrain-source.sh \
        .gitignore \
        src-tauri/tauri.conf.json \
        CLAUDE.md
git commit -F docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_0.txt

# Hand-off docs separately (or fold into the commit above — your call)
git add docs/superpowers/handoff/
git commit -m "docs(gbrain): Sprint 2.0 hand-off + commit message"
```

## Real verification (the part that actually exercises the new pipeline)

```bash
# Make scripts executable (sandbox set 700; tighten if you want)
chmod +x scripts/setup-bun-runtime.sh scripts/setup-gbrain-source.sh

# 1. Bun runtime
./scripts/setup-bun-runtime.sh
# Expected: ~50MB downloaded, src-tauri/bunembed/bun exists + runs
./src-tauri/bunembed/bun --version

# 2. gbrain source (this is where things might wobble)
./scripts/setup-gbrain-source.sh
# Expected: clones garrytan/gbrain @ main, runs bun install,
#           src-tauri/gbrain-source/{package.json,node_modules/...} exist
ls src-tauri/gbrain-source/
du -sh src-tauri/gbrain-source/
# Size estimate: ~80-100MB (gbrain source + node_modules including
# @electric-sql/pglite WASM at ~5MB). Script will print PGlite version
# + WASM file inventory during install — check that output to confirm
# the WASM landed where expected. ENOENT on those files at runtime is
# what killed Path C-1 in Sprint 0.

# If garrytan/gbrain isn't reachable or has different structure, override:
GBRAIN_REPO=https://github.com/<correct>/gbrain.git \
GBRAIN_REF=<tag> \
./scripts/setup-gbrain-source.sh

# 3. Confirm Tauri bundle picks them up
cd src-tauri
cargo tauri dev 2>&1 | tee /tmp/tauri-dev.log
# After the window opens (or first compile completes), look in the log:
grep -iE "bun|gbrain|resource" /tmp/tauri-dev.log | head -20
# Expected: resources are copied into the dev bundle without error.
```

## Things that could go wrong

1. **garrytan/gbrain doesn't exist / is private / has moved.**
   The default repo URL is a best guess based on Engines spec line 10.
   Use `GBRAIN_REPO` override. If you don't know the right repo,
   leave the scripts in but skip running them — Sprint 2.1 won't
   spawn the MCP entry when resources are missing.

2. **gbrain doesn't have a standard Bun entry point.**
   Script probes `src/index.ts`, `src/main.ts`, `src/cli.ts`,
   `src/mcp.ts`, `src/server.ts`, `index.ts`, `main.ts`. If none
   match, you'll see a warning and the path needs to be set manually
   in Sprint 2.1's MCP config.

3. **Bun version 1.1.42 pin gets too old.**
   The script tries GitHub API first, falls back to the pin. Override
   with `BUN_VERSION=1.1.55`. If the API works the pin is unused.

4. **Tauri bundle size jump triggers a CI guard.**
   We're adding ~120MB (50 Bun + ~70 gbrain). If there's a packaging
   size threshold somewhere, it'll trip. No such guard exists today
   (verified by grepping `cargo tauri build` configuration).

## What's NOT done here

- Sprint 2.1 — `mcp_servers.json` default entry for the gbrain
  stdio MCP. Needs to spawn `bunembed/bun gbrain-source/<entry> --stdio`
  at boot via the MCP auto-connect hook from PR-1.
    - **PGlite data dir** — Sprint 2.1 MUST pass an env var
      (probably `PGLITE_DATA_DIR` or whatever gbrain reads — check
      gbrain source) pointing at `~/.uclaw/gbrain/pgdata/`. Without
      this, PGlite tries to write to the Tauri resource dir which
      is read-only on macOS (.app bundle). Create the dir before
      spawn with `std::fs::create_dir_all`.
- Sprint 2.2 — data migration from SQLite EntityPages into gbrain.
- Sprint 2.3 — Foundation deprecation roadmap (when to stop
  maintaining the native `memory_nodes` substrate).
- Sprint 2.4 — Engines spec (Phase 15-21) revision marking pieces
  as superseded by gbrain MCP.

## Push

After verifying `cargo tauri dev` boots cleanly with the new
resources:

```bash
git push -u origin <branch>
```

Report back: sizes of bunembed + gbrain-source after install, any
deviations from the expected entry-point names, and whether
`cargo tauri dev` boots without errors mentioning the new resources.
