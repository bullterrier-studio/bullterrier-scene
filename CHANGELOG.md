# Changelog

All notable changes to this project are documented here, in the [Keep a Changelog](https://keepachangelog.com/) format.
`uv run poe release VERSION` writes each release's section from the Conventional Commits since the last one.

## [Unreleased]

### Added

- **A GPU compositor** (feature `gpu`): `Gpu`, `Gpu::render`, `Gpu::download`; Python `Gpu`, `render(size, gpu=)`; `bt-scene render --gpu`; `GpuError`.
