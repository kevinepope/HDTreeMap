// Vite config for the HDTreeMap web demo.
//
// GitHub Pages serves the demo at https://larsbrubaker.github.io/HDTreeMap/, so asset paths
// are relative (`./`), which works both there and under `vite dev`.
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  // Stamped into the bundle and appended to the wasm-pack asset URLs in main.ts. The pkg/
  // files keep stable (unhashed) names, so without this browsers keep serving a stale
  // cached wasm after a deploy.
  define: {
    __BUILD_ID__: JSON.stringify(Date.now().toString(36)),
  },
  server: { host: true },
  preview: { host: true },
});
