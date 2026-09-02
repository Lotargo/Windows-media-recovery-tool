use crate::{NtfsBootSector, read_exact_at, read_u16, read_u32, read_u64};
use recovery_core::BlockSource;
use std::io;

const FILE_SIGNATURE: &[u8; 4] = b"FILE";
const FILE_RECORD_HEADER_BYTES: usize = 42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsFileRecordHeader {
    pub update_sequence_offset: u16,
    pub update_sequence_count: u16,
    pub sequence_number: u16,
    pub hard_link_count: u16,
    pub first_attribute_offset: u16,
    pub flags: u16,
    pub used_size: u32,
    pub allocated_size: u32,
    pub base_file_record: u64,
    pub next_attribute_id: u16,
    pub record_number: Option<u32>,
}

impl NtfsFileRecordHeader {
    pub const fn is_in_use(self) -> bool {
        self.flags & 0x0001 != 0
    }

    pub const fn is_directory(self) -> bool {
        self.flags & 0x0002 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsFileRecord {
    pub index: u64,
    pub byte_offset: u64,
    pub header: NtfsFileRecordHeader,
    pub bytes: Vec<u8>,
}

pub struct NtfsVolume<'a> {
    source: &'a dyn BlockSource,
    boot: NtfsBootSector,
}

impl<'a> NtfsVolume<'a> {
    pub fn open(source: &'a dyn BlockSource) -> io::Result<Self> {
        let boot = NtfsBootSector::read(source)?;
        Ok(Self { source, boot })
    }

    pub const fn boot_sector(&self) -> NtfsBootSector {
        self.boot
    }

    pub fn file_record_byte_offset(&self, index: u64) -> io::Result<u64> {
        let relative = index
            .checked_mul(u64::from(self.boot.file_record_size))
            .ok_or_else(|| io::Error::other("NTFS file record offset overflow"))?;
        self.boot
            .mft_byte_offset()?
            .checked_add(relative)
            .ok_or_else(|| io::Error::other("NTFS MFT record byte offset overflow"))
    }

    pub fn read_file_record(&self, index: u64) -> io::Result<NtfsFileRecord> {
        let byte_offset = self.file_record_byte_offset(index)?;
        let record_size = usize::try_from(self.boot.file_record_size)
            .map_err(|_| io::Error::other("NTFS file record size does not fit memory indexing"))?;
        let record_end = byte_offset
            .checked_add(u64::from(self.boot.file_record_size))
            .ok_or_else(|| io::Error::other("NTFS file record end offset overflow"))?;
        if record_end > self.boot.total_bytes || record_end > self.source.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS file record lies outside the volume",
            ));
        }

        let mut bytes = vec![0_u8; record_size];
        read_exact_at(self.source, byte_offset, &mut bytes)?;
        apply_update_sequence_fixups(&mut bytes, self.boot.bytes_per_sector)?;
        let header = parse_file_record_header(&bytes)?;

        Ok(NtfsFileRecord {
            index,
            byte_offset,
            header,
            bytes,
        })
    }
}

fn parse_file_record_header(record: &[u8]) -> io::Result<NtfsFileRecordHeader> {
    if record.len() < FILE_RECORD_HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "NTFS FILE record header is truncated",
        ));
    }
    if record.get(..4) != Some(FILE_SIGNATURE) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS FILE record signature is missing",
        ));
    }

    let first_attribute_offset = read_u16(record, 20)?;
    let used_size = read_u32(record, 24)?;
    let allocated_size = read_u32(record, 28)?;

    if used_size > allocated_size || u64::from(allocated_size) > record.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS FILE record size fields are inconsistent",
        ));
    }
    if usize::from(first_attribute_offset) < FILE_RECORD_HEADER_BYTES
        || u32::from(first_attribute_offset) > used_size
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS FILE record first attribute offset is invalid",
        ));
    }

    Ok(NtfsFileRecordHeader {
        update_sequence_offset: read_u16(record, 4)?,
        update_sequence_count: read_u16(record, 6)?,
        sequence_number: read_u16(record, 16)?,
        hard_link_count: read_u16(record, 18)?,
        first_attribute_offset,
        flags: read_u16(record, 22)?,
        used_size,
        allocated_size,
        base_file_record: read_u64(record, 32)?,
        next_attribute_id: read_u16(record, 40)?,
        record_number: if record.len() >= 48 {
            Some(read_u32(record, 44)?)
        } else {
            None
        },
    })
}

