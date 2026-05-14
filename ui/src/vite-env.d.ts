/// <reference types="vite/client" />

// Injected at build time by vite.config.ts (`define`) — see VersionWatermark.tsx.
declare const __APP_VERSION__: string
declare const __APP_COMMIT__: string

interface Window {
  __pendingAgentFileData?: Map<string, string>
  __pendingAttachmentData?: Map<string, string>
}
