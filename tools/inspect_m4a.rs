use sounda::{
    ReadAt, Result, firmware,
    mp4::{MAX_AAC_FRAME, Track},
    read_ahead::ReadAhead,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};
struct Source {
    file: File,
    length: u64,
}
impl ReadAt for Source {
    fn len(&self) -> u64 {
        self.length
    }
    fn read_at(&mut self, offset: u64, data: &mut [u8]) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|_| "seek failed")?;
        self.file.read_exact(data).map_err(|_| "read failed")
    }
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: inspect-m4a FILE [RAW_AAC_OUTPUT]")?;
    let file = File::open(path).map_err(|_| "file open failed")?;
    let length = file.metadata().map_err(|_| "metadata failed")?.len();
    let mut source = Source { file, length };
    let mut track = Track::open(&mut source)?;
    let mut source = ReadAhead::new(source);
    let mut output = args
        .next()
        .map(File::create)
        .transpose()
        .map_err(|_| "output open failed")?;
    let mut frame = [0; MAX_AAC_FRAME];
    let mut bytes = 0;
    let mut hash = 0xcbf29ce484222325u64;
    let mut access_units = Vec::new();
    while let Some(n) = track.next_frame(&mut source, &mut frame)? {
        access_units.push(frame[..n].to_vec());
        if let Some(output) = &mut output {
            output.write_all(&frame[..n]).map_err(|_| "write failed")?;
        }
        for b in &frame[..n] {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        bytes += n;
    }
    // Check random, backward, chunk-boundary and EOF seeks against the
    // sequential access units, whose concatenation is compared to FFmpeg.
    for index in [
        track.frame_count(),
        track.frame_count() / 2,
        0,
        track.frame_count().saturating_sub(1),
        1,
        0,
    ] {
        track.seek_frame(index)?;
        for expected in access_units.iter().skip(index).take(4) {
            let n = track
                .next_frame(&mut source, &mut frame)?
                .ok_or("Seek lost an AAC frame")?;
            if &frame[..n] != expected {
                return Err("AAC seek byte mismatch");
            }
        }
        if index == track.frame_count() && track.next_frame(&mut source, &mut frame)?.is_some() {
            return Err("EOF seek returned audio");
        }
    }
    let sections = firmware::sections(firmware::FIRMWARE, |_| Ok(()))?;
    println!(
        "{{\"sample_rate\":{},\"channels\":{},\"frames\":{},\"raw_bytes\":{},\"fnv64\":\"{:016x}\",\"firmware_sections\":{},\"seek_checks\":6}}",
        track.sample_rate,
        track.channels,
        track.frame_count(),
        bytes,
        hash,
        sections
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
