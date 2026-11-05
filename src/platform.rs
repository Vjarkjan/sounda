use alloc::{ffi::CString, string::String, vec::Vec};
use core::{
    alloc::{GlobalAlloc, Layout},
    ffi::{c_char, c_int, c_void},
    ptr::NonNull,
};
use sounda::{
    ReadAt, Result,
    browser::{self, Entry},
};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut u8;
    fn memalign(align: usize, size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
    pub fn sounda_init();
    pub fn sounda_is_dsi() -> c_int;
    pub fn sounda_mount_sd() -> c_int;
    pub fn sounda_ms() -> u32;
    pub fn sounda_vblank();
    pub fn sounda_keys() -> u32;
    pub fn sounda_held() -> u32;
    pub fn sounda_touch() -> u32;
    pub fn sounda_backlights(on: c_int);
    pub fn sounda_headphones() -> c_int;
    pub fn sounda_dsp_clock(on: c_int);
    pub fn sounda_rtc() -> u32;
    pub fn sounda_present_rows(screen: u32, first: u32, count: u32, frame: *const u16);
    pub fn sounda_diagnostic(index: u32, value: u32);
    pub fn sounda_dsp_decoded(milliseconds: u32);
    pub fn sounda_fatal();
    fn sounda_file_open(path: *const c_char, length: *mut u64) -> *mut c_void;
    fn sounda_file_read(file: *mut c_void, offset: u64, data: *mut u8, length: u32) -> c_int;
    fn sounda_file_close(file: *mut c_void);
    fn sounda_dir_open(path: *const c_char) -> *mut c_void;
    fn sounda_dir_next(
        dir: *mut c_void,
        base: *const c_char,
        name: *mut u8,
        capacity: u32,
    ) -> c_int;
    fn sounda_dir_close(dir: *mut c_void);
    pub fn sounda_dsp_prepare() -> c_int;
    pub fn sounda_dsp_section(code: c_int, address: u16, data: *const u8, size: u32);
    pub fn sounda_dsp_boot() -> c_int;
    pub fn sounda_dsp_off();
    pub fn sounda_dsp_send(word: u16) -> c_int;
    pub fn sounda_dsp_reply(reg: u32) -> c_int;
    pub fn sounda_dsp_housekeeping();
    pub fn sounda_dsp_buffers(frame: *const u8, length: u32, addresses: *mut u32);
    pub fn sounda_audio_init() -> c_int;
    pub fn sounda_audio_reset(rate: u32);
    pub fn sounda_audio_free() -> u32;
    pub fn sounda_audio_played() -> u32;
    pub fn sounda_audio_underruns() -> u32;
    pub fn sounda_audio_active() -> c_int;
    pub fn sounda_audio_push(channels: u32) -> c_int;
    pub fn sounda_audio_service();
    pub fn sounda_audio_pause(pause: c_int);
    pub fn sounda_audio_eof();
    pub fn sounda_audio_finished() -> c_int;
}

pub struct Allocator;
// All allocation happens on ARM9's main thread, including the manual refill.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            if layout.align() <= 8 {
                malloc(layout.size().max(1))
            } else {
                memalign(layout.align(), layout.size().max(1))
            }
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        unsafe { free(ptr) }
    }
}

pub fn milliseconds() -> u32 {
    unsafe { sounda_ms() }
}

pub struct File {
    handle: NonNull<c_void>,
    length: u64,
}
impl File {
    pub fn open(path: &str) -> Result<Self> {
        let path = CString::new(path).map_err(|_| "Invalid SD path")?;
        let mut length = 0;
        let handle = NonNull::new(unsafe { sounda_file_open(path.as_ptr(), &mut length) })
            .ok_or("Unable to open the M4A file")?;
        Ok(Self { handle, length })
    }
}
impl ReadAt for File {
    fn len(&self) -> u64 {
        self.length
    }
    fn read_at(&mut self, offset: u64, data: &mut [u8]) -> Result<()> {
        if offset
            .checked_add(data.len() as u64)
            .is_none_or(|end| end > self.length)
        {
            return Err("Truncated file on SD card");
        }
        if unsafe {
            sounda_file_read(
                self.handle.as_ptr(),
                offset,
                data.as_mut_ptr(),
                data.len() as u32,
            )
        } == 0
        {
            Ok(())
        } else {
            Err("SD card read failed")
        }
    }
}
impl Drop for File {
    fn drop(&mut self) {
        unsafe { sounda_file_close(self.handle.as_ptr()) }
    }
}

pub struct DirectoryScan {
    pub path: String,
    base: CString,
    handle: NonNull<c_void>,
    pub entries: Vec<Entry>,
    pub limited: bool,
    started: u32,
}
impl DirectoryScan {
    pub fn open(path: String) -> Result<Self> {
        let base = CString::new(path.as_str()).map_err(|_| "Invalid folder path")?;
        let handle = NonNull::new(unsafe { sounda_dir_open(base.as_ptr()) })
            .ok_or("Unable to read this SD folder")?;
        Ok(Self {
            path,
            base,
            handle,
            entries: Vec::new(),
            limited: false,
            started: milliseconds(),
        })
    }
    /// Read a small batch, then return to audio and input servicing.
    pub fn step(&mut self) -> Result<bool> {
        let since = milliseconds();
        for index in 0..32 {
            if index != 0 && milliseconds().wrapping_sub(since) >= 2 {
                break;
            }
            let mut name = [0u8; 768];
            let kind = unsafe {
                sounda_dir_next(
                    self.handle.as_ptr(),
                    self.base.as_ptr(),
                    name.as_mut_ptr(),
                    name.len() as u32,
                )
            };
            if kind < 0 {
                return Err("SD folder read failed");
            }
            if kind == 0 {
                unsafe {
                    sounda_diagnostic(12, self.entries.len() as u32);
                    sounda_diagnostic(13, milliseconds().wrapping_sub(self.started));
                }
                return Ok(true);
            }
            if kind == 3 {
                continue;
            }
            let end = name
                .iter()
                .position(|b| *b == 0)
                .ok_or("Invalid SD filename")?;
            let name =
                core::str::from_utf8(&name[..end]).map_err(|_| "SD filename is not UTF-8")?;
            if kind != 2 && !browser::is_music(name) {
                continue;
            }
            if self.entries.len() == browser::MAX_ENTRIES {
                self.limited = true;
                return Ok(true);
            }
            self.entries.push(Entry {
                name: name.into(),
                directory: kind == 2,
            });
        }
        Ok(false)
    }
}
impl Drop for DirectoryScan {
    fn drop(&mut self) {
        unsafe { sounda_dir_close(self.handle.as_ptr()) }
    }
}
