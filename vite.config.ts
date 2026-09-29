import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';

// PULSE runs inside a Tauri webview in production, but the dev server is a plain
// Vite server. Keep the config Tauri-aware but free of platform-specific logic.
const host = process.env.TAURI_DEV_HOST;

/**
 * PULSE's dedicated development ports.
 *
 * Deliberately NOT Tauri's default 1420: that default is shared by every
 * Tauri project, so two of them cannot run at once. Developers here work on
 * other Tauri/Vite projects in parallel, and PULSE must never require another
 * project to be shut down before it can start.
 *
 * `strictPort` is kept on because Tauri's `devUrl` is a fixed address — a
 * silent fallback to another port would just make the webview load nothing.
 * If these ports ever clash, change them here and in `src-tauri/tauri.conf.json`
 * (`build.devUrl`) together; they must always agree.
 */
const DEV_SERVER_PORT = 1421;
const HMR_PORT = 1422;

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  // Tauri expects a fixed port and fails if it is not available.
  clearScreen: false,
  server: {
    port: DEV_SERVER_PORT,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: HMR_PORT } : undefined,
    watch: {
      // The Rust side has its own watcher; never let Vite scan build artifacts.
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}', 'integrations/**/*.test.js'],
  },
});
