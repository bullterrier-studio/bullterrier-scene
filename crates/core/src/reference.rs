//! References: a layer that draws another layer of its scene again, and how a scene resolves them.
//!
//! A [`Reference`] is what `Layer::of` keeps: the name, and what the reference adds to the body it inherits (its looks, its
//! own-map masks, an extra place). `resolve` is called by `Scene::add` and turns each reference of the layers it adds, those
//! in the stack and those in mask layers, into the body it draws, so that nothing at render time knows a reference exists.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::label;
use crate::finish::MapMask;
use crate::layer::{Base, Body};
use crate::placement::Placed;
use crate::source::picture::Look;
use crate::{Error, Layer};

/// A reference to the layer called `name` of the scene, with what it adds to that layer's body.
#[derive(Debug, Clone)]
pub(crate) struct Reference {
    /// The layer it names.
    pub(crate) name: String,
    /// Its own looks, in call order, each checked: run after the named layer's.
    pub(crate) looks: Vec<Value>,
    /// Its own masks by a map of the picture it inherits, by the number of the `mask` call.
    pub(crate) masks: Vec<MapMask>,
    /// Its extra place, applied after the named layer's.
    pub(crate) place: Placed,
}

impl Reference {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            looks: Vec::new(),
            masks: Vec::new(),
            place: Placed::default(),
        }
    }

    /// The body this reference draws: that of `target` (the layer it names, itself resolved), then this reference's looks
    /// and own-map masks after the target's, and its extra place after the target's own and the extras of the chain that led
    /// to it.
    ///
    /// # Errors
    /// What image refuses of a look, and a mask by a map that is not one of the target's picture.
    fn body(&self, target: &Body) -> Result<Body, Error> {
        let mut body = target.clone();
        for look in &self.looks {
            body.takes_look()?;
            body.finish.add_look(Look::new(look)?);
        }
        for mask in &self.masks {
            let number = mask.number;
            body.mask_by_map(mask.clone())
                .map_err(|error| error.in_layer(&label(number)))?;
        }
        if self.place != Placed::default() {
            body.extras.push(self.place);
        }
        body.chain += 1;
        Ok(body)
    }
}

/// The names `layer` refers to anywhere in it: its own, and those of its mask layers, theirs in turn. The walk over mask
/// layers is as deep as they nest, at most `MAX_DEPTH`.
fn references(layer: &Layer) -> Vec<&str> {
    let mut names = Vec::new();
    if let Base::Of(reference) = &layer.base {
        names.push(reference.name.as_str());
    }
    for mask in &layer.join.masks {
        names.extend(references(mask.layer()));
    }
    names
}

/// `layer` with each reference in it, its own and those of its mask layers, replaced by the body it draws, and its cost
/// counted again with the references it leads through. `target` gives the resolved body of the layer a name is.
fn resolved(layer: &Layer, target: &impl Fn(&str) -> Result<Body, Error>) -> Result<Layer, Error> {
    let mut made = layer.clone();
    if let Base::Of(reference) = &layer.base {
        made.base = Base::Body(reference.body(&target(&reference.name)?)?);
    }
    let masks = layer
        .join
        .masks
        .iter()
        .map(|mask| {
            mask.remade(|inner| resolved(inner, target))
                .map_err(|error| error.in_layer(&label(mask.number())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (own, chain) = {
        let body = made.drawable()?;
        (body.source.budget(), body.chain)
    };
    made.join.budget = masks
        .iter()
        .fold(own, |budget, mask| budget.needing(mask.layer().budget()))
        .through(chain);
    made.join.masks = masks;
    Ok(made)
}

/// The layers of `stack` after the first `drawn.len()`, each as it is drawn: references resolved. `drawn` holds the earlier
/// layers so, `index` gives the place in `stack` of each name.
///
/// A reference names a layer of the stack, before it or after it (the stack order is what is drawn first; the order here is
/// the order of dependence), and a chain of references that leads back to itself, also through a mask layer, is refused. The
/// walks are iterative.
///
/// # Errors
/// [`Error::Invalid`] naming `<layer>.of` for a name that is no layer of the stack and for a chain that leads back to
/// itself (the message names the cycle); for what a reference's looks and masks need of the layer it names, `<layer>.look`
/// and `<layer>.mask`.
pub(crate) fn resolve(stack: &[Layer], drawn: &[Layer], index: &HashMap<String, usize>) -> Result<Vec<Layer>, Error> {
    let first = drawn.len();
    let name_of = |at: usize| stack[at].name.clone().unwrap_or_else(|| format!("layer{}", at + 1));
    // who each new layer refers to
    let mut edges: Vec<Vec<usize>> = Vec::with_capacity(stack.len() - first);
    for (at, layer) in stack.iter().enumerate().skip(first) {
        let mut found = Vec::new();
        for name in references(layer) {
            let Some(&place) = index.get(name) else {
                return Err(Error::invalid(
                    "of",
                    format!(
                        "{name:?} is no layer of this scene: a reference names a layer added before it, or in the same add call"
                    ),
                )
                .in_layer(&name_of(at)));
            };
            found.push(place);
        }
        edges.push(found);
    }
    // depth first, iteratively: a layer is resolved after the layers it refers to
    let mut order = Vec::with_capacity(edges.len());
    let mut state = vec![0_u8; edges.len()]; // 0 not seen, 1 on the path, 2 done
    for root in first..stack.len() {
        if state[root - first] != 0 {
            continue;
        }
        let mut path: Vec<(usize, usize)> = vec![(root, 0)];
        state[root - first] = 1;
        while let Some(&(at, next)) = path.last() {
            if let Some(&to) = edges[at - first].get(next) {
                if let Some(last) = path.last_mut() {
                    last.1 += 1;
                }
                // a layer before `first` is resolved already, and cannot refer to a later one
                if to < first {
                    continue;
                }
                match state[to - first] {
                    0 => {
                        state[to - first] = 1;
                        path.push((to, 0));
                    }
                    1 => {
                        let from = path.iter().position(|&(place, _)| place == to).unwrap_or(0);
                        let mut names: Vec<String> = path[from..].iter().map(|&(place, _)| name_of(place)).collect();
                        names.push(name_of(to));
                        return Err(Error::invalid(
                            "of",
                            format!("references lead back to themselves: {}", names.join(" -> ")),
                        )
                        .in_layer(&name_of(at)));
                    }
                    _ => {}
                }
            } else {
                state[at - first] = 2;
                order.push(at);
                path.pop();
            }
        }
    }
    let mut made: Vec<Option<Layer>> = vec![None; edges.len()];
    for at in order {
        let target = |name: &str| -> Result<Body, Error> {
            let place = index[name];
            let layer = if place < first {
                Some(&drawn[place])
            } else {
                made[place - first].as_ref()
            };
            layer
                .ok_or_else(|| Error::invalid("of", format!("{name:?} was not resolved before it was used")))?
                .drawable()
                .cloned()
        };
        made[at - first] = Some(resolved(&stack[at], &target).map_err(|error| error.in_layer(&name_of(at)))?);
    }
    Ok(made.into_iter().flatten().collect())
}
