import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { validateRuntimePack } from './validate-runtime-pack.mjs'
import { CHROMIUM_REVISION, manifest } from './runtime-pack-constants.mjs'

async function makePackFixture({ missing = [], manifestOverrides = {} } = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'uclaw-runtime-pack-'))
  const write = async (relative, contents = '') => {
    if (missing.includes(relative)) return
    const target = path.join(root, relative)
    await fs.mkdir(path.dirname(target), { recursive: true })
    await fs.writeFile(target, contents)
  }
  await write(
    'runtime-pack.manifest.json',
    JSON.stringify({ ...manifest(), ...manifestOverrides }, null, 2),
  )
  await write('node/bin/node', '#!/bin/sh\necho v22.16.0\n')
  if (!missing.includes('node/bin/node')) {
    await fs.chmod(path.join(root, 'node/bin/node'), 0o755)
  }
  await write('node_modules/playwright/package.json', JSON.stringify({ version: '1.53.0' }))
  await write('node_modules/@playwright/mcp/package.json', JSON.stringify({ version: '0.0.75' }))
  await write('worker/uclaw-playwright-worker.mjs', 'console.log("worker")\n')
  const chromiumRelative =
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`
  await write(chromiumRelative, 'chromium')
  if (!missing.includes(chromiumRelative)) {
    await fs.chmod(path.join(root, chromiumRelative), 0o755)
  }
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

test('validator reports missing manifest', async () => {
  const root = await makePackFixture({ missing: ['runtime-pack.manifest.json'] })
  const result = await validateRuntimePack(root, { runtimeChecks: false })

  assert.equal(result.ok, false)
  assert.ok(result.errors.some((error) => error.includes('runtime-pack.manifest.json')))
})

test('validator reports manifest version mismatch', async () => {
  const root = await makePackFixture({ manifestOverrides: { nodeVersion: '20.0.0' } })
  const result = await validateRuntimePack(root, { runtimeChecks: false })

  assert.equal(result.ok, false)
  assert.ok(result.errors.some((error) => error.includes('nodeVersion mismatch')))
})

test('validator reports dangling symlinks', async () => {
  const root = await makePackFixture()
  await fs.mkdir(path.join(root, 'node_modules/.bin'), { recursive: true })
  await fs.symlink(
    path.join(root, 'deleted-workdir/playwright/cli.js'),
    path.join(root, 'node_modules/.bin/playwright'),
  )

  const result = await validateRuntimePack(root, { runtimeChecks: false })

  assert.equal(result.ok, false)
  assert.ok(result.errors.some((error) => (
    error.includes('dangling symlink') && error.includes('node_modules/.bin/playwright')
  )))
})
