//! Parameters: every value of a scene that a caller may set, each at an address, and `with_values`, which sets them.
//!
//! **The address grammar is written here, once.** An address is `<layer>.<field>`, the layer by its name (`layer<n>` for a
//! layer without one, by its place), and the field is one of:
//!
//! | Field | What |
//! |---|---|
//! | `opacity`, `blend`, `light`, `dx`, `dy`, `scale`, `aspect`, `rotate`, `skew_x`, `skew_y`, `flip_x`, `flip_y` | the layer's own |
//! | `picture`, `map.<name>`, `text`, `color`, `drawing.<param>` | its source's |
//! | `look.<image address>`, `look#2.<image address>` | its looks' (a reference's own looks, counted from 1) |
//! | `scene.<address>` | an address of the scene a group draws, which has its own layers |
//! | `mask.<field>`, `mask#2.<field>` | a field of the layer a mask is made by, numbered by the layer's `mask` calls |
//!
//! `Layer::parameters` writes the addresses and `Layer::with_values` reads them, side by side in this file. A value is set
//! only at an address the layer lists, so the two cannot disagree: an address that is not listed is refused, whatever it
//! looks like. A sibling's own address (image's `grade.exposure`, vector's Param name) is kept verbatim after its head.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::blend::Blend;
use crate::error::label;
use crate::layer::Base;
use crate::placement::Placement;
use crate::ranges::FRACTION;
use crate::suggest::nearest;
use crate::{Error, Layer, Map, Raster, Scene};

/// What a parameter holds: the kinds of the ecosystem's parameter format that a scene has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// A number.
    Float,
    /// A whole number.
    Int,
    /// A switch.
    Bool,
    /// One of a list of names.
    Choice,
    /// A colour, `#rrggbb` or `#rrggbbaa`.
    Color,
    /// Free text.
    Text,
    /// A picture or a map: an array, a file, or nothing yet (a hole).
    Picture,
}

impl Kind {
    /// The kind as the parameter format spells it: `float`, `int`, `bool`, `choice`, `color`, `text`, `picture`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Choice => "choice",
            Self::Color => "color",
            Self::Text => "text",
            Self::Picture => "picture",
        }
    }
}

/// A value of a parameter, or a value to set one to.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// A number: a float, or a whole number for an `int`.
    Number(f64),
    /// A switch.
    Bool(bool),
    /// Text: free text, a colour or a choice's name.
    Text(String),
    /// A picture held in memory.
    Picture(Arc<Raster>),
    /// A map held in memory.
    Map(Arc<Map>),
    /// A file, read when the scene renders.
    File(PathBuf),
    /// Nothing: a picture or a map not given yet.
    Hole,
}

impl From<f64> for Value {
    fn from(number: f64) -> Self {
        Self::Number(number)
    }
}

