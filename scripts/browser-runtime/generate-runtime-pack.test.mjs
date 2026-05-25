import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { generateRuntimePack } from './generate-runtime-pack.mjs'
import { CHROMIUM_REVISION, DEFAULT_WORKER_SOURCE, PACK_VERSION } from './runtime-pack-constants.mjs'
import { validateRuntimePack } from './validate-runtime-pack.mjs'

test('generator writes expected layout in no-download fixture mode', async () => {
  const outputDir = await fs.mkdtemp(path.join(os.tmpdir(), 'uclaw-generated-runtime-pack-'))
  const result = await generateRuntimePack({
    outputDir,
    fromLocalToolchain: true,
    skipDownloadsForTest: true,
  })

  assert.equal(result.outputDir, outputDir)
  assert.equal(result.source, 'test_fixture')
  assert.equal(
    JSON.parse(await fs.readFile(path.join(outputDir, 'runtime-pack.manifest.json'), 'utf8'))
      .packVersion,
    PACK_VERSION,
  )
  assert.ok(await exists(path.join(outputDir, 'node/bin/node')))
  assert.ok(await exists(path.join(outputDir, 'node_modules/playwright/package.json')))
  assert.ok(await exists(path.join(outputDir, 'node_modules/@playwright/mcp/package.json')))
  assert.ok(await exists(path.join(outputDir, 'worker/uclaw-playwright-worker.mjs')))
  assert.equal(
    await fs.readFile(path.join(outputDir, 'worker/uclaw-playwright-worker.mjs'), 'utf8'),
    await fs.readFile(DEFAULT_WORKER_SOURCE, 'utf8'),
  )
  assert.ok(await exists(path.join(
    outputDir,
    `ms-playwright/chromium-${CHROMIUM_REVISION}/chrome-mac/Chromium.app/Contents/MacOS/Chromium`,
  )))

  const validation = await validateRuntimePack(outputDir, { runtimeChecks: false })
  assert.equal(validation.ok, true)
})

test('generator requires explicit local-toolchain mode for no-download tests', async () => {
  const outputDir = await fs.mkdtemp(path.join(os.tmpdir(), 'uclaw-generated-runtime-pack-'))

  await assert.rejects(
    () => generateRuntimePack({ outputDir, skipDownloadsForTest: true }),
    /skipDownloadsForTest is only allowed/,
  )
})

async function exists(target) {
  try {
    await fs.access(target)
    return true
  } catch {
    return false
  }
}
