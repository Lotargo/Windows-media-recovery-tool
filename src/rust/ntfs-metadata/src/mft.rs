use crate::attributes::{NtfsAttributeForm, NtfsAttributeType, NtfsDataRun};
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
    pub mft_stream_offset: u64,
    pub byte_offset: u64,
    pub header: NtfsFileRecordHeader,
    pub bytes: Vec<u8>,
}

pub struct NtfsVolume<'a> {
    source: &'a dyn BlockSource,
    boot: NtfsBootSector,
    mft_runs: Vec<NtfsDataRun>,
    mft_file_size: u64,
}

impl<'a> NtfsVolume<'a> {
    pub fn open(source: &'a dyn BlockSource) -> io::Result<Self> {
        let boot = NtfsBootSector::read(source)?;
        let bootstrap_record = read_bootstrap_record_zero(source, boot)?;
        let (mft_runs, mft_file_size) = bootstrap_mft_stream(&bootstrap_record, boot)?;

        Ok(Self {
            source,
            boot,
            mft_runs,
            mft_file_size,
        })
    }

    pub const fn boot_sector(&self) -> NtfsBootSector {
        self.boot
    }

    pub fn mft_runs(&self) -> &[NtfsDataRun] {
        &self.mft_runs
    }

    pub const fn mft_file_size(&self) -> u64 {
        self.mft_file_size
    }

    pub fn file_record_stream_offset(&self, index: u64) -> io::Result<u64> {
        index
            .checked_mul(u64::from(self.boot.file_record_size))
            .ok_or_else(|| io::Error::other("NTFS file record stream offset overflow"))
    }

    pub fn file_record_byte_offset(&self, index: u64) -> io::Result<u64> {
        let stream_offset = self.file_record_stream_offset(index)?;
        self.mft_physical_offset(stream_offset)
    }

    pub fn mft_physical_offset(&self, stream_offset: u64) -> io::Result<u64> {
        if stream_offset >= self.mft_file_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS MFT stream offset lies beyond the file size",
            ));
        }
        map_stream_offset_to_physical(&self.mft_runs, self.boot, stream_offset)
    }

    pub fn read_file_record(&self, index: u64) -> io::Result<NtfsFileRecord> {
        let stream_offset = self.file_record_stream_offset(index)?;
        let record_end = stream_offset
            .checked_add(u64::from(self.boot.file_record_size))
            .ok_or_else(|| io::Error::other("NTFS file record stream end overflow"))?;
        if record_end > self.mft_file_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS file record lies outside the $MFT file size",
            ));
        }

        let record_size = usize::try_from(self.boot.file_record_size)
            .map_err(|_| io::Error::other("NTFS file record size does not fit memory indexing"))?;
        let byte_offset = self.mft_physical_offset(stream_offset)?;
        let mut bytes = vec![0_u8; record_size];
        read_mft_stream_exact(
            self.source,
            self.boot,
            &self.mft_runs,
            self.mft_file_size,
            stream_offset,
            &mut bytes,
        )?;
        apply_update_sequence_fixups(&mut bytes, self.boot.bytes_per_sector)?;
        let header = parse_file_record_header(&bytes)?;

        Ok(NtfsFileRecord {
            index,
            mft_stream_offset: stream_offset,
            byte_offset,
            header,
            bytes,
        })
    }
}

fn read_bootstrap_record_zero(
    source: &dyn BlockSource,
    boot: NtfsBootSector,
) -> io::Result<NtfsFileRecord> {
    let byte_offset = boot.mft_byte_offset()?;
    let record_size = usize::try_from(boot.file_record_size)
        .map_err(|_| io::Error::other("NTFS file record size does not fit memory indexing"))?;
    let record_end = byte_offset
        .checked_add(u64::from(boot.file_record_size))
        .ok_or_else(|| io::Error::other("NTFS bootstrap record end overflow"))?;
    if record_end > boot.total_bytes || record_end > source.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "NTFS bootstrap MFT record lies outside the volume",
        ));
    }

    let mut bytes = vec![0_u8; record_size];
    read_exact_at(source, byte_offset, &mut bytes)?;
    apply_update_sequence_fixups(&mut bytes, boot.bytes_per_sector)?;
    let header = parse_file_record_header(&bytes)?;

    Ok(NtfsFileRecord {
        index: 0,
        mft_stream_offset: 0,
        byte_offset,
        header,
        bytes,
    })
}

