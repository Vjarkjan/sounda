pub const BROWSE_TIMEOUT_MS: u32 = 10_000;
#[derive(Default)]
pub struct Activity {
    last: Option<u32>,
}
impl Activity {
    pub fn touch(&mut self, now: u32) {
        self.last = Some(now);
    }
    pub fn active(&self, now: u32) -> bool {
        self.last
            .is_some_and(|last| now.wrapping_sub(last) < BROWSE_TIMEOUT_MS)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_returns_to_player_after_ten_seconds_and_handles_timer_wrap() {
        let mut a = Activity::default();
        assert!(!a.active(0));
        a.touch(100);
        assert!(a.active(10_099));
        assert!(!a.active(10_100));
        a.touch(u32::MAX - 100);
        assert!(a.active(50));
        assert!(!a.active(10_000));
        a.touch(10_000);
        assert!(a.active(19_999));
    }
}
