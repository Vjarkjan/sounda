//! Bounded, seek-based demuxer for unencrypted AAC-LC in ordinary M4A/MP4 files.
//! The DSP takes individual raw AAC access units, without ADTS headers.
use crate::{ReadAt, Result};
use alloc::{vec, vec::Vec};

pub const MAX_AAC_FRAME: usize = 1700;
pub const MAX_SAMPLES: usize = 262_144;
const MAX_STSC: usize = 16_384;
const MAX_BOXES: usize = 16_384;

#[derive(Clone, Copy, Debug)]
struct Atom {
    kind: [u8; 4],
    data: u64,
    end: u64,
}

fn be16(b: &[u8], at: usize) -> Result<u16> {
    let s = b.get(at..at + 2).ok_or("Truncated M4A metadata")?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}
fn be32(b: &[u8], at: usize) -> Result<u32> {
    let s = b.get(at..at + 4).ok_or("Truncated M4A metadata")?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}
fn be64(b: &[u8], at: usize) -> Result<u64> {
    Ok(((be32(b, at)? as u64) << 32) | be32(b, at + 4)? as u64)
}
fn read<const N: usize>(source: &mut impl ReadAt, offset: u64) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    source.read_at(offset, &mut bytes)?;
    Ok(bytes)
}
fn atoms(source: &mut impl ReadAt, start: u64, end: u64) -> Result<Vec<Atom>> {
    if end > source.len() || start > end {
        return Err("M4A box exceeds file");
    }
    let mut cursor = start;
    let mut result = Vec::new();
    while cursor < end {
        if end - cursor < 8 || result.len() == MAX_BOXES {
            return Err("Invalid M4A box table");
        }
        let h = read::<8>(source, cursor)?;
        let size = be32(&h, 0)? as u64;
        let (size, header) = match size {
            0 => (end - cursor, 8),
            1 => (be64(&read::<8>(source, cursor + 8)?, 0)?, 16),
            n => (n, 8),
        };
        let next = cursor.checked_add(size).ok_or("M4A box overflow")?;
        if size < header || next > end {
            return Err("Truncated M4A box");
        }
        result.push(Atom {
            kind: h[4..8].try_into().unwrap(),
            data: cursor + header,
            end: next,
        });
        cursor = next;
    }
    Ok(result)
}
fn child(list: &[Atom], kind: &[u8; 4]) -> Result<Atom> {
    let mut found = list.iter().filter(|a| &a.kind == kind);
    let a = *found.next().ok_or("Missing M4A sample table")?;
    if found.next().is_some() {
        return Err("Duplicate M4A sample table");
    }
    Ok(a)
}
fn payload(source: &mut impl ReadAt, atom: Atom, limit: usize) -> Result<Vec<u8>> {
    let n = atom.end - atom.data;
    if n > limit as u64 {
        return Err("M4A metadata too large");
    }
    let mut data = vec![0; n as usize];
    source.read_at(atom.data, &mut data)?;
    Ok(data)
}

#[derive(Clone, Copy, Debug)]
struct ChunkRun {
    first: u32,
    samples: u32,
}

#[derive(Debug)]
pub struct Track {
    pub sample_rate: u32,
    pub channels: u16,
    sizes: Vec<u16>,
    chunks: Vec<u32>,
    runs: Vec<ChunkRun>,
    frame: usize,
    chunk: usize,
    within_chunk: u32,
    run: usize,
    offset: u64,
}

