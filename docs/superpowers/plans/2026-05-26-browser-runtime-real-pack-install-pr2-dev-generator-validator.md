# Browser Runtime Real Pack Install PR2 Dev Generator And Validator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add dev-only scripts that generate and validate a complete macOS arm64 Browser runtime pack staging source without committing large runtime artifacts.

**Architecture:** Keep generation and validation in plain Node ESM scripts under `scripts/browser-runtime/`. The validator is deterministic and can run on small fixtures; the generator defaults to pinned downloads/install steps and exposes an explicit `--from-local-toolchain` escape hatch for developer machines.

**Tech Stack:** Node.js ESM, built-in `node:test`, `fs/promises`, `child_process`, repo `.gitignore`, existing Rust manifest constants mirrored in script constants.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `scripts/browser-runtime/runtime-pack-constants.mjs` | Shared versions, paths, and manifest helpers for generator/validator/tests. |
| `scripts/browser-runtime/validate-runtime-pack.mjs` | CLI validator for a pack directory. |
| `scripts/browser-runtime/generate-runtime-pack.mjs` | CLI generator for full dev staging packs. |
| `scripts/browser-runtime/validate-runtime-pack.test.mjs` | Node tests for validator behavior with small fixtures. |
| `scripts/browser-runtime/generate-runtime-pack.test.mjs` | Node tests for generator layout/argument behavior without downloading Chromium. |
| `.gitignore` | Ignore generated staging pack directory. |
| `docs/superpowers/specs/2026-05-26-browser-runtime-real-pack-install-design.md` | Add command notes only if implementation diverges from spec wording. |

## Boundaries

- This PR does not call the Rust installer.
- This PR does not change UI.
- This PR does not commit generated runtime packs.
- Script tests must not download real Node or Chromium.
- Full generator manual validation may require network and can be documented as manual.

## ADR 18 Answers

1. Intent: create a real local runtime-pack source for Browser Runtime prepare.
2. Autonomy: dev script builds app-managed assets outside git.
3. Truth source: validator checks actual files, versions, and runtime imports.
4. TaskEvent: not applicable to scripts; generator logs explicit steps.
5. Context: pinned versions and output path.
6. Capability: supplies source pack required by Playwright CLI/MCP.
7. Hooks: validator, node tests, manual full generation.
8. Projection: future UI resolver can see dev staging source.
9. Harness: node tests plus manual generator/validator command.
10. Rollback: delete ignored staging directory.
11. Non-ownership: no release packaging, no remote signed update channel, no UI.

### Task 1: Add Shared Script Constants

**Files:**
- Create: `scripts/browser-runtime/runtime-pack-constants.mjs`

- [ ] **Step 1: Create constants module**

Add:

```js
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')

export const PACK_VERSION = 'browser-runtime-pack-v1'
export const NODE_VERSION = '22.16.0'
export const PLAYWRIGHT_VERSION = '1.53.0'
export const PLAYWRIGHT_MCP_VERSION = '0.0.75'
export const WORKER_VERSION = '0.1.0'
export const CHROMIUM_REVISION = '1181'
export const DEFAULT_OUTPUT_DIR = path.join(
  repoRoot,
  'src-tauri/.runtime-pack-staging',
  PACK_VERSION,
)
export const DEFAULT_WORKER_SOURCE = path.join(
  repoRoot,
  'scripts/browser-runtime/worker/uclaw-playwright-worker.mjs',
)
export const NODE_DARWIN_ARM64_TARBALL_URL =
  `https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-darwin-arm64.tar.gz`

export function manifest() {
  return {
    packVersion: PACK_VERSION,
    nodeVersion: NODE_VERSION,
    playwrightVersion: PLAYWRIGHT_VERSION,
    playwrightMcpVersion: PLAYWRIGHT_MCP_VERSION,
    workerVersion: WORKER_VERSION,
    chromiumRevision: CHROMIUM_REVISION,
    downloadUrl: 'app-managed-dev-staging',
    archiveSizeBytes: 0,
    sha256: 'dev-staging-source',
    minimumAppVersion: '0.1.0',
    rollbackPackVersion: 'browser-runtime-pack-v0',
    releaseChannel: 'stable',
  }
}

export function requiredPaths() {
  return [
    'runtime-pack.manifest.json',
    'node/bin/node',
    'node_modules/playwright',
    'node_modules/@playwright/mcp',
    'worker/uclaw-playwright-worker.mjs',
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`,
  ]
}
```

- [ ] **Step 2: Commit constants**

```bash
git add scripts/browser-runtime/runtime-pack-constants.mjs
git commit -m "chore(browser-runtime): add runtime pack script constants" -m "Verification: node -e \"import('./scripts/browser-runtime/runtime-pack-constants.mjs').then(m => console.log(m.PACK_VERSION))\" (expected prints browser-runtime-pack-v1)"
```

### Task 2: Add Validator CLI

**Files:**
- Create: `scripts/browser-runtime/validate-runtime-pack.mjs`
- Create: `scripts/browser-runtime/validate-runtime-pack.test.mjs`

- [ ] **Step 1: Write validator tests**

Create `scripts/browser-runtime/validate-runtime-pack.test.mjs`:

```js
import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { validateRuntimePack } from './validate-runtime-pack.mjs'
import { CHROMIUM_REVISION, manifest } from './runtime-pack-constants.mjs'

