# Design decisions

The reasons behind the template, once. The rules themselves are in `CLAUDE.md`.

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

## A gate that passed is not repeated on the same tree

`poe check` notes the tree of `HEAD` when it starts and, when every step passed, records it in the git folder if the working tree
was clean at both ends and `HEAD` was unchanged; the pre-push hook skips the gate for a tip with that tree. *Reason:* the hook
repeated a minutes-long gate on contents just checked, and a slow loop gets worked around. *Not seen by the record:* state outside
the tree (installed cargo tools, the advisory database, the built extension); CI checks everything again.

## No runtime dependency

The package installs with nothing but its wheel. *Reason:* every dependency is something to audit, resolve and ship; add one with
its reason here.
