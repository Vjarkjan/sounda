//! Prebuilt 8x16/16x16 glyphs. No font engine or SD glyph access on the DSi.
const RECORD: usize = 40;
static DATA: &[u8] = include_bytes!("../assets/fonts/unicode.bin");

#[derive(Clone, Copy)]
pub struct Glyph {
    index: usize,
}
impl Glyph {
    pub fn width(self) -> usize {
        DATA[self.index * RECORD + 4] as usize
    }
    pub fn row(self, row: usize) -> u16 {
        let offset = self.index * RECORD + 8 + row * 2;
        u16::from_le_bytes([DATA[offset], DATA[offset + 1]])
    }
}

fn find(c: char) -> Option<Glyph> {
    let point = c as u32;
    let mut low = 0;
    let mut high = DATA.len() / RECORD;
    while low < high {
        let mid = (low + high) / 2;
        let p = mid * RECORD;
        let key = u32::from_le_bytes([DATA[p], DATA[p + 1], DATA[p + 2], DATA[p + 3]]);
        match key.cmp(&point) {
            core::cmp::Ordering::Less => low = mid + 1,
            core::cmp::Ordering::Greater => high = mid,
            core::cmp::Ordering::Equal => return Some(Glyph { index: mid }),
        }
    }
    None
}
pub fn glyph(c: char) -> Glyph {
    find(c)
        .or_else(|| find('□'))
        .expect("font replacement glyph")
}
pub fn width(text: &str, limit: usize) -> usize {
    crate::unicode::characters(text)
        .map(|c| glyph(c).width())
        .scan(0, |total, w| {
            *total += w;
            Some(*total)
        })
        .take_while(|total| *total <= limit)
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_scripts_have_distinct_real_glyphs() {
        for c in "éëŁąćЖёЇПривет日本語かなカナ中国中文音樂".chars() {
            let g = find(c).expect("requested script glyph");
            assert!((0..16).any(|row| g.row(row) != 0));
            assert_ne!(g.index, glyph('□').index);
        }
        assert_eq!(glyph('中').width(), 16);
        assert_eq!(glyph('e').width(), 8);
        assert_eq!(width("e\u{301}中", 100), 24);
    }
}