impl Track {
    pub fn open(source: &mut impl ReadAt) -> Result<Self> {
        // newlib's file offset ABI and DSi RAM impose explicit limits.
        if source.len() > i32::MAX as u64 {
            return Err("M4A exceeds the 2 GiB limit");
        }
        let top = atoms(source, 0, source.len())?;
        if top.iter().any(|a| a.kind == *b"moof") {
            return Err("Fragmented M4A is unsupported");
        }
        let moov = child(&top, b"moov")?;
        let children = atoms(source, moov.data, moov.end)?;
        let mut audio = None;
        for trak in children.iter().filter(|a| a.kind == *b"trak") {
            let trak_boxes = atoms(source, trak.data, trak.end)?;
            let mdia = child(&trak_boxes, b"mdia")?;
            let media = atoms(source, mdia.data, mdia.end)?;
            let hdlr = child(&media, b"hdlr")?;
            if hdlr.end - hdlr.data < 12 {
                return Err("Invalid M4A track handler");
            }
            if &read::<12>(source, hdlr.data)?[8..12] != b"soun" {
                continue;
            }
            if audio.is_some() {
                return Err("Select an M4A with one audio track");
            }
            let minf = child(&media, b"minf")?;
            let minf_boxes = atoms(source, minf.data, minf.end)?;
            let stbl = child(&minf_boxes, b"stbl")?;
            audio = Some(atoms(source, stbl.data, stbl.end)?);
        }
        let tables = audio.ok_or("No audio track in this file")?;
        let (sample_rate, channels) = sample_description(source, child(&tables, b"stsd")?)?;

        let sizes_data = payload(source, child(&tables, b"stsz")?, 12 + MAX_SAMPLES * 4)?;
        let fixed_size = be32(&sizes_data, 4)?;
        let count = be32(&sizes_data, 8)? as usize;
        if count == 0 || count > MAX_SAMPLES {
            return Err("Too many AAC frames (limit 262144)");
        }
        let mut sizes = Vec::with_capacity(count);
        if fixed_size == 0 && sizes_data.len() != 12 + count * 4 {
            return Err("Invalid M4A frame sizes");
        }
        for i in 0..count {
            let size = if fixed_size == 0 {
                be32(&sizes_data, 12 + i * 4)?
            } else {
                fixed_size
            };
            if size == 0 || size > MAX_AAC_FRAME as u32 {
                return Err("AAC frame exceeds DSP's 1700-byte limit");
            }
            sizes.push(size as u16);
        }
        drop(sizes_data);

        let wide = tables.iter().any(|a| a.kind == *b"co64");
        if wide && tables.iter().any(|a| a.kind == *b"stco") {
            return Err("Conflicting M4A chunk tables");
        }
        let chunk_data = payload(
            source,
            child(&tables, if wide { b"co64" } else { b"stco" })?,
            8 + MAX_SAMPLES * 8,
        )?;
        let chunk_count = be32(&chunk_data, 4)? as usize;
        let width = if wide { 8 } else { 4 };
        if chunk_count == 0 || chunk_count > count || chunk_data.len() != 8 + chunk_count * width {
            return Err("Invalid M4A chunk offsets");
        }
        let mut chunks = Vec::with_capacity(chunk_count);
        for i in 0..chunk_count {
            let offset = if wide {
                be64(&chunk_data, 8 + i * width)?
            } else {
                be32(&chunk_data, 8 + i * width)? as u64
            };
            if offset > i32::MAX as u64 {
                return Err("M4A chunk outside supported file range");
            }
            chunks.push(offset as u32);
        }
        drop(chunk_data);

        let runs_data = payload(source, child(&tables, b"stsc")?, 8 + MAX_STSC * 12)?;
        let run_count = be32(&runs_data, 4)? as usize;
        if run_count == 0 || run_count > MAX_STSC || runs_data.len() != 8 + run_count * 12 {
            return Err("Invalid M4A chunk map");
        }
        let mut runs = Vec::with_capacity(run_count);
        for i in 0..run_count {
            let first = be32(&runs_data, 8 + i * 12)?;
            let samples = be32(&runs_data, 12 + i * 12)?;
            if first == 0
                || first as usize > chunk_count
                || samples == 0
                || samples as usize > count
                || be32(&runs_data, 16 + i * 12)? != 1
                || (i == 0 && first != 1)
                || runs.last().is_some_and(|r: &ChunkRun| r.first >= first)
            {
                return Err("Unsupported M4A chunk map");
            }
            runs.push(ChunkRun { first, samples });
        }
        drop(runs_data);

        // Prove that every compressed sample lies within mdat, including files
        // whose audio chunks are interleaved with video or other track data.
        let mut sample = 0usize;
        let mut run = 0;
        for (i, offset) in chunks.iter().enumerate() {
            if run + 1 < runs.len() && i + 1 >= runs[run + 1].first as usize {
                run += 1;
            }
            let end_sample = sample
                .checked_add(runs[run].samples as usize)
                .ok_or("M4A sample count overflow")?;
            let group = sizes
                .get(sample..end_sample)
                .ok_or("M4A chunk map exceeds frame count")?;
            let end = *offset as u64 + group.iter().map(|n| *n as u64).sum::<u64>();
            if !top
                .iter()
                .any(|a| a.kind == *b"mdat" && *offset as u64 >= a.data && end <= a.end)
            {
                return Err("M4A audio chunk outside mdat");
            }
            sample = end_sample;
        }
        if sample != count {
            return Err("M4A frame and chunk counts disagree");
        }

        // AAC-LC has 1024 output samples per access unit. A shorter final entry
        // is normal encoder padding; the decoder still returns a full block.
        let times = payload(source, child(&tables, b"stts")?, 8 + MAX_SAMPLES * 8)?;
        let entries = be32(&times, 4)? as usize;
        if entries == 0 || entries > count || times.len() != 8 + entries * 8 {
            return Err("Invalid M4A timing table");
        }
        let mut timed_samples = 0u64;
        for i in 0..entries {
            let n = be32(&times, 8 + i * 8)?;
            let duration = be32(&times, 12 + i * 8)?;
            if n == 0 || duration == 0 {
                return Err("Invalid AAC frame duration");
            }
            timed_samples += n as u64;
        }
        if timed_samples != count as u64 {
            return Err("M4A timing and frame counts disagree");
        }
        let offset = chunks[0] as u64;
        Ok(Self {
            sample_rate,
            channels,
            sizes,
            chunks,
            runs,
            frame: 0,
            chunk: 0,
            within_chunk: 0,
            run: 0,
            offset,
        })
    }

