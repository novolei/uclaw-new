#!/usr/bin/env node
import fs from 'node:fs/promises'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import {
  DEFAULT_OUTPUT_DIR,
  DEFAULT_WORKER_SOURCE,
  CHROMIUM_REVISION,
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
    await writeTestFixtureRuntime(outputDir)
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
  assertMacArm64()
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
  await installPackagesAndChromium(outputDir, nodeBin, npmCli, npmPrefix)
  await fs.rm(buildDir, { recursive: true, force: true })
  await fs.rm(npmPrefix, { recursive: true, force: true })
}

async function copyLocalToolchain(outputDir) {
  assertMacArm64()
  const node = spawnSync('node', ['--version'], { encoding: 'utf8' })
  if (node.status !== 0) {
    throw new Error('local node is not available')
  }
  if (node.stdout.trim() !== `v${NODE_VERSION}`) {
    throw new Error(`local node must be v${NODE_VERSION}, got ${node.stdout.trim()}`)
  }
  const nodeRoot = path.join(outputDir, 'node')
  const npmPrefix = path.join(outputDir, 'npm-work')
  await fs.mkdir(path.join(nodeRoot, 'bin'), { recursive: true })
  await fs.copyFile(process.execPath, path.join(nodeRoot, 'bin/node'))
  await fs.chmod(path.join(nodeRoot, 'bin/node'), 0o755)
  await fs.mkdir(npmPrefix, { recursive: true })
  await installPackagesAndChromium(outputDir, path.join(nodeRoot, 'bin/node'), 'npm', npmPrefix)
  await fs.rm(npmPrefix, { recursive: true, force: true })
}

async function installPackagesAndChromium(outputDir, nodeBin, npmEntry, npmPrefix) {
  runNodeOrCommand(nodeBin, npmEntry, ['init', '-y'], { cwd: npmPrefix })
  runNodeOrCommand(nodeBin, npmEntry, [
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
}

async function writeTestFixtureRuntime(outputDir) {
  await fs.mkdir(path.join(outputDir, 'node/bin'), { recursive: true })
  await fs.writeFile(path.join(outputDir, 'node/bin/node'), '#!/bin/sh\necho v22.16.0\n')
  await fs.chmod(path.join(outputDir, 'node/bin/node'), 0o755)
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
  const chromiumDir = path.join(
    outputDir,
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS`,
  )
  await fs.mkdir(chromiumDir, { recursive: true })
  const chromiumPath = path.join(chromiumDir, 'Chromium')
  await fs.writeFile(chromiumPath, '')
  await fs.chmod(chromiumPath, 0o755)
}

async function downloadFile(url, targetPath) {
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`download failed ${response.status}: ${url}`)
  }
  await fs.writeFile(targetPath, Buffer.from(await response.arrayBuffer()))
}

function runNodeOrCommand(nodeBin, entry, args, options = {}) {
  if (entry.endsWith('.js')) {
    run(nodeBin, [entry, ...args], options)
  } else {
    run(entry, args, options)
  }
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

function assertMacArm64() {
  if (process.platform !== 'darwin' || process.arch !== 'arm64') {
    throw new Error('browser-runtime-pack-v1 generation supports macOS arm64 only')
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
    console.error(error.stack || error.message)
    process.exit(1)
  })
}