async function makePackFixture({ missing = [] } = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'uclaw-runtime-pack-'))
  const write = async (relative, contents = '') => {
    if (missing.includes(relative)) return
    const target = path.join(root, relative)
    await fs.mkdir(path.dirname(target), { recursive: true })
    await fs.writeFile(target, contents)
  }
  await write('runtime-pack.manifest.json', JSON.stringify(manifest(), null, 2))
  await write('node/bin/node', '#!/bin/sh\necho v22.16.0\n')
  await fs.chmod(path.join(root, 'node/bin/node'), 0o755)
  await write('node_modules/playwright/package.json', JSON.stringify({ version: '1.53.0' }))
  await write('node_modules/@playwright/mcp/package.json', JSON.stringify({ version: '0.0.75' }))
  await write('worker/uclaw-playwright-worker.mjs', 'console.log("worker")\n')
  await write(
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`,
    'chromium',
  )
  await fs.chmod(
    path.join(root, `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`),
    0o755,
  )
  return root
}

test('validator passes complete small fixture with runtime checks disabled', async () => {
  const root = await makePackFixture()
  const result = await validateRuntimePack(root, { runtimeChecks: false })

  assert.equal(result.ok, true)
  assert.deepEqual(result.errors, [])
})

test('validator reports missing required paths', async () => {
  const root = await makePackFixture({ missing: ['node/bin/node'] })
  const result = await validateRuntimePack(root, { runtimeChecks: false })

  assert.equal(result.ok, false)
  assert.ok(result.errors.some((error) => error.includes('node/bin/node')))
})
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
node --test scripts/browser-runtime/validate-runtime-pack.test.mjs
```

Expected: FAIL because validator module does not exist.

- [ ] **Step 3: Implement validator**

Create `scripts/browser-runtime/validate-runtime-pack.mjs`:

```js
#!/usr/bin/env node
import fs from 'node:fs/promises'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import {
  CHROMIUM_REVISION,
  NODE_VERSION,
  PACK_VERSION,
  PLAYWRIGHT_MCP_VERSION,
  PLAYWRIGHT_VERSION,
  WORKER_VERSION,
  requiredPaths,
} from './runtime-pack-constants.mjs'

async function exists(target) {
  try {
    await fs.access(target)
    return true
  } catch {
    return false
  }
}

async function readJson(target) {
  return JSON.parse(await fs.readFile(target, 'utf8'))
}

export async function validateRuntimePack(root, options = {}) {
  const runtimeChecks = options.runtimeChecks ?? true
  const errors = []
  for (const relative of requiredPaths()) {
    if (!(await exists(path.join(root, relative)))) {
      errors.push(`missing ${relative}`)
    }
  }

  const manifestPath = path.join(root, 'runtime-pack.manifest.json')
  if (await exists(manifestPath)) {
    try {
      const value = await readJson(manifestPath)
      const expected = {
        packVersion: PACK_VERSION,
        nodeVersion: NODE_VERSION,
        playwrightVersion: PLAYWRIGHT_VERSION,
        playwrightMcpVersion: PLAYWRIGHT_MCP_VERSION,
        workerVersion: WORKER_VERSION,
        chromiumRevision: CHROMIUM_REVISION,
      }
      for (const [key, expectedValue] of Object.entries(expected)) {
        if (value[key] !== expectedValue) {
          errors.push(`${key} mismatch: expected ${expectedValue}, got ${value[key]}`)
        }
      }
    } catch (error) {
      errors.push(`invalid runtime-pack.manifest.json: ${error.message}`)
    }
  }

  if (runtimeChecks && errors.length === 0) {
    const nodeBin = path.join(root, 'node/bin/node')
    const nodeVersion = spawnSync(nodeBin, ['--version'], { encoding: 'utf8' })
    if (nodeVersion.status !== 0 || !nodeVersion.stdout.trim().includes(NODE_VERSION)) {
      errors.push(`node --version failed or mismatched: ${nodeVersion.stderr || nodeVersion.stdout}`)
    }

    const playwrightCheck = spawnSync(
      nodeBin,
      ['-e', "require('playwright'); require('@playwright/mcp/package.json')"],
      {
        cwd: root,
        env: { ...process.env, NODE_PATH: path.join(root, 'node_modules') },
        encoding: 'utf8',
      },
    )
    if (playwrightCheck.status !== 0) {
      errors.push(`node package check failed: ${playwrightCheck.stderr || playwrightCheck.stdout}`)
    }
  }

  return { ok: errors.length === 0, errors }
}

async function main() {
  const root = process.argv[2]
  if (!root) {
    console.error('Usage: validate-runtime-pack.mjs <browser-runtime-pack-v1>')
    process.exit(2)
  }
  const result = await validateRuntimePack(path.resolve(root))
  if (!result.ok) {
    for (const error of result.errors) console.error(error)
    process.exit(1)
  }
  console.log(`Runtime pack valid: ${path.resolve(root)}`)
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error)
    process.exit(1)
  })
}
```

- [ ] **Step 4: Run validator tests**

Run:

```bash
node --test scripts/browser-runtime/validate-runtime-pack.test.mjs
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/browser-runtime/validate-runtime-pack.mjs scripts/browser-runtime/validate-runtime-pack.test.mjs
git commit -m "test(browser-runtime): validate runtime pack layout" -m "Verification: node --test scripts/browser-runtime/validate-runtime-pack.test.mjs (expected PASS)"
```

### Task 3: Add Generator CLI Skeleton With Testable No-Download Path

**Files:**
- Create: `scripts/browser-runtime/generate-runtime-pack.mjs`
- Create: `scripts/browser-runtime/generate-runtime-pack.test.mjs`
- Create: `scripts/browser-runtime/worker/uclaw-playwright-worker.mjs`

- [ ] **Step 1: Write generator tests**

Create `scripts/browser-runtime/generate-runtime-pack.test.mjs`:

```js
import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { generateRuntimePack } from './generate-runtime-pack.mjs'
import { PACK_VERSION } from './runtime-pack-constants.mjs'

test('generator requires explicit local toolchain flag for local mode', async () => {
  await assert.rejects(
    () => generateRuntimePack({ fromLocalToolchain: false, skipDownloadsForTest: true }),
    /skipDownloadsForTest is only allowed with explicit test fixtures/,
  )
})

test('generator writes manifest and worker in test fixture mode', async () => {
  const outputRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'uclaw-runtime-pack-out-'))
  const outputDir = path.join(outputRoot, PACK_VERSION)
  const result = await generateRuntimePack({
    outputDir,
    fromLocalToolchain: true,
    skipDownloadsForTest: true,
  })

  assert.equal(result.outputDir, outputDir)
  assert.ok(await exists(path.join(outputDir, 'runtime-pack.manifest.json')))
  assert.ok(await exists(path.join(outputDir, 'worker/uclaw-playwright-worker.mjs')))
})

async function exists(target) {
  try {
    await fs.access(target)
    return true
  } catch {
    return false
  }
}
```

- [ ] **Step 2: Run failing generator tests**

Run:

```bash
node --test scripts/browser-runtime/generate-runtime-pack.test.mjs
```

Expected: FAIL because generator does not exist.

- [ ] **Step 3: Add worker script**

Create `scripts/browser-runtime/worker/uclaw-playwright-worker.mjs`:

```js
#!/usr/bin/env node
process.stdin.setEncoding('utf8')
process.stdout.write(JSON.stringify({
  type: 'uclaw.playwright.worker.ready',
  workerVersion: '0.1.0',
}) + '\n')
```

- [ ] **Step 4: Implement generator CLI**

Create `scripts/browser-runtime/generate-runtime-pack.mjs`:

```js
#!/usr/bin/env node
import fs from 'node:fs/promises'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import {
  DEFAULT_OUTPUT_DIR,
  DEFAULT_WORKER_SOURCE,
  NODE_DARWIN_ARM64_TARBALL_URL,
  NODE_VERSION,
  PACK_VERSION,
  PLAYWRIGHT_MCP_VERSION,
  PLAYWRIGHT_VERSION,
  manifest,
} from './runtime-pack-constants.mjs'
import { validateRuntimePack } from './validate-runtime-pack.mjs'

export async function generateRuntimePack(options = {}) {
  const outputDir = path.resolve(options.outputDir ?? DEFAULT_OUTPUT_DIR)
  const fromLocalToolchain = options.fromLocalToolchain ?? false
  const skipDownloadsForTest = options.skipDownloadsForTest ?? false

  if (skipDownloadsForTest && !fromLocalToolchain) {
    throw new Error('skipDownloadsForTest is only allowed with explicit test fixtures')
  }

  await fs.rm(outputDir, { recursive: true, force: true })
  await fs.mkdir(outputDir, { recursive: true })
  await fs.mkdir(path.join(outputDir, 'worker'), { recursive: true })
  await fs.copyFile(
    DEFAULT_WORKER_SOURCE,
    path.join(outputDir, 'worker/uclaw-playwright-worker.mjs'),
  )
  await fs.writeFile(
    path.join(outputDir, 'runtime-pack.manifest.json'),
    JSON.stringify(manifest(), null, 2),
  )

  if (skipDownloadsForTest) {
    return { outputDir, source: 'test_fixture' }
  }

  if (fromLocalToolchain) {
    console.warn('[browser-runtime] generating from local toolchain; this is a dev escape hatch')
    await copyLocalToolchain(outputDir)
  } else {
    await installPinnedRuntime(outputDir)
  }

  const validation = await validateRuntimePack(outputDir)
  if (!validation.ok) {
    throw new Error(`generated runtime pack failed validation:\n${validation.errors.join('\n')}`)
  }
  return { outputDir, source: fromLocalToolchain ? 'local_toolchain' : 'pinned_download' }
}

async function installPinnedRuntime(outputDir) {
  const buildDir = path.join(outputDir, '.build')
  const nodeArchive = path.join(buildDir, `node-v${NODE_VERSION}-darwin-arm64.tar.gz`)
  const extractedNodeDir = path.join(buildDir, `node-v${NODE_VERSION}-darwin-arm64`)
  const nodeRoot = path.join(outputDir, 'node')
  const npmPrefix = path.join(outputDir, 'npm-work')
  await fs.mkdir(buildDir, { recursive: true })
  await fs.mkdir(npmPrefix, { recursive: true })

  console.log(`[browser-runtime] download pinned Node ${NODE_VERSION}`)
  await downloadFile(NODE_DARWIN_ARM64_TARBALL_URL, nodeArchive)
  run('tar', ['-xzf', nodeArchive, '-C', buildDir])
  await fs.rm(nodeRoot, { recursive: true, force: true })
  await fs.rename(extractedNodeDir, nodeRoot)

  const nodeBin = path.join(nodeRoot, 'bin/node')
  const npmCli = path.join(nodeRoot, 'lib/node_modules/npm/bin/npm-cli.js')
  run(nodeBin, [npmCli, 'init', '-y'], { cwd: npmPrefix })
  run(nodeBin, [
    npmCli,
    'install',
    `playwright@${PLAYWRIGHT_VERSION}`,
    `@playwright/mcp@${PLAYWRIGHT_MCP_VERSION}`,
  ], {
    cwd: npmPrefix,
    env: { ...process.env, PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: '1' },
  })
  await fs.cp(path.join(npmPrefix, 'node_modules'), path.join(outputDir, 'node_modules'), {
    recursive: true,
  })

  const browserPath = path.join(outputDir, 'ms-playwright')
  run(nodeBin, [
    path.join(outputDir, 'node_modules/playwright/cli.js'),
    'install',
    'chromium',
  ], {
    cwd: outputDir,
    env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: browserPath },
  })
  await fs.rm(path.join(outputDir, '.build'), { recursive: true, force: true })
  await fs.rm(path.join(outputDir, 'npm-work'), { recursive: true, force: true })
}

async function copyLocalToolchain(outputDir) {
  const node = spawnSync('node', ['--version'], { encoding: 'utf8' })
  if (node.status !== 0) {
    throw new Error('local node is not available')
  }
  if (node.stdout.trim() !== `v${NODE_VERSION}`) {
    throw new Error(`local node must be v${NODE_VERSION}, got ${node.stdout.trim()}`)
  }
  await fs.mkdir(path.join(outputDir, 'node/bin'), { recursive: true })
  const nodePath = process.execPath
  await fs.copyFile(nodePath, path.join(outputDir, 'node/bin/node'))
  await fs.mkdir(path.join(outputDir, 'node_modules/playwright'), { recursive: true })
  await fs.mkdir(path.join(outputDir, 'node_modules/@playwright/mcp'), { recursive: true })
  await fs.writeFile(
    path.join(outputDir, 'node_modules/playwright/package.json'),
    JSON.stringify({ version: PLAYWRIGHT_VERSION }),
  )
  await fs.writeFile(
    path.join(outputDir, 'node_modules/@playwright/mcp/package.json'),
    JSON.stringify({ version: PLAYWRIGHT_MCP_VERSION }),
  )
  await fs.mkdir(
    path.join(outputDir, 'ms-playwright/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS'),
    { recursive: true },
  )
  await fs.writeFile(
    path.join(outputDir, 'ms-playwright/chromium-1181/chrome-mac/Chromium.app/Contents/MacOS/Chromium'),
    '',
  )
}

async function downloadFile(url, targetPath) {
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`download failed ${response.status}: ${url}`)
  }
  await fs.writeFile(targetPath, Buffer.from(await response.arrayBuffer()))
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    ...options,
  })
  if (result.status !== 0) {
    throw new Error(`command failed: ${command} ${args.join(' ')}`)
  }
}

function parseArgs(argv) {
  const args = { outputDir: DEFAULT_OUTPUT_DIR, fromLocalToolchain: false }
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '--from-local-toolchain') args.fromLocalToolchain = true
    else if (arg === '--output') args.outputDir = argv[++index]
    else throw new Error(`Unknown argument: ${arg}`)
  }
  return args
}

async function main() {
  const result = await generateRuntimePack(parseArgs(process.argv.slice(2)))
  console.log(`Generated ${PACK_VERSION} at ${result.outputDir} (${result.source})`)
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error.message)
    process.exit(1)
  })
}
```

- [ ] **Step 5: Run generator tests**

Run:

```bash
node --test scripts/browser-runtime/generate-runtime-pack.test.mjs
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/browser-runtime/generate-runtime-pack.mjs scripts/browser-runtime/generate-runtime-pack.test.mjs scripts/browser-runtime/worker/uclaw-playwright-worker.mjs
git commit -m "feat(browser-runtime): add runtime pack generator" -m "Verification: node --test scripts/browser-runtime/generate-runtime-pack.test.mjs (expected PASS)"
```

### Task 4: Verify Pinned Install Path

**Files:**
- Verify: `scripts/browser-runtime/generate-runtime-pack.mjs`

- [ ] **Step 1: Run script tests**

Run:

```bash
node --test scripts/browser-runtime/validate-runtime-pack.test.mjs scripts/browser-runtime/generate-runtime-pack.test.mjs
```

Expected: PASS.

- [ ] **Step 2: Manual full generation on macOS arm64**

Run:

```bash
node scripts/browser-runtime/generate-runtime-pack.mjs
node scripts/browser-runtime/validate-runtime-pack.mjs src-tauri/.runtime-pack-staging/browser-runtime-pack-v1
```

Expected: generator downloads/installs packages and Chromium, then validator prints `Runtime pack valid`.

- [ ] **Step 3: Confirm generated pack remains untracked**

```bash
git status --short src-tauri/.runtime-pack-staging
```

Expected: generated staging files are untracked before Task 5 adds the `.gitignore` rule. Do not stage runtime-pack artifacts.

### Task 5: Ignore Generated Staging And Final PR2 Verification

**Files:**
- Modify: `.gitignore`
- Verify: script tests and optional manual generation.

- [ ] **Step 1: Add gitignore entry**

Add near embedded/runtime ignores in `.gitignore`:

```gitignore
# Browser runtime pack staging generated by scripts/browser-runtime/generate-runtime-pack.mjs
src-tauri/.runtime-pack-staging/
```

- [ ] **Step 2: Verify generated pack is ignored**

Run:

```bash
git check-ignore -v src-tauri/.runtime-pack-staging/browser-runtime-pack-v1/runtime-pack.manifest.json
```

Expected: prints the `.gitignore` rule.

- [ ] **Step 3: Run script tests**

Run:

```bash
node --test scripts/browser-runtime/validate-runtime-pack.test.mjs scripts/browser-runtime/generate-runtime-pack.test.mjs
git diff --check
```

Expected: PASS / exit 0.

- [ ] **Step 4: Run GitNexus detect**

Run:

```bash
npx gitnexus detect-changes --scope staged --repo uclaw-new
```

Expected: exit 0. Include stale-index warnings in PR body if present.

- [ ] **Step 5: Commit gitignore**

```bash
git add .gitignore
git commit -m "chore(browser-runtime): ignore generated runtime pack staging" -m "Verification: git check-ignore -v src-tauri/.runtime-pack-staging/browser-runtime-pack-v1/runtime-pack.manifest.json; node --test scripts/browser-runtime/validate-runtime-pack.test.mjs scripts/browser-runtime/generate-runtime-pack.test.mjs; git diff --check (expected PASS)"
```
