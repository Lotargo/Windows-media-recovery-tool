use recovery_core::BlockSource;
use std::io;

const MBR_BYTES: usize = 512;
const MBR_SIGNATURE_OFFSET: usize = 510;
const MBR_PARTITION_OFFSET: usize = 446;
const MBR_PARTITION_BYTES: usize = 16;
const MBR_PARTITION_COUNT: usize = 4;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_MIN_HEADER_BYTES: usize = 92;
const GPT_MIN_ENTRY_BYTES: usize = 128;
const GPT_MAX_ENTRY_BYTES: usize = 4096;
const MAX_LOGICAL_SECTOR_BYTES: u32 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionScheme {
    Mbr,
    Gpt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionKind {
    Mbr { type_code: u8 },
    Gpt { type_guid: [u8; 16] },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionEntry {
    pub index: u32,
    pub start_lba: u64,
    pub sector_count: u64,
    pub byte_offset: u64,
    pub byte_length: u64,
    pub kind: PartitionKind,
    pub unique_guid: Option<[u8; 16]>,
    pub attributes: Option<u64>,
    pub name: Option<String>,
}

impl PartitionEntry {
    pub fn is_extended_mbr(&self) -> bool {
        matches!(
            self.kind,
            PartitionKind::Mbr {
                type_code: 0x05 | 0x0f | 0x85
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionTable {
    pub scheme: PartitionScheme,
    pub entries: Vec<PartitionEntry>,
}

pub fn discover_partition_table(source: &dyn BlockSource) -> io::Result<Option<PartitionTable>> {
    let sector_size = validate_sector_size(source.logical_sector_size())?;
    if source.len() < MBR_BYTES as u64 {
        return Ok(None);
    }

    let mut mbr = [0_u8; MBR_BYTES];
    read_exact_at(source, 0, &mut mbr)?;
    if mbr[MBR_SIGNATURE_OFFSET..MBR_BYTES] != [0x55, 0xaa] {
        return Ok(None);
    }

    let mbr_entries = parse_mbr_entries(&mbr, sector_size)?;
    let protective_gpt = mbr_entries.iter().any(|entry| {
        matches!(
            entry.kind,
            PartitionKind::Mbr { type_code: 0xee }
        )
    });

    if protective_gpt {
        return parse_gpt(source, sector_size).map(Some);
    }

    Ok(Some(PartitionTable {
        scheme: PartitionScheme::Mbr,
        entries: mbr_entries,
    }))
}

fn validate_sector_size(sector_size: u32) -> io::Result<u64> {
    if sector_size < MBR_BYTES as u32
        || sector_size > MAX_LOGICAL_SECTOR_BYTES
        || !sector_size.is_power_of_two()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "logical sector size is outside the supported range",
        ));
    }

    Ok(sector_size as u64)
}

fn parse_mbr_entries(mbr: &[u8; MBR_BYTES], sector_size: u64) -> io::Result<Vec<PartitionEntry>> {
    let mut entries = Vec::new();

    for slot in 0..MBR_PARTITION_COUNT {
        let offset = MBR_PARTITION_OFFSET + slot * MBR_PARTITION_BYTES;
        let record = &mbr[offset..offset + MBR_PARTITION_BYTES];
        let type_code = record[4];
        let start_lba = u32::from_le_bytes(record[8..12].try_into().unwrap()) as u64;
        let sector_count = u32::from_le_bytes(record[12..16].try_into().unwrap()) as u64;

        if type_code == 0 || sector_count == 0 {
            continue;
        }

        let (byte_offset, byte_length) = byte_range(start_lba, sector_count, sector_size)?;
        entries.push(PartitionEntry {
            index: slot as u32,
            start_lba,
            sector_count,
            byte_offset,
            byte_length,
            kind: PartitionKind::Mbr { type_code },
            unique_guid: None,
            attributes: None,
            name: None,
        });
    }

    Ok(entries)
}

fn parse_gpt(source: &dyn BlockSource, sector_size: u64) -> io::Result<PartitionTable> {
    let header_offset = sector_size;
    let header_len = usize::try_from(sector_size)
        .map_err(|_| io::Error::other("logical sector size does not fit memory indexing"))?;
    let mut header = vec![0_u8; header_len];
    read_exact_at(source, header_offset, &mut header)?;

    if header.get(..GPT_SIGNATURE.len()) != Some(GPT_SIGNATURE) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protective MBR is present but GPT signature is missing",
        ));
    }

    let header_size = read_u32(&header, 12)? as usize;
    if !(GPT_MIN_HEADER_BYTES..=header.len()).contains(&header_size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GPT header size is invalid",
        ));
    }

    let entries_lba = read_u64(&header, 72)?;
    let entry_count = read_u32(&header, 80)?;
    let entry_size = read_u32(&header, 84)? as usize;

    if !(GPT_MIN_ENTRY_BYTES..=GPT_MAX_ENTRY_BYTES).contains(&entry_size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GPT partition entry size is outside the supported range",
        ));
    }

    let entries_offset = entries_lba
        .checked_mul(sector_size)
        .ok_or_else(|| io::Error::other("GPT partition array offset overflow"))?;
    let array_bytes = u64::from(entry_count)
        .checked_mul(entry_size as u64)
        .ok_or_else(|| io::Error::other("GPT partition array size overflow"))?;
    let entries_end = entries_offset
        .checked_add(array_bytes)
        .ok_or_else(|| io::Error::other("GPT partition array end overflow"))?;

    if entries_end > source.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "GPT partition array exceeds the source length",
        ));
    }

    let mut entries = Vec::new();
    for index in 0..entry_count {
        let offset = entries_offset
            .checked_add(u64::from(index) * entry_size as u64)
            .ok_or_else(|| io::Error::other("GPT partition entry offset overflow"))?;
        let mut record = vec![0_u8; entry_size];
        read_exact_at(source, offset, &mut record)?;

        let type_guid: [u8; 16] = record[0..16].try_into().unwrap();
        if type_guid == [0; 16] {
            continue;
        }

        let unique_guid: [u8; 16] = record[16..32].try_into().unwrap();
        let first_lba = read_u64(&record, 32)?;
        let last_lba = read_u64(&record, 40)?;
        if first_lba > last_lba {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "GPT partition has an inverted LBA range",
            ));
        }

        let sector_count = last_lba
            .checked_sub(first_lba)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| io::Error::other("GPT partition sector count overflow"))?;
        let (byte_offset, byte_length) = byte_range(first_lba, sector_count, sector_size)?;
        let partition_end = byte_offset
            .checked_add(byte_length)
            .ok_or_else(|| io::Error::other("GPT partition byte range overflow"))?;
        if partition_end > source.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "GPT partition exceeds the source length",
            ));
        }

        entries.push(PartitionEntry {
            index,
            start_lba: first_lba,
            sector_count,
            byte_offset,
            byte_length,
            kind: PartitionKind::Gpt { type_guid },
            unique_guid: Some(unique_guid),
            attributes: Some(read_u64(&record, 48)?),
            name: parse_gpt_name(&record),
        });
    }

    Ok(PartitionTable {
        scheme: PartitionScheme::Gpt,
        entries,
    })
}

