//! Architecture tests: they read the crate's own source as code (which a behaviour test never does).
//! The core is pure Rust: it never names `PyO3` or numpy, so it builds anywhere, a browser included. The scan has rows
//! that prove it sees what it must and only that.

use std::fs;
use std::path::Path;

use crate::common::rust_files;

/// The lines of `text` that name `PyO3` or numpy in code (a `//` comment, a doc comment included, names them freely).
fn python_in_code(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter(|line| line.contains("pyo3") || line.contains("numpy"))
        .collect()
}

#[test]
fn the_core_never_names_python() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = rust_files(&source);
    assert!(!files.is_empty(), "no source under {}", source.display());
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_else(|error| unreachable!("{}: {error}", file.display()));
        assert_eq!(python_in_code(&text), Vec::<&str>::new(), "{}", file.display());
    }
}

#[test]
fn the_scan_sees_code_that_names_python_and_not_a_comment_that_does() {
    assert_eq!(python_in_code("use pyo3::prelude::*;\n"), ["use pyo3::prelude::*;"]);
    assert_eq!(
        python_in_code("let array = numpy::zeros();\n"),
        ["let array = numpy::zeros();"]
    );
    assert_eq!(
        python_in_code("//! No PyO3 and no numpy here.\n    // pyo3\nfn f() {}\n"),
        Vec::<&str>::new()
    );
}

/// The lines of `text` that name the crate `krate` in code.
fn naming<'a>(text: &'a str, krate: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter(|line| line.contains(krate))
        .collect()
}

/// The lines of `text` that declare a public item (`pub`, not `pub(crate)`).
fn public_items(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("pub ") && !line.starts_with("pub use"))
        .collect()
}

/// Everything scene knows of a sibling crate is in one adapter file, so a change in it, or another pinned version, touches
/// that file; and neither crate's types are in scene's public signatures, so they stay out of its semver.
#[test]
fn only_the_two_adapter_files_name_the_sibling_crates_and_neither_declares_a_public_item() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // image has a second home, the GPU context (`gpu.rs`), which holds image's context on the device and returns image's
    // `GpuImage` (the one sibling type in a public signature: `gpu_signatures_name_one_sibling_type`)
    let adapters = [
        ("bullterrier_image", "source/picture.rs"),
        ("bullterrier_vector", "source/drawing.rs"),
    ];
    let also = [("bullterrier_image", "gpu.rs")];
    for file in rust_files(&source) {
        let text = fs::read_to_string(&file).unwrap_or_else(|error| unreachable!("{}: {error}", file.display()));
        let relative = file
            .strip_prefix(&source)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        for (krate, home) in adapters {
            let names = naming(&text, krate);
            if also.contains(&(krate, relative.as_str())) {
                continue;
            }
            if relative == home {
                assert!(!names.is_empty(), "{home} no longer uses {krate}: is the adapter gone?");
            } else {
                assert_eq!(names, Vec::<&str>::new(), "{relative} names {krate}; only {home} may");
            }
        }
        if adapters.iter().any(|(_, home)| *home == relative) {
            assert_eq!(
                public_items(&text),
                Vec::<&str>::new(),
                "{relative} declares a public item"
            );
        }
    }
}

/// The GPU context's public items name no sibling type but image's `GpuImage` (what a resident render returns), and image's
/// context and errors stay private.
#[cfg(feature = "gpu")]
#[test]
fn gpu_signatures_name_one_sibling_type() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gpu.rs"))
        .unwrap_or_else(|error| unreachable!("{error}"));
    for line in public_items(&text)
        .into_iter()
        .filter(|line| line.contains("fn ") || line.contains("struct "))
    {
        let tail = line.replace("GpuImage", "");
        assert!(
            !tail.contains("ImageGpu") && !tail.contains("bullterrier_image") && !tail.contains("ErrorKind"),
            "a public item of gpu.rs names a sibling type: {line}"
        );
    }
    assert!(
        text.contains("-> Result<GpuImage, Error>"),
        "the resident render returns image's GpuImage"
    );
}

#[test]
fn the_scan_sees_a_sibling_crate_named_in_code_and_a_public_item_and_not_a_comment() {
    assert_eq!(
        naming("use bullterrier_image::Recipe;\n", "bullterrier_image"),
        ["use bullterrier_image::Recipe;"]
    );
    assert_eq!(
        naming(
            "//! Reads bullterrier_image.\n    // bullterrier_image\nfn f() {}\n",
            "bullterrier_image"
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        public_items("    pub fn render(&self) {}\npub(crate) fn hidden() {}\npub use x::Y;\n"),
        ["pub fn render(&self) {}"]
    );
}
