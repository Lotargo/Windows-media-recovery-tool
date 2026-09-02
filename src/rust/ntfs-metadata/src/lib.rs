mod boot;
mod mft;

pub use boot::NtfsBootSector;
pub use mft::{NtfsFileRecord, NtfsFileRecordHeader, NtfsVolume};

use recovery_core::BlockSource;
use std::io;

pub(crate) fn read_exact_at(
    source: &dyn BlockSource,
    offset: u64,
    buffer: &mut [u8],
) -> io::Result<()> {
    let mut filled = 0_usize;
    while filled < buffer.len() {
        let current_offset = offset
            .checked_add(filled as u64)
            .ok_or_else(|| io::Error::other("NTFS metadata read offset overflow"))?;
        let read = source.read_at(current_offset, &mut buffer[filled..])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS metadata read ended early",
            ));
        }
        filled = filled
            .checked_add(read)
            .ok_or_else(|| io::Error::other("NTFS metadata read length overflow"))?;
    }
    Ok(())
}

pub(crate) fn read_u16(buffer: &[u8], offset: usize) -> io::Result<u16> {
    let bytes = buffer
        .get(offset..offset + 2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "u16 field is truncated"))?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

pub(crate) fn read_u32(buffer: &[u8], offset: usize) -> io::Result<u32> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "u32 field is truncated"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(crate) fn read_u64(buffer: &[u8], offset: usize) -> io::Result<u64> {
    let bytes = buffer
        .get(offset..offset + 8)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "u64 field is truncated"))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}
