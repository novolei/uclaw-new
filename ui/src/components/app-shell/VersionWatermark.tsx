import * as React from 'react'

/**
 * Bottom-right version watermark — shows the app version + the git commit
 * the frontend bundle was built from, so it's obvious at a glance which
 * build is running.
 *
 * `pointer-events-none` is mandatory: this is a top-layer overlay and must
 * never intercept a click. Values are injected at build time by
 * `vite.config.ts` via `define` (`__APP_VERSION__` / `__APP_COMMIT__`).
 */
export function VersionWatermark(): React.ReactElement {
  return (
    <div
      aria-hidden="true"
      className="fixed bottom-6 right-6 z-[9999] pointer-events-none select-none
                 font-mono text-[10px] leading-none text-muted-foreground/40"
    >
      v{__APP_VERSION__} · {__APP_COMMIT__}
    </div>
  )
}
