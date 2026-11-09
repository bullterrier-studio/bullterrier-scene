# CLAUDE.md: bullterrier-scene

A Rust library (crate `bullterrier-scene`) with a Python interface that puts layers (pictures, image looks, vector drawings) together
into one picture: placed, masked, blended, rendered at any size and in bands. **The code API is the product**; JSON is the same scene
written down. `README.md` (use), `docs/design.md` (what and how), `docs/design-decisions.md` (why), `docs/roadmap.md` (the batches),
`docs/ecosystem.md` (the sibling packages), `docs/shared-files.md` (what stays identical across projects), `docs/releasing.md`.

**Write docs terse.** This file, the README, `docs/`, docstrings, reports and briefs: state the rule or fact, drop preamble,
repetition and justification a reader does not need to act. Reasons go in `docs/design-decisions.md`, once. Committed files are
public: no absolute paths, no references to folders outside the repository.

## Start here

```bash
uv sync && uv run poe setup          # once per clone: environment (builds the Rust extension: needs rustup), git hooks, commit template, cargo tools
uv run pytest tests/test_scale.py -n 0   # the tests near your change, while you work
cargo test -p bullterrier-scene --test it scale::  # one Rust area (inline: cargo test -p bullterrier-scene --lib scaling::)
uv run poe fast                      # the inner loop, as often as you like: format, lint, types, quick tests
uv run poe check                     # the gate, before you report or merge (then `uv run lint-imports`)
uv run poe full                      # `check` and the gate proofs: before a tag (`poe release` runs it) and in CI
```

| To ... | Look at |
|---|---|
| add logic | a `pub` function in `crates/core/src/<concept>.rs`, re-exported in `lib.rs`; `scaling.rs` is the model |
| expose it to Python | a `#[pyfunction]` in `crates/python/src/lib.rs` exported from `mod _core`; its stub line in `src/bullterrier_scene/_native/_core.pyi`; the name in `src/bullterrier_scene/__init__.py` |
| add a command | `src/bullterrier_scene/cli.py` |
| add an error | the core's `Error` (`error.rs`), mapped in `crates/python/src/error.rs`, a class in `errors.py` |
| test it | "Testing" and "Rust tests" below; `crates/core/tests/it/` (Rust), `tests/` (Python) |
| add a macros crate | "Adding a macros crate" below |
| change a gate | `pyproject.toml` (Python tools, poe tasks), `Cargo.toml` (Rust lints, tool pins); proof in `tests/tools/` |
| keep files identical across projects | `docs/shared-files.md` |

**State:** scaffold only (S0): the template's one feature `scale(value, factor)` stands in end to end (core, extension, package, the
command `bt-scene scale`, each tested) until batch C1 replaces it. Next: A1 (the code API, `docs/roadmap.md`). No work starts until image E4 and vector V10 are finished, merged and pushed (`docs/roadmap.md`, "When work starts").

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
question ("draw this rectangle at this scale"); bands, caching and resampling stay behind `render`.

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
_native    home of the Rust extension `_native._core`   (imports `errors` only, lazily, to raise its errors)
errors     BullterrierSceneError > InvalidError
```

**Rust core:** `crates/core` (pure Rust: no PyO3, no numpy, `#![forbid(unsafe_code)]`; one
file per concept; `error.rs` the one error type) and `crates/python` (PyO3 bindings building `bullterrier_scene._native._core`; generic: no
logic of the core, only the mapping of calls and errors). A refusal of the core is a Python `InvalidError`, imported when the first
one is raised. There is no other boundary. Python code never reimplements the core; a core function is exposed as it is.

## Rules

@docs/rules.md

The shared rules above come from the template (`docs/rules.md`, updated by `copier update`). This project's own rules go
here, below them; this file is the project's and an update never changes it.
