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
pub struct CpuProfile {
    pub architecture: CpuArchitecture,
    pub pointer_width_bits: u32,
    pub logical_processors: usize,
    pub simd: SimdFeatures,
}

impl CpuProfile {
    pub fn detect() -> Self {
        Self {
            architecture: current_architecture(),
            pointer_width_bits: usize::BITS,
            logical_processors: std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1),
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
