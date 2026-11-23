# JavaScript API

The core built for WebAssembly (`crates/wasm`), for a browser page or Node: a saved scene (JSON) read from files the page holds,
rendered to 8-bit RGBA pixels that look the same as the native render at the same size (the one tolerance, `docs/api.md`,
"Looks the same"). CPU only; no GPU in the browser package yet. Sizes, speed and reasons: [design decisions](design-decisions.md),
"A WebAssembly package, CPU". The names are the Python package's (`InvalidError`, `FormatError`, `ReadError`, `parameters()`,
`withValues()`).

## Use

```js
// a browser page or a Web Worker: the web build (below)
import init, { Scene } from "./web-pkg/bullterrier_scene_wasm.js";
await init();                                              // loads the .wasm

const files = {                                            // bytes by the path exactly as the document writes it
  "photo.png": new Uint8Array(await (await fetch("photo.png")).arrayBuffer()),
};
let scene = Scene.fromJson(sceneText, { files });
const out = scene.render({ size: 800 });                   // long side in px; default: the canvas's
ctx.putImageData(new ImageData(out.data, out.width, out.height), 0, 0);

// a slider: a new scene per move, the previous one freed
const next = scene.withValues({ "photo.look.grade.exposure": 0.4 });
scene.free();
scene = next;

// Node: the package `poe wasm-test` builds (CommonJS), no init
const { Scene } = require("./pkg/bullterrier_scene_wasm.js");
```

| | |
|---|---|
| `Scene.fromJson(text, options?)` | A scene from the JSON `toJson()` or Python's `Scene.to_json()` writes. `options.files`: an object of path to `Uint8Array`. `FormatError` (with `pointer`) for a document that is not a scene, and for a `make` layer (a Python plugin) naming its kind; `ReadError` (with `path`) for a file the document names that `files` lacks; `TypeError` naming `text` or `options` for another type. |
| `scene.width`, `scene.height` | The canvas, in pixels. |
| `scene.parameters()` | Every value that can be set: `bt-scene parameters --json`'s keys (`address`, `kind`, `meaning`, `default`, `unit`, `min`, `max`, `cyclic`, plus `choices`, `min_length`, `nullable` where they apply) and `value`, what the scene holds now (a picture's: its path, or `null` for a hole). `unit` is `null` for a kind that is not a number. |
| `scene.withValues({ address: value })` | A new scene with those set, all or nothing; the scene itself is left as it is. A value is a number, a boolean, a text or `[r, g, b]` (three numbers). `InvalidError` (with `address`) for an address that is not there or a value its field refuses; `TypeError` naming `values.<address>` for another type. No file values: to swap a photo, write a new document and `fromJson` it. |
| `scene.render(options?)` | `{ width, height, data }`: `data` is a `Uint8ClampedArray` the page owns, straight alpha, for `new ImageData(data, width, height)`. `options.size`: a whole number from 1 to 65,000 (`InvalidError` with `address: "size"` outside it; `TypeError` naming `size` for another type). |
| `scene.toJson()` | The scene as JSON text. |
| `scene.free()` | Releases the scene's memory in the module. |

Every error is a JavaScript `Error` whose `name` is the Python exception's (`InvalidError`, `FormatError`, `ReadError`) and whose
message is the core's; one with no Python class is a `BullterrierSceneError`. The types are in the generated `.d.ts`
(`SceneOptions`, `RenderOptions`, `Rendered`, `SceneParameter`).

## Files

The browser has no folder and no network access to the package. A file the document names (a picture, a map, a font, a look
file, a drawing file) comes from `files`, by the path exactly as the document writes it, `maps/matte.png` and all. A look file is TOML, as on disk.

- A file the document names that `files` lacks is a `ReadError` naming that path, at `fromJson`.
- A file whose bytes are not the ones the scene was saved with (a digest mismatch) is a `ReadError` at render.
- A `make` layer (a Python plugin) is a `FormatError` naming the kind.

## Memory

Every `Scene` a call returns (`fromJson`, `withValues`) holds wasm memory until `free()`. In a slider loop, free the previous
scene after each `withValues`. A render's `data` is a copy the page owns: nothing to free.

## Build

`uv run poe wasm-test` builds the package for Node (`--target nodejs`, into the ignored `pkg/`) and tests it. For a page or a Web
Worker, build the same `.wasm` for the web, which gives the ES module the page imports (`wasm-bindgen` is the version pinned in
`Cargo.toml`):

```sh
RUSTFLAGS="-C target-feature=+simd128" cargo build -p bullterrier-scene-wasm --target wasm32-unknown-unknown --release --locked
wasm-bindgen --target web --out-dir web-pkg target/wasm32-unknown-unknown/release/bullterrier_scene_wasm.wasm
wasm-opt -Oz --all-features -o web-pkg/bullterrier_scene_wasm_bg.wasm web-pkg/bullterrier_scene_wasm_bg.wasm   # optional: 15.0 MB to 8.8 MB
```

A render runs on the calling thread and blocks it: run it in a Web Worker to keep a page responsive.