fn parse_gpt_name(record: &[u8]) -> Option<String> {
    let name_end = record.len().min(GPT_MIN_ENTRY_BYTES);
    if name_end <= 56 {
        return None;
    }

    let mut units = Vec::new();
    for bytes in record[56..name_end].chunks_exact(2) {
        let unit = u16::from_le_bytes([bytes[0], bytes[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }

    if units.is_empty() {
        None
    } else {
        Some(String::from_utf16_lossy(&units))
    }
}

fn byte_range(start_lba: u64, sector_count: u64, sector_size: u64) -> io::Result<(u64, u64)> {
    let byte_offset = start_lba
        .checked_mul(sector_size)
        .ok_or_else(|| io::Error::other("partition byte offset overflow"))?;
    let byte_length = sector_count
        .checked_mul(sector_size)
        .ok_or_else(|| io::Error::other("partition byte length overflow"))?;
    Ok((byte_offset, byte_length))
}

fn read_exact_at(source: &dyn BlockSource, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
    let mut filled = 0_usize;
    while filled < buffer.len() {
        let current_offset = offset
            .checked_add(filled as u64)
            .ok_or_else(|| io::Error::other("read offset overflow"))?;
        let read = source.read_at(current_offset, &mut buffer[filled..])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "partition metadata read ended early",
            ));
        }
        filled = filled
            .checked_add(read)
            .ok_or_else(|| io::Error::other("read length overflow"))?;
    }
    Ok(())
}

