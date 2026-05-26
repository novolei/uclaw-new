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

async function isExecutable(target) {
  try {
    await fs.access(target, fs.constants.X_OK)
    return true
  } catch {
    return false
  }
}

async function readJson(target) {
  return JSON.parse(await fs.readFile(target, 'utf8'))
}

function run(command, args, options = {}) {
  return spawnSync(command, args, {
    encoding: 'utf8',
    ...options,
  })
}

function expectManifestField(errors, manifest, field, expected) {
  if (manifest[field] !== expected) {
    errors.push(`${field} mismatch: expected ${expected}, got ${manifest[field]}`)
  }
}

export async function validateRuntimePack(root, options = {}) {
  const runtimeChecks = options.runtimeChecks ?? true
  const errors = []
  const absoluteRoot = path.resolve(root)

  for (const relative of await findDanglingSymlinks(absoluteRoot)) {
    errors.push(`dangling symlink: ${relative}`)
  }

  for (const relative of requiredPaths()) {
    if (!(await exists(path.join(absoluteRoot, relative)))) {
      errors.push(`missing required path: ${relative}`)
    }
  }

  let packManifest = null
  try {
    packManifest = await readJson(path.join(absoluteRoot, 'runtime-pack.manifest.json'))
  } catch (error) {
    errors.push(`missing or invalid runtime-pack.manifest.json: ${error.message}`)
  }

  if (packManifest) {
    expectManifestField(errors, packManifest, 'packVersion', PACK_VERSION)
    expectManifestField(errors, packManifest, 'nodeVersion', NODE_VERSION)
    expectManifestField(errors, packManifest, 'playwrightVersion', PLAYWRIGHT_VERSION)
    expectManifestField(errors, packManifest, 'playwrightMcpVersion', PLAYWRIGHT_MCP_VERSION)
    expectManifestField(errors, packManifest, 'workerVersion', WORKER_VERSION)
    expectManifestField(errors, packManifest, 'chromiumRevision', CHROMIUM_REVISION)
  }

  const nodePath = path.join(absoluteRoot, 'node/bin/node')
  const chromiumPath = path.join(
    absoluteRoot,
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`,
  )

  if (await exists(nodePath)) {
    if (!(await isExecutable(nodePath))) {
      errors.push('node/bin/node is not executable')
    }
  }
  if (await exists(chromiumPath)) {
    if (!(await isExecutable(chromiumPath))) {
      errors.push(`chromium binary is not executable: ${path.relative(absoluteRoot, chromiumPath)}`)
    }
  }

  if (runtimeChecks && (await exists(nodePath))) {
    const nodeVersion = run(nodePath, ['--version'], { cwd: absoluteRoot })
    if (nodeVersion.status !== 0) {
      errors.push(`node --version failed: ${nodeVersion.stderr || nodeVersion.stdout}`)
    } else if (nodeVersion.stdout.trim() !== `v${NODE_VERSION}`) {
      errors.push(`node --version mismatch: expected v${NODE_VERSION}, got ${nodeVersion.stdout.trim()}`)
    }

    const playwrightRequire = run(
      nodePath,
      ['-e', "require('playwright'); console.log(require('playwright/package.json').version)"],
      { cwd: absoluteRoot },
    )
    if (playwrightRequire.status !== 0) {
      errors.push(`require('playwright') failed: ${playwrightRequire.stderr || playwrightRequire.stdout}`)
    } else if (playwrightRequire.stdout.trim() !== PLAYWRIGHT_VERSION) {
      errors.push(`playwright runtime mismatch: expected ${PLAYWRIGHT_VERSION}, got ${playwrightRequire.stdout.trim()}`)
    }

    const mcpRequire = run(
      nodePath,
      ['-e', "console.log(require('@playwright/mcp/package.json').version)"],
      { cwd: absoluteRoot },
    )
    if (mcpRequire.status !== 0) {
      errors.push(`require('@playwright/mcp/package.json') failed: ${mcpRequire.stderr || mcpRequire.stdout}`)
    } else if (mcpRequire.stdout.trim() !== PLAYWRIGHT_MCP_VERSION) {
      errors.push(`@playwright/mcp runtime mismatch: expected ${PLAYWRIGHT_MCP_VERSION}, got ${mcpRequire.stdout.trim()}`)
    }
  }

  return { ok: errors.length === 0, errors }
}

async function findDanglingSymlinks(root) {
  const results = []
  async function visit(current) {
    let entries = []
    try {
      entries = await fs.readdir(current, { withFileTypes: true })
    } catch {
      return
    }

    for (const entry of entries) {
      const target = path.join(current, entry.name)
      if (entry.isSymbolicLink()) {
        try {
          await fs.access(target)
        } catch {
          results.push(path.relative(root, target))
        }
      } else if (entry.isDirectory()) {
        await visit(target)
      }
    }
  }

  await visit(root)
  return results.sort()
}

async function main() {
  const root = process.argv[2]
  if (!root) {
    console.error('Usage: validate-runtime-pack.mjs <browser-runtime-pack-v1-dir>')
    process.exit(2)
  }
  const result = await validateRuntimePack(root)
  if (result.ok) {
    console.log('Runtime pack valid')
    return
  }
  for (const error of result.errors) console.error(`- ${error}`)
  process.exit(1)
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error.stack || error.message)
    process.exit(1)
  })
}