impl From<bool> for Value {
    fn from(on: bool) -> Self {
        Self::Bool(on)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<Raster> for Value {
    fn from(raster: Raster) -> Self {
        Self::Picture(Arc::new(raster))
    }
}

impl From<Map> for Value {
    fn from(map: Map) -> Self {
        Self::Map(Arc::new(map))
    }
}

impl From<PathBuf> for Value {
    fn from(path: PathBuf) -> Self {
        Self::File(path)
    }
}

impl From<&Path> for Value {
    fn from(path: &Path) -> Self {
        Self::File(path.to_owned())
    }
}

impl Value {
    /// What this value is, for a message: its kind and, for a short one, itself.
    fn describe(&self) -> String {
        match self {
            Self::Number(number) => format!("the number {number}"),
            Self::Bool(on) => format!("the bool {on}"),
            Self::Text(text) if text.chars().count() <= 40 => format!("the text {text:?}"),
            Self::Text(text) => format!("a text of {} characters", text.chars().count()),
            Self::Picture(raster) => format!("a picture of {} x {} pixels", raster.width(), raster.height()),
            Self::Map(map) => format!("a map of {} x {} pixels", map.width(), map.height()),
            Self::File(path) => format!("the file {}", path.display()),
            Self::Hole => "none".to_owned(),
        }
    }

    /// The refusal of this value for the parameter `field`, which wants `wanted`.
    pub(crate) fn wrong(&self, field: &str, wanted: &str) -> Error {
        Error::invalid(field, format!("{wanted} is wanted, got {}", self.describe()))
    }

    /// This value as a number for `field`.
    pub(crate) fn number(&self, field: &str) -> Result<f64, Error> {
        match self {
            Self::Number(number) => Ok(*number),
            other => Err(other.wrong(field, "a number")),
        }
    }

    /// This value as a bool for `field`.
    pub(crate) fn flag(&self, field: &str) -> Result<bool, Error> {
        match self {
            Self::Bool(on) => Ok(*on),
            other => Err(other.wrong(field, "a bool")),
        }
    }

    /// This value as text for `field`.
    pub(crate) fn text(&self, field: &str) -> Result<&str, Error> {
        match self {
            Self::Text(text) => Ok(text),
            other => Err(other.wrong(field, "a text")),
        }
    }
}

/// One value a scene can be given, and what it may be: the parameter format's description (`address` stands for its `name`: a
/// dotted address is not a valid `name`) and the value the scene holds now.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    /// Where it is: `<layer>.<field>`.
    pub address: String,
    /// What it holds.
    pub kind: Kind,
    /// What it does, in the field's own terms; may be empty.
    pub meaning: String,
    /// The value it takes when nothing is said: the scene's own default, or for a text, a colour and the numbers of a look the
    /// value the scene holds (image gives no other, and the value it was built with is not kept). [`Value::Hole`] for a picture.
    pub default: Value,
    /// The unit of a number: `px`, `fraction`, `degrees`, `ratio`, `cycles`, `count`...; none for the other kinds.
    pub unit: Option<String>,
    /// The lowest value of a number, inclusive; for a cyclic one, where its turn starts.
    pub min: Option<f64>,
    /// The highest value of a number, inclusive; for a cyclic one, where its turn ends.
    pub max: Option<f64>,
    /// A direction: any value is accepted and wrapped into the turn.
    pub cyclic: bool,
    /// The names of a choice, in order; empty for the other kinds.
    pub choices: Vec<String>,
    /// Whether the value may be left out: a picture or a map.
    pub nullable: bool,
    /// The fewest characters of a text.
    pub min_length: Option<usize>,
    /// The value the scene holds now.
    pub value: Value,
}

impl Parameter {
    fn of(address: &str, kind: Kind, meaning: &str, default: Value, value: Value) -> Self {
        Self {
            address: address.to_owned(),
            kind,
            meaning: meaning.to_owned(),
            default,
            unit: None,
            min: None,
            max: None,
            cyclic: false,
            choices: Vec::new(),
            nullable: false,
            min_length: None,
            value,
        }
    }

    /// A number in `unit`, `range` inclusive; `whole` makes it an int.
    pub(crate) fn number(
        address: &str,
        meaning: &str,
        unit: &str,
        range: (f64, f64),
        (whole, cyclic): (bool, bool),
        default: f64,
        value: f64,
    ) -> Self {
        let kind = if whole { Kind::Int } else { Kind::Float };
        let mut parameter = Self::of(address, kind, meaning, Value::Number(default), Value::Number(value));
        parameter.unit = Some(unit.to_owned());
        parameter.min = Some(range.0);
        parameter.max = Some(range.1);
        parameter.cyclic = cyclic;
        parameter
    }

    /// A switch.
    pub(crate) fn flag(address: &str, meaning: &str, default: bool, value: bool) -> Self {
        Self::of(address, Kind::Bool, meaning, Value::Bool(default), Value::Bool(value))
    }

    /// One of `choices`.
    pub(crate) fn choice(address: &str, meaning: &str, choices: &[&str], default: &str, value: &str) -> Self {
        let mut parameter = Self::of(address, Kind::Choice, meaning, default.into(), value.into());
        parameter.choices = choices.iter().map(|name| (*name).to_owned()).collect();
        parameter
    }

