//! Read the linked Teak COFF supplied by the user; never interpret it as an ARM archive.
use crate::Result;

pub const FIRMWARE: &[u8] = include_bytes!("../aac.a");

#[derive(Clone, Copy, Debug)]
pub struct Section<'a> {
    pub code: bool,
    pub word_address: u16,
    pub size: usize,
    /// None reserves zero-initialized memory (the COFF NOLOAD flags).
    pub data: Option<&'a [u8]>,
}

fn u16le(bytes: &[u8], at: usize) -> Result<u16> {
    let b = bytes.get(at..at + 2).ok_or("Truncated DSP firmware")?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn u32le(bytes: &[u8], at: usize) -> Result<u32> {
    let b = bytes.get(at..at + 4).ok_or("Truncated DSP firmware")?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Validate every placement before the caller changes the DSP memory map.
/// BLK_HDR entries describe the same sections a second time and must be skipped.
pub fn sections<'a>(
    bytes: &'a [u8],
    mut visit: impl FnMut(Section<'a>) -> Result<()>,
) -> Result<usize> {
    if u16le(bytes, 0)? != 0x00a4 {
        return Err("Firmware is not Teak COFF");
    }
    let count = u16le(bytes, 2)? as usize;
    let table = 20 + u16le(bytes, 16)? as usize;
    if count == 0 || count > 1024 || table + count * 40 > bytes.len() {
        return Err("Invalid DSP section table");
    }
    let strings = (u32le(bytes, 8)? as u64 + u32le(bytes, 12)? as u64 * 18) as usize;
    let mut placements = 0;
    for i in 0..count {
        let at = table + i * 40;
        let flags = u32le(bytes, at + 36)?;
        let size = u32le(bytes, at + 16)? as usize;
        if flags & 0x80 != 0 || size == 0 {
            continue;
        }
        let name = if u32le(bytes, at)? == 0 {
            let offset = strings
                .checked_add(u32le(bytes, at + 4)? as usize)
                .ok_or("Invalid DSP name")?;
            let tail = bytes.get(offset..).ok_or("Invalid DSP name")?;
            let end = tail
                .iter()
                .take(256)
                .position(|b| *b == 0)
                .ok_or("Invalid DSP name")?;
            &tail[..end]
        } else {
            let name = &bytes[at..at + 8];
            &name[..name.iter().position(|b| *b == 0).unwrap_or(8)]
        };
        let in_code = name.windows(3).any(|part| part == b"@c0");
        let in_data = name.windows(3).any(|part| part == b"@d0");
        if !in_code && !in_data {
            return Err("Unsupported DSP memory page");
        }
        // Some initialized data exists in BOTH spaces. The firmware's CRT
        // copies the program-space image back into data space during boot.
        // `.data@c0@d0` therefore needs two placements, not just the suffix.
        for (code, map, noload, address_at, selected) in [
            (true, 8, 32, at + 8, in_code),
            (false, 16, 64, at + 12, in_data),
        ] {
            if flags & map == 0 {
                continue;
            }
            if !selected {
                continue;
            }
            let address = u32le(bytes, address_at)? as usize;
            if size & 1 != 0
                || address
                    .checked_mul(2)
                    .and_then(|a| a.checked_add(size))
                    .is_none_or(|end| end > 0x20000)
            {
                return Err("DSP section outside memory");
            }
            let data = if flags & noload != 0 {
                None
            } else {
                let offset = u32le(bytes, at + 20)? as usize;
                Some(
                    bytes
                        .get(offset..offset.checked_add(size).ok_or("Invalid DSP data")?)
                        .ok_or("Invalid DSP data")?,
                )
            };
            visit(Section {
                code,
                word_address: address as u16,
                size,
                data,
            })?;
            placements += 1;
        }
    }
    if placements == 0 {
        return Err("DSP firmware has no sections");
    }
    Ok(placements)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_firmware_has_boot_code_and_decoder_data() {
        let mut boot = false;
        let mut code_end = 0;
        let mut data_end = 0;
        let n = sections(FIRMWARE, |s| {
            if s.code && s.word_address == 0 && s.data.is_some() {
                boot = true;
            }
            let end = s.word_address as usize * 2 + s.size;
            if s.code {
                code_end = code_end.max(end);
            } else {
                data_end = data_end.max(end);
            }
            Ok(())
        })
        .unwrap();
        assert!(boot);
        assert!(n > 50);
        assert!(code_end > 0xa000 && data_end > 0x19000);
    }
    #[test]
    fn truncation_and_wrong_format_are_rejected() {
        assert!(sections(b"!<arch>\n", |_| Ok(())).is_err());
        assert!(sections(&FIRMWARE[..0x3000], |_| Ok(())).is_err());
    }
    #[test]
    fn boot_constants_are_placed_in_both_program_and_data_memory() {
        let mut code_constants = None;
        let mut data_constants = None;
        let mut code_data = None;
        let mut data_data = None;
        let count = sections(FIRMWARE, |section| {
            match (section.code, section.word_address) {
                (true, 0x2cbf) => code_constants = section.data,
                (false, 0x9000) => data_constants = section.data,
                (true, 0x2cd3) => code_data = section.data,
                (false, 0x9014) => data_data = section.data,
                _ => {}
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 107);
        assert_eq!(code_constants.unwrap().len(), 0x28);
        assert_eq!(code_data.unwrap().len(), 0x1de);
        assert_eq!(code_constants, data_constants);
        assert_eq!(code_data, data_data);
    }
}
