use alloc::vec::Vec;

pub const PAGE_KEYS: usize = 30;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Keyboard {
    #[default]
    Latin,
    Cyrillic,
    Kana,
    Filenames,
}
impl Keyboard {
    pub fn keys(self, filenames: &[char]) -> Vec<char> {
        match self {
            Self::Latin => "abcdefghijklmnopqrstuvwxyz 0123456789-_'".chars().collect(),
            Self::Cyrillic => "абвгдеёжзийклмнопрстуфхцчшщъыьэюя іїєґў0123456789".chars().collect(),
            Self::Kana => "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんがぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽぁぃぅぇぉゃゅょっー ".chars().collect(),
            Self::Filenames => filenames.to_vec(),
        }
    }
}

pub fn filename_characters(names: impl Iterator<Item = impl AsRef<str>>) -> Vec<char> {
    let mut result: Vec<char> = names
        .flat_map(|name| {
            name.as_ref()
                .chars()
                .filter(|c| !c.is_ascii() && c.is_alphanumeric())
                .collect::<Vec<_>>()
        })
        .collect();
    result.sort_unstable();
    result.dedup();
    result
}
