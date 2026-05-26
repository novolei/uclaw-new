import { mkdir, writeFile, access } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { stdin, stdout } from 'node:process';

const SCHEMA_VERSION = 1;
const PROVIDER_ID = 'browser.playwright_cli';

async function main() {
  if (process.argv.includes('--health-check')) {
    writeEnvelope({
      type: 'uclaw.playwright.worker.ready',
      schemaVersion: SCHEMA_VERSION,
      providerId: PROVIDER_ID,
    });
    return;
  }

  const request = JSON.parse(await readStdin());
  validateRequest(request);

  let browser;
  try {
    const { chromium } = await loadPlaywright();
    browser = await chromium.launch({ headless: true });
    const hasState = request.sessionStatePath && await fileExists(request.sessionStatePath);
    const contextOptions = hasState ? { storageState: request.sessionStatePath } : {};
    const context = await browser.newContext(contextOptions);
    const page = await context.newPage();
    page.setDefaultTimeout?.(request.timeoutMs);

    const result = await runAction(page, request);
    if (request.sessionStatePath) {
      const dir = dirname(request.sessionStatePath);
      await mkdir(dir, { recursive: true });
      await context.storageState({ path: request.sessionStatePath });
    }
    writeEnvelope({
      schemaVersion: SCHEMA_VERSION,
      providerId: PROVIDER_ID,
      requestId: request.requestId,
      status: 'succeeded',
      summary: result.summary,
      artifactRefs: result.artifactRefs ?? [],
      output: result.output ?? null,
    });
  } catch (error) {
    writeEnvelope(failureEnvelope(request, error));
  } finally {
    await browser?.close?.().catch(() => {});
  }
}

function readStdin() {
  return new Promise((resolve, reject) => {
    let data = '';
    stdin.setEncoding('utf8');
    stdin.on('data', (chunk) => {
      data += chunk;
    });
    stdin.on('end', () => resolve(data));
    stdin.on('error', reject);
  });
}

async function loadPlaywright() {
  const mod = await import('playwright');
  const chromium = mod.chromium ?? mod.default?.chromium;
  if (!chromium) {
    throw codeError('playwright_unavailable', 'Playwright chromium export is unavailable.', false);
  }
  return { chromium };
}

function validateRequest(request) {
  if (request?.schemaVersion !== SCHEMA_VERSION) {
    throw codeError('invalid_schema', 'Unsupported Playwright CLI envelope schema.', false);
  }
  if (request.providerId !== PROVIDER_ID) {
    throw codeError('invalid_provider', 'Unsupported Playwright CLI provider id.', false);
  }
  if (!request.requestId || !request.action?.kind) {
    throw codeError('invalid_request', 'Playwright CLI request is missing requestId or action.', false);
  }
}

async function runAction(page, request) {
  const timeout = request.timeoutMs ?? request.timeout_ms;
  const { action } = request;
  const res = await (async () => {
    switch (action.kind) {
      case 'navigate':
        await page.goto(action.url, { timeout });
        return {
          summary: `navigated to ${action.url}`,
          output: { url: page.url?.() ?? action.url },
        };
      case 'click': {
        const target = resolveTarget(page, action.target);
        const stateDiff = await observeActionStateDiff(page, async () => {
          if (target.kind === 'coordinates') {
            await page.mouse.click(action.target.x, action.target.y);
          } else {
            await target.locator.click({ timeout });
          }
        });
        return {
          summary: `clicked ${action.target.kind}`,
          output: { addressingKind: action.target.kind, stateDiff },
        };
      }
      case 'type': {
        const target = resolveTarget(page, action.target);
        const stateDiff = await observeActionStateDiff(page, async () => {
          if (target.kind === 'coordinates') {
            await page.mouse.click(action.target.x, action.target.y);
            await page.keyboard.type(action.text);
          } else {
            await target.locator.fill(action.text, { timeout });
          }
        });
        return {
          summary: `typed ${action.text.length} characters into ${action.target.kind}`,
          output: { addressingKind: action.target.kind, textLength: action.text.length, stateDiff },
        };
      }
      case 'screenshot': {
        const fullPage = Boolean(action.fullPage ?? action.full_page);
        const bytes = await page.screenshot({ fullPage });
        const artifactRefs = await maybeWriteScreenshotArtifact(request.requestId, bytes);
        return {
          summary: `captured screenshot (${bytes.length} bytes)`,
          artifactRefs,
          output: { fullPage, bytes: bytes.length },
        };
      }
      case 'extract': {
        const text = action.target
          ? await resolveTarget(page, action.target).locator.textContent({ timeout })
          : await page.textContent('body', { timeout });
        return {
          summary: 'extracted text',
          output: { text: text ?? '' },
        };
      }
      case 'wait': {
        const target = resolveTarget(page, action.target);
        const waitTimeout = action.timeoutMs ?? action.timeout_ms ?? timeout;
        const stateDiff = await observeActionStateDiff(page, async () => {
          if (target.kind === 'coordinates') {
            await page.waitForTimeout?.(waitTimeout);
          } else {
            await target.locator.waitFor({ state: 'visible', timeout: waitTimeout });
          }
        });
        return {
          summary: `waited for ${action.target.kind}`,
          output: { addressingKind: action.target.kind, timeoutMs: waitTimeout, stateDiff },
        };
      }
      default:
        throw codeError('unsupported_action', `Unsupported Playwright CLI action: ${action.kind}`, false);
    }
  })();

  if (!res.output) {
    res.output = {};
  }
  res.output.url = await safeRead(() => page.url?.()) ?? null;
  res.output.title = await safeRead(() => page.title?.()) ?? null;
  return res;
}

