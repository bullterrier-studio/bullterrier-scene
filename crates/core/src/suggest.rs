//! The one rule that names what a mistyped word may have meant: a blend mode, a layer, an address.

/// The number of single-character edits (insert, delete, change) that turn `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(x != *y)).min(above + 1).min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The `names` to suggest for `word`, which is none of them, by one rule. The names that begin with what was typed (an
/// abbreviation); failing that the names fewest edits away, if that is within a third of the letters typed and at least one (a
/// typo costs one or two edits, and a longer word may have more slips); failing that none, so that a word that is nothing like
/// a name gets no suggestion. The caller brings `word` to the spelling of the names (case, separators).
pub(crate) fn nearest<'a>(word: &str, names: impl Iterator<Item = &'a str> + Clone) -> Vec<&'a str> {
    if word.is_empty() {
        return Vec::new();
    }
    let begins: Vec<&str> = names.clone().filter(|name| name.starts_with(word)).collect();
    if !begins.is_empty() {
        return begins;
    }
    let nearest = names
        .clone()
        .map(|name| distance(word, name))
        .fold(usize::MAX, usize::min);
    if nearest > (word.chars().count() / 3).max(1) {
        return Vec::new();
    }
    names.filter(|name| distance(word, name) == nearest).collect()
}
