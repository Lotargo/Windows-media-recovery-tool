mod enumeration;
mod profile;

pub use enumeration::{
    DiscoveredPhysicalDrive, MountedVolume, StorageDiscovery, VolumeDriveType,
    enumerate_storage_sources,
};
pub use profile::{
    StorageAlignment, StorageBusType, StorageIdentity, StorageKind, StorageProfile,
    classify_storage,
};

use recovery_core::BlockSource;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const GENERIC_READ: u32 = 0x8000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
const DEFAULT_IMAGE_SECTOR_SIZE: u32 = 512;

#[derive(Debug)]
pub struct RawDiskImage {
    file: File,
    path: PathBuf,
    len: u64,
    logical_sector_size: u32,
}

impl RawDiskImage {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with_sector_size(path, DEFAULT_IMAGE_SECTOR_SIZE)
    }

    pub fn open_with_sector_size(
        path: impl AsRef<Path>,
        logical_sector_size: u32,
    ) -> io::Result<Self> {
        if logical_sector_size == 0 || !logical_sector_size.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "logical sector size must be a non-zero power of two",
            ));
        }

        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let len = file.metadata()?.len();

        Ok(Self {
            file,
            path,
            len,
            logical_sector_size,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl BlockSource for RawDiskImage {
    fn len(&self) -> u64 {
        self.len
    }

    fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || offset >= self.len {
            return Ok(0);
        }

        let remaining = self.len - offset;
        let read_len = buffer.len().min(remaining.min(usize::MAX as u64) as usize);
        self.file.seek_read(&mut buffer[..read_len], offset)
    }
}

#[derive(Debug)]
pub struct WindowsPhysicalDrive {
    file: File,
    index: u32,
    path: String,
    profile: StorageProfile,
}

impl WindowsPhysicalDrive {
    pub fn open_read_only(index: u32) -> io::Result<Self> {
        let path = physical_drive_path(index);
        let file = OpenOptions::new()
            .access_mode(GENERIC_READ)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&path)?;
        let profile = profile::query_storage_profile(&file)?;

        Ok(Self {
            file,
            index,
            path,
            profile,
        })
    }

    pub const fn index(&self) -> u32 {
        self.index
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn profile(&self) -> &StorageProfile {
        &self.profile
    }
}

impl BlockSource for WindowsPhysicalDrive {
    fn len(&self) -> u64 {
        self.profile.length_bytes
    }

    fn logical_sector_size(&self) -> u32 {
        self.profile.logical_sector_size
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || offset >= self.profile.length_bytes {
            return Ok(0);
        }

        let remaining = self.profile.length_bytes - offset;
        let read_len = buffer.len().min(remaining.min(usize::MAX as u64) as usize);
        self.file.seek_read(&mut buffer[..read_len], offset)
    }
}

pub fn physical_drive_path(index: u32) -> String {
    format!(r"\\.\PhysicalDrive{index}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_image_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "windows-media-recovery-{}-{unique}.img",
            std::process::id()
        ))
    }

    #[test]
    fn physical_drive_paths_use_windows_device_namespace() {
        assert_eq!(physical_drive_path(0), r"\\.\PhysicalDrive0");
        assert_eq!(physical_drive_path(17), r"\\.\PhysicalDrive17");
    }

    #[test]
    fn raw_disk_image_supports_offset_reads() {
        let path = temporary_image_path();
        std::fs::write(&path, b"0123456789abcdef").expect("fixture should be written");

        let image = RawDiskImage::open(&path).expect("fixture should open read-only");
        let mut buffer = [0_u8; 4];
        let read = image
            .read_at(6, &mut buffer)
            .expect("offset read should work");

        assert_eq!(read, 4);
        assert_eq!(&buffer, b"6789");
        assert_eq!(image.len(), 16);
        assert_eq!(image.logical_sector_size(), 512);

        drop(image);
        std::fs::remove_file(path).expect("fixture should be removable");
    }

    #[test]
    fn raw_disk_image_boundary_and_eof_reads() {
        let path = temporary_image_path();
        std::fs::write(&path, b"abcdefghijklmnop").expect("fixture should be written");

        let image = RawDiskImage::open(&path).expect("fixture should open read-only");

        // 0-byte buffer
        let mut empty_buf = [0_u8; 0];
        assert_eq!(image.read_at(0, &mut empty_buf).unwrap(), 0);

        // Read at EOF
        let mut buf = [0_u8; 8];
        assert_eq!(image.read_at(16, &mut buf).unwrap(), 0);

        // Read past EOF
        assert_eq!(image.read_at(100, &mut buf).unwrap(), 0);

        // Partial read crossing EOF: read 8 bytes from offset 12 (length is 16, so 4 bytes left)
        let mut partial_buf = [0_u8; 8];
        let read = image.read_at(12, &mut partial_buf).unwrap();
        assert_eq!(read, 4);
        assert_eq!(&partial_buf[..4], b"mnop");

        drop(image);
        std::fs::remove_file(path).expect("fixture should be removable");
    }

    #[test]
    fn raw_disk_image_custom_sector_size_and_path() {
        let path = temporary_image_path();
        std::fs::write(&path, [0xaa; 4096]).expect("fixture should be written");

        let image = RawDiskImage::open_with_sector_size(&path, 4096)
            .expect("4096-byte sector size should be valid");
        assert_eq!(image.logical_sector_size(), 4096);
        assert_eq!(image.path(), path.as_path());
        assert_eq!(image.len(), 4096);

        drop(image);
        std::fs::remove_file(path).expect("fixture should be removable");
    }

    #[test]
    fn raw_disk_image_non_existent_file_returns_error() {
        let non_existent = std::env::temp_dir().join("non_existent_image_12345_xyz.img");
        let error = RawDiskImage::open(&non_existent).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn raw_disk_image_rejects_invalid_sector_size() {
        let path = temporary_image_path();
        std::fs::write(&path, [0_u8; 1]).expect("fixture should be written");

        let error = RawDiskImage::open_with_sector_size(&path, 1000)
            .expect_err("non power-of-two sector size should fail");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        std::fs::remove_file(path).expect("fixture should be removable");
    }
}
