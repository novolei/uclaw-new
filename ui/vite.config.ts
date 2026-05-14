import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    fs: {
      // node_modules is a symlink to the parent project; Vite resolves symlinks
      // before checking the allow-list, so the real path falls outside the
      // worktree root and fonts/assets get blocked. Allow the symlink target.
      allow: ['.', path.resolve(__dirname, 'node_modules')],
    },
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          // PDF and office parsers — bundle separately to reduce initial bundle size
          if (id.includes('pdfjs-dist/build/pdf.worker')) return 'pdfjs-worker'
          // W4d editor stack — lazy chunk so read-only sessions pay zero
          if (
            id.includes('node_modules/@codemirror/') ||
            id.includes('node_modules/@tiptap/') ||
            id.includes('node_modules/lowlight/') ||
            id.includes('node_modules/prosemirror-')
          ) return 'editors'
          if (
            id.includes('node_modules/jszip') ||
            id.includes('node_modules/@xmldom') ||
            id.includes('node_modules/mammoth')
          ) return 'office-parsers'
          // VS-Code-style file icons — ~620 KB pre-tree-shake. Splitting it
          // out keeps the main bundle thin; the files-rail mount loads it
          // synchronously the first time a tree row is rendered.
          if (id.includes('node_modules/@react-symbols/icons')) return 'file-icons'
          // Existing vendor splits — preserve
          if (id.includes('node_modules/react/') || id.includes('node_modules/react-dom/')) {
            return 'react'
          }
          if (id.includes('node_modules/@tauri-apps/')) {
            return 'tauri'
          }
          if (
            id.includes('node_modules/jotai') ||
            id.includes('node_modules/clsx') ||
            id.includes('node_modules/tailwind-merge')
          ) {
            return 'vendor'
          }
          // Lottie runtime — only the Kaleidoscope entry icon uses it.
          // Own chunk so the ~50KB gzip cost is isolated from the main bundle.
          if (id.includes('node_modules/lottie-react') || id.includes('node_modules/lottie-web')) {
            return 'lottie'
          }
          // Shiki: keep ONLY the core engine in a shared chunk; let Vite's natural
          // code-splitting handle each language/theme as its own dynamic-import chunk
          // (otherwise we force-bundle ~10 MB of shiki langs into one file and defeat
          // shiki's own lazy loading.)
          if (
            id.includes('node_modules/shiki/dist/core') ||
            id.includes('node_modules/shiki/dist/index') ||
            id.includes('node_modules/@shikijs/core') ||
            id.includes('node_modules/@shikijs/engine-')
          ) {
            return 'shiki-core'
          }
          // NEW: route-level splits — heaviest views become their own async chunks
          if (id.includes('/components/settings/')) return 'view-settings'
          if (id.includes('/components/memory/')) return 'view-memory'
          if (id.includes('/components/automation/')) return 'view-automation'
          if (id.includes('/components/agent/')) return 'view-agent'
          return undefined
        },
      },
    },
  },
});
