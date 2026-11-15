# CLAUDE.md: bullterrier-scene

A Rust library (crate `bullterrier-scene`) with a Python interface that puts layers (pictures, image looks, vector drawings) together
into one picture: placed, masked, blended, rendered at any size and in bands. **The code API is the product**; JSON is the same scene
written down. `README.md` (use), `docs/api.md` (the calls, with `docs/examples/`), `docs/design.md` (what and how), `docs/design-decisions.md` (why),
`docs/roadmap.md` (the batches), `docs/ecosystem.md` (the sibling packages), `docs/shared-files.md` (what stays identical across projects), `docs/releasing.md`.

**Write docs terse.** This file, the README, `docs/`, docstrings, reports and briefs: state the rule or fact, drop preamble,
repetition and justification a reader does not need to act. Reasons go in `docs/design-decisions.md`, once. Committed files are
public: no absolute paths, no references to folders outside the repository.

## Start here

```bash
uv sync && uv run poe setup          # once per clone: environment (builds the Rust extension: needs rustup), git hooks, commit template, cargo tools
uv run pytest tests/test_scene.py -n 0   # the tests near your change, while you work
cargo test -p bullterrier-scene --test it compose::  # one Rust area (inline: cargo test -p bullterrier-scene --lib raster::)
uv run poe look                      # render the example scenes to dist/look/ and the gallery pieces, and print the first path
uv run poe fast                      # the inner loop, as often as you like: format, lint, types, quick tests
uv run poe check                     # the gate, before you report or merge (then `uv run lint-imports`)
uv run poe full                      # `check` and the gate proofs: before a tag (`poe release` runs it) and in CI
```

| To ... | Look at |
|---|---|
| add logic | a module in `crates/core/src/<concept>.rs` (a layer source is `source/<kind>.rs`), reached through `Scene` or `Layer`; `render.rs` is the model |
| expose it to Python | a method of `Scene` or `Layer` in `crates/python/src/lib.rs`; its stub line in `src/bullterrier_scene/_native/_core.pyi`; the call in `src/bullterrier_scene/scene.py` or `layer.py`; a new name in `__init__.py` |
| add a command | a module `crates/cli/src/<command>.rs` (`Args`, `command`) and its variant in `crates/cli/src/lib.rs`; its tests in `crates/cli/tests/it/`; `src/bullterrier_scene/cli.py` only starts it |
| add an error | the core's `Error` (`error.rs`), mapped in `crates/python/src/error.rs`, a class in `errors.py` (`InvalidError`, `FormatError`, `ReadError` so far) |
| test it | "Testing" and "Rust tests" below; `crates/core/tests/it/` (Rust), `tests/` (Python) |
| add a macros crate | "Adding a macros crate" below |
| change a gate | `pyproject.toml` (Python tools, poe tasks), `Cargo.toml` (Rust lints, tool pins); proof in `tests/tools/` |
| keep files identical across projects | `docs/shared-files.md` |