    pub fn frame_count(&self) -> usize {
        self.sizes.len()
    }
    pub fn frame_index(&self) -> usize {
        self.frame
    }
    pub fn duration_samples(&self) -> u32 {
        self.sizes.len() as u32 * 1024
    }
    pub fn finished(&self) -> bool {
        self.frame == self.sizes.len()
    }

    /// Locate an AAC access unit using stsc/chunk offsets, without decoding or
    /// reading earlier media. The caller primes AAC overlap before the target.
    pub fn seek_frame(&mut self, frame: usize) -> Result<()> {
        if frame > self.sizes.len() {
            return Err("Seek beyond the end of the song");
        }
        self.frame = frame;
        if frame == self.sizes.len() {
            return Ok(());
        }
        let mut base = 0usize;
        for (run, entry) in self.runs.iter().enumerate() {
            let first = entry.first as usize - 1;
            let last = self
                .runs
                .get(run + 1)
                .map_or(self.chunks.len(), |next| next.first as usize - 1);
            let samples = entry.samples as usize;
            let length = (last - first) * samples;
            if frame < base + length {
                let delta = frame - base;
                self.chunk = first + delta / samples;
                self.within_chunk = (delta % samples) as u32;
                self.run = run;
                let start = frame - self.within_chunk as usize;
                self.offset = self.chunks[self.chunk] as u64
                    + self.sizes[start..frame]
                        .iter()
                        .map(|n| *n as u64)
                        .sum::<u64>();
                return Ok(());
            }
            base += length;
        }
        Err("Invalid AAC seek table")
    }

    pub fn next_frame(
        &mut self,
        source: &mut impl ReadAt,
        buffer: &mut [u8; MAX_AAC_FRAME],
    ) -> Result<Option<usize>> {
        let Some(size) = self.sizes.get(self.frame).copied() else {
            return Ok(None);
        };
        source.read_at(self.offset, &mut buffer[..size as usize])?;
        self.offset += size as u64;
        self.frame += 1;
        self.within_chunk += 1;
        if self.frame < self.sizes.len() && self.within_chunk == self.runs[self.run].samples {
            self.chunk += 1;
            self.within_chunk = 0;
            if self.run + 1 < self.runs.len()
                && self.chunk + 1 >= self.runs[self.run + 1].first as usize
            {
                self.run += 1;
            }
            self.offset = self.chunks[self.chunk] as u64;
        }
        Ok(Some(size as usize))
    }
}

