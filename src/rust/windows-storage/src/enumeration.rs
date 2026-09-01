use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};

const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;
const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
const ERROR_MORE_DATA: i32 = 234;
const INITIAL_DEVICE_BUFFER_CHARS: usize = 16 * 1024;
const INITIAL_EXTENTS_BUFFER_BYTES: usize = 1024;
const MAX_EXTENTS_BUFFER_BYTES: usize = 64 * 1024;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "QueryDosDeviceW"]
    fn query_dos_device_w(
        device_name: *const u16,
        target_path: *mut u16,
        max_chars: u32,
    ) -> u32;

    #[link_name = "GetLogicalDriveStringsW"]
    fn get_logical_drive_strings_w(buffer_length: u32, buffer: *mut u16) -> u32;

    #[link_name = "GetDriveTypeW"]
    fn get_drive_type_w(root_path_name: *const u16) -> u32;

    #[link_name = "DeviceIoControl"]
    fn device_io_control(
        device: *mut c_void,
        control_code: u32,
        input_buffer: *const c_void,
        input_buffer_size: u32,
        output_buffer: *mut c_void,
        output_buffer_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DiskExtent {
    disk_number: u32,
    _starting_offset: i64,
    _extent_length: i64,
}

#[repr(C)]
struct VolumeDiskExtentsOne {
    _number_of_disk_extents: u32,
    extents: [DiskExtent; 1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeDriveType {
    Unknown,
    NoRootDirectory,
    Removable,
    Fixed,
    Remote,
    Cdrom,
    RamDisk,
    Other(u32),
}

impl VolumeDriveType {
    pub const fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::NoRootDirectory,
            2 => Self::Removable,
            3 => Self::Fixed,
            4 => Self::Remote,
            5 => Self::Cdrom,
            6 => Self::RamDisk,
            other => Self::Other(other),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NoRootDirectory => "no-root-directory",
            Self::Removable => "removable",
            Self::Fixed => "fixed",
            Self::Remote => "remote",
            Self::Cdrom => "cdrom",
            Self::RamDisk => "ram-disk",
            Self::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedVolume {
    pub root_path: String,
    pub drive_type: VolumeDriveType,
    pub disk_numbers: Vec<u32>,
    pub mapping_error: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPhysicalDrive {
    pub index: u32,
    pub path: String,
    pub mount_points: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDiscovery {
    pub physical_drives: Vec<DiscoveredPhysicalDrive>,
    pub mounted_volumes: Vec<MountedVolume>,
}

pub fn enumerate_storage_sources() -> io::Result<StorageDiscovery> {
    let mut physical_drives = enumerate_physical_drives()?;
    let mounted_volumes = enumerate_mounted_volumes()?;

    for drive in &mut physical_drives {
        drive.mount_points = mounted_volumes
            .iter()
            .filter(|volume| volume.disk_numbers.contains(&drive.index))
            .map(|volume| volume.root_path.clone())
            .collect();
    }

    Ok(StorageDiscovery {
        physical_drives,
        mounted_volumes,
    })
}

fn enumerate_physical_drives() -> io::Result<Vec<DiscoveredPhysicalDrive>> {
    let mut drives = enumerate_dos_device_names()?
        .into_iter()
        .filter_map(|name| parse_physical_drive_index(&name))
        .map(|index| DiscoveredPhysicalDrive {
            index,
            path: super::physical_drive_path(index),
            mount_points: Vec::new(),
        })
        .collect::<Vec<_>>();

    drives.sort_by_key(|drive| drive.index);
    drives.dedup_by_key(|drive| drive.index);
    Ok(drives)
}

fn enumerate_mounted_volumes() -> io::Result<Vec<MountedVolume>> {
    let mut volumes = Vec::new();

    for root_path in enumerate_logical_drive_roots()? {
        let drive_type = query_drive_type(&root_path);
        let volume_path = drive_root_to_device_path(&root_path);

        let (disk_numbers, mapping_error) = match open_volume_query_handle(&volume_path)
            .and_then(|file| query_volume_disk_numbers(&file))
        {
            Ok(numbers) => (numbers, None),
            Err(error) => (Vec::new(), error.raw_os_error()),
        };

        volumes.push(MountedVolume {
            root_path,
            drive_type,
            disk_numbers,
            mapping_error,
        });
    }

    Ok(volumes)
}

fn enumerate_dos_device_names() -> io::Result<Vec<String>> {
    let mut capacity = INITIAL_DEVICE_BUFFER_CHARS;

    loop {
        let mut buffer = vec![0_u16; capacity];
        let copied = unsafe {
            query_dos_device_w(null(), buffer.as_mut_ptr(), buffer.len() as u32)
        };

        if copied != 0 {
            buffer.truncate(copied as usize);
            return Ok(parse_multi_sz(&buffer));
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER) {
            return Err(error);
        }

        capacity = capacity
            .checked_mul(2)
            .ok_or_else(|| io::Error::other("DOS device enumeration buffer overflow"))?;
    }
}

fn enumerate_logical_drive_roots() -> io::Result<Vec<String>> {
    let required = unsafe { get_logical_drive_strings_w(0, null_mut()) };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut buffer = vec![0_u16; required as usize + 1];
    let copied = unsafe {
        get_logical_drive_strings_w(buffer.len() as u32, buffer.as_mut_ptr())
    };
    if copied == 0 {
        return Err(io::Error::last_os_error());
    }

    buffer.truncate(copied as usize + 1);
    Ok(parse_multi_sz(&buffer))
}

fn query_drive_type(root_path: &str) -> VolumeDriveType {
    let wide = wide_null(root_path);
    VolumeDriveType::from_raw(unsafe { get_drive_type_w(wide.as_ptr()) })
}

fn open_volume_query_handle(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
}

fn query_volume_disk_numbers(file: &File) -> io::Result<Vec<u32>> {
    let mut capacity = INITIAL_EXTENTS_BUFFER_BYTES;

    loop {
        let mut buffer = vec![0_u8; capacity];
        let mut bytes_returned = 0_u32;
        let result = unsafe {
            device_io_control(
                file.as_raw_handle(),
                IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                null(),
                0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut bytes_returned,
                null_mut(),
            )
        };

        if result != 0 {
            return parse_volume_disk_extents(&buffer[..bytes_returned as usize]);
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_MORE_DATA) || capacity >= MAX_EXTENTS_BUFFER_BYTES {
            return Err(error);
        }

        capacity = (capacity * 2).min(MAX_EXTENTS_BUFFER_BYTES);
    }
}

fn parse_volume_disk_extents(buffer: &[u8]) -> io::Result<Vec<u32>> {
    if buffer.len() < offset_of!(VolumeDiskExtentsOne, extents) {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "volume extent response is too small",
        ));
    }

    let count = unsafe { (buffer.as_ptr().cast::<u32>()).read_unaligned() } as usize;
    let extent_offset = offset_of!(VolumeDiskExtentsOne, extents);
    let extents_size = count
        .checked_mul(size_of::<DiskExtent>())
        .ok_or_else(|| io::Error::other("volume extent response size overflow"))?;
    let required = extent_offset
        .checked_add(extents_size)
        .ok_or_else(|| io::Error::other("volume extent response size overflow"))?;

    if buffer.len() < required {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "volume extent response is truncated",
        ));
    }

    let mut disk_numbers = Vec::with_capacity(count);
    for index in 0..count {
        let offset = extent_offset + index * size_of::<DiskExtent>();
        let extent = unsafe {
            (buffer.as_ptr().add(offset).cast::<DiskExtent>()).read_unaligned()
        };
        disk_numbers.push(extent.disk_number);
    }

    disk_numbers.sort_unstable();
    disk_numbers.dedup();
    Ok(disk_numbers)
}

fn parse_physical_drive_index(name: &str) -> Option<u32> {
    name.strip_prefix("PhysicalDrive")?.parse().ok()
}

fn drive_root_to_device_path(root_path: &str) -> String {
    let trimmed = root_path.trim_end_matches('\\');
    format!(r"\\.\{trimmed}")
}

fn parse_multi_sz(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|value| *value == 0)
        .take_while(|slice| !slice.is_empty())
        .filter_map(|slice| String::from_utf16(slice).ok())
        .collect()
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_drive_names_are_parsed_without_probing() {
        assert_eq!(parse_physical_drive_index("PhysicalDrive0"), Some(0));
        assert_eq!(parse_physical_drive_index("PhysicalDrive27"), Some(27));
        assert_eq!(parse_physical_drive_index("HarddiskVolume1"), None);
    }

    #[test]
    fn drive_root_is_converted_to_volume_device_path() {
        assert_eq!(drive_root_to_device_path(r"C:\"), r"\\.\C:");
    }

    #[test]
    fn multi_sz_parser_stops_at_double_null() {
        let buffer = [
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            0,
            b'D' as u16,
            b':' as u16,
            b'\\' as u16,
            0,
            0,
        ];

        assert_eq!(parse_multi_sz(&buffer), vec![r"C:\", r"D:\"]);
    }

    #[test]
    fn drive_type_names_are_stable() {
        assert_eq!(VolumeDriveType::from_raw(2).as_str(), "removable");
        assert_eq!(VolumeDriveType::from_raw(3).as_str(), "fixed");
    }
}
