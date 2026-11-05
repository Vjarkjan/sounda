use crate::Result;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

pub const ROOT: &str = "sd:/";
pub const MAX_ENTRIES: usize = 2048;
pub const MAX_PATH: usize = 1023;
pub const VISIBLE_ROWS: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub directory: bool,
}

impl Entry {
    /// The browser only stores folders and M4A files. Keep the original name
    /// for opening the file and borrow its label instead of allocating one on
    /// every selection change.
    pub fn display_name(&self) -> &str {
        if self.directory {
            &self.name
        } else {
            self.name
                .strip_suffix(".m4a")
                .or_else(|| {
                    self.name
                        .rsplit_once('.')
                        .filter(|(_, extension)| extension.eq_ignore_ascii_case("m4a"))
                        .map(|(stem, _)| stem)
                })
                .unwrap_or(&self.name)
        }
    }
}

pub fn is_music(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("m4a"))
}

pub fn join(directory: &str, name: &str) -> Result<String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
    {
        return Err("Invalid SD filename");
    }
    let mut result = directory.to_string();
    if !result.ends_with('/') {
        result.push('/');
    }
    result.push_str(name);
    if result.len() > MAX_PATH {
        return Err("SD path is too long");
    }
    Ok(result)
}

pub struct Browser {
    pub path: String,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub first: usize,
    pub limited: bool,
    visible: Vec<usize>,
    names: Vec<Vec<char>>,
    query: String,
    search: Option<crate::search::Search>,
    visible_rows: usize,
}
impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}
impl Browser {
    pub fn new() -> Self {
        Self {
            path: ROOT.into(),
            entries: Vec::new(),
            selected: 0,
            first: 0,
            limited: false,
            visible: Vec::new(),
            names: Vec::new(),
            query: String::new(),
            search: None,
            visible_rows: VISIBLE_ROWS,
        }
    }
    pub fn replace(&mut self, path: String, mut entries: Vec<Entry>, limited: bool) {
        entries.sort_unstable_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| {
                    a.name
                        .bytes()
                        .map(|b| b.to_ascii_lowercase())
                        .cmp(b.name.bytes().map(|b| b.to_ascii_lowercase()))
                })
                .then_with(|| a.name.cmp(&b.name))
        });
        self.replace_ordered(path, entries, limited);
    }
    /// Queue views retain user order instead of alphabetically sorting it.
    pub fn replace_ordered(&mut self, path: String, entries: Vec<Entry>, limited: bool) {
        self.path = path;
        self.names = entries
            .iter()
            .map(|entry| crate::unicode::normalize(entry.display_name()))
            .collect();
        self.visible = (0..entries.len()).collect();
        self.entries = entries;
        self.query.clear();
        self.search = None;
        self.limited = limited;
        self.selected = 0;
        self.first = 0;
    }
    pub fn move_by(&mut self, amount: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(amount)
            .min(self.len().saturating_sub(1));
        if self.selected < self.first {
            self.first = self.selected;
        }
        if self.selected >= self.first + self.visible_rows {
            self.first = self.selected + 1 - self.visible_rows;
        }
        self.first = self.first.min(self.len().saturating_sub(self.visible_rows));
    }
    pub fn set_visible_rows(&mut self, rows: usize) {
        self.visible_rows = rows.max(1);
        self.move_by(0);
    }
    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }
    pub fn scroll_to(&mut self, fraction: u16) {
        let offset = self.selected.saturating_sub(self.first);
        self.first = (self.len().saturating_sub(self.visible_rows) as u64 * fraction as u64
            / u16::MAX as u64) as usize;
        self.selected =
            (self.first + offset.min(self.visible_rows - 1)).min(self.len().saturating_sub(1));
    }
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.visible.get(index).and_then(|i| self.entries.get(*i))
    }
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entry(self.selected)
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
        self.first = 0;
        if self.query.is_empty() {
            self.visible = (0..self.entries.len()).collect();
            self.search = None;
        } else {
            self.search = Some(crate::search::Search::new(&self.query));
        }
    }
    pub fn search_step(&mut self) -> bool {
        if let Some(search) = &mut self.search
            && search.step(&self.names)
        {
            self.visible = self.search.take().unwrap().finish();
            return true;
        }
        false
    }
    pub fn parent_path(&self) -> String {
        if self.path == ROOT {
            return ROOT.into();
        }
        let path = self.path.trim_end_matches('/');
        match path.rfind('/') {
            Some(i) if i >= ROOT.len() => path[..i].into(),
            _ => ROOT.into(),
        }
    }
    pub fn music(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| !e.directory && is_music(&e.name))
            .map(|e| e.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_is_incremental_and_preserves_full_unicode_paths_and_folder_queue() {
        let mut browser = Browser::new();
        let mut entries: Vec<Entry> = (0..200)
            .map(|i| Entry {
                name: alloc::format!("Other {i:03}.m4a"),
                directory: false,
            })
            .collect();
        entries.push(Entry {
            name: "Beyoncé - Żółć 日本語.m4a".into(),
            directory: false,
        });
        browser.replace("sd:/音楽".into(), entries, false);
        browser.set_query("beyonce zolc".into());
        assert!(browser.searching());
        assert!(!browser.search_step());
        let mut batches = 1;
        while browser.searching() {
            browser.search_step();
            batches += 1;
        }
        assert_eq!(batches, 13);
        assert_eq!(browser.len(), 1);
        assert_eq!(
            join(&browser.path, &browser.selected_entry().unwrap().name).unwrap(),
            "sd:/音楽/Beyoncé - Żółć 日本語.m4a"
        );
        assert_eq!(browser.music().len(), 201);
        browser.set_query(String::new());
        assert_eq!(browser.len(), 201);
    }
    #[test]
    fn changing_screen_capacity_and_dragging_preserves_a_valid_selection() {
        let mut browser = Browser::new();
        browser.replace(
            "sd:/Music".into(),
            (0..200)
                .map(|i| Entry {
                    name: alloc::format!("{i:03}.m4a"),
                    directory: false,
                })
                .collect(),
            false,
        );
        browser.set_visible_rows(14);
        browser.move_by(199);
        assert_eq!(browser.first, 186);
        browser.set_visible_rows(6);
        assert_eq!(browser.selected, 199);
        assert_eq!(browser.first, 194);
        browser.set_visible_rows(14);
        assert_eq!(browser.first, 186);
        browser.scroll_to(0);
        assert_eq!(browser.first, 0);
        assert_eq!(browser.selected, 13);
        browser.scroll_to(u16::MAX);
        assert_eq!(browser.first, 186);
        assert_eq!(browser.selected, 199);
        browser.replace("sd:/Empty".into(), Vec::new(), false);
        browser.scroll_to(u16::MAX);
        assert_eq!((browser.first, browser.selected), (0, 0));
    }
    #[test]
    fn paths_cannot_escape_the_sd_root() {
        let mut b = Browser::new();
        b.path = "sd:/Music/Album".into();
        assert_eq!(b.parent_path(), "sd:/Music");
        b.path = b.parent_path();
        assert_eq!(b.parent_path(), ROOT);
        assert_eq!(join(ROOT, "A song.M4A"), Ok("sd:/A song.M4A".into()));
        assert!(join(ROOT, "../x").is_err());
        assert!(is_music("A.M4A"));
        assert!(!is_music("a.m4a.backup"));
    }

    #[test]
    fn large_directory_keeps_folders_first_and_selection_in_view() {
        let mut entries: Vec<Entry> = (0..200)
            .rev()
            .map(|i| Entry {
                name: alloc::format!("Track {i:03}.m4a"),
                directory: false,
            })
            .collect();
        entries.push(Entry {
            name: "Album".into(),
            directory: true,
        });
        let mut browser = Browser::new();
        browser.replace("sd:/Music".into(), entries, false);
        assert_eq!(browser.entries.len(), 201);
        assert_eq!(browser.entries[0].name, "Album");
        assert_eq!(browser.entries[1].display_name(), "Track 000");
        assert_eq!(browser.entries[200].display_name(), "Track 199");
        for _ in 0..220 {
            browser.move_by(1);
            assert!(browser.selected >= browser.first);
            assert!(browser.selected < browser.first + VISIBLE_ROWS);
        }
        assert_eq!(browser.selected, 200);
        assert_eq!(browser.first, 201 - VISIBLE_ROWS);
        assert_eq!(browser.music().len(), 200);
    }

    #[test]
    fn display_labels_preserve_utf8_and_original_paths() {
        let file = Entry {
            name: "Canción.M4A".into(),
            directory: false,
        };
        assert_eq!(file.display_name(), "Canción");
        assert_eq!(
            join("sd:/Music", &file.name),
            Ok("sd:/Music/Canción.M4A".into())
        );
        let directory = Entry {
            name: "Album.m4a".into(),
            directory: true,
        };
        assert_eq!(directory.display_name(), "Album.m4a");
    }
}
