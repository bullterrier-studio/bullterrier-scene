//! The table of layer kinds by name: what a saved scene's `{"make": {"kind": ...}}` is read through, and what Python's
//! `Layer.make` asks. A plugin registers a [`LayerKindFactory`] in a [`LayerKinds`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::document::SOURCES;
use crate::suggest::nearest;
use crate::{Error, Layer, LayerKind};

/// The longest name of a kind: a name is part of every saved scene, and of every message that names it.
const MAX_NAME: usize = 64;

/// Builds the [`LayerKind`] a saved scene's `options` describe, so that JSON can recreate a layer by name.
pub trait LayerKindFactory: Send + Sync {
    /// The name the kind is registered, saved and asked for under: `^[a-z0-9][a-z0-9_.-]*$`, at most 64 characters, and
    /// none of the keys that say what a layer draws (`picture`, `drawing`, `text`, `fill`, `scene`, `of`, `make`).
    fn name(&self) -> &str;

    /// Who provides the kind. Registering a kind under a taken name replaces the earlier one only when both have the same
    /// identity (a module loaded again); otherwise it is refused.
    fn identity(&self) -> &str;

    /// The kind with `options` (any JSON, an object by convention). Its [`LayerKind::name`] is this factory's name, and
    /// reading its [`LayerKind::options`] back into `create` gives an equal kind.
    ///
    /// # Errors
    /// [`Error::Invalid`] for options the kind refuses; the message says the rule and the value.
    fn create(&self, options: &Value) -> Result<Arc<dyn LayerKind>, Error>;
}

/// Layer kinds by name. A registry is a value, not a global: [`LayerKinds::new`] is empty and a clone is cheap (the
/// factories are shared). The scene holds none: a layer made through one remembers its kind and options, and
/// [`Scene::from_json_with`](crate::Scene::from_json_with) reads a document through one.
///
/// Getting in is checked once, in [`LayerKinds::register`]; each refusal is an [`Error::Invalid`] naming the kind and the rule:
/// - the name matches `^[a-z0-9][a-z0-9_.-]*$` and has at most 64 characters;
/// - the name is not one of the keys that say what a layer draws;
/// - a name taken by another kind is refused, unless both have the same [`LayerKindFactory::identity`]: then the new one
///   replaces the old (a module loaded again).
#[derive(Clone, Default)]
pub struct LayerKinds {
    factories: BTreeMap<String, Arc<dyn LayerKindFactory>>,
}

impl fmt::Debug for LayerKinds {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.factories.keys()).finish()
    }
}

/// `name` for a message: a name of a megabyte is cut.
fn shown(name: &str) -> String {
    match name.char_indices().nth(40) {
        Some((end, _)) => format!("{}... ({} characters)", &name[..end], name.chars().count()),
        None => name.to_owned(),
    }
}

impl LayerKinds {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `factory` under its name, or replaces the one of the same identity.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming the kind and the rule it breaks (see [`LayerKinds`]).
    pub fn register(&mut self, factory: Arc<dyn LayerKindFactory>) -> Result<(), Error> {
        let name = factory.name();
        let refuse = |rule: &str| Err(Error::invalid("kind", format!("{}: {rule}", shown(name))));
        let mut characters = name.chars();
        let starts = characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
        let rest = characters.all(|next| next.is_ascii_lowercase() || next.is_ascii_digit() || "_.-".contains(next));
        if !(starts && rest && name.len() <= MAX_NAME) {
            return refuse(&format!(
                "a kind's name matches ^[a-z0-9][a-z0-9_.-]*$ and has at most {MAX_NAME} characters"
            ));
        }
        if SOURCES.contains(&name) {
            return refuse("the name is a key that says what a layer draws, and cannot be taken");
        }
        if let Some(taken) = self.factories.get(name)
            && taken.identity() != factory.identity()
        {
            return refuse(&format!(
                "the name is taken by '{}'; the same identity may replace it, another may not",
                shown(taken.identity())
            ));
        }
        self.factories.insert(name.to_owned(), factory);
        Ok(())
    }

    /// The factory registered under `name`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `kind`, listing the registered names and the nearest to `name`.
    pub fn get(&self, name: &str) -> Result<&dyn LayerKindFactory, Error> {
        self.factories.get(name).map(|factory| &**factory).ok_or_else(|| {
            let near = nearest(name, self.factories.keys().map(String::as_str));
            let hint = match near.as_slice() {
                [] => String::new(),
                [one] => format!(" (did you mean {one}?)"),
                several => format!(" (did you mean one of {}?)", several.join(", ")),
            };
            let known = if self.factories.is_empty() {
                "none".to_owned()
            } else {
                self.names().join(", ")
            };
            Error::invalid(
                "kind",
                format!("unknown layer kind {:?}; the kinds are: {known}{hint}", shown(name)),
            )
        })
    }

    /// Every registered name, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// A layer of the kind `name` with `options`: like [`Layer::make`], and a scene with it has a JSON form, `{"make":
    /// {"kind": name, "options": ...}}`, that [`Scene::from_json_with`](crate::Scene::from_json_with) reads back through
    /// a registry that holds the kind.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `kind` for a name that is not registered (listing the names), naming `options` for options
    /// the factory refuses, and naming `kind` for a factory that makes a kind of another name.
    pub fn make(&self, name: &str, options: &Value) -> Result<Layer, Error> {
        let kind = self
            .get(name)?
            .create(options)
            .map_err(|error| error.about("options"))?;
        if kind.name() != name {
            return Err(Error::invalid(
                "kind",
                format!(
                    "the factory of {:?} made a kind named {:?}",
                    shown(name),
                    shown(kind.name())
                ),
            ));
        }
        Ok(Layer::made_by_name(kind))
    }
}
