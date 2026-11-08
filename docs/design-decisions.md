# Design decisions

The reasons behind the project's choices, once. The rules themselves are in `CLAUDE.md`; what the package is: [design.md](design.md).

## Platforms

- **One abi3 wheel per platform** (`abi3-py312`): Linux x86_64 and aarch64 (in the manylinux 2_28 container of the runner's own
  architecture, so nothing is cross-compiled), macOS arm64 and x86_64 (the latter cross-compiled on the arm64 runner), Windows
  x86_64, plus the sdist. *Reason:* one wheel serves every supported Python, and the pure-Rust core has no platform code.
- **Every wheel is installed on its own platform from the built file alone and run** before anything is published; the sdist is
  built and run on Linux. *Reason:* a wheel that builds can still fail to load.
- **No build cache in the jobs that build what may be published.** *Reason:* nothing restored from a cache goes into a release.

## The core is pure Rust

`crates/core` has no PyO3 and no numpy (`deny.toml` bans them outside the bindings, an architecture test scans `src/`) and
forbids unsafe code. *Reason:* the logic runs without Python or the GIL and is tested without an interpreter; the bindings
are a mapping that can be read in minutes.

## The core's tests are one binary, black-box by default

Every `tests/*.rs` is an executable of its own, which macOS scans on first launch (12 s measured, more under load: 62 binaries
made one `cargo test` take 18 minutes). So `tests/it/main.rs` holds a module per area, and `rust-test-layout`
(`tools/check_rust_test_layout.py`, the table `[tool.rust-tests.own-binaries]`) fails a stray binary, a listed file without its
`//! Own binary: <why>` first line, a stale listing, a `[[test]]` target outside the listing, a module `main.rs` does not declare,
and `autotests = false` beside a `tests/it/`. A test is in `tests/it/` unless no public route reaches what it checks (a private
kernel, a `pub(crate)` item): then it stays inline. *Reason:* a test through the public contract survives a refactor of the
internals, and widening an item to `pub` to move a test out would make the API larger for the tests' sake. *Cost:* a helper both
sides need exists twice. Bindings have `test = false` (the Python suite tests them) and doctests are off (each is an executable
too).

## Rust tests run under a memory cap, not an allocator in the binary

`poe rust-test` runs `cargo test` through `tools/run_capped.py`, which sums the resident memory of the whole process group ten
times a second and kills it past `cap-mb` (exit 99; a deadline is 98). *Reason:* runaway tests reached 43, 57 and 152 GB and
froze the machine; a timeout limits seconds, not memory. A cap on the group covers every binary, doctests and children, where an
allocator in one binary covered only that binary, and the crate needs no dev-dependency for it. The runner's own tests kill a
known-bad allocation. It is not enforced on Windows (no process groups or `ps`); the CI legs there run uncapped.

## Three tiers; a gate that passed is not repeated on the same tree

`fast` (the inner loop), `check` (everything but the gate proofs, which run when the change touches what they test) and `full` (`check` and
the proofs, for a tag and in CI). *Reason:* the proofs, tests that copy the workspace and build in it to show each gate fails on a known-bad
input, were most of a gate that took 19 minutes in a sibling project; a library change cannot break a gate's ability to fail, so they need
running when the gates or their inputs change, and always before a release. *Cost:* a proof broken from outside its triggers shows in CI or at
the release, not at the push.

`poe check` and `full` note the tree of `HEAD` when they start and, when every step passed, record it with the tier in the git folder if the
working tree was clean at both ends and `HEAD` was unchanged; the pre-push hook skips the gate for a tip with that tree, if the pass was of
the tier the push needs or a higher one (`full` covers `check`, never the reverse). *Reason:* the hook
repeated a minutes-long gate on contents just checked, and a slow loop gets worked around. *Not seen by the record:* state outside
the tree (installed cargo tools, the advisory database, the built extension); CI checks everything again.

## No runtime dependency

The package installs with nothing but its wheel. *Reason:* every dependency is something to audit, resolve and ship; add one with
its reason here.

## Scene is a package of its own (2026-11-08)

Compositing is neither image's (one picture in, one out; image may not depend on vector) nor vector's (geometry, no raster
library) nor video's (decoding, encoding, sound and a timeline a still does not need). *Reason:* posters need layers across
image and vector, and video is unstarted, so splitting compositing out now is the cheapest moment. The options weighed are in
[design.md](design.md), "Why a package of its own".

## Scene knows image and vector (2026-11-08)

Unlike its siblings, scene depends on two of them, each in one adapter file (`source/picture.rs`, `source/drawing.rs`). It is
an integration package, as video will be. *Reason:* a layer must be drawn by the package that owns it, at the final size, so a
draft equals the print. *Consequence:* the repository is private until image and vector are released.

## Image and vector come in as git dependencies pinned to a revision (2026-11-08)

Neither crate is published (`publish = false`). A path dependency would tie scene's CI to a local checkout in the middle of a
branch. *Reason:* a pinned revision builds the same everywhere and a bump is a reviewed commit. *Consequence:* `deny.toml`
allows exactly those two git sources; a dependency arrives with its first user (C1), since an unused crate fails the gate.

## No WebAssembly build yet (2026-11-08)

Generated with `wasm = false`. *Reason:* nobody has shown that image's crate (rayon, rustfft, image decoders) builds for
`wasm32-unknown-unknown`, and scene's browser story is open. *Consequence:* `copier update` with `wasm = true` adds the gate
when a page needs it.

## Blending in sRGB values, straight alpha at the boundary (2026-11-08)

Vector blends sRGB values and returns straight alpha; the W3C spec blends the same way. *Reason:* a drawing must look the same
alone and in a scene. *Consequence:* linear-light blending, if wanted, is an explicit option; the scene premultiplies only
inside.