    /// A colour, written in lower case.
    pub(crate) fn color(address: &str, meaning: &str, color: &str) -> Self {
        let color = color.to_ascii_lowercase();
        Self::of(
            address,
            Kind::Color,
            meaning,
            color.as_str().into(),
            color.as_str().into(),
        )
    }

    /// A text of at least one character.
    pub(crate) fn text(address: &str, meaning: &str, content: &str) -> Self {
        let mut parameter = Self::of(address, Kind::Text, meaning, content.into(), content.into());
        parameter.min_length = Some(1);
        parameter
    }

    /// A picture or a map: none by default.
    pub(crate) fn picture(address: &str, meaning: &str, value: Value) -> Self {
        let mut parameter = Self::of(address, Kind::Picture, meaning, Value::Hole, value);
        parameter.nullable = true;
        parameter
    }

    /// This parameter at `<head>.<address>`.
    pub(crate) fn under(mut self, head: &str) -> Self {
        self.address = format!("{head}.{}", self.address);
        self
    }
}

/// The values to give a scene, by address: `Values::new().set("title.text", "Pilot").set("band.opacity", 0.5)`. Setting an
/// address again replaces its value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    entries: Vec<(String, Value)>,
}

impl Values {
    /// No values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These values with `address` set to `value` (a number, a bool, text, a [`Raster`], a [`Map`], a path, or [`Value::Hole`]).
    #[must_use]
    pub fn set(mut self, address: impl Into<String>, value: impl Into<Value>) -> Self {
        let address = address.into();
        let value = value.into();
        match self.entries.iter_mut().find(|(known, _)| *known == address) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((address, value)),
        }
        self
    }

    /// Each address with its value, in the order they were first set.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(address, value)| (address.as_str(), value))
    }
}

/// `head` and the number of the call that made it: `look`, `look#2`.
pub(crate) fn numbered(head: &str, number: usize) -> String {
    if number == 1 {
        head.to_owned()
    } else {
        format!("{head}#{number}")
    }
}

/// `address` split at its head: the name before the first `.`, and what follows. `look#2.grade.exposure` is (`look`, 2,
/// `grade.exposure`); `opacity` is (`opacity`, 1, "").
fn split(address: &str) -> (&str, usize, &str) {
    let (head, rest) = address.split_once('.').unwrap_or((address, ""));
    match head.split_once('#') {
        Some((name, number)) => (name, number.parse().unwrap_or(0), rest),
        None => (head, 1, rest),
    }
}

/// `text` shortened for a message: an address of a megabyte is not printed whole.
fn clip(text: &str) -> String {
    if text.chars().count() <= 60 {
        text.to_owned()
    } else {
        format!(
            "{}... ({} characters)",
            text.chars().take(50).collect::<String>(),
            text.chars().count()
        )
    }
}

/// The refusal of `address`, whose `word` (the whole of it, or its layer) is none of `known`: the list of them and the nearest
/// (a list of more than `LISTED` is cut).
fn unknown(address: &str, word: &str, what: &str, known: &[&str]) -> Error {
    const LISTED: usize = 40;
    // a word of hundreds of characters is no slip of a name: no edit distance is worked out for it
    let near = if word.len() <= 200 {
        nearest(word, known.iter().copied())
    } else {
        Vec::new()
    };
    let listed = known.iter().take(LISTED).copied().collect::<Vec<_>>().join(", ");
    let more = known.len().saturating_sub(LISTED);
    let cut = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    let hint = match near.as_slice() {
        [] => String::new(),
        [one] => format!(" (did you mean {one}?)"),
        several => format!(" (did you mean one of {}?)", several.join(", ")),
    };
    Error::invalid(&clip(address), format!("no such {what}: {listed}{cut}{hint}"))
}

