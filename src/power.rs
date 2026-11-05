//! Lid policy, independent of the ARM7 GPIO transport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Open,
    Headphones,
    Suspended,
}

#[derive(Default)]
pub struct Lid {
    pub state: State,
    resume: bool,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub pause: bool,
    pub resume: bool,
    pub suspend: bool,
    pub wake: bool,
}
impl Lid {
    pub fn update(&mut self, closed: bool, headphones: bool, playing: bool) -> Change {
        let next = if !closed {
            State::Open
        } else if headphones {
            State::Headphones
        } else {
            State::Suspended
        };
        if next == self.state {
            return Change::default();
        }
        let mut change = Change::default();
        if next == State::Suspended {
            self.resume = playing;
            change.pause = playing;
            change.suspend = true;
        } else if self.state == State::Suspended {
            change.wake = true;
            change.resume = self.resume;
            self.resume = false;
        }
        self.state = next;
        change
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_playback_follows_jack_changes_and_preserves_manual_pause() {
        let mut lid = Lid::default();
        assert_eq!(lid.update(true, true, true), Change::default());
        assert_eq!(lid.state, State::Headphones);
        assert_eq!(
            lid.update(true, false, true),
            Change {
                pause: true,
                suspend: true,
                ..Change::default()
            }
        );
        assert_eq!(lid.update(true, false, false), Change::default());
        assert_eq!(
            lid.update(true, true, false),
            Change {
                resume: true,
                wake: true,
                ..Change::default()
            }
        );
        assert_eq!(lid.update(false, true, true), Change::default());
        assert_eq!(
            lid.update(true, false, false),
            Change {
                suspend: true,
                ..Change::default()
            }
        );
        assert_eq!(
            lid.update(false, false, false),
            Change {
                wake: true,
                ..Change::default()
            }
        );
    }
}
