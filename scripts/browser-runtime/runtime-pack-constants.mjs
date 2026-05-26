import { fileURLToPath } from 'node:url'
import path from 'node:path'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')

export const PACK_VERSION = 'browser-runtime-pack-v1'
export const NODE_VERSION = '22.16.0'
export const PLAYWRIGHT_VERSION = '1.53.0'
export const PLAYWRIGHT_MCP_VERSION = '0.0.75'
export const WORKER_VERSION = '0.1.0'
export const CHROMIUM_REVISION = '1178'
export const DEFAULT_OUTPUT_DIR = path.join(
  repoRoot,
  'src-tauri/.runtime-pack-staging',
  PACK_VERSION,
)
export const DEFAULT_WORKER_SOURCE = path.join(
  repoRoot,
  'src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs',
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
