use ntfs_metadata::NtfsVolume;
use recovery_core::{BlockSource, BoundedBlockSource, SourceRange};
use std::io;

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

#[test]
fn bounded_partition_flows_into_ntfs_mft_reader() {
    const PARTITION_OFFSET: usize = 1024 * 1024;
    const PARTITION_LENGTH: usize = 32 * 1024 * 1024;
    const CLUSTER_SIZE: usize = 4096;
    const MFT_LCN: usize = 4;

    let mut parent = vec![0xcc_u8; PARTITION_OFFSET + PARTITION_LENGTH + 4096];
    let volume = &mut parent[PARTITION_OFFSET..PARTITION_OFFSET + PARTITION_LENGTH];
    let total_sectors = PARTITION_LENGTH as u64 / 512;

    volume[3..11].copy_from_slice(b"NTFS    ");
    volume[11..13].copy_from_slice(&512_u16.to_le_bytes());
    volume[13] = 8;
    volume[40..48].copy_from_slice(&total_sectors.to_le_bytes());
    volume[48..56].copy_from_slice(&(MFT_LCN as u64).to_le_bytes());
    volume[56..64].copy_from_slice(&8_u64.to_le_bytes());
    volume[64] = (-10_i8) as u8;
    volume[68] = 1;
    volume[510] = 0x55;
    volume[511] = 0xaa;

    let mft_offset = MFT_LCN * CLUSTER_SIZE;
    let record = &mut volume[mft_offset..mft_offset + 1024];
    record.fill(0);
    record[0..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&48_u16.to_le_bytes());
    record[6..8].copy_from_slice(&3_u16.to_le_bytes());
    record[16..18].copy_from_slice(&1_u16.to_le_bytes());
    record[18..20].copy_from_slice(&1_u16.to_le_bytes());
    record[20..22].copy_from_slice(&56_u16.to_le_bytes());
    record[22..24].copy_from_slice(&1_u16.to_le_bytes());
    record[24..28].copy_from_slice(&80_u32.to_le_bytes());
    record[28..32].copy_from_slice(&1024_u32.to_le_bytes());
    record[44..48].copy_from_slice(&0_u32.to_le_bytes());

    let usn = 0x5a5a_u16.to_le_bytes();
    record[48..50].copy_from_slice(&usn);
    record[50..52].copy_from_slice(&0x1234_u16.to_le_bytes());
    record[52..54].copy_from_slice(&0x5678_u16.to_le_bytes());
    record[510..512].copy_from_slice(&usn);
    record[1022..1024].copy_from_slice(&usn);

    let parent = MemorySource { bytes: parent };
    let partition = BoundedBlockSource::new(
        &parent,
        SourceRange::new(PARTITION_OFFSET as u64, PARTITION_LENGTH as u64),
    )
    .unwrap();
    let ntfs = NtfsVolume::open(&partition).unwrap();
    let record = ntfs.read_file_record(0).unwrap();

    assert_eq!(ntfs.boot_sector().bytes_per_cluster, 4096);
    assert_eq!(ntfs.boot_sector().file_record_size, 1024);
    assert_eq!(record.byte_offset, (MFT_LCN * CLUSTER_SIZE) as u64);
    assert_eq!(record.header.record_number, Some(0));
    assert_eq!(&record.bytes[510..512], &0x1234_u16.to_le_bytes());
    assert_eq!(&record.bytes[1022..1024], &0x5678_u16.to_le_bytes());
}
