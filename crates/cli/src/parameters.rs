//! `bt-scene parameters SCENE`.

use std::path::PathBuf;

use bullterrier_scene::{Error, Parameter, Value};
use clap::Parser;
use serde_json::{Map, json};

use crate::{open, say};

/// The arguments of `parameters`.
#[derive(Parser)]
pub(crate) struct Args {
    /// The saved scene (JSON).
    scene: PathBuf,
    /// Print the parameter format's JSON (a list of objects, one per parameter) instead of the table.
    #[arg(long)]
    json: bool,
    /// The folder the scene's relative paths are relative to (default: the scene file's folder).
    #[arg(long, value_name = "DIR")]
    base: Option<PathBuf>,
}

pub(crate) fn command(args: &Args, out: &mut dyn std::io::Write) -> Result<(), Error> {
    let parameters = open(&args.scene, args.base.as_deref())?.parameters();
    if args.json {
        let described: Vec<_> = parameters.iter().map(described).collect();
        return say(out, &serde_json::Value::Array(described).to_string());
    }
    let rows: Vec<[String; 5]> = parameters
        .iter()
        .map(|parameter| {
            [
                parameter.address.clone(),
                parameter.kind.name().to_owned(),
                parameter.unit.clone().unwrap_or_default(),
                range(parameter),
                shown(&parameter.value),
            ]
        })
        .collect();
    let head = ["ADDRESS", "KIND", "UNIT", "RANGE", "VALUE"].map(str::to_owned);
    let widths: Vec<usize> = (0..5)
        .map(|column| {
            std::iter::once(&head)
                .chain(&rows)
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in std::iter::once(&head).chain(&rows) {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        say(out, cells.join("  ").trim_end())?;
    }
    Ok(())
}

/// What a value may be, for the table: a number's range (a cyclic one says so), how many names a choice has (`--json` lists them), a text's least length.
fn range(parameter: &Parameter) -> String {
    if let (Some(min), Some(max)) = (parameter.min, parameter.max) {
        format!(
            "{}..{}{}",
            number(min),
            number(max),
            if parameter.cyclic { " cyclic" } else { "" }
        )
    } else if !parameter.choices.is_empty() {
        format!("{} choices", parameter.choices.len())
    } else if let Some(least) = parameter.min_length {
        format!("{least}+ characters")
    } else {
        String::new()
    }
}

/// A number for the table: six decimals at most (a value held in 32 bits is not shown with the digits of its 64-bit twin).
fn number(value: f64) -> String {
    let written = format!("{value:.6}");
    written.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A value for the table.
fn shown(value: &Value) -> String {
    match value {
        Value::Number(value) => number(*value),
        Value::Bool(on) => on.to_string(),
        Value::Text(text) => text.clone(),
        Value::File(path) => path.display().to_string(),
        Value::Picture(raster) => format!("picture {} x {}", raster.width(), raster.height()),
        Value::Map(map) => format!("map {} x {}", map.width(), map.height()),
        _ => "none".to_owned(),
    }
}

/// A default as JSON.
fn plain(value: &Value) -> serde_json::Value {
    match value {
        Value::Number(number) => json!(number),
        Value::Bool(on) => json!(on),
        Value::Text(text) => json!(text),
        Value::File(path) => json!(path.display().to_string()),
        _ => serde_json::Value::Null,
    }
}

/// The parameter as the parameter format writes it (`address` for `name`; no `value`), the keys of `Parameter.to_dict()` of
/// the Python package.
fn described(parameter: &Parameter) -> serde_json::Value {
    let mut written = Map::new();
    written.insert("address".to_owned(), json!(parameter.address));
    written.insert("kind".to_owned(), json!(parameter.kind.name()));
    written.insert("meaning".to_owned(), json!(parameter.meaning));
    written.insert("default".to_owned(), plain(&parameter.default));
    written.insert("unit".to_owned(), json!(parameter.unit));
    written.insert("min".to_owned(), json!(parameter.min));
    written.insert("max".to_owned(), json!(parameter.max));
    written.insert("cyclic".to_owned(), json!(parameter.cyclic));
    if !parameter.choices.is_empty() {
        written.insert("choices".to_owned(), json!(parameter.choices));
    }
    if let Some(least) = parameter.min_length {
        written.insert("min_length".to_owned(), json!(least));
    }
    if parameter.nullable {
        written.insert("nullable".to_owned(), json!(true));
    }
    serde_json::Value::Object(written)
}
