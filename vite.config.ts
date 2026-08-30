import { defineConfig } from 'vite';

// The site is served from the root of https://www.openpower.tools, so the
// default base of '/' is correct. The wasm-bindgen glue in src/wasm-pkg refers
// to its .wasm file via `new URL('...', import.meta.url)`, which Vite rewrites
// into a hashed asset at build time; no wasm plugin is needed.
export default defineConfig({
  base: '/',
  build: {
    outDir: 'dist',
  },
});
