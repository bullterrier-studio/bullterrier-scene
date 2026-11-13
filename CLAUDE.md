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
| add a command | `src/bullterrier_scene/cli.py` |
| add an error | the core's `Error` (`error.rs`), mapped in `crates/python/src/error.rs`, a class in `errors.py` (`InvalidError`, `FormatError`, `ReadError` so far) |
| test it | "Testing" and "Rust tests" below; `crates/core/tests/it/` (Rust), `tests/` (Python) |
| add a macros crate | "Adding a macros crate" below |
| change a gate | `pyproject.toml` (Python tools, poe tasks), `Cargo.toml` (Rust lints, tool pins); proof in `tests/tools/` |
| keep files identical across projects | `docs/shared-files.md` |

**State:** C1, C2a, C2b and C3 done (`docs/roadmap.md`): a `Scene` of picture, drawing, fill and text layers with an image look on a picture, each
`fit` into a box (cover, contain, stretch) and `place`d (flip, scale, aspect, skew, rotate about an anchor, move) by one warp, normal blend and opacity, rendered whole at
any size to a float32 array (Python) and a `Raster` (Rust). What is built and where it differs from the contract is "What is built" in
`docs/api.md`. A layer can also be put on a convex quadrilateral (`perspective`). A picture holds named maps and hands them to its looks. A layer is masked by a layer drawn on the canvas or by one of its own maps (`mask`: alpha or luminance, `feather`, `invert`; masks nest to `MAX_DEPTH`). Next: blend modes and groups (C4).

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
| Rendering in bands, caching per layer, output (picture, PNG, later PDF) | Shapes, type, booleans → vector |
| Describing every parameter of the scene | Values over time → control; footage, timeline, sound → video |

Conventions that bind every batch (`docs/design.md`): a `px` value is a pixel of the scene's design canvas; blend in sRGB values;
straight alpha at the boundary, premultiplied inside; a layer's room follows its effects' footprint. A layer source answers one
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
_native    home of the Rust extension `_native._core`   (imports `errors` only, lazily, to raise its errors)
errors     BullterrierSceneError > InvalidError, FormatError, ReadError
```

**Rust core:** `crates/core` (pure Rust: no PyO3, no numpy, `#![forbid(unsafe_code)]`; one file per concept): `scene` (`Scene`),
`layer` (`Layer`), `raster` (`Raster`), `render` (size, normal blend, straight alpha), `warp` (the one resample: bilinear and area average), `placement` (`Fit`,
`Place`, `Deg`: where a layer's box goes), `perspective` (`Quad`: the convex quadrilateral and its homography), `warp/footprint` (the filter of a map that does not keep the axes), `geometry` (`Rect`, `Scale`: the one rule for fractional boxes), `map` (`Map`), `mask` (`Mask`, `Use`: the masks a layer is drawn through, as planes of the canvas plus a margin), `feather` (the blur of a mask: three box passes), `text` (`Text`: the options of a text layer), `error` (the one error type), and `source/` (the trait with one question, "draw this part of your box at this scale", and its three answers: `fill`, `picture` (the image adapter: its maps and the masks by them), `drawing` (the vector adapter, and the text layers)). `crates/python` (PyO3 bindings
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
