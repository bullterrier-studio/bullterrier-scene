//! The command line of bullterrier-scene: `bt-scene COMMAND ...`.
//!
//! A thin layer over the core: parse, call, print. [`main`] takes the arguments and the two streams, so a test (or the
//! Python console script) runs it in-process and sees the exit code and the output.

mod parameters;
mod render;
mod validate;

use std::io::Write;
use std::path::{Path, PathBuf};

use bullterrier_scene::{Error, Scene};
use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

const EXIT_CODES: &str = "\
exit codes:
  0    success
  1    the command failed (reason after \"error:\" on stderr)
  2    wrong usage: an unknown command or option (the message explains)
  130  interrupted with Ctrl-C (the console script)

`bt-scene COMMAND --help` shows a command's options and an example. Errors go to stderr, results to stdout.";

#[derive(Parser)]
#[command(
    name = "bt-scene",
    about = "Put layers into one picture: render a saved scene, list its parameters, check it.",
    after_help = EXIT_CODES
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a saved scene to a PNG.
    #[command(
        after_help = "example: bt-scene render docs/examples/cover.scene.json out.png --size 800 --set title.opacity=0.5"
    )]
    Render(render::Args),
    /// List every parameter of a saved scene: its address, kind, unit, range and value.
    #[command(after_help = "example: bt-scene parameters docs/examples/cover.scene.json --json")]
    Parameters(parameters::Args),
    /// Check that a saved scene can be rendered: it reads, its files are there and are the ones it was saved with.
    #[command(after_help = "example: bt-scene validate docs/examples/cover.scene.json")]
    Validate(validate::Args),
}

/// The scene file `path` (a saved scene, "Saved scene" in `docs/api.md`) and the folder its relative paths are relative to:
/// `base`, or the file's own folder.
fn open(path: &Path, base: Option<&Path>) -> Result<Scene, Error> {
    let text = std::fs::read_to_string(path).map_err(|error| Error::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let folder = base.map_or_else(
        || path.parent().map_or_else(PathBuf::new, Path::to_path_buf),
        Path::to_path_buf,
    );
    Scene::from_json(&text, &folder)
}

/// A result line, to `out`.
fn say(out: &mut dyn Write, line: &str) -> Result<(), Error> {
    writeln!(out, "{line}").map_err(|error| Error::Write {
        path: "stdout".to_owned(),
        message: error.to_string(),
    })
}

/// Runs `bt-scene` with `argv` (the arguments after the program's name) and returns the exit code: 0 success, 1 the command
/// failed (`error: <what is wrong>` on `err`), 2 wrong usage. Results go to `out`. `version` is the package's, which
/// `--version` prints. Ctrl-C is the console script's: it restores the default signal handler, so the process ends with 130.
pub fn main(argv: &[String], version: &str, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let arguments = std::iter::once("bt-scene").chain(argv.iter().map(String::as_str));
    let matches = match Cli::command()
        .version(version.to_owned())
        .try_get_matches_from(arguments)
    {
        Ok(matches) => matches,
        Err(usage) => {
            let shown = usage.render().to_string();
            // a closed stream cannot be told about: the code is the answer
            return if matches!(usage.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                let _ = write!(out, "{shown}");
                0
            } else {
                let _ = write!(err, "{shown}");
                2
            };
        }
    };
    let Ok(cli) = Cli::from_arg_matches(&matches) else {
        return 2;
    };
    let outcome = match cli.command {
        Command::Render(given) => render::command(&given, out),
        Command::Parameters(given) => parameters::command(&given, out),
        Command::Validate(given) => validate::command(&given, out),
    };
    match outcome {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(err, "error: {error}");
            1
        }
    }
}