fn bootstrap_mft_stream(
    record_zero: &NtfsFileRecord,
    boot: NtfsBootSector,
) -> io::Result<(Vec<NtfsDataRun>, u64)> {
    let mut extents = Vec::new();
    let mut mft_file_size = None;

    for attribute in record_zero.attributes()? {
        if attribute.header.attribute_type != NtfsAttributeType::Data
            || attribute.header.name.is_some()
        {
            continue;
        }

        let NtfsAttributeForm::NonResident {
            lowest_vcn,
            highest_vcn,
            file_size,
            ..
        } = attribute.header.form
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT unnamed $DATA attribute must be nonresident",
            ));
        };

        if lowest_vcn == 0 && mft_file_size.replace(file_size).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT contains multiple base unnamed $DATA extents",
            ));
        }

        let runs = attribute.data_runs()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT nonresident $DATA attribute has no data runs",
            )
        })?;
        extents.push((lowest_vcn, highest_vcn, runs));
    }

    let file_size = mft_file_size.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "$MFT record 0 does not contain a base unnamed $DATA attribute",
        )
    })?;
    if file_size < u64::from(boot.file_record_size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "$MFT file size is smaller than one FILE record",
        ));
    }

    extents.sort_by_key(|(lowest_vcn, _, _)| *lowest_vcn);
    let mut expected_vcn = 0_u64;
    let mut all_runs = Vec::new();
    for (lowest_vcn, highest_vcn, runs) in extents {
        if lowest_vcn != expected_vcn {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "$MFT runlist continuation requires $ATTRIBUTE_LIST support",
            ));
        }
        expected_vcn = highest_vcn
            .checked_add(1)
            .ok_or_else(|| io::Error::other("$MFT highest VCN overflow"))?;
        all_runs.extend(runs);
    }

    validate_mft_runs(&all_runs, boot, file_size)?;
    Ok((all_runs, file_size))
}

fn validate_mft_runs(runs: &[NtfsDataRun], boot: NtfsBootSector, file_size: u64) -> io::Result<()> {
    let first = runs
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "$MFT runlist is empty"))?;
    if first.vcn_start != 0 || first.lcn_start != Some(boot.mft_lcn) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "$MFT runlist does not begin at the boot-sector MFT location",
        ));
    }

    let cluster_size = u64::from(boot.bytes_per_cluster);
    let volume_clusters = boot.total_bytes / cluster_size;
    let mut expected_vcn = 0_u64;

    for run in runs {
        if run.vcn_start != expected_vcn || run.cluster_count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT runlist contains a VCN gap or zero-length run",
            ));
        }
        let lcn_start = run.lcn_start.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT runlist contains a sparse run",
            )
        })?;
        let lcn_end = lcn_start
            .checked_add(run.cluster_count)
            .ok_or_else(|| io::Error::other("$MFT LCN range overflow"))?;
        if lcn_end > volume_clusters {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "$MFT runlist points outside the NTFS volume",
            ));
        }
        expected_vcn = run
            .vcn_end()
            .ok_or_else(|| io::Error::other("$MFT VCN range overflow"))?;
    }

    let covered_bytes = expected_vcn
        .checked_mul(cluster_size)
        .ok_or_else(|| io::Error::other("$MFT runlist coverage overflow"))?;
    if covered_bytes < file_size {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "$MFT runlist is incomplete; $ATTRIBUTE_LIST continuation is not implemented yet",
        ));
    }

    Ok(())
}

fn map_stream_offset_to_physical(
    runs: &[NtfsDataRun],
    boot: NtfsBootSector,
    stream_offset: u64,
) -> io::Result<u64> {
    let cluster_size = u64::from(boot.bytes_per_cluster);
    let vcn = stream_offset / cluster_size;
    let within_cluster = stream_offset % cluster_size;
    let run = runs
        .iter()
        .find(|run| {
            run.vcn_end()
                .is_some_and(|end| run.vcn_start <= vcn && vcn < end)
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "$MFT stream offset is not covered by the current runlist",
            )
        })?;
    let lcn_start = run.lcn_start.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "$MFT cannot contain sparse data runs",
        )
    })?;
    let lcn = lcn_start
        .checked_add(vcn - run.vcn_start)
        .ok_or_else(|| io::Error::other("$MFT mapped LCN overflow"))?;
    lcn.checked_mul(cluster_size)
        .and_then(|offset| offset.checked_add(within_cluster))
        .ok_or_else(|| io::Error::other("$MFT physical byte offset overflow"))
}

