//! Explicit upcoming songs take priority over the automatic folder cursor.
//! Folder paths are only navigation metadata; one media file is opened at a time.
use crate::{
    Result,
    browser::MAX_ENTRIES,
    playlist::{Mode, PREVIOUS_SONGS, Playlist},
};
use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueTarget {
    Top,
    Second,
    Bottom,
}

pub struct Playback {
    pub mode: Mode,
    queue: Vec<String>,
    folder: Vec<String>,
    automatic: Playlist,
    history: Vec<String>,
    history_pos: usize,
    seed: u32,
}
impl Playback {
    pub fn new(seed: u32) -> Self {
        Self {
            mode: Mode::Sequential,
            queue: Vec::new(),
            folder: Vec::new(),
            automatic: Playlist::new(0, 0, seed),
            history: Vec::new(),
            history_pos: 0,
            seed,
        }
    }
    pub fn current(&self) -> Option<&str> {
        self.history.get(self.history_pos).map(String::as_str)
    }
    pub fn queue(&self) -> &[String] {
        &self.queue
    }
    pub fn sequence(&self) -> Option<(usize, usize)> {
        let path = self.current()?;
        self.folder
            .iter()
            .position(|s| s == path)
            .map(|index| (index + 1, self.folder.len()))
    }
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.automatic.set_mode(mode);
    }
    fn record(&mut self, path: String) {
        self.history.truncate(self.history_pos + 1);
        self.history.push(path);
        if self.history.len() > PREVIOUS_SONGS + 1 {
            self.history.remove(0);
        }
        self.history_pos = self.history.len() - 1;
    }
    pub fn play_folder(&mut self, paths: Vec<String>, index: usize) -> bool {
        let Some(path) = paths.get(index).cloned() else {
            return false;
        };
        if paths == self.folder {
            self.automatic.select(index);
        } else {
            self.folder = paths;
            self.automatic = Playlist::new(self.folder.len(), index, self.seed);
            self.automatic.set_mode(self.mode);
        }
        self.record(path);
        true
    }
    pub fn add(&mut self, path: String) -> Result<()> {
        if self.queue.len() == MAX_ENTRIES {
            return Err("Queue is full (2048 songs)");
        }
        self.queue.push(path);
        Ok(())
    }
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.queue.len() || to >= self.queue.len() || from == to {
            return;
        }
        let path = self.queue.remove(from);
        self.queue.insert(to, path);
    }
    pub fn move_to(&mut self, from: usize, target: QueueTarget) -> Option<usize> {
        if from >= self.queue.len() {
            return None;
        }
        let to = match target {
            QueueTarget::Top => 0,
            QueueTarget::Second => 1.min(self.queue.len() - 1),
            QueueTarget::Bottom => self.queue.len() - 1,
        };
        self.move_item(from, to);
        Some(to)
    }
    pub fn remove(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
        }
    }
    pub fn play_queued(&mut self, index: usize) -> bool {
        if index >= self.queue.len() {
            return false;
        }
        let path = self.queue.remove(index);
        self.record(path);
        true
    }
    pub fn start(&mut self) -> bool {
        self.current().is_some() || self.play_queued(0)
    }
    pub fn advance(&mut self, automatic: bool) -> bool {
        if automatic && self.mode == Mode::RepeatOne && self.current().is_some() {
            return true;
        }
        if self.history_pos + 1 < self.history.len() {
            self.history_pos += 1;
            return true;
        }
        if !self.queue.is_empty() {
            return self.play_queued(0);
        }
        if let Some(index) = self.automatic.advance(false) {
            self.record(self.folder[index].clone());
            return true;
        }
        false
    }
    pub fn previous(&mut self) -> bool {
        if self.history_pos > 0 {
            self.history_pos -= 1;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    fn folder() -> Vec<String> {
        (0..200)
            .map(|i| alloc::format!("sd:/Music/{i:03}.m4a"))
            .collect()
    }
    #[test]
    fn folder_playback_leaves_queue_empty_and_added_songs_play_first_in_order() {
        let mut p = Playback::new(42);
        assert!(p.play_folder(folder(), 0));
        assert!(p.queue().is_empty());
        p.add("sd:/Elsewhere/B.m4a".into()).unwrap();
        p.add("sd:/Elsewhere/C.m4a".into()).unwrap();
        assert!(p.advance(true));
        assert_eq!(p.current(), Some("sd:/Elsewhere/B.m4a"));
        assert_eq!(p.queue().len(), 1);
        assert!(p.advance(true));
        assert_eq!(p.current(), Some("sd:/Elsewhere/C.m4a"));
        assert!(p.queue().is_empty());
        assert!(p.advance(true));
        assert_eq!(p.current(), Some("sd:/Music/001.m4a"));
        assert!(p.queue().is_empty());
    }
    #[test]
    fn queue_jumps_remove_and_manual_play_preserve_the_current_song_and_order() {
        let mut p = Playback::new(7);
        p.play_folder(folder(), 0);
        for name in ["a", "b", "c", "d"] {
            p.add(name.into()).unwrap();
        }
        p.move_to(3, QueueTarget::Top);
        assert_eq!(p.queue(), ["d", "a", "b", "c"]);
        p.move_to(3, QueueTarget::Second);
        assert_eq!(p.queue(), ["d", "c", "a", "b"]);
        p.move_to(1, QueueTarget::Bottom);
        assert_eq!(p.queue(), ["d", "a", "b", "c"]);
        p.remove(1);
        assert_eq!(p.queue(), ["d", "b", "c"]);
        assert_eq!(p.current(), Some("sd:/Music/000.m4a"));
        assert!(p.play_queued(1));
        assert_eq!(p.current(), Some("b"));
        assert_eq!(p.queue(), ["d", "c"]);
        p.advance(false);
        assert_eq!(p.current(), Some("d"));
    }
    #[test]
    fn shuffle_history_tracks_actual_folder_and_queued_visits_for_ten_steps() {
        let mut p = Playback::new(42);
        p.play_folder(folder(), 0);
        p.set_mode(Mode::Shuffle);
        let mut visits = alloc::vec![p.current().unwrap().into()];
        for i in 0..25 {
            if i % 3 == 0 {
                p.add(alloc::format!("queued {i}")).unwrap();
            }
            p.advance(false);
            visits.push(p.current().unwrap().to_string());
        }
        for expected in visits[15..25].iter().rev() {
            assert!(p.previous());
            assert_eq!(p.current(), Some(expected.as_str()));
        }
        assert!(!p.previous());
        for expected in &visits[16..] {
            assert!(p.advance(false));
            assert_eq!(p.current(), Some(expected.as_str()));
        }
    }
    #[test]
    fn queue_only_playback_drains_and_loop_one_does_not_consume_upcoming() {
        let mut p = Playback::new(4);
        p.add("a".into()).unwrap();
        p.add("b".into()).unwrap();
        assert!(p.start());
        assert_eq!(p.current(), Some("a"));
        p.set_mode(Mode::RepeatOne);
        p.advance(true);
        assert_eq!(p.queue(), ["b"]);
        assert_eq!(p.current(), Some("a"));
        assert!(p.advance(false));
        assert_eq!(p.current(), Some("b"));
        p.set_mode(Mode::Sequential);
        assert!(!p.advance(true));
        assert!(p.previous());
        assert_eq!(p.current(), Some("a"));
    }
}
