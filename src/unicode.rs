//! Case/Latin-diacritic/width folding for search, without modifying filenames.
use crate::unicode_data::{COMPOSE, FOLDS};
use alloc::{string::String, vec::Vec};

pub fn compose(base: char, mark: char) -> Option<char> {
    COMPOSE
        .binary_search_by_key(&(base as u32, mark as u32), |&(a, b, _)| (a, b))
        .ok()
        .and_then(|i| char::from_u32(COMPOSE[i].2))
}

pub fn characters(text: &str) -> impl Iterator<Item = char> + '_ {
    let mut source = text.chars().peekable();
    core::iter::from_fn(move || {
        let mut current = source.next()?;
        while let Some(next) = source
            .peek()
            .copied()
            .and_then(|mark| compose(current, mark))
        {
            current = next;
            source.next();
        }
        Some(current)
    })
}

pub fn normalize(text: &str) -> Vec<char> {
    let mut folded = String::new();
    for c in characters(text) {
        if let Ok(i) = FOLDS.binary_search_by_key(&(c as u32), |&(cp, _)| cp) {
            folded.push_str(FOLDS[i].1);
        } else {
            folded.push(c);
        }
    }
    let mut result = Vec::with_capacity(folded.chars().count());
    for c in characters(&folded) {
        // Remaining Latin combining accents also match their base letter.
        if (0x300..=0x36f).contains(&(c as u32))
            && result
                .last()
                .is_some_and(|last: &char| last.is_ascii_alphabetic())
        {
            continue;
        }
        if c.is_alphanumeric() || c == 'ー' {
            result.push(c);
        } else if result.last().is_some_and(|last| *last != ' ') {
            result.push(' ');
        }
    }
    if result.last() == Some(&' ') {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folds_accents_polish_case_width_and_kana_without_renaming() {
        assert_eq!(normalize("Éë E\u{301}"), normalize("ee e"));
        assert_eq!(
            normalize("Zażółć gęślą jaźń"),
            normalize("ZAZOLC GESLA JAZN")
        );
        assert_eq!(
            normalize("Björk Æther Straße"),
            normalize("bjork aether strasse")
        );
        assert_eq!(normalize("ПРИВЕТ ЁЖ"), normalize("привет ёж"));
        assert_eq!(normalize("ﾐｭｰｼﾞｯｸ"), normalize("ミュージック"));
        assert_eq!(normalize("カ\u{3099}"), normalize("が"));
        assert_eq!(normalize("日本語 中文 音樂"), normalize("日本語 中文 音樂"));
    }
}
