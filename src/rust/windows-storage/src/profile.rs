use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::mem::{MaybeUninit, size_of};
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
const MAX_STORAGE_DESCRIPTOR_BYTES: usize = 64 * 1024;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "DeviceIoControl"]
    fn device_io_control(
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
    _additional_parameters: [u8; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StorageDescriptorHeader {
    _version: u32,
    size: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GetLengthInformation {
    length: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DiskGeometry {
    _cylinders: i64,
    _media_type: i32,
    _tracks_per_cylinder: u32,
    _sectors_per_track: u32,
    bytes_per_sector: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StorageAccessAlignmentDescriptor {
    _version: u32,
    _size: u32,
    bytes_per_cache_line: u32,
    bytes_offset_for_cache_alignment: u32,
    bytes_per_logical_sector: u32,
    bytes_per_physical_sector: u32,
    bytes_offset_for_sector_alignment: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DeviceSeekPenaltyDescriptor {
    _version: u32,
    _size: u32,
    incurs_seek_penalty: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DeviceTrimDescriptor {
    _version: u32,
    _size: u32,
    trim_enabled: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct StorageDeviceDescriptorHeader {
    _version: u32,
    _size: u32,
    _device_type: u8,
    _device_type_modifier: u8,
    removable_media: u8,
    command_queueing: u8,
    vendor_id_offset: u32,
    product_id_offset: u32,
    product_revision_offset: u32,
    serial_number_offset: u32,
    bus_type: i32,
    _raw_properties_length: u32,
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
pub enum StorageKind {
    Hdd,
    Ssd,
    Nvme,
    Removable,
    SdMmc,
    Virtual,
    Unknown,
}

impl StorageKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hdd => "hdd",
            Self::Ssd => "ssd",
            Self::Nvme => "nvme",
            Self::Removable => "removable",
            Self::SdMmc => "sd-mmc",
            Self::Virtual => "virtual",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageIdentity {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub revision: Option<String>,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StorageAlignment {
    pub bytes_per_cache_line: Option<u32>,
    pub bytes_offset_for_cache_alignment: Option<u32>,
    pub bytes_offset_for_sector_alignment: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageProfile {
    pub length_bytes: u64,
    pub logical_sector_size: u32,
    pub physical_sector_size: Option<u32>,
    pub incurs_seek_penalty: Option<bool>,
    pub trim_enabled: Option<bool>,
    pub bus_type: Option<StorageBusType>,
    pub removable: Option<bool>,
    pub command_queueing: Option<bool>,
    pub kind: StorageKind,
    pub identity: StorageIdentity,
    pub alignment: StorageAlignment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceDescriptorInfo {
    bus_type: StorageBusType,
    removable: bool,
    command_queueing: bool,
    identity: StorageIdentity,
}

pub(crate) fn query_storage_profile(file: &File) -> io::Result<StorageProfile> {
    let length_bytes = query_length(file)?;
    let alignment =
        query_property::<StorageAccessAlignmentDescriptor>(file, STORAGE_ACCESS_ALIGNMENT_PROPERTY)
            .ok();
    let geometry = query_without_input::<DiskGeometry>(file, IOCTL_DISK_GET_DRIVE_GEOMETRY).ok();

    let logical_sector_size = alignment
        .map(|value| value.bytes_per_logical_sector)
        .filter(|value| *value != 0)
        .or_else(|| {
            geometry
                .map(|value| value.bytes_per_sector)
                .filter(|value| *value != 0)
        })
        .ok_or_else(|| io::Error::other("Windows did not report a logical sector size"))?;

    let physical_sector_size = alignment
        .map(|value| value.bytes_per_physical_sector)
        .filter(|value| *value != 0);

    let incurs_seek_penalty =
        query_property::<DeviceSeekPenaltyDescriptor>(file, STORAGE_DEVICE_SEEK_PENALTY_PROPERTY)
            .ok()
            .map(|value| value.incurs_seek_penalty != 0);

    let trim_enabled = query_property::<DeviceTrimDescriptor>(file, STORAGE_DEVICE_TRIM_PROPERTY)
        .ok()
        .map(|value| value.trim_enabled != 0);

    let device = query_device_descriptor(file).ok();
    let bus_type = device.as_ref().map(|value| value.bus_type);
    let removable = device.as_ref().map(|value| value.removable);
    let command_queueing = device.as_ref().map(|value| value.command_queueing);
    let kind = classify_storage(bus_type, incurs_seek_penalty, removable);

    Ok(StorageProfile {
        length_bytes,
        logical_sector_size,
        physical_sector_size,
        incurs_seek_penalty,
        trim_enabled,
        bus_type,
        removable,
        command_queueing,
        kind,
        identity: device
            .as_ref()
            .map(|value| value.identity.clone())
            .unwrap_or_default(),
        alignment: StorageAlignment {
            bytes_per_cache_line: alignment
                .map(|value| value.bytes_per_cache_line)
                .filter(|value| *value != 0),
            bytes_offset_for_cache_alignment: alignment
                .map(|value| value.bytes_offset_for_cache_alignment),
            bytes_offset_for_sector_alignment: alignment
                .map(|value| value.bytes_offset_for_sector_alignment),
        },
    })
}

pub fn classify_storage(
    bus_type: Option<StorageBusType>,
    incurs_seek_penalty: Option<bool>,
    removable: Option<bool>,
) -> StorageKind {
    match bus_type {
        Some(StorageBusType::Nvme) => StorageKind::Nvme,
        Some(StorageBusType::Sd | StorageBusType::Mmc) => StorageKind::SdMmc,
        Some(StorageBusType::Virtual | StorageBusType::FileBackedVirtual) => StorageKind::Virtual,
        _ => match incurs_seek_penalty {
            Some(true) => StorageKind::Hdd,
            Some(false) if removable == Some(true) => StorageKind::Removable,
            Some(false) => StorageKind::Ssd,
            None if removable == Some(true) => StorageKind::Removable,
            None => StorageKind::Unknown,
        },
    }
}

fn query_length(file: &File) -> io::Result<u64> {
    let value = query_without_input::<GetLengthInformation>(file, IOCTL_DISK_GET_LENGTH_INFO)?;
    u64::try_from(value.length)
        .map_err(|_| io::Error::other("Windows reported a negative disk length"))
}

fn query_device_descriptor(file: &File) -> io::Result<DeviceDescriptorInfo> {
    let query = StoragePropertyQuery {
        property_id: STORAGE_DEVICE_PROPERTY,
        query_type: PROPERTY_STANDARD_QUERY,
        _additional_parameters: [0],
    };
    let header: StorageDescriptorHeader =
        query_with_input(file, IOCTL_STORAGE_QUERY_PROPERTY, &query)?;
    let descriptor_size = header.size as usize;

    if descriptor_size < size_of::<StorageDeviceDescriptorHeader>()
        || descriptor_size > MAX_STORAGE_DESCRIPTOR_BYTES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Windows reported an invalid storage descriptor size",
        ));
    }

    let mut buffer = vec![0_u8; descriptor_size];
    let mut bytes_returned = 0_u32;
    let result = unsafe {
        device_io_control(
            file.as_raw_handle(),
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&query as *const StoragePropertyQuery).cast(),
            size_of::<StoragePropertyQuery>() as u32,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut bytes_returned,
            null_mut(),
        )
    };

    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    let bytes_returned = bytes_returned as usize;
    if bytes_returned < size_of::<StorageDeviceDescriptorHeader>() || bytes_returned > buffer.len()
    {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Windows returned a truncated storage descriptor",
        ));
    }

    buffer.truncate(bytes_returned);
    parse_device_descriptor(&buffer)
}

fn parse_device_descriptor(buffer: &[u8]) -> io::Result<DeviceDescriptorInfo> {
    if buffer.len() < size_of::<StorageDeviceDescriptorHeader>() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "storage device descriptor header is truncated",
        ));
    }

    let descriptor =
        unsafe { (buffer.as_ptr().cast::<StorageDeviceDescriptorHeader>()).read_unaligned() };

    Ok(DeviceDescriptorInfo {
        bus_type: StorageBusType::from_raw(descriptor.bus_type),
        removable: descriptor.removable_media != 0,
        command_queueing: descriptor.command_queueing != 0,
        identity: StorageIdentity {
            vendor: descriptor_string(buffer, descriptor.vendor_id_offset),
            model: descriptor_string(buffer, descriptor.product_id_offset),
            revision: descriptor_string(buffer, descriptor.product_revision_offset),
            serial_number: descriptor_string(buffer, descriptor.serial_number_offset),
        },
    })
}

fn descriptor_string(buffer: &[u8], offset: u32) -> Option<String> {
    if offset == 0 {
        return None;
    }

    let start = offset as usize;
    let tail = buffer.get(start..)?;
    let end = tail
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(tail.len());
    let value = String::from_utf8_lossy(&tail[..end]).trim().to_owned();

    (!value.is_empty()).then_some(value)
}

fn query_property<T: Copy + Default>(file: &File, property_id: i32) -> io::Result<T> {
    let query = StoragePropertyQuery {
        property_id,
        query_type: PROPERTY_STANDARD_QUERY,
        _additional_parameters: [0],
    };

    query_with_input(file, IOCTL_STORAGE_QUERY_PROPERTY, &query)
}

fn query_without_input<T: Copy + Default>(file: &File, control_code: u32) -> io::Result<T> {
    let mut output = MaybeUninit::<T>::zeroed();
    let mut bytes_returned = 0_u32;

    let result = unsafe {
        device_io_control(
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
        device_io_control(
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
        let bus_cases = [
            (0, StorageBusType::Unknown, "unknown"),
            (1, StorageBusType::Scsi, "scsi"),
            (2, StorageBusType::Atapi, "atapi"),
            (3, StorageBusType::Ata, "ata"),
            (7, StorageBusType::Usb, "usb"),
            (8, StorageBusType::Raid, "raid"),
            (9, StorageBusType::Iscsi, "iscsi"),
            (10, StorageBusType::Sas, "sas"),
            (11, StorageBusType::Sata, "sata"),
            (12, StorageBusType::Sd, "sd"),
            (13, StorageBusType::Mmc, "mmc"),
            (14, StorageBusType::Virtual, "virtual"),
            (15, StorageBusType::FileBackedVirtual, "file-backed-virtual"),
            (16, StorageBusType::Spaces, "spaces"),
            (17, StorageBusType::Nvme, "nvme"),
        ];

        for (raw, expected_variant, expected_str) in bus_cases {
            let bus = StorageBusType::from_raw(raw);
            assert_eq!(bus, expected_variant);
            assert_eq!(bus.as_str(), expected_str);
        }
    }

    #[test]
    fn unknown_bus_values_are_preserved() {
        let unknown = StorageBusType::from_raw(1234);
        assert_eq!(unknown, StorageBusType::Other(1234));
        assert_eq!(unknown.as_str(), "other");
    }

    #[test]
    fn storage_kind_string_representations() {
        assert_eq!(StorageKind::Hdd.as_str(), "hdd");
        assert_eq!(StorageKind::Ssd.as_str(), "ssd");
        assert_eq!(StorageKind::Nvme.as_str(), "nvme");
        assert_eq!(StorageKind::Removable.as_str(), "removable");
        assert_eq!(StorageKind::SdMmc.as_str(), "sd-mmc");
        assert_eq!(StorageKind::Virtual.as_str(), "virtual");
        assert_eq!(StorageKind::Unknown.as_str(), "unknown");
    }

    #[test]
    fn storage_classification_prefers_strong_device_signals() {
        // NVMe bus overrides seek penalty/removable
        assert_eq!(
            classify_storage(Some(StorageBusType::Nvme), Some(true), Some(true)),
            StorageKind::Nvme
        );

        // SD / MMC bus
        assert_eq!(
            classify_storage(Some(StorageBusType::Sd), None, Some(true)),
            StorageKind::SdMmc
        );
        assert_eq!(
            classify_storage(Some(StorageBusType::Mmc), None, Some(false)),
            StorageKind::SdMmc
        );

        // Virtual disks
        assert_eq!(
            classify_storage(Some(StorageBusType::Virtual), Some(false), Some(false)),
            StorageKind::Virtual
        );
        assert_eq!(
            classify_storage(Some(StorageBusType::FileBackedVirtual), None, None),
            StorageKind::Virtual
        );

        // HDD by seek penalty
        assert_eq!(
            classify_storage(Some(StorageBusType::Sata), Some(true), Some(false)),
            StorageKind::Hdd
        );

        // SSD by no seek penalty on non-removable
        assert_eq!(
            classify_storage(Some(StorageBusType::Sata), Some(false), Some(false)),
            StorageKind::Ssd
        );
        assert_eq!(classify_storage(None, Some(false), None), StorageKind::Ssd);

        // Removable drive
        assert_eq!(
            classify_storage(Some(StorageBusType::Usb), Some(false), Some(true)),
            StorageKind::Removable
        );
        assert_eq!(
            classify_storage(None, None, Some(true)),
            StorageKind::Removable
        );

        // Unknown
        assert_eq!(classify_storage(None, None, None), StorageKind::Unknown);
        assert_eq!(
            classify_storage(Some(StorageBusType::Ata), None, Some(false)),
            StorageKind::Unknown
        );
    }

    #[test]
    fn descriptor_strings_are_read_from_offsets() {
        let mut buffer = vec![0_u8; 32];
        buffer.extend_from_slice(b"  Example Device  \0");

        assert_eq!(
            descriptor_string(&buffer, 32),
            Some("Example Device".to_owned())
        );
        assert_eq!(descriptor_string(&buffer, 0), None);
        assert_eq!(descriptor_string(&buffer, 4096), None);
    }

    #[test]
    fn parse_device_descriptor_reads_full_header_and_strings() {
        let header_len = size_of::<StorageDeviceDescriptorHeader>();
        let mut buffer = vec![0_u8; header_len];

        let vendor_str = b"SanDisk\0";
        let product_str = b"Ultra USB 3.0\0";
        let revision_str = b"1.00\0";
        let serial_str = b"SN123456789\0";

        let vendor_offset = header_len;
        let product_offset = vendor_offset + vendor_str.len();
        let revision_offset = product_offset + product_str.len();
        let serial_offset = revision_offset + revision_str.len();

        // Populate header fields
        let header_ptr = buffer.as_mut_ptr().cast::<StorageDeviceDescriptorHeader>();
        unsafe {
            (*header_ptr).removable_media = 1;
            (*header_ptr).command_queueing = 1;
            (*header_ptr).bus_type = 7; // USB
            (*header_ptr).vendor_id_offset = vendor_offset as u32;
            (*header_ptr).product_id_offset = product_offset as u32;
            (*header_ptr).product_revision_offset = revision_offset as u32;
            (*header_ptr).serial_number_offset = serial_offset as u32;
        }

        // Append string data
        buffer.extend_from_slice(vendor_str);
        buffer.extend_from_slice(product_str);
        buffer.extend_from_slice(revision_str);
        buffer.extend_from_slice(serial_str);

        let parsed = parse_device_descriptor(&buffer).unwrap();
        assert_eq!(parsed.bus_type, StorageBusType::Usb);
        assert!(parsed.removable);
        assert!(parsed.command_queueing);
        assert_eq!(parsed.identity.vendor.as_deref(), Some("SanDisk"));
        assert_eq!(parsed.identity.model.as_deref(), Some("Ultra USB 3.0"));
        assert_eq!(parsed.identity.revision.as_deref(), Some("1.00"));
        assert_eq!(
            parsed.identity.serial_number.as_deref(),
            Some("SN123456789")
        );
    }

    #[test]
    fn parse_device_descriptor_rejects_truncated_buffer() {
        let short_buffer = [0_u8; 10];
        assert_eq!(
            parse_device_descriptor(&short_buffer).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn parse_device_descriptor_and_string_fuzz_never_panics() {
        struct Lcg(u64);
        impl Lcg {
            fn next_u64(&mut self) -> u64 {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
                self.0
            }
            fn next_u8(&mut self) -> u8 {
                (self.next_u64() >> 56) as u8
            }
            fn next_u32(&mut self) -> u32 {
                (self.next_u64() >> 32) as u32
            }
        }

        let mut rng = Lcg(0x9876_5432_10fe_dcba);

        for len in 0..512 {
            let mut byte_buf = Vec::with_capacity(len);
            for _ in 0..len {
                byte_buf.push(rng.next_u8());
            }
            let _ = parse_device_descriptor(&byte_buf);
            let _ = descriptor_string(&byte_buf, rng.next_u32() % (len.max(1) as u32 * 2));
        }
    }
}
