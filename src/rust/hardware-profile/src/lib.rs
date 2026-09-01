use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::ptr::null_mut;

const RELATION_PROCESSOR_CORE: i32 = 0;
const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
const PROCESSOR_RECORD_HEADER_BYTES: usize = 8;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "GetLogicalProcessorInformationEx"]
    fn get_logical_processor_information_ex(
        relationship_type: i32,
        buffer: *mut c_void,
        returned_length: *mut u32,
    ) -> i32;

    #[link_name = "GetActiveProcessorGroupCount"]
    fn get_active_processor_group_count() -> u16;

    #[link_name = "GlobalMemoryStatusEx"]
    fn global_memory_status_ex(buffer: *mut MemoryStatusEx) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_physical: u64,
    available_physical: u64,
    _total_page_file: u64,
    _available_page_file: u64,
    _total_virtual: u64,
    _available_virtual: u64,
    _available_extended_virtual: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuArchitecture {
    X86,
    X64,
    Arm64,
    Unknown,
}

impl CpuArchitecture {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SimdFeatures {
    pub sse2: bool,
    pub sse4_2: bool,
    pub avx2: bool,
    pub avx512f: bool,
    pub neon: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryProfile {
    pub total_physical_bytes: u64,
    pub available_physical_bytes: u64,
    pub memory_load_percent: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuProfile {
    pub architecture: CpuArchitecture,
    pub pointer_width_bits: u32,
    pub logical_processors: usize,
    pub physical_cores: Option<usize>,
    pub processor_groups: Option<u16>,
    pub memory: Option<MemoryProfile>,
    pub simd: SimdFeatures,
}

impl CpuProfile {
    pub fn detect() -> Self {
        let processor_groups = unsafe { get_active_processor_group_count() };

        Self {
            architecture: current_architecture(),
            pointer_width_bits: usize::BITS,
            logical_processors: std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1),
            physical_cores: query_physical_core_count().ok(),
            processor_groups: (processor_groups > 0).then_some(processor_groups),
            memory: query_memory_profile().ok(),
            simd: detect_simd(),
        }
    }
}

pub fn current_architecture() -> CpuArchitecture {
    match std::env::consts::ARCH {
        "x86" => CpuArchitecture::X86,
        "x86_64" => CpuArchitecture::X64,
        "aarch64" => CpuArchitecture::Arm64,
        _ => CpuArchitecture::Unknown,
    }
}

fn query_physical_core_count() -> io::Result<usize> {
    let mut required_bytes = 0_u32;
    let first_result = unsafe {
        get_logical_processor_information_ex(
            RELATION_PROCESSOR_CORE,
            null_mut(),
            &mut required_bytes,
        )
    };

    if first_result != 0 || required_bytes == 0 {
        return Err(io::Error::other(
            "Windows did not report processor topology buffer size",
        ));
    }

    let first_error = io::Error::last_os_error();
    if first_error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER) {
        return Err(first_error);
    }

    let mut buffer = vec![0_u8; required_bytes as usize];
    let result = unsafe {
        get_logical_processor_information_ex(
            RELATION_PROCESSOR_CORE,
            buffer.as_mut_ptr().cast(),
            &mut required_bytes,
        )
    };

    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    buffer.truncate(required_bytes as usize);
    count_processor_core_records(&buffer)
}

fn count_processor_core_records(buffer: &[u8]) -> io::Result<usize> {
    let mut offset = 0_usize;
    let mut count = 0_usize;

    while offset < buffer.len() {
        if buffer.len() - offset < PROCESSOR_RECORD_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "processor topology record header is truncated",
            ));
        }

        let relationship = unsafe { (buffer.as_ptr().add(offset).cast::<i32>()).read_unaligned() };
        let record_size = unsafe {
            (buffer
                .as_ptr()
                .add(offset + size_of::<i32>())
                .cast::<u32>())
            .read_unaligned()
        } as usize;

        if record_size < PROCESSOR_RECORD_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "processor topology record size is invalid",
            ));
        }

        let end = offset
            .checked_add(record_size)
            .ok_or_else(|| io::Error::other("processor topology size overflow"))?;
        if end > buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "processor topology record is truncated",
            ));
        }

        if relationship == RELATION_PROCESSOR_CORE {
            count += 1;
        }

        offset = end;
    }

    if count == 0 {
        return Err(io::Error::other("Windows reported zero physical cores"));
    }

    Ok(count)
}

fn query_memory_profile() -> io::Result<MemoryProfile> {
    let mut status = MemoryStatusEx {
        length: size_of::<MemoryStatusEx>() as u32,
        ..MemoryStatusEx::default()
    };

    let result = unsafe { global_memory_status_ex(&mut status) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(MemoryProfile {
        total_physical_bytes: status.total_physical,
        available_physical_bytes: status.available_physical,
        memory_load_percent: status.memory_load,
    })
}

fn detect_simd() -> SimdFeatures {
    let mut features = SimdFeatures::default();

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        features.sse2 = std::arch::is_x86_feature_detected!("sse2");
        features.sse4_2 = std::arch::is_x86_feature_detected!("sse4.2");
        features.avx2 = std::arch::is_x86_feature_detected!("avx2");
        features.avx512f = std::arch::is_x86_feature_detected!("avx512f");
    }

    #[cfg(target_arch = "aarch64")]
    {
        features.neon = true;
    }

    features
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_release_architectures_have_stable_names() {
        let architecture = current_architecture();
        assert_ne!(architecture, CpuArchitecture::Unknown);
        assert!(!architecture.as_str().is_empty());
    }

    #[test]
    fn cpu_profile_has_at_least_one_logical_processor() {
        let profile = CpuProfile::detect();
        assert!(profile.logical_processors >= 1);
    }

    #[test]
    fn detected_windows_topology_is_sane_when_available() {
        let profile = CpuProfile::detect();

        if let Some(physical_cores) = profile.physical_cores {
            assert!(physical_cores >= 1);
        }

        if let Some(processor_groups) = profile.processor_groups {
            assert!(processor_groups >= 1);
        }
    }

    #[test]
    fn detected_memory_is_sane_when_available() {
        let profile = CpuProfile::detect();

        if let Some(memory) = profile.memory {
            assert!(memory.total_physical_bytes > 0);
            assert!(memory.available_physical_bytes <= memory.total_physical_bytes);
            assert!(memory.memory_load_percent <= 100);
        }
    }

    #[test]
    fn processor_record_parser_counts_core_records() {
        let mut buffer = Vec::new();
        for _ in 0..3 {
            buffer.extend_from_slice(&RELATION_PROCESSOR_CORE.to_ne_bytes());
            buffer.extend_from_slice(&(PROCESSOR_RECORD_HEADER_BYTES as u32).to_ne_bytes());
        }

        assert_eq!(count_processor_core_records(&buffer).unwrap(), 3);
    }

    #[test]
    fn pointer_width_matches_release_architecture() {
        let profile = CpuProfile::detect();

        match profile.architecture {
            CpuArchitecture::X86 => assert_eq!(profile.pointer_width_bits, 32),
            CpuArchitecture::X64 | CpuArchitecture::Arm64 => {
                assert_eq!(profile.pointer_width_bits, 64);
            }
            CpuArchitecture::Unknown => panic!("unsupported CI architecture"),
        }
    }
}