fn read_u32(buffer: &[u8], offset: usize) -> io::Result<u32> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "u32 field is truncated"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_u64(buffer: &[u8], offset: usize) -> io::Result<u64> {
    let bytes = buffer
        .get(offset..offset + 8)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "u64 field is truncated"))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemorySource {
        bytes: Vec<u8>,
        sector_size: u32,
    }

    impl BlockSource for MemorySource {
        fn len(&self) -> u64 {
            self.bytes.len() as u64
        }

        fn logical_sector_size(&self) -> u32 {
            self.sector_size
        }

        fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
            let start = usize::try_from(offset)
                .map_err(|_| io::Error::other("test offset does not fit usize"))?;
            if start >= self.bytes.len() {
                return Ok(0);
            }

            let count = buffer.len().min(self.bytes.len() - start);
            buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
            Ok(count)
        }
    }

    fn set_mbr_entry(
        bytes: &mut [u8],
        slot: usize,
        type_code: u8,
        start_lba: u32,
        sector_count: u32,
    ) {
        let offset = MBR_PARTITION_OFFSET + slot * MBR_PARTITION_BYTES;
        bytes[offset + 4] = type_code;
        bytes[offset + 8..offset + 12].copy_from_slice(&start_lba.to_le_bytes());
        bytes[offset + 12..offset + 16].copy_from_slice(&sector_count.to_le_bytes());
    }

    #[test]
    fn discovers_basic_mbr_partition() {
        let mut bytes = vec![0_u8; 4 * 1024 * 1024];
        bytes[MBR_SIGNATURE_OFFSET..MBR_BYTES].copy_from_slice(&[0x55, 0xaa]);
        set_mbr_entry(&mut bytes, 0, 0x07, 2048, 1024);
        let source = MemorySource {
            bytes,
            sector_size: 512,
        };

        let table = discover_partition_table(&source).unwrap().unwrap();

        assert_eq!(table.scheme, PartitionScheme::Mbr);
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[0].start_lba, 2048);
        assert_eq!(table.entries[0].sector_count, 1024);
        assert_eq!(table.entries[0].byte_offset, 2048 * 512);
        assert_eq!(table.entries[0].byte_length, 1024 * 512);
        assert_eq!(
            table.entries[0].kind,
            PartitionKind::Mbr { type_code: 0x07 }
        );
    }

    #[test]
    fn discovers_gpt_partition_behind_protective_mbr() {
        let mut bytes = vec![0_u8; 2 * 1024 * 1024];
        bytes[MBR_SIGNATURE_OFFSET..MBR_BYTES].copy_from_slice(&[0x55, 0xaa]);
        set_mbr_entry(&mut bytes, 0, 0xee, 1, 4095);

        let header = &mut bytes[512..1024];
        header[0..8].copy_from_slice(GPT_SIGNATURE);
        header[12..16].copy_from_slice(&(GPT_MIN_HEADER_BYTES as u32).to_le_bytes());
        header[24..32].copy_from_slice(&1_u64.to_le_bytes());
        header[72..80].copy_from_slice(&2_u64.to_le_bytes());
        header[80..84].copy_from_slice(&4_u32.to_le_bytes());
        header[84..88].copy_from_slice(&(GPT_MIN_ENTRY_BYTES as u32).to_le_bytes());

        let entry = &mut bytes[1024..1024 + GPT_MIN_ENTRY_BYTES];
        entry[0..16].copy_from_slice(&[0x11; 16]);
        entry[16..32].copy_from_slice(&[0x22; 16]);
        entry[32..40].copy_from_slice(&40_u64.to_le_bytes());
        entry[40..48].copy_from_slice(&100_u64.to_le_bytes());
        entry[48..56].copy_from_slice(&7_u64.to_le_bytes());
        for (index, unit) in "Photos".encode_utf16().enumerate() {
            let offset = 56 + index * 2;
            entry[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }

        let source = MemorySource {
            bytes,
            sector_size: 512,
        };
        let table = discover_partition_table(&source).unwrap().unwrap();

        assert_eq!(table.scheme, PartitionScheme::Gpt);
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[0].start_lba, 40);
        assert_eq!(table.entries[0].sector_count, 61);
        assert_eq!(table.entries[0].unique_guid, Some([0x22; 16]));
        assert_eq!(table.entries[0].attributes, Some(7));
        assert_eq!(table.entries[0].name.as_deref(), Some("Photos"));
    }

    #[test]
    fn missing_partition_signature_returns_none() {
        let source = MemorySource {
            bytes: vec![0_u8; 4096],
            sector_size: 512,
        };

        assert_eq!(discover_partition_table(&source).unwrap(), None);
    }

    #[test]
    fn extended_mbr_partition_is_identified() {
        let entry = PartitionEntry {
            index: 0,
            start_lba: 1,
            sector_count: 1,
            byte_offset: 512,
            byte_length: 512,
            kind: PartitionKind::Mbr { type_code: 0x0f },
            unique_guid: None,
            attributes: None,
            name: None,
        };

        assert!(entry.is_extended_mbr());
    }
}
