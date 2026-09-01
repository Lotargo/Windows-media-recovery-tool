use hardware_profile::CpuProfile;
use recovery_core::{BlockSource, ENGINE_NAME};
use recovery_protocol::{WORKER_NAME, WorkerCapabilities};
use windows_storage::RawDiskImage;

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

fn print_help() {
    println!("{WORKER_NAME}");
    println!("Windows media recovery worker.");
    println!();
    println!("Usage:");
    println!("  MediaRecovery.Worker --version");
    println!("  MediaRecovery.Worker --capabilities");
    println!("  MediaRecovery.Worker --hardware-profile");
    println!("  MediaRecovery.Worker --raw-image-info <path>");
}
