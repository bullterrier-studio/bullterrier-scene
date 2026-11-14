//! `bt-scene validate SCENE`.

use std::path::PathBuf;

use bullterrier_scene::Error;
use clap::Parser;

use crate::{open, say};

/// The arguments of `validate`.
#[derive(Parser)]
pub(crate) struct Args {
    /// The saved scene (JSON).
    scene: PathBuf,
    /// The folder the scene's relative paths are relative to (default: the scene file's folder).
    #[arg(long, value_name = "DIR")]
    base: Option<PathBuf>,
}

pub(crate) fn command(args: &Args, out: &mut dyn std::io::Write) -> Result<(), Error> {
    let scene = open(&args.scene, args.base.as_deref())?;
    // one output pixel: every file is read and checked against its digest, every layer is drawn, a hole and a map of another
    // size are refused, and nothing is written
    scene.render(1)?;
    say(
        out,
        &format!(
            "ok: {} x {}, {} layers",
            scene.width(),
            scene.height(),
            scene.layers().len()
        ),
    )
}