impl Layer {
    /// The name this layer has in a scene at `place` (from 1): its own, or `layer<place>`.
    pub(crate) fn called(&self, place: usize) -> String {
        self.name.clone().unwrap_or_else(|| format!("layer{place}"))
    }

    /// This layer's parameters by address relative to the layer, in order: its own fields, its source's, its looks', its
    /// masks'. A mask layer has no blend and no light (a mask is not joined to what is below). A layer under `perspective` has
    /// no placement fields (its corners are not a parameter, and setting `dx` would replace them).
    pub(crate) fn parameters(&self, mask: bool) -> Vec<Parameter> {
        let mut found = vec![Parameter::number(
            "opacity",
            "how much of the layer shows",
            "fraction",
            FRACTION.bounds(),
            (false, false),
            1.0,
            f64::from(self.join.opacity),
        )];
        if !mask {
            found.push(Parameter::choice(
                "blend",
                "how the layer joins what is below it",
                &Blend::names(),
                "normal",
                &self.join.blend.to_string(),
            ));
            found.push(Parameter::flag(
                "light",
                "the layer's alpha is made from the brightness of its colour",
                false,
                self.join.light,
            ));
        }
        match &self.base {
            Base::Body(body) => {
                if let Placement::Place(placed) = body.placement {
                    found.extend(placed.parameters());
                }
                found.extend(body.source.parameters());
                found.extend(looks(body.finish.looks()));
            }
            Base::Of(reference) => {
                found.extend(reference.place.parameters());
                found.extend(looks(&reference.looks));
            }
        }
        for canvas in &self.join.masks {
            let head = label(canvas.number());
            found.extend(
                canvas
                    .layer()
                    .parameters(true)
                    .into_iter()
                    .map(|parameter| parameter.under(&head)),
            );
        }
        found
    }

    /// This layer with each of `settings` (an address relative to the layer, and a value) set, or the first refusal: the
    /// layer is unchanged unless every one is accepted. An address the layer does not list is refused with the nearest of
    /// them. The settings of one look, one mask layer and the source are each given to it together, so that a rule between
    /// two numbers set at once holds for the set.
    pub(crate) fn with_values(&self, mask: bool, settings: &[(&str, &Value)]) -> Result<Self, Error> {
        let listed = self.parameters(mask);
        let known: HashSet<&str> = listed.iter().map(|parameter| parameter.address.as_str()).collect();
        let mut layer = self.clone();
        let mut source: Vec<(&str, &Value)> = Vec::new();
        let mut looks: BTreeMap<usize, Vec<(&str, f64)>> = BTreeMap::new();
        let mut masks: BTreeMap<usize, Vec<(&str, &Value)>> = BTreeMap::new();
        for &(address, value) in settings {
            if !known.contains(address) {
                let names: Vec<&str> = listed.iter().map(|parameter| parameter.address.as_str()).collect();
                return Err(unknown(
                    address,
                    address,
                    "address in this layer; its addresses are",
                    &names,
                ));
            }
            let (head, number, rest) = split(address);
            match head {
                "opacity" => layer = layer.opacity(value.number(address)?)?,
                "blend" => layer.join.blend = value.text(address)?.parse()?,
                "light" => layer.join.light = value.flag(address)?,
                "dx" | "dy" | "scale" | "aspect" | "rotate" | "skew_x" | "skew_y" | "flip_x" | "flip_y" => {
                    match &mut layer.base {
                        Base::Body(body) => {
                            if let Placement::Place(placed) = body.placement {
                                body.placement = Placement::Place(placed.with_field(address, value)?);
                            }
                        }
                        Base::Of(reference) => reference.place = reference.place.with_field(address, value)?,
                    }
                }
                "look" => looks.entry(number).or_default().push((rest, value.number(address)?)),
                "mask" => masks.entry(number).or_default().push((rest, value)),
                // a source's: `picture`, `map.<name>`, `text`, `color`, `drawing.<param>`, `scene.<address>`
                _ => source.push((address, value)),
            }
        }
        for (number, set) in looks {
            let changed = match &mut layer.base {
                Base::Body(body) => body.finish.change_look(number - 1, &set),
                Base::Of(reference) => reference.looks[number - 1].with_values(&set).map(|look| {
                    reference.looks[number - 1] = look;
                }),
            };
            // image's message names the numbers it refuses; the address is the one number, or the look
            let at = match set.as_slice() {
                [(only, _)] => format!("{}.{only}", numbered("look", number)),
                _ => numbered("look", number),
            };
            changed.map_err(|error| error.about(&at))?;
        }
        if !source.is_empty()
            && let Base::Body(body) = &mut layer.base
        {
            body.source = body.source.with_values(&source)?;
        }
        for canvas in &mut layer.join.masks {
            if let Some(set) = masks.remove(&canvas.number()) {
                let head = label(canvas.number());
                *canvas = canvas
                    .remade(|inner| inner.with_values(true, &set))
                    .map_err(|error| error.in_layer(&head))?;
            }
        }
        Ok(layer)
    }
}

