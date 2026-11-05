//! Bounded compressed-data read ahead. Metadata is parsed before enabling it,
//! so walking MP4 headers does not trigger large speculative SD reads.
use crate::{ReadAt, Result};
use alloc::{vec, vec::Vec};

pub const CAPACITY: usize = 64 * 1024;

pub struct ReadAhead<R> {
    source: R,
    bytes: Vec<u8>,
    start: u64,
    valid: usize,
}

impl<R: ReadAt> ReadAhead<R> {
    pub fn new(source: R) -> Self {
        Self {
            source,
            bytes: vec![0; CAPACITY],
            start: 0,
            valid: 0,
        }
    }
}

impl<R: ReadAt> ReadAt for ReadAhead<R> {
    fn len(&self) -> u64 {
        self.source.len()
    }

    fn read_at(&mut self, mut offset: u64, mut destination: &mut [u8]) -> Result<()> {
        if offset
            .checked_add(destination.len() as u64)
            .is_none_or(|end| end > self.len())
        {
            return Err("Truncated file on SD card");
        }
        while !destination.is_empty() {
            if offset < self.start || offset >= self.start + self.valid as u64 {
                self.start = offset & !511; // Sector-aligned bulk SD reads.
                self.valid = 0; // A failed refill must never expose stale data.
                let length = (self.len() - self.start).min(CAPACITY as u64) as usize;
                self.source.read_at(self.start, &mut self.bytes[..length])?;
                self.valid = length;
            }
            let index = (offset - self.start) as usize;
            let count = destination.len().min(self.valid - index);
            destination[..count].copy_from_slice(&self.bytes[index..index + count]);
            destination = &mut destination[count..];
            offset += count as u64;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Source {
        calls: usize,
    }
    impl ReadAt for Source {
        fn len(&self) -> u64 {
            256_123
        }
        fn read_at(&mut self, offset: u64, destination: &mut [u8]) -> Result<()> {
            self.calls += 1;
            assert!(offset + destination.len() as u64 <= self.len());
            for (i, value) in destination.iter_mut().enumerate() {
                *value = (offset + i as u64).wrapping_mul(17) as u8;
            }
            Ok(())
        }
    }

    #[test]
    fn sequential_aac_frames_share_bulk_reads() {
        let mut reader = ReadAhead::new(Source { calls: 0 });
        let mut frame = [0; 777];
        for i in 0..300 {
            let offset = 123 + i * frame.len() as u64;
            reader.read_at(offset, &mut frame).unwrap();
            for (j, value) in frame.iter().enumerate() {
                assert_eq!(*value, (offset + j as u64).wrapping_mul(17) as u8);
            }
        }
        assert_eq!(reader.source.calls, 4);
        reader.read_at(13, &mut frame).unwrap();
        assert_eq!(reader.source.calls, 5);
        assert_eq!(frame[0], 13u8.wrapping_mul(17));
        reader.read_at(256_120, &mut frame[..3]).unwrap();
        assert!(reader.read_at(256_120, &mut frame[..4]).is_err());
        reader.read_at(256_123, &mut []).unwrap();
    }
}
