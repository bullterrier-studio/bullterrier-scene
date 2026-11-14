//! The command line through `main(argv)`: exit code, stdout and stderr, and the files it writes, in-process. The boundary: a
//! test calls `bullterrier_scene_cli::main` and reads files; it reaches nothing else of the crate. Pictures are decoded with
//! image's reader and compared with what the core renders.

mod common;

mod parameters;
mod render;
mod usage;
mod validate;