/// The parameters of `looks` (`look`, `look#2`...).
fn looks(looks: &[crate::source::picture::Look]) -> Vec<Parameter> {
    looks
        .iter()
        .enumerate()
        .flat_map(|(at, look)| {
            let head = numbered("look", at + 1);
            look.parameters()
                .into_iter()
                .map(move |parameter| parameter.under(&head))
        })
        .collect()
}

impl Scene {
    /// Every parameter of the scene, layer by layer, bottom first: the layer's own fields, its source's, its looks' and its
    /// masks', each at the address `<layer>.<field>` (see the module's table). Each carries the value the scene holds.
    #[must_use]
    pub fn parameters(&self) -> Vec<Parameter> {
        self.layers()
            .iter()
            .enumerate()
            .flat_map(|(at, layer)| {
                let head = layer.called(at + 1);
                layer
                    .parameters(false)
                    .into_iter()
                    .map(move |parameter| parameter.under(&head))
            })
            .collect()
    }

    /// This scene with the parameters at the addresses of `values` set to the values: a new scene (this one is unchanged) that
    /// shares what did not change. Nothing is set unless every value is accepted.
    ///
    /// A number outside its range is refused, never clamped (a cyclic one wraps into its turn); a whole-number parameter takes
    /// `3.0`; a picture or a map takes a [`Raster`] or [`Map`], a path, or [`Value::Hole`]; an address in a look or a drawing is
    /// checked by the sibling crate's own rules, with its message; an address in a scene a layer draws changes that scene as
    /// a value, and every reference is found again.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming the address, with the rule and the value: an address no parameter has (the message lists the
    /// addresses there are and the nearest), a value of the wrong kind or outside its range, a blend that is no mode, a colour
    /// that is no colour, a text of no character.
    pub fn with_values(&self, values: &Values) -> Result<Self, Error> {
        let names: HashMap<String, usize> = self
            .layers()
            .iter()
            .enumerate()
            .map(|(at, layer)| (layer.called(at + 1), at))
            .collect();
        let mut by_layer: BTreeMap<usize, Vec<(&str, &Value)>> = BTreeMap::new();
        for (address, value) in values.iter() {
            let Some((name, rest)) = address.split_once('.') else {
                return Err(Error::invalid(
                    &clip(address),
                    "an address is `<layer>.<field>`, as `scene.parameters()` lists them",
                ));
            };
            let Some(&at) = names.get(name) else {
                let mut layers: Vec<&str> = names.keys().map(String::as_str).collect();
                layers.sort_unstable();
                return Err(unknown(address, name, "layer; the layers are", &layers));
            };
            by_layer.entry(at).or_default().push((rest, value));
        }
        let mut layers = self.layers().to_vec();
        for (at, settings) in by_layer {
            let name = layers[at].called(at + 1);
            layers[at] = layers[at]
                .with_values(false, &settings)
                .map_err(|error| error.in_layer(&name))?;
        }
        Self::new(self.width(), self.height())?.add(layers)
    }
}
