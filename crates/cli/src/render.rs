//! `bt-scene render SCENE OUT`.

use std::path::PathBuf;

use bullterrier_scene::{BAND_PIXELS, Error, Kind, PdfOptions, Values, WriteOptions};
use clap::Parser;

use crate::{open, say};

/// The arguments of `render`.
#[derive(Parser)]
pub(crate) struct Args {
    /// The saved scene (JSON).
    scene: PathBuf,
    /// The picture to write: a PNG (`.png`) or a PDF (`.pdf`), by its extension.
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
    /// Pixels per inch. A PDF: fixes the page's physical size (the picture's width over DPI inches), 1 to 2400 (default 300). A PNG
    /// or a JPEG: the resolution the file says it prints at, 1 to 65,535 (default none).
    #[arg(long, value_name = "DPI")]
    dpi: Option<f64>,
    /// PDF: millimetres of bleed on every side, the picture's edge repeated outward; 0 to 50 (default 0).
    #[arg(long, value_name = "MM")]
    bleed: Option<f64>,
    /// PDF: draw crop marks outside the bleed.
    #[arg(long)]
    crop_marks: bool,
    /// PDF: put the picture over white instead of keeping its transparency.
    #[arg(long)]
    flatten: bool,
    /// PDF: write CMYK through this ICC output profile and embed it.
    #[arg(long, value_name = "FILE.icc")]
    cmyk_profile: Option<PathBuf>,
}

/// The PDF options the flags give, or `None` for a picture that is not a PDF (its `--dpi` is [`file_options`]).
///
/// # Errors
/// [`Error::Invalid`] naming the first PDF flag given with an output that is not a PDF, or the one a value is wrong for.
fn pdf_options(args: &Args) -> Result<Option<PdfOptions>, Error> {
    let is_pdf = args
        .out
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
    if !is_pdf {
        let given = [
            ("bleed", args.bleed.is_some()),
            ("crop-marks", args.crop_marks),
            ("flatten", args.flatten),
            ("cmyk-profile", args.cmyk_profile.is_some()),
        ];
        return match given.iter().find(|(_, set)| *set) {
            Some((flag, _)) => Err(Error::Invalid {
                address: (*flag).to_owned(),
                message: "only a PDF takes this option; name the output `.pdf`".to_owned(),
            }),
            None => Ok(None),
        };
    }
    let mut options = PdfOptions::new(args.dpi.unwrap_or(DEFAULT_DPI))?
        .bleed_mm(args.bleed.unwrap_or(0.0))?
        .crop_marks(args.crop_marks)
        .flatten(args.flatten);
    if let Some(profile) = &args.cmyk_profile {
        options = options.cmyk_profile(profile);
    }
    Ok(Some(options))
}

/// The options of a picture that is not a PDF: the resolution `--dpi` gives, if any.
///
/// # Errors
/// [`Error::Invalid`] naming `dpi` for a number outside 1 to 65,535.
fn file_options(args: &Args) -> Result<WriteOptions, Error> {
    args.dpi
        .map_or_else(|| Ok(WriteOptions::new()), |dpi| WriteOptions::new().dpi(dpi))
}

/// The resolution of a PDF when none is given: the usual one for print.
const DEFAULT_DPI: f64 = 300.0;

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
    let pdf = pdf_options(args)?;
    let file = file_options(args)?;
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
    let (columns, rows) = scene.picture_size(size)?;
    if columns * rows > BAND_PIXELS {
        // more than a band: drawn and written band by band, never held whole
        match &pdf {
            Some(options) => scene.render_to_pdf(size, &args.out, options)?,
            None => scene.render_to_png_with(size, &args.out, &file)?,
        }
    } else {
        let picture = scene.render(size)?;
        match &pdf {
            Some(options) => picture.write_pdf(&args.out, options)?,
            None => picture.write_with(&args.out, &file)?,
        }
    }
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
