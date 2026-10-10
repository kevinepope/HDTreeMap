// Browser bootstrap for the HDTreeMap demo.
//
// The Rust wasm module (hdtreemap-wasm, on agg-gui-web-shell) owns everything the moment it
// loads: canvas sizing, the frame loop, pointer / wheel / keyboard input and DPR tracking.
// This file only loads the module and reports a boot failure.
//
// Must NOT render visible UI beyond the canvas: every button, label and banner is painted by
// agg-gui inside the canvas (CLAUDE.md).

// Build stamp injected by vite (vite.config.ts `define`), appended to the pkg/ asset URLs so a
// new deploy always busts the browser cache.
declare const __BUILD_ID__: string;

// wasm-pack --no-typescript emits no .d.ts files; the module is referenced structurally.
type WasmModule = {
  default: (url?: string | URL | { module_or_path: string | URL }) => Promise<unknown>;
};

const canvas = document.getElementById("hdtreemap-canvas") as HTMLCanvasElement;

function showBootError(err: unknown): void {
  console.error("hdtreemap: failed to boot wasm app", err);
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return;
  }
  canvas.width = Math.max(1, canvas.clientWidth || window.innerWidth);
  canvas.height = Math.max(1, canvas.clientHeight || window.innerHeight);
  ctx.fillStyle = "#1b1b1b";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#f2f2f2";
  ctx.font = "20px sans-serif";
  ctx.fillText("HDTreeMap failed to load.", 24, 48);
  ctx.font = "14px sans-serif";
  ctx.fillText("Check that the wasm-pack output exists at demo/public/pkg.", 24, 78);
  ctx.fillText(String((err as Error)?.message ?? err ?? ""), 24, 102);
}

async function boot(): Promise<void> {
  // Resolve against `document.baseURI` so the URL is right wherever Vite puts the bundle and
  // under the `/HDTreeMap/` Pages sub-path.
  const v = `?v=${__BUILD_ID__}`;
  const url = new URL(`pkg/hdtreemap_wasm.js${v}`, document.baseURI).href;
  const mod = (await import(/* @vite-ignore */ url)) as WasmModule;
  const wasmUrl = new URL(`pkg/hdtreemap_wasm_bg.wasm${v}`, document.baseURI).href;
  // Module init runs the Rust `#[wasm_bindgen(start)]`, which boots the whole shell.
  await mod.default({ module_or_path: wasmUrl });
}

void boot().catch(showBootError);
