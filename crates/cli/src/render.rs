//! `bt-scene render SCENE OUT`.

use std::path::PathBuf;

use bullterrier_scene::{Error, Kind, Values};
use clap::Parser;

use crate::{open, say};

/// The arguments of `render`.
#[derive(Parser)]
pub(crate) struct Args {
    /// The saved scene (JSON).
    scene: PathBuf,
    /// The picture to write: a PNG (`.png`).
    out: PathBuf,
    /// The picture's long side in pixels (default: the canvas's long side); 1 to 65,000.
    #[arg(long, value_name = "PIXELS")]
    size: Option<i64>,
    /// Set a parameter before rendering, by address (`bt-scene parameters` lists them): a number, `true` or `false`, a colour
    /// `#rrggbb`, a text or choice, or the path of a picture file. Repeat it for more.
    #[arg(long = "set", value_name = "ADDRESS=VALUE", value_parser = assignment)]
    set: Vec<(String, String)>,
    /// The folder the scene's relative paths are relative to (default: the scene file's folder).
    #[arg(long, value_name = "DIR")]
    base: Option<PathBuf>,
}

/// `ADDRESS=VALUE` split at the first `=`.
fn assignment(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((address, value)) if !address.is_empty() => Ok((address.to_owned(), value.to_owned())),
        _ => Err(format!("`{}` is not ADDRESS=VALUE", shortened(text))),
    }
}

/// `text` cut to 60 characters for a message.
fn shortened(text: &str) -> String {
    let mut shown: String = text.chars().take(60).collect();
    if shown.len() < text.len() {
        shown.push_str("...");
    }
    shown
}

pub(crate) fn command(args: &Args, out: &mut dyn std::io::Write) -> Result<(), Error> {
    let scene = open(&args.scene, args.base.as_deref())?;
    let scene = if args.set.is_empty() {
        scene
    } else {
        let parameters = scene.parameters();
        let mut values = Values::new();
        for (address, text) in &args.set {
            let kind = parameters
                .iter()
                .find(|parameter| parameter.address == *address)
                .map(|parameter| parameter.kind);
            values = values.set(address.as_str(), value(address, kind, text)?);
        }
        scene.with_values(&values)?
    };
    let size = args
        .size
        .unwrap_or_else(|| i64::from(scene.width().max(scene.height())));
    scene.render(size)?.write(&args.out)?;
    say(out, &args.out.display().to_string())
}

/// `text` as the value of the parameter `address` of `kind`: an address the scene does not have (`None`) is passed on as text,
/// for the scene to refuse with the addresses it has.
fn value(address: &str, kind: Option<Kind>, text: &str) -> Result<bullterrier_scene::Value, Error> {
    let wanted = |what: &str| Error::Invalid {
        address: address.to_owned(),
        message: format!("{what} is wanted, got `{}`", shortened(text)),
    };
    Ok(match kind {
        Some(Kind::Float | Kind::Int) => text.parse::<f64>().map_err(|_| wanted("a number"))?.into(),
        Some(Kind::Bool) => match text {
            "true" => true.into(),
            "false" => false.into(),
            _ => return Err(wanted("`true` or `false`")),
        },
        Some(Kind::Picture) => PathBuf::from(text).into(),
        _ => text.into(),
    })
}
