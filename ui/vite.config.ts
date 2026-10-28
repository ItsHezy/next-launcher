import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// WebView2 is evergreen Chromium: ship modern JS, no polyfills.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: {
    target: 'es2022',
    modulePreload: { polyfill: false },
    cssCodeSplit: true,
    sourcemap: false,
    reportCompressedSize: true,
    assetsInlineLimit: 0,
  },
});