fn apply_update_sequence_fixups(record: &mut [u8], bytes_per_sector: u16) -> io::Result<()> {
    if record.len() < 8 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "NTFS FILE record is too short for update sequence metadata",
        ));
    }

    let sector_size = usize::from(bytes_per_sector);
    if sector_size < 2 || record.len() % sector_size != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS FILE record size is not aligned to the NTFS sector size",
        ));
    }

    let usa_offset = usize::from(read_u16(record, 4)?);
    let usa_count = usize::from(read_u16(record, 6)?);
    let sector_count = record.len() / sector_size;
    if usa_count != sector_count + 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS update sequence count does not match the FILE record sector count",
        ));
    }

    let usa_bytes = usa_count
        .checked_mul(2)
        .ok_or_else(|| io::Error::other("NTFS update sequence array size overflow"))?;
    let usa_end = usa_offset
        .checked_add(usa_bytes)
        .ok_or_else(|| io::Error::other("NTFS update sequence array end overflow"))?;
    if usa_offset < 8 || usa_end > record.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS update sequence array lies outside the FILE record",
        ));
    }

    let update_sequence_number = [record[usa_offset], record[usa_offset + 1]];
    for sector_index in 0..sector_count {
        let trailer_offset = (sector_index + 1)
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| io::Error::other("NTFS sector trailer offset overflow"))?;
        if record[trailer_offset..trailer_offset + 2] != update_sequence_number {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS FILE record update sequence mismatch",
            ));
        }

        let replacement_offset = usa_offset + (sector_index + 1) * 2;
        let replacement = [record[replacement_offset], record[replacement_offset + 1]];
        record[trailer_offset..trailer_offset + 2].copy_from_slice(&replacement);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemorySource {
        bytes: Vec<u8>,
    }

    impl BlockSource for MemorySource {
        fn len(&self) -> u64 {
            self.bytes.len() as u64
        }

        fn logical_sector_size(&self) -> u32 {
            512
        }

        fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
            let start = usize::try_from(offset).map_err(|_| io::Error::other("offset too large"))?;
            if start >= self.bytes.len() {
                return Ok(0);
            }
            let count = buffer.len().min(self.bytes.len() - start);
            buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
            Ok(count)
        }
    }

    fn volume_with_mft_record() -> MemorySource {
        let mut bytes = vec![0_u8; 32 * 1024 * 1024];
        let total_sectors = bytes.len() as u64 / 512;
        bytes[3..11].copy_from_slice(b"NTFS    ");
        bytes[11..13].copy_from_slice(&512_u16.to_le_bytes());
        bytes[13] = 8;
        bytes[40..48].copy_from_slice(&total_sectors.to_le_bytes());
        bytes[48..56].copy_from_slice(&4_u64.to_le_bytes());
        bytes[56..64].copy_from_slice(&8_u64.to_le_bytes());
        bytes[64] = (-10_i8) as u8;
        bytes[68] = 1;
        bytes[510] = 0x55;
        bytes[511] = 0xaa;

        let mft_offset = 4 * 4096;
        let record = &mut bytes[mft_offset..mft_offset + 1024];
        record[0..4].copy_from_slice(b"FILE");
        record[4..6].copy_from_slice(&48_u16.to_le_bytes());
        record[6..8].copy_from_slice(&3_u16.to_le_bytes());
        record[16..18].copy_from_slice(&7_u16.to_le_bytes());
        record[18..20].copy_from_slice(&1_u16.to_le_bytes());
        record[20..22].copy_from_slice(&56_u16.to_le_bytes());
        record[22..24].copy_from_slice(&1_u16.to_le_bytes());
        record[24..28].copy_from_slice(&80_u32.to_le_bytes());
        record[28..32].copy_from_slice(&1024_u32.to_le_bytes());
        record[40..42].copy_from_slice(&3_u16.to_le_bytes());
        record[44..48].copy_from_slice(&0_u32.to_le_bytes());

        let usn = 0xabcd_u16.to_le_bytes();
        record[48..50].copy_from_slice(&usn);
        record[50..52].copy_from_slice(&0x1111_u16.to_le_bytes());
        record[52..54].copy_from_slice(&0x2222_u16.to_le_bytes());
        record[510..512].copy_from_slice(&usn);
        record[1022..1024].copy_from_slice(&usn);

        MemorySource { bytes }
    }

    #[test]
    fn reads_mft_record_and_applies_update_sequence_fixups() {
        let source = volume_with_mft_record();
        let volume = NtfsVolume::open(&source).unwrap();
        let record = volume.read_file_record(0).unwrap();

        assert_eq!(record.index, 0);
        assert_eq!(record.byte_offset, 4 * 4096);
        assert_eq!(record.header.sequence_number, 7);
        assert_eq!(record.header.first_attribute_offset, 56);
        assert!(record.header.is_in_use());
        assert!(!record.header.is_directory());
        assert_eq!(&record.bytes[510..512], &0x1111_u16.to_le_bytes());
        assert_eq!(&record.bytes[1022..1024], &0x2222_u16.to_le_bytes());
    }

    #[test]
    fn rejects_corrupt_update_sequence_trailer() {
        let mut source = volume_with_mft_record();
        let mft_offset = 4 * 4096;
        source.bytes[mft_offset + 510] = 0;
        let volume = NtfsVolume::open(&source).unwrap();

        assert_eq!(
            volume.read_file_record(0).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn file_record_offset_uses_u64_arithmetic() {
        let source = volume_with_mft_record();
        let volume = NtfsVolume::open(&source).unwrap();
        assert_eq!(volume.file_record_byte_offset(10).unwrap(), 4 * 4096 + 10 * 1024);
    }
}
