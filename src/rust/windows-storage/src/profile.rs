use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};

const IOCTL_DISK_GET_DRIVE_GEOMETRY: u32 = 0x0007_0000;
const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007_405c;
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002d_1400;

const STORAGE_DEVICE_PROPERTY: i32 = 0;
const STORAGE_ACCESS_ALIGNMENT_PROPERTY: i32 = 6;
const STORAGE_DEVICE_SEEK_PENALTY_PROPERTY: i32 = 7;
const STORAGE_DEVICE_TRIM_PROPERTY: i32 = 8;
const PROPERTY_STANDARD_QUERY: i32 = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn DeviceIoControl(
        h_device: *mut c_void,
        io_control_code: u32,
        input_buffer: *const c_void,
        input_buffer_size: u32,
        output_buffer: *mut c_void,
        output_buffer_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StoragePropertyQuery {
    property_id: i32,
    query_type: i32,
    additional_parameters: [u8; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GetLengthInformation {
    length: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DiskGeometry {
    cylinders: i64,
    media_type: i32,
    tracks_per_cylinder: u32,
    sectors_per_track: u32,
    bytes_per_sector: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StorageAccessAlignmentDescriptor {
    version: u32,
    size: u32,
    bytes_per_cache_line: u32,
    bytes_offset_for_cache_alignment: u32,
    bytes_per_logical_sector: u32,
    bytes_per_physical_sector: u32,
    bytes_offset_for_sector_alignment: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DeviceSeekPenaltyDescriptor {
    version: u32,
    size: u32,
    incurs_seek_penalty: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DeviceTrimDescriptor {
    version: u32,
    size: u32,
    trim_enabled: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StorageDeviceDescriptorHeader {
    version: u32,
    size: u32,
    device_type: u8,
    device_type_modifier: u8,
    removable_media: u8,
    command_queueing: u8,
    vendor_id_offset: u32,
    product_id_offset: u32,
    product_revision_offset: u32,
    serial_number_offset: u32,
    bus_type: i32,
    raw_properties_length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBusType {
    Unknown,
    Scsi,
    Atapi,
    Ata,
    Usb,
    Raid,
    Iscsi,
    Sas,
    Sata,
    Sd,
    Mmc,
    Virtual,
    FileBackedVirtual,
    Spaces,
    Nvme,
    Other(i32),
}

impl StorageBusType {
    pub const fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::Scsi,
            2 => Self::Atapi,
            3 => Self::Ata,
            7 => Self::Usb,
            8 => Self::Raid,
            9 => Self::Iscsi,
            10 => Self::Sas,
            11 => Self::Sata,
            12 => Self::Sd,
            13 => Self::Mmc,
            14 => Self::Virtual,
            15 => Self::FileBackedVirtual,
            16 => Self::Spaces,
            17 => Self::Nvme,
            other => Self::Other(other),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Scsi => "scsi",
            Self::Atapi => "atapi",
            Self::Ata => "ata",
            Self::Usb => "usb",
            Self::Raid => "raid",
            Self::Iscsi => "iscsi",
            Self::Sas => "sas",
            Self::Sata => "sata",
            Self::Sd => "sd",
            Self::Mmc => "mmc",
            Self::Virtual => "virtual",
            Self::FileBackedVirtual => "file-backed-virtual",
            Self::Spaces => "spaces",
            Self::Nvme => "nvme",
            Self::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageProfile {
    pub length_bytes: u64,
    pub logical_sector_size: u32,
    pub physical_sector_size: Option<u32>,
    pub incurs_seek_penalty: Option<bool>,
    pub trim_enabled: Option<bool>,
    pub bus_type: Option<StorageBusType>,
    pub removable: Option<bool>,
}

pub(crate) fn query_storage_profile(file: &File) -> io::Result<StorageProfile> {
    let length_bytes = query_length(file)?;
    let alignment = query_property::<StorageAccessAlignmentDescriptor>(
        file,
        STORAGE_ACCESS_ALIGNMENT_PROPERTY,
    )
    .ok();
    let geometry = query_without_input::<DiskGeometry>(file, IOCTL_DISK_GET_DRIVE_GEOMETRY).ok();

    let logical_sector_size = alignment
        .map(|value| value.bytes_per_logical_sector)
        .filter(|value| *value != 0)
        .or_else(|| geometry.map(|value| value.bytes_per_sector).filter(|value| *value != 0))
        .ok_or_else(|| io::Error::other("Windows did not report a logical sector size"))?;

    let physical_sector_size = alignment
        .map(|value| value.bytes_per_physical_sector)
        .filter(|value| *value != 0);

    let incurs_seek_penalty = query_property::<DeviceSeekPenaltyDescriptor>(
        file,
        STORAGE_DEVICE_SEEK_PENALTY_PROPERTY,
    )
    .ok()
    .map(|value| value.incurs_seek_penalty != 0);

    let trim_enabled = query_property::<DeviceTrimDescriptor>(file, STORAGE_DEVICE_TRIM_PROPERTY)
        .ok()
        .map(|value| value.trim_enabled != 0);

    let device = query_property::<StorageDeviceDescriptorHeader>(file, STORAGE_DEVICE_PROPERTY).ok();

    Ok(StorageProfile {
        length_bytes,
        logical_sector_size,
        physical_sector_size,
        incurs_seek_penalty,
        trim_enabled,
        bus_type: device.map(|value| StorageBusType::from_raw(value.bus_type)),
        removable: device.map(|value| value.removable_media != 0),
    })
}

fn query_length(file: &File) -> io::Result<u64> {
    let value = query_without_input::<GetLengthInformation>(file, IOCTL_DISK_GET_LENGTH_INFO)?;
    u64::try_from(value.length).map_err(|_| io::Error::other("Windows reported a negative disk length"))
}

fn query_property<T: Copy + Default>(file: &File, property_id: i32) -> io::Result<T> {
    let query = StoragePropertyQuery {
        property_id,
        query_type: PROPERTY_STANDARD_QUERY,
        additional_parameters: [0],
    };

    query_with_input(file, IOCTL_STORAGE_QUERY_PROPERTY, &query)
}

fn query_without_input<T: Copy + Default>(file: &File, control_code: u32) -> io::Result<T> {
    let mut output = MaybeUninit::<T>::zeroed();
    let mut bytes_returned = 0_u32;

    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            control_code,
            null(),
            0,
            output.as_mut_ptr().cast(),
            size_of::<T>() as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };

    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    if bytes_returned < size_of::<T>() as u32 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "DeviceIoControl returned a truncated descriptor",
        ));
    }

    Ok(unsafe { output.assume_init() })
}

fn query_with_input<TInput, TOutput: Copy + Default>(
    file: &File,
    control_code: u32,
    input: &TInput,
) -> io::Result<TOutput> {
    let mut output = MaybeUninit::<TOutput>::zeroed();
    let mut bytes_returned = 0_u32;

    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            control_code,
            (input as *const TInput).cast(),
            size_of::<TInput>() as u32,
            output.as_mut_ptr().cast(),
            size_of::<TOutput>() as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };

    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    if bytes_returned < size_of::<TOutput>() as u32 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "DeviceIoControl returned a truncated descriptor",
        ));
    }

    Ok(unsafe { output.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_bus_types_have_stable_names() {
        assert_eq!(StorageBusType::from_raw(7).as_str(), "usb");
        assert_eq!(StorageBusType::from_raw(11).as_str(), "sata");
        assert_eq!(StorageBusType::from_raw(17).as_str(), "nvme");
    }

    #[test]
    fn unknown_bus_values_are_preserved() {
        assert_eq!(StorageBusType::from_raw(1234), StorageBusType::Other(1234));
    }
}
