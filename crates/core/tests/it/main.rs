//! The integration tests of the core: one binary, a module per area, through the public API only.
//! A test stays next to its code only when no public route reaches what it checks (a private kernel, a `pub(crate)`
//! item); an item is never made `pub` to move a test out. Run one area: `cargo test -p bullterrier-scene --test it data_boundary::`.

mod common;

// the cover template of `docs/api.md`, in Rust: a program that is also a test of the saved scene (`saved`)
#[expect(dead_code, reason = "its own `main`, the program of the example")]
mod cover_template;

mod architecture;
mod bands;
mod blend;
mod cache;
mod compose;
mod data_boundary;
mod drawing;
mod groups;
mod layer_kinds;
mod light;
mod look;
mod maps;
mod masks;
mod output;
mod parameters;
mod pdf;
mod perspective;
mod picture;
mod placement;
mod plugin;
mod references;
mod room;
mod saved;
mod structure;
mod text;
