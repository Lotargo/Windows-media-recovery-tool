pub const PROTOCOL_VERSION: u32 = 1;
pub const WORKER_NAME: &str = "MediaRecovery.Worker";

const _: () = assert!(PROTOCOL_VERSION > 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerArchitecture {
    X86,
    X64,
    Arm64,
    Unknown,
}

impl WorkerArchitecture {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
            Self::Unknown => "unknown",
        }
    }
}

pub fn current_architecture() -> WorkerArchitecture {
    match std::env::consts::ARCH {
        "x86" => WorkerArchitecture::X86,
        "x86_64" => WorkerArchitecture::X64,
        "aarch64" => WorkerArchitecture::Arm64,
        _ => WorkerArchitecture::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerCapabilities {
    pub protocol_version: u32,
    pub architecture: WorkerArchitecture,
    pub physical_drive_access: bool,
    pub raw_image_source: bool,
}

impl WorkerCapabilities {
    pub fn stage_one_foundation() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            architecture: current_architecture(),
            physical_drive_access: true,
            raw_image_source: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_constants_have_expected_values() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(WORKER_NAME, "MediaRecovery.Worker");
    }

    #[test]
    fn worker_architecture_string_representations() {
        assert_eq!(WorkerArchitecture::X86.as_str(), "x86");
        assert_eq!(WorkerArchitecture::X64.as_str(), "x64");
        assert_eq!(WorkerArchitecture::Arm64.as_str(), "arm64");
        assert_eq!(WorkerArchitecture::Unknown.as_str(), "unknown");
    }

    #[test]
    fn current_architecture_has_stable_wire_name() {
        let arch = current_architecture();
        assert_ne!(arch, WorkerArchitecture::Unknown);
        assert!(!arch.as_str().is_empty());
    }

    #[test]
    fn stage_one_foundation_reports_storage_sources() {
        let capabilities = WorkerCapabilities::stage_one_foundation();
        assert_eq!(capabilities.protocol_version, PROTOCOL_VERSION);
        assert_eq!(capabilities.architecture, current_architecture());
        assert!(capabilities.raw_image_source);
        assert!(capabilities.physical_drive_access);

        let cloned = capabilities.clone();
        assert_eq!(capabilities, cloned);

        let debug_str = format!("{capabilities:?}");
        assert!(debug_str.contains("WorkerCapabilities"));
    }
}