fn read_mft_stream_exact(
    source: &dyn BlockSource,
    boot: NtfsBootSector,
    runs: &[NtfsDataRun],
    file_size: u64,
    stream_offset: u64,
    buffer: &mut [u8],
) -> io::Result<()> {
    let end = stream_offset
        .checked_add(buffer.len() as u64)
        .ok_or_else(|| io::Error::other("$MFT stream read range overflow"))?;
    if end > file_size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "$MFT stream read exceeds the file size",
        ));
    }

    let cluster_size = u64::from(boot.bytes_per_cluster);
    let mut filled = 0_usize;
    while filled < buffer.len() {
        let logical_offset = stream_offset
            .checked_add(filled as u64)
            .ok_or_else(|| io::Error::other("$MFT stream cursor overflow"))?;
        let vcn = logical_offset / cluster_size;
        let within_cluster = logical_offset % cluster_size;
        let run = runs
            .iter()
            .find(|run| {
                run.vcn_end()
                    .is_some_and(|end| run.vcn_start <= vcn && vcn < end)
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "$MFT stream cursor is not covered by the runlist",
                )
            })?;
        let run_end_vcn = run
            .vcn_end()
            .ok_or_else(|| io::Error::other("$MFT VCN range overflow"))?;
        let bytes_until_run_end = (run_end_vcn - vcn)
            .checked_mul(cluster_size)
            .and_then(|bytes| bytes.checked_sub(within_cluster))
            .ok_or_else(|| io::Error::other("$MFT run byte range overflow"))?;
        let chunk =
            (buffer.len() - filled).min(usize::try_from(bytes_until_run_end).unwrap_or(usize::MAX));
        if chunk == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "$MFT run mapping produced an empty read chunk",
            ));
        }

        let physical_offset = map_stream_offset_to_physical(runs, boot, logical_offset)?;
        let physical_end = physical_offset
            .checked_add(chunk as u64)
            .ok_or_else(|| io::Error::other("$MFT physical read range overflow"))?;
        if physical_end > boot.total_bytes || physical_end > source.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "$MFT physical read exceeds the NTFS volume",
            ));
        }
        read_exact_at(source, physical_offset, &mut buffer[filled..filled + chunk])?;
        filled += chunk;
    }

    Ok(())
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
    if sector_size < 2 || !record.len().is_multiple_of(sector_size) {
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
            let start =
                usize::try_from(offset).map_err(|_| io::Error::other("offset too large"))?;
            if start >= self.bytes.len() {
                return Ok(0);
            }
            let count = buffer.len().min(self.bytes.len() - start);
            buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
            Ok(count)
        }
    }

    fn initialize_file_record(record: &mut [u8], record_number: u32, used_size: u32) {
        record.fill(0);
        record[0..4].copy_from_slice(b"FILE");
        record[4..6].copy_from_slice(&48_u16.to_le_bytes());
        record[6..8].copy_from_slice(&3_u16.to_le_bytes());
        record[16..18].copy_from_slice(&7_u16.to_le_bytes());
        record[18..20].copy_from_slice(&1_u16.to_le_bytes());
        record[20..22].copy_from_slice(&56_u16.to_le_bytes());
        record[22..24].copy_from_slice(&1_u16.to_le_bytes());
        record[24..28].copy_from_slice(&used_size.to_le_bytes());
        record[28..32].copy_from_slice(&1024_u32.to_le_bytes());
        record[40..42].copy_from_slice(&3_u16.to_le_bytes());
        record[44..48].copy_from_slice(&record_number.to_le_bytes());
    }

    fn apply_test_usa(record: &mut [u8]) {
        let usn = 0xabcd_u16.to_le_bytes();
        record[48..50].copy_from_slice(&usn);
        record[50..52].copy_from_slice(&0x1111_u16.to_le_bytes());
        record[52..54].copy_from_slice(&0x2222_u16.to_le_bytes());
        record[510..512].copy_from_slice(&usn);
        record[1022..1024].copy_from_slice(&usn);
    }

    fn write_mft_data_attribute(record: &mut [u8]) {
        const ATTRIBUTE_OFFSET: usize = 56;
        const ATTRIBUTE_LENGTH: usize = 80;
        const MAPPING_OFFSET: usize = 72;
        let attribute = &mut record[ATTRIBUTE_OFFSET..ATTRIBUTE_OFFSET + ATTRIBUTE_LENGTH];
        attribute[0..4].copy_from_slice(&0x80_u32.to_le_bytes());
        attribute[4..8].copy_from_slice(&(ATTRIBUTE_LENGTH as u32).to_le_bytes());
        attribute[8] = 1;
        attribute[16..24].copy_from_slice(&0_u64.to_le_bytes());
        attribute[24..32].copy_from_slice(&2_u64.to_le_bytes());
        attribute[32..34].copy_from_slice(&(MAPPING_OFFSET as u16).to_le_bytes());
        attribute[40..48].copy_from_slice(&12288_u64.to_le_bytes());
        attribute[48..56].copy_from_slice(&12288_u64.to_le_bytes());
        attribute[56..64].copy_from_slice(&12288_u64.to_le_bytes());
        attribute[64..72].copy_from_slice(&12288_u64.to_le_bytes());
        attribute[MAPPING_OFFSET..MAPPING_OFFSET + 7]
            .copy_from_slice(&[0x11, 0x01, 0x04, 0x11, 0x02, 0x10, 0x00]);
        record[ATTRIBUTE_OFFSET + ATTRIBUTE_LENGTH..ATTRIBUTE_OFFSET + ATTRIBUTE_LENGTH + 4]
            .copy_from_slice(&0xffff_ffff_u32.to_le_bytes());
    }

    fn volume_with_fragmented_mft() -> MemorySource {
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

        let record_zero_offset = 4 * 4096;
        let record_zero = &mut bytes[record_zero_offset..record_zero_offset + 1024];
        initialize_file_record(record_zero, 0, 140);
        write_mft_data_attribute(record_zero);
        apply_test_usa(record_zero);

        let record_four_offset = 20 * 4096;
        let record_four = &mut bytes[record_four_offset..record_four_offset + 1024];
        initialize_file_record(record_four, 4, 60);
        record_four[56..60].copy_from_slice(&0xffff_ffff_u32.to_le_bytes());
        apply_test_usa(record_four);

        MemorySource { bytes }
    }

    #[test]
    fn reads_record_zero_and_applies_update_sequence_fixups() {
        let source = volume_with_fragmented_mft();
        let volume = NtfsVolume::open(&source).unwrap();
        let record = volume.read_file_record(0).unwrap();

        assert_eq!(record.index, 0);
        assert_eq!(record.mft_stream_offset, 0);
        assert_eq!(record.byte_offset, 4 * 4096);
        assert_eq!(record.header.sequence_number, 7);
        assert_eq!(record.header.first_attribute_offset, 56);
        assert!(record.header.is_in_use());
        assert!(!record.header.is_directory());
        assert_eq!(&record.bytes[510..512], &0x1111_u16.to_le_bytes());
        assert_eq!(&record.bytes[1022..1024], &0x2222_u16.to_le_bytes());
    }

    #[test]
    fn reads_file_record_from_second_mft_run() {
        let source = volume_with_fragmented_mft();
        let volume = NtfsVolume::open(&source).unwrap();
        let record = volume.read_file_record(4).unwrap();

        assert_eq!(volume.mft_runs().len(), 2);
        assert_eq!(volume.mft_file_size(), 12288);
        assert_eq!(record.index, 4);
        assert_eq!(record.mft_stream_offset, 4096);
        assert_eq!(record.byte_offset, 20 * 4096);
        assert_eq!(record.header.record_number, Some(4));
    }

    #[test]
    fn rejects_corrupt_update_sequence_trailer() {
        let mut source = volume_with_fragmented_mft();
        source.bytes[4 * 4096 + 510] = 0;

        assert_eq!(
            NtfsVolume::open(&source).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn file_record_offsets_follow_fragmented_mft_mapping() {
        let source = volume_with_fragmented_mft();
        let volume = NtfsVolume::open(&source).unwrap();

        assert_eq!(volume.file_record_stream_offset(10).unwrap(), 10 * 1024);
        assert_eq!(
            volume.file_record_byte_offset(10).unwrap(),
            21 * 4096 + 2048
        );
    }
}
