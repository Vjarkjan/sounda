#![no_std]

extern crate alloc;

pub mod activity;
pub mod browser;
pub mod firmware;
pub mod font;
pub mod keyboard;
pub mod mp4;
pub mod playback;
pub mod playlist;
pub mod power;
pub mod read_ahead;
pub mod search;
pub mod ui;
pub mod unicode;
mod unicode_data;

pub type Result<T> = core::result::Result<T, &'static str>;

/// Implemented with std::fs on the host and newlib FILE on the DSi.
pub trait ReadAt {
    fn len(&self) -> u64;
    fn read_at(&mut self, offset: u64, destination: &mut [u8]) -> Result<()>;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
