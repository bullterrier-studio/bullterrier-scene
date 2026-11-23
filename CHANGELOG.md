# Changelog

All notable changes to this project are documented here, in the [Keep a Changelog](https://keepachangelog.com/) format.
`uv run poe release VERSION` writes each release's section from the Conventional Commits since the last one.

## [Unreleased]

### Added

- **A WebAssembly package** (`crates/wasm`, CPU only): `Scene.fromJson(text, {files})`, `parameters()`, `withValues`, `render({size})`, `toJson()`, errors by the Python names; tested in Node against the Python package (`uv run poe wasm-test`); `docs/javascript-api.md`. Image's `read_bytes` and `read_map_bytes` (a pin bump) read the files from bytes.
- **A GPU compositor** (feature `gpu`): `Gpu`, `Gpu::render`, `Gpu::download`; Python `Gpu`, `render(size, gpu=)`; `bt-scene render --gpu`; `GpuError`. Supported on macOS and the browser; on other platforms experimental, refused unless `BT_GPU_EXPERIMENTAL=1`.
- **A saved scene's files from bytes** (Rust): `Files` (`folder`, `given`, `with`).

### Changed

- **Breaking:** `Scene::from_json_with(text, &Files, &LayerKinds)` takes `Files` in place of its `base` folder (`Files::folder(base)`).
