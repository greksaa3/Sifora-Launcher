import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

// Tauri expects a fixed port and does not want the renderer to obscure Rust errors.
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    // Chromium (WebView2) baseline; keeps the bundle small and startup fast.
    target: ['es2021', 'chrome105'],
    minify: 'esbuild',
    cssMinify: true,
    sourcemap: false,
    reportCompressedSize: false,
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        // Keep the entry lean; vendor code is stable across launches.
        manualChunks(id) {
          if (id.includes('node_modules')) return 'vendor'
          return undefined
        },
      },
    },
  },
})