use crate::{read_exact_at, read_u16, read_u32, read_u64};
use recovery_core::BlockSource;
use std::io;

const BOOT_SECTOR_BYTES: usize = 512;
const NTFS_OEM_ID: &[u8; 8] = b"NTFS    ";
const MAX_NTFS_CLUSTER_BYTES: u32 = 2 * 1024 * 1024;
const MAX_METADATA_STRUCTURE_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsBootSector {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub bytes_per_cluster: u32,
    pub total_sectors: u64,
    pub total_bytes: u64,
    pub mft_lcn: u64,
    pub mft_mirror_lcn: u64,
    pub file_record_size: u32,
    pub index_buffer_size: u32,
    pub volume_serial_number: u64,
    pub checksum: u32,
}

impl NtfsBootSector {
    pub fn read(source: &dyn BlockSource) -> io::Result<Self> {
        if source.len() < BOOT_SECTOR_BYTES as u64 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source is too short to contain an NTFS boot sector",
            ));
        }

        let mut sector = [0_u8; BOOT_SECTOR_BYTES];
        read_exact_at(source, 0, &mut sector)?;

        if sector.get(3..11) != Some(NTFS_OEM_ID) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS OEM identifier is missing",
            ));
        }
        if sector[510..512] != [0x55, 0xaa] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS boot signature is missing",
            ));
        }

        let bytes_per_sector = read_u16(&sector, 11)?;
        if !(512..=4096).contains(&bytes_per_sector) || !bytes_per_sector.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS bytes-per-sector value is unsupported",
            ));
        }

        let sectors_per_cluster = sector[13];
        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS sectors-per-cluster value is invalid",
            ));
        }

        let bytes_per_cluster = u32::from(bytes_per_sector)
            .checked_mul(u32::from(sectors_per_cluster))
            .ok_or_else(|| io::Error::other("NTFS cluster size overflow"))?;
        if bytes_per_cluster > MAX_NTFS_CLUSTER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS cluster size exceeds the supported Windows limit",
            ));
        }

        let total_sectors = read_u64(&sector, 40)?;
        if total_sectors == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS volume reports zero sectors",
            ));
        }
        let total_bytes = total_sectors
            .checked_mul(u64::from(bytes_per_sector))
            .ok_or_else(|| io::Error::other("NTFS volume byte length overflow"))?;
        if total_bytes > source.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS volume length exceeds the bounded source",
            ));
        }

        let cluster_count = total_sectors / u64::from(sectors_per_cluster);
        if cluster_count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS volume does not contain a complete cluster",
            ));
        }

        let mft_lcn = read_u64(&sector, 48)?;
        let mft_mirror_lcn = read_u64(&sector, 56)?;
        if mft_lcn >= cluster_count || mft_mirror_lcn >= cluster_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS MFT location is outside the volume",
            ));
        }

        let file_record_size =
            decode_structure_size(sector[64] as i8, bytes_per_cluster, "file record")?;
        let index_buffer_size =
            decode_structure_size(sector[68] as i8, bytes_per_cluster, "index buffer")?;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            total_sectors,
            total_bytes,
            mft_lcn,
            mft_mirror_lcn,
            file_record_size,
            index_buffer_size,
            volume_serial_number: read_u64(&sector, 72)?,
            checksum: read_u32(&sector, 80)?,
        })
    }

    pub fn mft_byte_offset(self) -> io::Result<u64> {
        self.mft_lcn
            .checked_mul(u64::from(self.bytes_per_cluster))
            .ok_or_else(|| io::Error::other("NTFS MFT byte offset overflow"))
    }

    pub fn mft_mirror_byte_offset(self) -> io::Result<u64> {
        self.mft_mirror_lcn
            .checked_mul(u64::from(self.bytes_per_cluster))
            .ok_or_else(|| io::Error::other("NTFS MFT mirror byte offset overflow"))
    }
}

