use crate::platform::{self as hw, File};
use sounda::{
    Result, firmware,
    mp4::{MAX_AAC_FRAME, Track},
    read_ahead::ReadAhead,
};

enum Decoder {
    Boot {
        ready: u8,
        since: u32,
    },
    Initialize {
        since: u32,
    },
    Idle,
    Feeding {
        words: [u16; 11],
        sent: usize,
        since: u32,
    },
    Waiting {
        since: u32,
    },
}

pub struct Player {
    file: ReadAhead<File>,
    track: Track,
    decoder: Decoder,
    pub paused: bool,
    eof: bool,
    buffer: [u8; MAX_AAC_FRAME],
    pumped_at: u32,
    max_service_gap: u32,
    seek_base: u32,
    discard_until: usize,
}

impl Player {
    pub fn open(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;
        let track = Track::open(&mut file)?;
        // First pass is side-effect free, so malformed firmware cannot leave
        // NWRAM partially reassigned. The embedded user file remains intact.
        firmware::sections(firmware::FIRMWARE, |_| Ok(()))?;
        if unsafe { hw::sounda_dsp_prepare() } != 0 {
            return Err("DSP locked. Launch in DSi mode with DSP/NWRAM access.");
        }
        firmware::sections(firmware::FIRMWARE, |s| {
            unsafe {
                hw::sounda_dsp_section(
                    i32::from(s.code),
                    s.word_address,
                    s.data.map_or(core::ptr::null(), |b| b.as_ptr()),
                    s.size as u32,
                )
            };
            Ok(())
        })?;
        if unsafe { hw::sounda_dsp_boot() } != 0 {
            return Err("Unable to map the DSP firmware");
        }
        unsafe { hw::sounda_audio_reset(track.sample_rate) };
        Ok(Self {
            file: ReadAhead::new(file),
            track,
            decoder: Decoder::Boot {
                ready: 0,
                since: hw::milliseconds(),
            },
            paused: false,
            eof: false,
            buffer: [0; MAX_AAC_FRAME],
            pumped_at: hw::milliseconds(),
            max_service_gap: 0,
            seek_base: 0,
            discard_until: 0,
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.track.sample_rate
    }
    pub fn elapsed(&self) -> u32 {
        (self.seek_base + unsafe { hw::sounda_audio_played() }) / self.track.sample_rate
    }
    pub fn position(&self) -> u16 {
        ((self.seek_base as u64 + unsafe { hw::sounda_audio_played() } as u64)
            .min(self.track.duration_samples() as u64)
            * u16::MAX as u64
            / self.track.duration_samples() as u64) as u16
    }

    pub fn seek(&mut self, fraction: u16) -> Result<()> {
        let target = (self.track.frame_count() as u64 * fraction as u64 / u16::MAX as u64) as usize;
        self.track.seek_frame(target.saturating_sub(2))?;
        unsafe {
            hw::sounda_audio_reset(self.track.sample_rate);
        }
        if unsafe { hw::sounda_dsp_prepare() } != 0 {
            return Err("Unable to reset the DSP for seeking");
        }
        firmware::sections(firmware::FIRMWARE, |s| {
            unsafe {
                hw::sounda_dsp_section(
                    i32::from(s.code),
                    s.word_address,
                    s.data.map_or(core::ptr::null(), |data| data.as_ptr()),
                    s.size as u32,
                );
            }
            Ok(())
        })?;
        if unsafe { hw::sounda_dsp_boot() } != 0 {
            return Err("Unable to restart the DSP for seeking");
        }
        self.seek_base = target as u32 * 1024;
        self.discard_until = target;
        self.eof = false;
        self.decoder = Decoder::Boot {
            ready: 0,
            since: hw::milliseconds(),
        };
        self.pumped_at = hw::milliseconds();
        unsafe {
            hw::sounda_audio_pause(i32::from(self.paused));
        }
        Ok(())
    }
    pub fn duration(&self) -> u32 {
        self.track.duration_samples() / self.track.sample_rate
    }
    pub fn underruns(&self) -> u32 {
        unsafe { hw::sounda_audio_underruns() }
    }
    pub fn status(&self) -> &'static str {
        if self.paused {
            "PAUSED"
        } else if matches!(
            self.decoder,
            Decoder::Boot { .. } | Decoder::Initialize { .. }
        ) {
            "LOADING AAC FIRMWARE"
        } else if unsafe { hw::sounda_audio_active() } == 0 {
            "BUFFERING"
        } else {
            "NOW PLAYING"
        }
    }
    pub fn pause(&mut self, pause: bool) {
        self.paused = pause;
        unsafe { hw::sounda_audio_pause(i32::from(pause)) };
    }
    /// Timer deadlines must exclude time spent with the DSP clock stopped.
    pub fn wake(&mut self, slept: u32) {
        match &mut self.decoder {
            Decoder::Boot { since, .. }
            | Decoder::Initialize { since }
            | Decoder::Feeding { since, .. }
            | Decoder::Waiting { since } => {
                *since = since.wrapping_add(slept);
            }
            Decoder::Idle => {}
        }
        self.pumped_at = hw::milliseconds();
    }
    pub fn finished(&self) -> bool {
        self.eof && !self.paused && unsafe { hw::sounda_audio_finished() } != 0
    }
    pub fn needs_service(&self) -> bool {
        !matches!(self.decoder, Decoder::Idle)
            || (!self.paused && !self.eof && unsafe { hw::sounda_audio_free() } >= 1024)
    }

    /// Nonblocking DSP protocol. Each call performs a bounded amount of work,
    /// so a stalled firmware leaves the touch interface responsive.
    pub fn pump(&mut self) -> Result<()> {
        unsafe { hw::sounda_audio_service() };
        let time = hw::milliseconds();
        self.max_service_gap = self.max_service_gap.max(time.wrapping_sub(self.pumped_at));
        self.pumped_at = time;
        unsafe {
            hw::sounda_diagnostic(9, self.max_service_gap);
            hw::sounda_diagnostic(17, time);
        }
        match &mut self.decoder {
            Decoder::Boot { ready, since } => {
                for reg in 0..3 {
                    if *ready & (1 << reg) != 0 {
                        continue;
                    }
                    let reply = unsafe { hw::sounda_dsp_reply(reg) };
                    if reply < 0 {
                        continue;
                    }
                    if reply != 1 {
                        return Err("Unexpected DSP startup reply");
                    }
                    *ready |= 1 << reg;
                }
                if *ready == 7 {
                    self.decoder = Decoder::Initialize { since: time };
                } else if time.wrapping_sub(*since) > 2000 {
                    return Err("DSP startup timeout. Check DSi mode and DSP access.");
                }
            }
            Decoder::Initialize { since } => {
                unsafe { hw::sounda_dsp_housekeeping() };
                if unsafe { hw::sounda_dsp_send(0) } != 0 {
                    self.decoder = Decoder::Idle;
                } else if time.wrapping_sub(*since) > 1000 {
                    return Err("DSP initialization timed out");
                }
            }
            Decoder::Feeding { words, sent, since } => {
                unsafe { hw::sounda_dsp_housekeeping() };
                while *sent < words.len() && unsafe { hw::sounda_dsp_send(words[*sent]) } != 0 {
                    *sent += 1;
                }
                if *sent == words.len() {
                    self.decoder = Decoder::Waiting { since: time };
                } else if time.wrapping_sub(*since) > 1000 {
                    return Err("DSP command timed out");
                }
            }
            Decoder::Waiting { since } => {
                unsafe { hw::sounda_dsp_housekeeping() };
                let reply = unsafe { hw::sounda_dsp_reply(0) };
                match reply {
                    0 => {
                        unsafe { hw::sounda_dsp_decoded(time.wrapping_sub(*since)) };
                        if self.track.frame_index() > self.discard_until
                            && unsafe { hw::sounda_audio_push(self.track.channels as u32) } != 0
                        {
                            return Err("PCM queue overflow");
                        }
                        self.decoder = Decoder::Idle;
                    }
                    1 => return Err("DSP rejected the AAC frame parameters"),
                    2 => return Err("DSP could not decode this AAC frame"),
                    n if n >= 0 => return Err("Unexpected AAC decoder reply"),
                    _ if time.wrapping_sub(*since) > 1000 => return Err("AAC decoder timed out"),
                    _ => {}
                }
            }
            Decoder::Idle => {
                unsafe { hw::sounda_dsp_housekeeping() };
                if self.paused || self.eof || unsafe { hw::sounda_audio_free() } < 1024 {
                    return Ok(());
                }
                let Some(length) = self.track.next_frame(&mut self.file, &mut self.buffer)? else {
                    self.eof = true;
                    unsafe { hw::sounda_audio_eof() };
                    return Ok(());
                };
                let mut pointers = [0u32; 3];
                unsafe {
                    hw::sounda_dsp_buffers(
                        self.buffer.as_ptr(),
                        length as u32,
                        pointers.as_mut_ptr(),
                    )
                };
                let [input, left, right] = pointers;
                self.decoder = Decoder::Feeding {
                    words: [
                        1,
                        length as u16,
                        (self.track.sample_rate >> 16) as u16,
                        self.track.sample_rate as u16,
                        self.track.channels,
                        (input >> 16) as u16,
                        input as u16,
                        (left >> 16) as u16,
                        left as u16,
                        (right >> 16) as u16,
                        right as u16,
                    ],
                    sent: 0,
                    since: time,
                };
            }
        }
        Ok(())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            hw::sounda_audio_reset(44100);
            hw::sounda_dsp_off();
        }
    }
}