fn sample_description(source: &mut impl ReadAt, atom: Atom) -> Result<(u32, u16)> {
    let bytes = payload(source, atom, 16_384)?;
    if be32(&bytes, 4)? != 1 {
        return Err("Multiple M4A sample descriptions unsupported");
    }
    let size = be32(&bytes, 8)? as usize;
    if bytes.get(12..16) != Some(b"mp4a") {
        return Err("This M4A is not AAC (ALAC/DRM unsupported)");
    }
    if size < 36 || size + 8 != bytes.len() || be16(&bytes, 24)? != 0 {
        return Err("Unsupported M4A audio sample entry");
    }
    let channels = be16(&bytes, 32)?;
    let rate = be32(&bytes, 40)? >> 16;
    let mut pos = 44;
    while pos < bytes.len() {
        let n = be32(&bytes, pos)? as usize;
        if n < 8 || pos.checked_add(n).is_none_or(|end| end > bytes.len()) {
            return Err("Invalid AAC decoder descriptor");
        }
        if bytes.get(pos + 4..pos + 8) == Some(b"esds") {
            let esds = bytes
                .get(pos + 12..pos + n)
                .ok_or("Truncated AAC descriptor")?;
            let asc = find_asc(esds, 0)?.ok_or("Missing AAC AudioSpecificConfig")?;
            let (asc_rate, asc_channels) = parse_asc(asc)?;
            // Some muxers put 2 in mp4a even for mono; ASC is authoritative.
            if rate != asc_rate || !(channels == 1 || channels == 2) {
                return Err("Conflicting AAC audio configuration");
            }
            return Ok((asc_rate, asc_channels));
        }
        pos += n;
    }
    Err("Missing AAC decoder configuration")
}

fn find_asc(bytes: &[u8], depth: usize) -> Result<Option<&[u8]>> {
    if depth > 4 {
        return Err("Invalid AAC descriptor nesting");
    }
    let mut at = 0;
    while at < bytes.len() {
        let tag = bytes[at];
        at += 1;
        let mut len = 0usize;
        let mut complete = false;
        for _ in 0..4 {
            let b = *bytes.get(at).ok_or("Truncated AAC descriptor")?;
            at += 1;
            len = (len << 7) | (b & 127) as usize;
            if b & 128 == 0 {
                complete = true;
                break;
            }
        }
        if !complete {
            return Err("Invalid AAC descriptor length");
        }
        let end = at.checked_add(len).ok_or("AAC descriptor overflow")?;
        let data = bytes.get(at..end).ok_or("Truncated AAC descriptor")?;
        match tag {
            5 => return Ok(Some(data)),
            3 => {
                let flags = *data.get(2).ok_or("Truncated AAC ES descriptor")?;
                let mut skip = 3 + if flags & 128 != 0 { 2 } else { 0 };
                if flags & 64 != 0 {
                    skip += 1 + *data.get(skip).ok_or("Invalid AAC URL")? as usize;
                }
                if flags & 32 != 0 {
                    skip += 2;
                }
                if let Some(asc) = find_asc(
                    data.get(skip..).ok_or("Invalid AAC ES descriptor")?,
                    depth + 1,
                )? {
                    return Ok(Some(asc));
                }
            }
            4 => {
                if data.first() != Some(&0x40) {
                    return Err("Unsupported MPEG audio codec");
                }
                if let Some(asc) = find_asc(
                    data.get(13..).ok_or("Invalid AAC config descriptor")?,
                    depth + 1,
                )? {
                    return Ok(Some(asc));
                }
            }
            _ => {}
        }
        at = end;
    }
    Ok(None)
}

struct Bits<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: usize) -> Result<u32> {
        let mut value = 0;
        for _ in 0..n {
            let b = self
                .data
                .get(self.cursor / 8)
                .ok_or("Truncated AAC configuration")?;
            value = (value << 1) | ((*b >> (7 - self.cursor % 8)) & 1) as u32;
            self.cursor += 1;
        }
        Ok(value)
    }
}

