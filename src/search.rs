//! Ranked filename matching. Operates on pre-folded names; no SD reads.
use alloc::vec::Vec;

fn word_score(query: &[char], name: &[char]) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    if name.starts_with(query) {
        return Some(0);
    }
    if let Some(position) = name.windows(query.len()).position(|part| part == query) {
        return Some(20 + position as u32);
    }
    let mut next = 0;
    let mut first = 0;
    let mut last = 0;
    for (position, &c) in name.iter().enumerate() {
        if c == query[next] {
            if next == 0 {
                first = position;
            }
            last = position;
            next += 1;
            if next == query.len() {
                break;
            }
        }
    }
    if next == query.len() {
        return Some(100 + first as u32 + (last + 1 - first - query.len()) as u32 * 3);
    }
    if query.len() < 3 || query.len() > 48 {
        return None;
    }
    let limit = if query.len() >= 6 { 2 } else { 1 };
    name.split(|c| *c == ' ')
        .filter(|word| word.len().abs_diff(query.len()) <= limit)
        .filter_map(|word| distance(query, word, limit))
        .min()
        .map(|n| 200 + n as u32 * 20)
}

/// Bounded Damerau edit distance, including one adjacent transposition.
fn distance(a: &[char], b: &[char], limit: usize) -> Option<usize> {
    if a.len() > 48 || b.len() > 50 {
        return None;
    }
    let mut old = [0usize; 51];
    let mut previous = [0usize; 51];
    let mut current = [0usize; 51];
    for (j, cell) in previous.iter_mut().enumerate().take(b.len() + 1) {
        *cell = j;
    }
    for i in 1..=a.len() {
        current[0] = i;
        let mut minimum = i;
        for j in 1..=b.len() {
            current[j] = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + usize::from(a[i - 1] != b[j - 1]));
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                current[j] = current[j].min(old[j - 2] + 1);
            }
            minimum = minimum.min(current[j]);
        }
        if minimum > limit {
            return None;
        }
        core::mem::swap(&mut old, &mut previous);
        core::mem::swap(&mut previous, &mut current);
    }
    (previous[b.len()] <= limit).then_some(previous[b.len()])
}

pub fn score(query: &[char], name: &[char]) -> Option<u32> {
    query
        .split(|c| *c == ' ')
        .try_fold(0u32, |sum, term| word_score(term, name).map(|n| sum + n))
}

pub struct Search {
    query: Vec<char>,
    next: usize,
    hits: Vec<(u32, usize)>,
}
impl Search {
    pub fn new(query: &str) -> Self {
        Self {
            query: crate::unicode::normalize(query),
            next: 0,
            hits: Vec::new(),
        }
    }
    pub fn step(&mut self, names: &[Vec<char>]) -> bool {
        let end = (self.next + 16).min(names.len());
        for (i, name) in names.iter().enumerate().take(end).skip(self.next) {
            if let Some(score) = score(&self.query, name) {
                self.hits.push((score, i));
            }
        }
        self.next = end;
        self.next == names.len()
    }
    pub fn finish(mut self) -> Vec<usize> {
        self.hits.sort_unstable();
        self.hits.into_iter().map(|(_, i)| i).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::normalize as n;
    #[test]
    fn matches_accents_subsequences_typos_and_original_scripts() {
        for (query, name) in [
            ("beyonce", "Beyoncé Halo"),
            ("zolc", "Żółć"),
            ("beynoce", "Beyoncé"),
            ("beyonxe", "Beyoncé"),
            ("bjrk", "Björk"),
            ("привет", "ПРИВЕТ мир"),
            ("かな", "カナの歌"),
            ("中国", "中国音乐"),
        ] {
            assert!(score(&n(query), &n(name)).is_some(), "{query}: {name}");
        }
        assert!(score(&n("nothing"), &n("Beyoncé")).is_none());
        assert!(score(&n("halo beyonce"), &n("Beyoncé - Halo")).is_some());
        assert!(score(&n("halo"), &n("Halo")) < score(&n("halo"), &n("A Halo")));
    }
}
