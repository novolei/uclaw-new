# Browser Runtime Pack Portable Symlinks

## Context

The real runtime pack can be generated and passes the existing validator, but
the Rust managed installer fails while copying the generated staging pack into
`~/.uclaw/browser-runtime`. Investigation found dangling `.bin` symlinks in the
generated `node_modules` tree that still point at the deleted `npm-work`
directory.

## Scope

- Make runtime-pack generation produce a portable `node_modules` tree.
- Make validation reject dangling symlinks so this cannot pass as "valid" again.
- Preserve valid symlinks when the Rust managed installer copies the pack.
- Add focused Node tests for the generator and validator behavior.

## Out Of Scope

- Browser provider routing, Settings UI redesign, MCP configuration, and Rust
  installer behavior.
- Reworking the runtime pack manifest or provider priority policy.

## Verification

- `node --test scripts/browser-runtime/*.test.mjs`
- `cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_pack_runner::tests::copy_dir_recursive_preserves_directory_symlinks`
- `node scripts/browser-runtime/generate-runtime-pack.mjs`
- `node scripts/browser-runtime/validate-runtime-pack.mjs src-tauri/.runtime-pack-staging/browser-runtime-pack-v1`