**State:** C1, C2a, C2b, C3, C4a, C4b, C4c, C5a, C5b-lite, C5c (room for light layers), C6a, C6b, C7, C8, O1 (PNG, PDF) and P1 (`bt-scene render|parameters|validate`, `docs/cli.md`) done (`docs/roadmap.md`): a `Scene` of picture, drawing, fill and text layers with an image look on any layer, each
`fit` into a box (cover, contain, stretch) and `place`d (flip, scale, aspect, skew, rotate about an anchor, move) by one warp, opacity, a blend mode (the sixteen W3C modes) and `as_light`, rendered whole at
any size to a float32 array (Python) and a `Raster` (Rust). What is built and where it differs from the contract is "What is built" in
`docs/api.md`. A layer can also be put on a convex quadrilateral (`perspective`). A picture holds named maps and hands them to its looks. A layer is masked by a layer drawn on the canvas or by one of its own maps (`mask`: alpha or luminance, `feather`, `invert`; masks nest to `MAX_DEPTH`). A layer can refer to another layer of its scene (`Layer.of`: its body again, with its own join, looks, own-map masks and an extra `place`; resolved by `Scene::add`, which also counts the layers a render draws and how deep they lean on one another). A scene is a layer too (`Layer.scene`: a group, drawn isolated at its layer's density, fading and blending as one picture; scenes count in the one depth and layer budget, and nested pictures are bounded in sum). A look runs on what a layer's source drew at the drawn size, whatever the source (a group's finished picture included); its `px` are image's design pixels, 1/480 of the canvas's long side (`Options::with_canvas`: the canvas at the layer's `place.scale`; a group's inner layers use the group's density). **Parameters (C6a):** every value of a scene has an address `<layer>.<field>` (`parameters.rs` writes the grammar once; `Scene::parameters` lists them, in the parameter format's fields, `Scene::with_values` sets some and returns a new scene, all or nothing, with the siblings' own checks for looks, drawings and the scenes of groups; a picture or a map can be a hole filled by a value). **Saved scene (C6b):** `Scene::to_json` and `Scene::from_json` (`document.rs` owns the format; `files.rs` a file's path, folder and digest), `==` on scenes and layers is the same document, `FormatError` carries the JSON Pointer. **Output and command line (O1, P1):** `Raster::write(path)` writes a PNG through image's writer (the picture adapter, `source/picture.rs`); `crates/cli` is the command line, a library whose `main(args, version, out, err)` the bindings export as `_core.main` for the console script. `Raster::write_pdf(path, &PdfOptions)` writes a raster PDF at a physical size (`pdf.rs`: dpi, bleed by edge repeat, crop marks, flatten, a caller's CMYK ICC profile through moxcms; `bt-scene render out.pdf --dpi --bleed --crop-marks`). **Cache (C7):** a `Scene` holds one shared, bounded (2 GiB) cache (`cache.rs`) of what each layer drew before its warp, of group rasters, mask planes and decoded files, found by a key every source, look and layer writes (`keyed`); scenes made by `add` and `with_values` share it, `==` never sees it, and a layer that only moves is not drawn again (the wow loop 0.51 to 0.19 s a frame, a poster of 15 A3 layers re-rendered 5.0 to 0.39 s); a chain of two or more looks keeps the raster after each look but the last (`Finish::run`, key = everything below the looks + the looks up to the stage + the chain's room; expendable entries that never evict a finished raster), so one look changing runs the looks after it only. **Plugin seam (C7):** the public trait `LayerKind` and `Layer::make`. **Layer kinds by name (C8):** `LayerKinds` (a registry value) of `LayerKindFactory`s, `Scene::from_json_with`; a layer made through a registry is saved as `{"make": {"kind", "options"}}`; Python `Layer.make`, `register_layer`, `layer_kind`, `layer_kinds` and the entry-point group `bullterrier_scene.layers`, loaded lazily (`plugin.py`). **Room (C5c):** a light layer's looks run on its raster grown by image's `Recipe::spread` (`Look::room`, `finish.rs`), so a halo shows past the box; a layer that is not light has none; a look image cannot bound gets none. M1 to M3 are built as pieces (`docs/examples/`, `bullterrier-demos`) and the polish batch they found is done bar two open rows (`docs/future.md`). **Bands (B1):** `Scene::render_bands(size, band_rows, sink)` draws the picture a band of rows at a time (`Target::window`: the rows of the one plane every layer is drawn for; `render` is one band through the same entry), `render_to_png` and `render_to_pdf` (`stream.rs`, `pdf.rs`) write it as it is made, `bt-scene render` streams a picture of more than `BAND_PIXELS` (2^24) pixels; a banded render equals the whole one bit for bit. Pictures, flat colours, groups, drawings and mask planes are drawn by band; a looked layer is drawn for the rows of a band and the margin its looks reach where image can band them, else whole once and cut, as a plugin kind is unless it says `by_position` (the floor; `docs/future.md`, "B1"), and a layer that draws only its own pixels has no 2^27 limit.

**Setup, once:** image and vector are private git dependencies (pinned by `rev` in `Cargo.toml`, bumped in a commit of its own).
A fetch needs git credentials for `github.com/bullterrier-studio` (a credential helper is enough; if cargo's own git client is
refused, `CARGO_NET_GIT_FETCH_WITH_CLI=true`; there is no `.cargo/config.toml`, the template forbids it). CI needs a token that
can read both repositories.

## Responsibility

`bullterrier-scene` is the **scene package** of the Bullterrier ecosystem. It composes layers into one picture and has **no concept
of time** (a parameter is a state; the caller sets it per frame). Image and vector never depend on each other; scene is the one
package that knows both (each in one adapter file), and video builds on scene. Private until image and vector are released.

| Responsible for | Not responsible for |
|---|---|
| The canvas, layers, groups; placement, masks, blend modes, effect chains per layer or group | Effects on one picture → image |
| Rendering in bands, caching per layer, output (picture, PNG, PDF) | Shapes, type, booleans → vector |
| Describing every parameter of the scene | Values over time → control; footage, timeline, sound → video |

Conventions that bind every batch (`docs/design.md`): a look's `px` is image's design pixel (1/480 of the canvas's long side), a scene's own lengths are canvas pixels; blend in sRGB values;
straight alpha at the boundary, premultiplied inside; a light layer's room follows its looks' spread. A layer source answers one
question ("draw this part of your box at this scale"); bands, caching and resampling stay behind `render`.

## Names

`bullterrier-scene` (PyPI package, GitHub repository, Rust crate; `bullterrier-scene-python` is the bindings crate), `bullterrier_scene` (import; alias `bts` in
docs), `bt-scene` (command, config file `bt-scene.toml`, cache folder), `BT_SCENE_*` (env vars), `BullterrierSceneError` (base exception).
Identity is spelled in full, what a person types with the short prefix; a hyphen for what lives in a shell or a file system,
an underscore only where Python, Rust or an environment variable needs it. Names inside the package carry no prefix.

## Architecture

Layers, imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.

```
app        __main__, cli
scene      Scene                                        (a canvas and its stack; a facade over `_core.Scene`)
layer      Layer                                        (what a call takes of the objects the siblings write; a facade over `_core.Layer`)
plugin     register_layer, layer_kind, layer_kinds      (the author-facing module and the entry-point loader; the extension calls its `_load` at run time)
_native    home of the Rust extension `_native._core`   (imports `errors` only, lazily, to raise its errors)
errors     BullterrierSceneError > InvalidError, FormatError, ReadError
```

**Rust core:** `crates/core` (pure Rust: no PyO3, no numpy, `#![forbid(unsafe_code)]`; one file per concept): `scene` (`Scene`),
`layer` (`Layer`: a body and a join), `reference` (`Layer.of`: how `Scene::add` resolves them), `budget` (`MAX_DEPTH`, `MAX_LAYERS`: one rule), `cache` (the bounded LRU and the `Keyed` key), `layer_kind` (`LayerKind`: the plugin seam), `layer_kinds` (`LayerKinds`, `LayerKindFactory`: kinds by name), `raster` (`Raster`), `blend` (`Blend`, the compositing equation and light), `render` (size, `draw`: the one entry that draws a stack, the join of each layer, straight alpha), `warp` (the one resample: bilinear and area average), `placement` (`Fit`,
`Place`, `Deg`: where a layer's box goes), `perspective` (`Quad`: the convex quadrilateral and its homography), `warp/footprint` (the filter of a map that does not keep the axes), `geometry` (`Rect`, `Scale`: the one rule for fractional boxes), `map` (`Map`), `mask` (`Mask`, `Use`: the masks a layer is drawn through, as planes of the canvas plus a margin), `feather` (the blur of a mask: three box passes), `text` (`Text`: the options of a text layer), `error` (the one error type), and `finish` (`Finish`: a layer's looks and the masks by its own maps, run by `Opened::draw` on what any source drew), `document` (the saved scene: written, read and compared) and `files` (a file a scene names), and `source/` (the trait with one question, "draw this part of your box at this scale", and its five answers: `fill`, `make` (a plugin kind), `picture` (the image adapter: the picture, its maps, and `Look`), `drawing` (the vector adapter, and the text layers), `scene` (a group: `render::draw` on a backdrop of its own)). `crates/cli` (the command line `bt-scene`: parse, call, print; clap; no logic of the core), `crates/python` (PyO3 bindings
building `bullterrier_scene._native._core`; generic: no logic of the core, only the mapping of calls, arrays and errors). A refusal
of the core is a Python `InvalidError`, `FormatError` or `ReadError`, imported when the first one is raised. There is no other
boundary. The Python facade turns what a caller holds (a look's dict, a drawing's JSON, a path) into the data the core takes and
never reimplements it.

## Rules

@docs/rules.md

The shared rules above come from the template (`docs/rules.md`, updated by `copier update`). This project's own rules go
here, below them; this file is the project's and an update never changes it.

## Scene's rules

1. **A sibling crate is named in one file.** `bullterrier_image` only in `source/picture.rs`, `bullterrier_vector` only in
   `source/drawing.rs`, and neither file declares a public item: no sibling type is in a public signature (`tests/it/architecture.rs`).
2. **Look and drawing enter as data**, a recipe's JSON and a drawing's JSON; Python imports neither sibling package.
3. **Pins move in a commit of their own**, with the gate run; the differential tests of `tests/it/data_boundary.rs` run first and
   name what a sibling changed.
4. **Straight alpha at every boundary, premultiplied inside; sRGB values** (`docs/design.md`, "Conventions").
