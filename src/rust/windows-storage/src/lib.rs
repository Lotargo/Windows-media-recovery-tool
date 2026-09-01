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
}

impl WindowsPhysicalDrive {
    pub fn open_read_only(index: u32) -> io::Result<Self> {
        let path = physical_drive_path(index);
        let file = OpenOptions::new()
            .access_mode(GENERIC_READ)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&path)?;

        Ok(Self { file, index, path })
    }

    pub const fn index(&self) -> u32 {
        self.index
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.seek_read(buffer, offset)
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
        let read = image.read_at(6, &mut buffer).expect("offset read should work");

        assert_eq!(read, 4);
        assert_eq!(&buffer, b"6789");
        assert_eq!(image.len(), 16);
        assert_eq!(image.logical_sector_size(), 512);

        drop(image);
        std::fs::remove_file(path).expect("fixture should be removable");
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
