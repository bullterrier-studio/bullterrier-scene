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

// an editor: the layer under a click, and a tree edit as an edited copy of the document's layers
const hit = scene.layerAt(px, py, { size: 800 });          // "headline", "g.scene.logo" or null
const layers = JSON.parse(scene.toJson()).layers;
const edited = scene.withLayers(layers.filter((layer) => layer.name !== hit), { files });
scene.free();
scene = edited;                                            // same cache: the layers left are not drawn again

// Node: the package `poe wasm-test` builds (CommonJS), no init
const { Scene } = require("./pkg/bullterrier_scene_wasm.js");
```

| | |
|---|---|
| `Scene.fromJson(text, options?)` | A scene from the JSON `toJson()` or Python's `Scene.to_json()` writes. `options.files`: an object of path to `Uint8Array`. `options.cacheBytes`: the most bytes the scene's cache keeps ("Memory"). `FormatError` (with `pointer`) for a document that is not a scene, and for a `make` layer (a Python plugin) naming its kind; `ReadError` (with `path`) for a file the document names that `files` lacks; `TypeError` naming `text` or `options` for another type. |
| `scene.width`, `scene.height` | The canvas, in pixels. |
| `scene.parameters()` | Every value that can be set: `bt-scene parameters --json`'s keys (`address`, `kind`, `meaning`, `default`, `unit`, `min`, `max`, `cyclic`, plus `choices`, `min_length`, `nullable` where they apply) and `value`, what the scene holds now (a picture's: its path, or `null` for a hole). `unit` is `null` for a kind that is not a number. |
| `scene.withValues({ address: value })` | A new scene with those set, all or nothing; the scene itself is left as it is. A value is a number, a boolean, a text or `[r, g, b]` (three numbers). `InvalidError` (with `address`) for an address that is not there or a value its field refuses; `TypeError` naming `values.<address>` for another type. No file values: to swap a photo, set the layer's `picture` to the new path in its document layer and `withLayers` it with the new file in `files` (the rest stays cached). |
| `scene.render(options?)` | `{ width, height, data }`: `data` is a `Uint8ClampedArray` the page owns, straight alpha, for `new ImageData(data, width, height)`. `options.size`: a whole number from 1 to 65,000 (`InvalidError` with `address: "size"` outside it; `TypeError` naming `size` for another type). |
| `scene.layerAt(x, y, options?)` | The address of the topmost layer that shows at the pixel (`x`, `y`) of `render(options)` (column, row from the top left; `options.size` as `render`'s), or `null`: the start of its `parameters()` addresses (`"headline"`, `"layer3"`, `"g.scene.logo"` inside the group `g`). A layer shows where, drawn alone, it leaves alpha an 8-bit picture keeps; a blend mode changes its colour, not where it is; opacity 0 or masked away is not there; a mask layer is never the answer, a reference to a group is the answer itself. `InvalidError` (with `address` `x` or `y`) outside the picture; `TypeError` naming `x` or `y` for a value that is no whole number. Cost: a row of the picture for each layer tried, top down. |
| `scene.withLayers(layers, options?)` | A new scene of this canvas with `layers` as its stack, bottom first, **sharing this scene's cache** (a layer that did not change is not drawn again; `fromJson` again would draw everything). `layers` are layers as the document writes them (`JSON.parse(scene.toJson()).layers`, edited: spliced to reorder, add or remove, moved into a `{ name, scene: { version, canvas, layers } }` layer to regroup); `options.files` as `fromJson`'s, for every file they name. A named layer keeps its addresses wherever it goes. `FormatError` (with `pointer` from `/layers/<index>`), `ReadError`, `InvalidError` as `fromJson`; `TypeError` naming `layers` unless it is an array. |
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

Scenes made by `withValues` share one cache of what each layer drew, at most `cacheBytes` bytes (default 256 MiB; `0` keeps
nothing; the native package's is 2 GiB). Wasm memory never shrinks, and `free()` releases the scene, not what it cached, so the
module's memory is the cache's bound plus what one render holds while it draws; it stops growing there. A smaller bound
redraws more: what does not fit is drawn again (a picture decoded again, a look run again) on the next render that needs it.

Measured in headless Chrome 157 (macOS arm64) on a 6-layer poster (a 5 megapixel photo and its matte, a three-ink halftone) at
640 x 800, 40 distinct values of the halftone's `cell`, then 10 values seen before; wasm memory (`memory.buffer.byteLength`):

| `cacheBytes` | after the first render | after 40 values and 10 repeats | a new value | a value seen before |
|---|---|---|---|---|
| 0 | 258 MiB | 258 MiB | 2.2 s | 1.5 s |
| 128 MiB | 354 MiB | 354 MiB | 1.9 s | 23 ms for the last 2, else 1.2 s |
| 256 MiB (default) | 382 MiB | 456 MiB | 1.85 s | 23 ms for the last 5, else 1.2 to 5 s |
| 512 MiB | 382 MiB | 730 MiB | 1.85 s | 23 ms for all 10 |
| 2 GiB | 382 MiB | 1,532 MiB, still growing | 1.9 s | 3 ms |

The memory stops at the bound plus 258 MiB here (what a render of this poster holds while it draws, the decoded photo and matte
among it; it grows with the render's size and the pictures' size). A phone's tab limit is not measured yet.

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
