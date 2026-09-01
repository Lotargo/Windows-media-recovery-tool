use recovery_core::ENGINE_NAME;
use recovery_protocol::{WorkerCapabilities, WORKER_NAME};

fn main() {
    let command = std::env::args().nth(1);

    match command.as_deref() {
        Some("--version") => print_version(),
        Some("--capabilities") => print_capabilities(),
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
    let capabilities = WorkerCapabilities::stage_zero();

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

fn print_help() {
    println!("{WORKER_NAME}");
    println!("Stage 0 recovery worker foundation.");
    println!();
    println!("Usage:");
    println!("  MediaRecovery.Worker --version");
    println!("  MediaRecovery.Worker --capabilities");
}