async function fileExists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

function resolveTarget(page, target) {
  switch (target?.kind) {
    case 'semantic_locator':
      return { kind: 'locator', locator: semanticLocator(page, target.locator) };
    case 'uclaw_dom_element_id':
      return {
        kind: 'locator',
        locator: page.locator(domElementSelector(target.elementId ?? target.element_id)),
      };
    case 'coordinates':
      return { kind: 'coordinates' };
    default:
      throw codeError('invalid_target', 'Unsupported Playwright CLI target.', false);
  }
}

function semanticLocator(page, locator) {
  if (locator.startsWith('text=')) return page.getByText(locator.slice(5));
  if (locator.startsWith('label=')) return page.getByLabel(locator.slice(6));
  if (locator.startsWith('testid=')) return page.getByTestId(locator.slice(7));
  const roleMatch = locator.match(/^role=([^[]+)(?:\\[name=(.*)\\])?$/);
  if (roleMatch) {
    const [, role, name] = roleMatch;
    return page.getByRole(role, name ? { name } : undefined);
  }
  return page.locator(locator);
}

function domElementSelector(elementId) {
  const value = cssString(elementId);
  return `[data-uclaw-id="${value}"], [data-uclaw-element-id="${value}"], [data-uclaw-index="${value}"]`;
}

function cssString(value) {
  return String(value).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

async function observeActionStateDiff(page, action) {
  const before = await captureStateSnapshot(page);
  await action();
  const after = await captureStateSnapshot(page);
  return diffStateSnapshots(before, after);
}

async function captureStateSnapshot(page) {
  const url = normalizeNullable(await safeRead(() => page.url?.()));
  const title = normalizeNullable(await safeRead(() => page.title?.()));
  const bodyText = normalizeNullable(await safeRead(() => page.textContent?.('body', { timeout: 250 })));
  const activeElement = normalizeNullable(await safeRead(() => page.evaluate?.(() => {
    const element = globalThis.document?.activeElement;
    if (!element) return null;
    return {
      tagName: element.tagName ?? null,
      id: element.id ?? null,
      name: element.getAttribute?.('name') ?? null,
      role: element.getAttribute?.('role') ?? null,
    };
  })));

  return {
    url,
    title,
    bodyTextHash: bodyText === null ? null : hashString(bodyText),
    bodyTextLength: bodyText === null ? null : bodyText.length,
    activeElement,
  };
}

function diffStateSnapshots(before, after) {
  const changedFields = Object.keys(after).filter((key) => JSON.stringify(before[key]) !== JSON.stringify(after[key]));
  return {
    observed: true,
    before,
    after,
    changedFields,
  };
}

async function safeRead(reader) {
  try {
    const value = reader();
    return value && typeof value.then === 'function' ? await value : value;
  } catch {
    return null;
  }
}

function normalizeNullable(value) {
  return value === undefined ? null : value;
}

function hashString(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

async function maybeWriteScreenshotArtifact(requestId, bytes) {
  const dir = process.env.UCLAW_BROWSER_ARTIFACT_DIR;
  if (!dir) return [];
  await mkdir(dir, { recursive: true });
  const fileName = `${safeFileName(requestId)}-screenshot.png`;
  const filePath = join(dir, fileName);
  await writeFile(filePath, bytes);
  return [`file://${filePath}`];
}

function safeFileName(value) {
  return String(value).replace(/[^a-zA-Z0-9_.-]/g, '_');
}

function failureEnvelope(request, error) {
  const classified = classifyError(error);
  return {
    schemaVersion: SCHEMA_VERSION,
    providerId: PROVIDER_ID,
    requestId: request?.requestId ?? 'unknown',
    status: 'failed',
    summary: classified.message,
    artifactRefs: [],
    output: null,
    error: classified,
  };
}

function classifyError(error) {
  if (error?.code) {
    return {
      code: error.code,
      message: error.message ?? String(error),
      retryable: Boolean(error.retryable),
    };
  }
  const message = error?.message ?? String(error);
  const timeout = /timeout/i.test(message);
  return {
    code: timeout ? 'timeout' : 'action_failed',
    message,
    retryable: timeout,
  };
}

function codeError(code, message, retryable) {
  const error = new Error(message);
  error.code = code;
  error.retryable = retryable;
  return error;
}

function writeEnvelope(envelope) {
  stdout.write(`${JSON.stringify(envelope)}\n`);
}

main().catch((error) => {
  writeEnvelope(failureEnvelope({ requestId: 'unknown' }, error));
  process.exitCode = 1;
});