fn decode_structure_size(raw: i8, cluster_size: u32, label: &str) -> io::Result<u32> {
    if raw == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NTFS {label} size encoding is zero"),
        ));
    }

    let bytes = if raw > 0 {
        u64::from(raw as u8)
            .checked_mul(u64::from(cluster_size))
            .ok_or_else(|| io::Error::other(format!("NTFS {label} size overflow")))?
    } else {
        let exponent = u32::try_from(-i16::from(raw))
            .map_err(|_| io::Error::other(format!("NTFS {label} exponent is invalid")))?;
        1_u64
            .checked_shl(exponent)
            .ok_or_else(|| io::Error::other(format!("NTFS {label} size overflow")))?
    };

    if bytes == 0 || bytes > u64::from(MAX_METADATA_STRUCTURE_BYTES) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("NTFS {label} size is outside the supported range"),
        ));
    }

    u32::try_from(bytes).map_err(|_| io::Error::other(format!("NTFS {label} size is too large")))
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

    fn valid_volume() -> MemorySource {
        let mut bytes = vec![0_u8; 32 * 1024 * 1024];
        let total_sectors = bytes.len() as u64 / 512;
        bytes[3..11].copy_from_slice(NTFS_OEM_ID);
        bytes[11..13].copy_from_slice(&512_u16.to_le_bytes());
        bytes[13] = 8;
        bytes[40..48].copy_from_slice(&total_sectors.to_le_bytes());
        bytes[48..56].copy_from_slice(&4_u64.to_le_bytes());
        bytes[56..64].copy_from_slice(&8_u64.to_le_bytes());
        bytes[64] = (-10_i8) as u8;
        bytes[68] = 1;
        bytes[72..80].copy_from_slice(&0x1122_3344_5566_7788_u64.to_le_bytes());
        bytes[80..84].copy_from_slice(&0xaabb_ccdd_u32.to_le_bytes());
        bytes[510] = 0x55;
        bytes[511] = 0xaa;
        MemorySource { bytes }
    }

    #[test]
    fn parses_ntfs_boot_geometry() {
        let source = valid_volume();
        let boot = NtfsBootSector::read(&source).unwrap();

        assert_eq!(boot.bytes_per_sector, 512);
        assert_eq!(boot.sectors_per_cluster, 8);
        assert_eq!(boot.bytes_per_cluster, 4096);
        assert_eq!(boot.file_record_size, 1024);
        assert_eq!(boot.index_buffer_size, 4096);
        assert_eq!(boot.mft_byte_offset().unwrap(), 4 * 4096);
        assert_eq!(boot.mft_mirror_byte_offset().unwrap(), 8 * 4096);
        assert_eq!(boot.volume_serial_number, 0x1122_3344_5566_7788);
        assert_eq!(boot.checksum, 0xaabb_ccdd);
    }

    #[test]
    fn positive_file_record_encoding_uses_clusters() {
        assert_eq!(decode_structure_size(2, 4096, "test").unwrap(), 8192);
    }

    #[test]
    fn rejects_invalid_oem_and_signature() {
        let mut source = valid_volume();
        source.bytes[3] = b'X';
        assert_eq!(
            NtfsBootSector::read(&source).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut source = valid_volume();
        source.bytes[510] = 0;
        assert_eq!(
            NtfsBootSector::read(&source).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn rejects_volume_length_beyond_source() {
        let mut source = valid_volume();
        let sectors = source.bytes.len() as u64 / 512 + 1;
        source.bytes[40..48].copy_from_slice(&sectors.to_le_bytes());
        assert_eq!(
            NtfsBootSector::read(&source).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn rejects_mft_outside_volume() {
        let mut source = valid_volume();
        let cluster_count = source.bytes.len() as u64 / 4096;
        source.bytes[48..56].copy_from_slice(&cluster_count.to_le_bytes());
        assert_eq!(
            NtfsBootSector::read(&source).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
