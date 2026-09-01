use hardware_profile::CpuProfile;
use recovery_core::{BlockSource, ENGINE_NAME};
use recovery_protocol::{WORKER_NAME, WorkerCapabilities};
use windows_storage::{RawDiskImage, WindowsPhysicalDrive};

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next();

    match command.as_deref() {
        Some("--version") => print_version(),
        Some("--capabilities") => print_capabilities(),
        Some("--hardware-profile") => print_hardware_profile(),
        Some("--raw-image-info") => match args.next() {
            Some(path) => {
                if let Err(error) = print_raw_image_info(&path) {
                    eprintln!("Failed to inspect raw image: {error}");
                    std::process::exit(1);
                }
            }
            None => {
                eprintln!("--raw-image-info requires a path");
                std::process::exit(2);
            }
        },
        Some("--physical-drive-info") => match args.next() {
            Some(index) => match index.parse::<u32>() {
                Ok(index) => {
                    if let Err(error) = print_physical_drive_info(index) {
                        eprintln!("Failed to inspect PhysicalDrive{index}: {error}");
                        std::process::exit(1);
                    }
                }
                Err(_) => {
                    eprintln!("--physical-drive-info requires a non-negative integer index");
                    std::process::exit(2);
                }
            },
            None => {
                eprintln!("--physical-drive-info requires a disk index");
                std::process::exit(2);
            }
        },
        Some("--help") | Some("-h") => print_help(),
        Some(other) => {
            eprintln!("Unknown argument: {other}");
            print_help();
            std::process::exit(2);
        }
        None => print_help(),
    }
}

fn print_version() {
    println!("{} {}", WORKER_NAME, env!("CARGO_PKG_VERSION"));
}

fn print_capabilities() {
    let capabilities = WorkerCapabilities::stage_one_foundation();

    println!(
        "{{\"worker\":\"{}\",\"engine\":\"{}\",\"version\":\"{}\",\"protocol_version\":{},\"architecture\":\"{}\",\"physical_drive_access\":{},\"raw_image_source\":{}}}",
        WORKER_NAME,
        ENGINE_NAME,
        env!("CARGO_PKG_VERSION"),
        capabilities.protocol_version,
        capabilities.architecture.as_str(),
        capabilities.physical_drive_access,
        capabilities.raw_image_source,
    );
}

fn print_hardware_profile() {
    let profile = CpuProfile::detect();

    println!(
        "{{\"architecture\":\"{}\",\"pointer_width_bits\":{},\"logical_processors\":{},\"simd\":{{\"sse2\":{},\"sse4_2\":{},\"avx2\":{},\"avx512f\":{},\"neon\":{}}}}}",
        profile.architecture.as_str(),
        profile.pointer_width_bits,
        profile.logical_processors,
        profile.simd.sse2,
        profile.simd.sse4_2,
        profile.simd.avx2,
        profile.simd.avx512f,
        profile.simd.neon,
    );
}

fn print_raw_image_info(path: &str) -> std::io::Result<()> {
    let image = RawDiskImage::open(path)?;

    println!(
        "{{\"length_bytes\":{},\"logical_sector_size\":{}}}",
        image.len(),
        image.logical_sector_size(),
    );

    Ok(())
}

fn print_physical_drive_info(index: u32) -> std::io::Result<()> {
    let drive = WindowsPhysicalDrive::open_read_only(index)?;
    let profile = drive.profile();
    let physical_sector_size = optional_number(profile.physical_sector_size);
    let seek_penalty = optional_bool(profile.incurs_seek_penalty);
    let trim_enabled = optional_bool(profile.trim_enabled);
    let bus_type = profile
        .bus_type
        .map(|value| format!("\"{}\"", value.as_str()))
        .unwrap_or_else(|| "null".to_owned());
    let removable = optional_bool(profile.removable);

    println!(
        "{{\"index\":{},\"path\":\"{}\",\"length_bytes\":{},\"logical_sector_size\":{},\"physical_sector_size\":{},\"incurs_seek_penalty\":{},\"trim_enabled\":{},\"bus_type\":{},\"removable\":{}}}",
        drive.index(),
        drive.path().replace('\\', "\\\\"),
        drive.len(),
        drive.logical_sector_size(),
        physical_sector_size,
        seek_penalty,
        trim_enabled,
        bus_type,
        removable,
    );

    Ok(())
}

fn optional_number(value: Option<u32>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

fn print_help() {
    println!("{WORKER_NAME}");
    println!("Windows media recovery worker.");
    println!();
    println!("Usage:");
    println!("  MediaRecovery.Worker --version");
    println!("  MediaRecovery.Worker --capabilities");
    println!("  MediaRecovery.Worker --hardware-profile");
    println!("  MediaRecovery.Worker --raw-image-info <path>");
    println!("  MediaRecovery.Worker --physical-drive-info <index>");
}