fn parse_asc(data: &[u8]) -> Result<(u32, u16)> {
    let mut bits = Bits { data, cursor: 0 };
    if bits.read(5)? != 2 {
        return Err("Use AAC-LC audio (HE-AAC/ALAC unsupported)");
    }
    let index = bits.read(4)?;
    let rate = if index == 15 {
        bits.read(24)?
    } else {
        *[
            96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
        ]
        .get(index as usize)
        .ok_or("Invalid AAC sample rate")?
    };
    let channels = bits.read(4)? as u16;
    if ![8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000].contains(&rate)
        || !(channels == 1 || channels == 2)
    {
        return Err("DSP needs mono/stereo AAC at 8-48 kHz");
    }
    if bits.read(1)? != 0 || bits.read(1)? != 0 || bits.read(1)? != 0 {
        return Err("Unsupported AAC frame configuration");
    }
    // Backward-compatible HE-AAC can start with object type 2 and signal SBR
    // afterwards. Do not let its 2048-sample frames reach 1024-sample buffers.
    if data.len() * 8 >= bits.cursor + 17
        && bits.read(11)? == 0x2b7
        && bits.read(5)? == 5
        && bits.read(1)? != 0
    {
        return Err("Use AAC-LC audio (HE-AAC/ALAC unsupported)");
    }
    Ok((rate, channels))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory(Vec<u8>);
    impl ReadAt for Memory {
        fn len(&self) -> u64 {
            self.0.len() as u64
        }
        fn read_at(&mut self, at: u64, b: &mut [u8]) -> Result<()> {
            let s = self
                .0
                .get(at as usize..at as usize + b.len())
                .ok_or("EOF")?;
            b.copy_from_slice(s);
            Ok(())
        }
    }
    #[test]
    fn seek_crosses_variable_chunk_runs_and_interleaved_media() {
        let mut track = Track {
            sample_rate: 44100,
            channels: 2,
            sizes: vec![2, 3, 4, 1, 2, 3, 4],
            chunks: vec![10, 40, 80],
            runs: vec![
                ChunkRun {
                    first: 1,
                    samples: 2,
                },
                ChunkRun {
                    first: 3,
                    samples: 3,
                },
            ],
            frame: 0,
            chunk: 0,
            within_chunk: 0,
            run: 0,
            offset: 10,
        };
        let mut data = vec![0xee; 100];
        let offsets = [10, 12, 40, 44, 80, 82, 85];
        for (index, &offset) in offsets.iter().enumerate() {
            data[offset..offset + track.sizes[index] as usize].fill(index as u8);
        }
        let mut source = Memory(data);
        let mut buffer = [0; MAX_AAC_FRAME];
        for first in [4, 1, 6, 0, 3, 2] {
            track.seek_frame(first).unwrap();
            for index in first..7 {
                let n = track.next_frame(&mut source, &mut buffer).unwrap().unwrap();
                assert!(buffer[..n].iter().all(|b| *b == index as u8));
            }
            assert_eq!(track.next_frame(&mut source, &mut buffer), Ok(None));
        }
        assert!(track.seek_frame(8).is_err());
        track.seek_frame(7).unwrap();
        assert!(track.finished());
        assert_eq!(track.next_frame(&mut source, &mut buffer), Ok(None));
    }
    #[test]
    fn accepts_lc_and_rejects_he_aac_or_multichannel() {
        assert_eq!(parse_asc(&[0x12, 0x10]), Ok((44100, 2)));
        assert_eq!(parse_asc(&[0x11, 0x88]), Ok((48000, 1)));
        assert!(parse_asc(&[0x2b, 0x92, 0x08]).is_err());
        assert!(parse_asc(&[0x12, 0x30]).is_err());
        assert!(parse_asc(&[0x12, 0x14]).is_err());
        assert_eq!(parse_asc(&[0x12, 0x10, 0x56, 0xe5, 0x00]), Ok((44100, 2)));
        assert!(parse_asc(&[0x12, 0x10, 0x56, 0xe5, 0x80]).is_err());
    }
    #[test]
    fn rejects_broken_box_lengths_without_looping_or_allocating_from_them() {
        for data in [
            vec![0; 3],
            vec![0, 0, 0, 4, b'm', b'o', b'o', b'v'],
            vec![
                0, 0, 0, 1, b'm', b'o', b'o', b'v', 255, 255, 255, 255, 255, 255, 255, 255,
            ],
        ] {
            let mut source = Memory(data);
            assert!(Track::open(&mut source).is_err());
        }
    }
}
