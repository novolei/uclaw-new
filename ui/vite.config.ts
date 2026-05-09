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
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id: string) {
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
          // NEW: Shiki + its languages/themes — biggest single-source contributor
          if (id.includes('node_modules/shiki') || id.includes('node_modules/@shikijs')) {
            return 'shiki'
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
