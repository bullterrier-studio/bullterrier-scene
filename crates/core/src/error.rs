//! What the core refuses, as one error type the bindings map to the package's exceptions.

use std::fmt;

/// Why a call was refused. New variants may come in a minor release, so a `match` needs a `_` arm.
///
/// A call that can be refused returns this at the call that got the bad value; a scene never defers an error, except what
/// only `render` can know (a file, a size).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A value or an argument the call refuses: the message says the rule and the value. For a layer that is part of a
    /// scene the address starts with the layer's name (`photo.look`).
    Invalid {
        /// The argument or the layer's field the value belongs to.
        address: String,
        /// The rule that was broken, with the value.
        message: String,
    },
    /// A document the call cannot read: a saved scene ([`Scene::from_json`](crate::Scene::from_json)), or a look or a drawing
    /// of a version this scene does not read.
    Format {
        /// The JSON Pointer of the value that is wrong: `/layers/0/opacity`, `/look`, `/drawing`; empty for the whole document.
        pointer: String,
        /// What is wrong, naming the versions.
        message: String,
    },
    /// A file the scene names cannot be read, or is not the file the scene was saved with (its SHA-256 differs).
    Read {
        /// The file.
        path: String,
        /// The layer and what went wrong.
        message: String,
    },
}

/// The name of the `number`th mask call of a layer, from 1 (`mask`, `mask#2`...): the address an error of a mask carries, written
/// once for the masks of a source and the masks of a layer.
pub(crate) fn label(number: usize) -> String {
    if number == 1 {
        "mask".to_owned()
    } else {
        format!("mask#{number}")
    }
}

impl Error {
    pub(crate) fn invalid(address: &str, message: impl Into<String>) -> Self {
        Self::Invalid {
            address: address.to_owned(),
            message: message.into(),
        }
    }

    /// This error with `address` as the field it is about: a value refused where it was built (`fill`) is refused where it
    /// was set (`color`).
    pub(crate) fn about(self, address: &str) -> Self {
        match self {
            Self::Invalid { message, .. } => Self::invalid(address, message),
            other => other,
        }
    }

    /// This error as it concerns the layer called `layer`: its address, or its message, starts with the name.
    pub(crate) fn in_layer(self, layer: &str) -> Self {
        match self {
            Self::Invalid { address, message } => Self::Invalid {
                address: format!("{layer}.{address}"),
                message,
            },
            Self::Format { pointer, message } => Self::Format {
                pointer,
                message: format!("{layer}: {message}"),
            },
            Self::Read { path, message } => Self::Read {
                path,
                message: format!("{layer}: {message}"),
            },
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { address, message } => write!(formatter, "{address}: {message}"),
            Self::Format { pointer, message } if pointer.is_empty() => write!(formatter, "{message}"),
            Self::Format { pointer, message } => write!(formatter, "{pointer}: {message}"),
            Self::Read { path, message } => write!(formatter, "{path}: {message}"),
        }
    }
}

impl std::error::Error for Error {}
