use alloc::vec::Vec;

pub const PREVIOUS_SONGS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Sequential,
    Shuffle,
    RepeatOne,
}

impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Sequential => Self::Shuffle,
            Self::Shuffle => Self::RepeatOne,
            Self::RepeatOne => Self::Sequential,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Sequential => "SEQUENTIAL",
            Self::Shuffle => "SHUFFLE",
            Self::RepeatOne => "REPEAT ONE",
        }
    }
}

/// A shuffled bag visits every track before reshuffling; previous follows history.
pub struct Playlist {
    pub mode: Mode,
    pub current: usize,
    count: usize,
    random: u32,
    bag: Vec<usize>,
    history: Vec<usize>,
    history_pos: usize,
    cycle_started: bool,
}

impl Playlist {
    pub fn new(count: usize, current: usize, seed: u32) -> Self {
        Self {
            mode: Mode::Sequential,
            current: current.min(count.saturating_sub(1)),
            count,
            random: seed.max(1),
            bag: Vec::new(),
            history: if count == 0 {
                Vec::new()
            } else {
                alloc::vec![current.min(count - 1)]
            },
            history_pos: 0,
            cycle_started: false,
        }
    }
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.bag.clear();
            self.cycle_started = false;
        }
        self.mode = mode;
    }
    fn random(&mut self) -> u32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random
    }
    pub fn advance(&mut self, automatic: bool) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        if automatic && self.mode == Mode::RepeatOne {
            return Some(self.current);
        }
        if self.mode == Mode::Shuffle {
            if self.history_pos + 1 < self.history.len() {
                self.history_pos += 1;
                self.current = self.history[self.history_pos];
                return Some(self.current);
            }
            if self.bag.is_empty() {
                self.bag.extend(0..self.count);
                for i in (1..self.bag.len()).rev() {
                    let j = self.random() as usize % (i + 1);
                    self.bag.swap(i, j);
                }
                // The first draw is at the end of the bag.
                if self.count > 1 && self.bag.last() == Some(&self.current) {
                    self.bag.swap(0, self.count - 1);
                }
                // The track already playing counts as this cycle's first draw.
                if !self.cycle_started {
                    self.bag.retain(|i| *i != self.current);
                    self.cycle_started = true;
                }
            }
            let next = self.bag.pop().unwrap_or(self.current);
            self.record(next);
        } else {
            self.record((self.current + 1) % self.count);
        }
        Some(self.current)
    }
    pub fn previous(&mut self) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        if self.mode == Mode::Shuffle && self.history_pos > 0 {
            self.history_pos -= 1;
            self.current = self.history[self.history_pos];
        } else if self.mode != Mode::Shuffle {
            self.record((self.current + self.count - 1) % self.count);
        } else {
            return None;
        }
        Some(self.current)
    }

    fn record(&mut self, index: usize) {
        self.history.truncate(self.history_pos + 1);
        if self.history.last() != Some(&index) {
            self.history.push(index);
        }
        if self.history.len() > PREVIOUS_SONGS + 1 {
            self.history.remove(0);
        }
        self.current = index;
        self.history_pos = self.history.len().saturating_sub(1);
    }

    pub fn select(&mut self, index: usize) -> Option<usize> {
        if index >= self.count {
            return None;
        }
        self.record(index);
        self.bag.clear();
        self.cycle_started = false;
        Some(index)
    }

    pub fn append(&mut self) {
        let index = self.count;
        self.count += 1;
        if index == 0 {
            self.current = 0;
            // Enqueuing is not a playback visit. select()/advance() records
            // the first song actually chosen by the player.
        } else if !self.bag.is_empty() {
            let position = self.random() as usize % (self.bag.len() + 1);
            self.bag.insert(position, index);
        }
    }

    /// Queue edits apply the same permutation to navigation and shuffle state.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.count || to >= self.count {
            return;
        }
        let map = |index| {
            if index == from {
                to
            } else if from < to && index > from && index <= to {
                index - 1
            } else if from > to && index >= to && index < from {
                index + 1
            } else {
                index
            }
        };
        self.current = map(self.current);
        for index in self.bag.iter_mut().chain(self.history.iter_mut()) {
            *index = map(*index);
        }
    }

    /// Returns whether the playing queue entry was removed.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.count {
            return false;
        }
        let was_current = self.current == index;
        self.count -= 1;
        self.bag.retain(|entry| *entry != index);
        for entry in &mut self.bag {
            if *entry > index {
                *entry -= 1;
            }
        }
        let position = self
            .history
            .iter()
            .take(self.history_pos + 1)
            .filter(|entry| **entry != index)
            .count()
            .saturating_sub(1);
        self.history.retain(|entry| *entry != index);
        for entry in &mut self.history {
            if *entry > index {
                *entry -= 1;
            }
        }
        self.history_pos = position.min(self.history.len().saturating_sub(1));
        if self.count == 0 {
            self.current = 0;
            self.history.clear();
            self.cycle_started = false;
        } else if was_current {
            self.record(index % self.count);
        } else if self.current > index {
            self.current -= 1;
        }
        was_current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    #[test]
    fn sequential_and_repeat_wrap_correctly() {
        let mut p = Playlist::new(3, 2, 7);
        assert_eq!(p.advance(true), Some(0));
        assert_eq!(p.previous(), Some(2));
        p.set_mode(Mode::RepeatOne);
        assert_eq!(p.advance(true), Some(2));
        assert_eq!(p.advance(false), Some(0));
        assert_eq!(Playlist::new(0, 0, 0).advance(true), None);
    }
    #[test]
    fn shuffle_visits_each_song_and_previous_retraces_it() {
        for seed in 1..50 {
            let mut p = Playlist::new(8, 3, seed);
            p.set_mode(Mode::Shuffle);
            let mut visited = vec![3];
            for _ in 0..7 {
                visited.push(p.advance(true).unwrap());
            }
            let last = *visited.last().unwrap();
            assert_eq!(p.previous(), Some(visited[6]));
            assert_eq!(p.advance(false), Some(last));
            visited.sort_unstable();
            assert_eq!(visited, (0..8).collect::<Vec<_>>());
            assert_ne!(p.advance(true), Some(last));
        }
    }

    #[test]
    fn shuffle_keeps_ten_previous_visits_and_replays_forward_history() {
        let mut p = Playlist::new(30, 0, 42);
        p.set_mode(Mode::Shuffle);
        let mut visits = alloc::vec![0];
        for _ in 0..25 {
            visits.push(p.advance(true).unwrap());
        }
        for expected in visits[15..25].iter().rev() {
            assert_eq!(p.previous(), Some(*expected));
        }
        assert_eq!(p.previous(), None);
        for expected in &visits[16..] {
            assert_eq!(p.advance(false), Some(*expected));
        }
        assert_eq!(p.history.len(), 11);
    }

    #[test]
    fn adding_songs_without_playing_does_not_invent_previous_visits() {
        let mut p = Playlist::new(0, 0, 42);
        for _ in 0..3 {
            p.append();
        }
        p.set_mode(Mode::Shuffle);
        p.select(2);
        assert_eq!(p.previous(), None);
        p.advance(false);
        assert_eq!(p.previous(), Some(2));
    }
    #[test]
    fn queue_edits_preserve_playing_identity_and_shuffle_history() {
        let mut p = Playlist::new(8, 3, 99);
        let mut identities: Vec<usize> = (0..8).collect();
        p.set_mode(Mode::Shuffle);
        let original = identities[p.current];
        let next = p.advance(false).unwrap();
        let now = identities[next];
        let item = identities.remove(3);
        identities.insert(7, item);
        p.move_item(3, 7);
        assert_eq!(identities[p.current], now);
        assert_eq!(identities[p.previous().unwrap()], original);
        assert_eq!(identities[p.advance(false).unwrap()], now);
        p.append();
        identities.push(8);
        let other = (p.current + 1) % identities.len();
        identities.remove(other);
        assert!(!p.remove(other));
        assert_eq!(identities[p.current], now);
        identities.remove(p.current);
        assert!(p.remove(p.current));
        assert!(p.current < identities.len());
        for _ in 0..100 {
            assert!(p.advance(false).unwrap() < identities.len());
        }
    }
}
