use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn worker_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_media-recovery-worker"))
}

fn temporary_fixture_path(suffix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "windows-media-recovery-worker-{}-{unique}.{suffix}",
        std::process::id()
    ))
}

#[test]
fn cli_help_and_no_arguments() {
    let output = worker_cmd().output().expect("worker binary should execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MediaRecovery.Worker"));
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--hardware-profile"));

    let help_output = worker_cmd()
        .arg("--help")
        .output()
        .expect("worker binary should execute");
    assert!(help_output.status.success());
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(help_stdout.contains("Usage:"));

    let short_help_output = worker_cmd()
        .arg("-h")
        .output()
        .expect("worker binary should execute");
    assert!(short_help_output.status.success());
}

#[test]
fn cli_version() {
    let output = worker_cmd()
        .arg("--version")
        .output()
        .expect("worker binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        format!("MediaRecovery.Worker {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn cli_capabilities() {
    let output = worker_cmd()
        .arg("--capabilities")
        .output()
        .expect("worker binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"worker\":\"MediaRecovery.Worker\""));
    assert!(stdout.contains("\"protocol_version\":1"));
    assert!(stdout.contains("\"raw_image_source\":true"));
    assert!(stdout.contains("\"physical_drive_access\":true"));
}

#[test]
fn cli_hardware_profile() {
    let output = worker_cmd()
        .arg("--hardware-profile")
        .output()
        .expect("worker binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"architecture\":"));
    assert!(stdout.contains("\"pointer_width_bits\":"));
    assert!(stdout.contains("\"logical_processors\":"));
    assert!(stdout.contains("\"simd\":{"));
}

#[test]
fn cli_list_sources() {
    let output = worker_cmd()
        .arg("--list-sources")
        .output()
        .expect("worker binary should execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"physical_drives\":["));
    assert!(stdout.contains("\"mounted_volumes\":["));
}

#[test]
fn cli_raw_image_info() {
    // Missing path argument -> exit code 2
    let no_path = worker_cmd()
        .arg("--raw-image-info")
        .output()
        .expect("worker binary should execute");
    assert_eq!(no_path.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&no_path.stderr);
    assert!(stderr.contains("--raw-image-info requires a path"));

    // Non-existent path -> exit code 1
    let missing_file = worker_cmd()
        .args(["--raw-image-info", "non_existent_file_xyz_123.img"])
        .output()
        .expect("worker binary should execute");
    assert_eq!(missing_file.status.code(), Some(1));
    let missing_stderr = String::from_utf8_lossy(&missing_file.stderr);
    assert!(missing_stderr.contains("Failed to inspect raw image"));

    // Valid temporary raw image -> exit code 0
    let fixture = temporary_fixture_path("img");
    fs::write(&fixture, [0_u8; 1024]).expect("fixture should be written");

    let valid_output = worker_cmd()
        .args(["--raw-image-info", fixture.to_str().unwrap()])
        .output()
        .expect("worker binary should execute");
    assert!(valid_output.status.success());
    let valid_stdout = String::from_utf8_lossy(&valid_output.stdout);
    assert!(valid_stdout.contains("\"length_bytes\":1024"));
    assert!(valid_stdout.contains("\"logical_sector_size\":512"));

    let _ = fs::remove_file(fixture);
}

#[test]
fn cli_partition_info_mbr_and_gpt() {
    // Missing argument -> exit code 2
    let no_path = worker_cmd()
        .arg("--partition-info")
        .output()
        .expect("worker binary should execute");
    assert_eq!(no_path.status.code(), Some(2));

    // Empty raw image without partition table -> scheme: null
    let empty_fixture = temporary_fixture_path("img");
    fs::write(&empty_fixture, vec![0_u8; 4096]).expect("fixture should be written");

    let empty_output = worker_cmd()
        .args(["--partition-info", empty_fixture.to_str().unwrap()])
        .output()
        .expect("worker binary should execute");
    assert!(empty_output.status.success());
    let empty_stdout = String::from_utf8_lossy(&empty_output.stdout);
    assert!(empty_stdout.contains("\"scheme\":null"));
    assert!(empty_stdout.contains("\"entries\":[]"));
    let _ = fs::remove_file(empty_fixture);

    // MBR fixture
    let mbr_fixture = temporary_fixture_path("img");
    let mut mbr_bytes = vec![0_u8; 2 * 1024 * 1024];
    mbr_bytes[510] = 0x55;
    mbr_bytes[511] = 0xAA;
    let entry_offset = 446;
    mbr_bytes[entry_offset + 4] = 0x07; // NTFS
    mbr_bytes[entry_offset + 8..entry_offset + 12].copy_from_slice(&2048_u32.to_le_bytes());
    mbr_bytes[entry_offset + 12..entry_offset + 16].copy_from_slice(&1024_u32.to_le_bytes());
    fs::write(&mbr_fixture, mbr_bytes).expect("fixture should be written");

    let mbr_output = worker_cmd()
        .args(["--partition-info", mbr_fixture.to_str().unwrap()])
        .output()
        .expect("worker binary should execute");
    assert!(mbr_output.status.success());
    let mbr_stdout = String::from_utf8_lossy(&mbr_output.stdout);
    assert!(mbr_stdout.contains("\"scheme\":\"mbr\""));
    assert!(mbr_stdout.contains("\"start_lba\":2048"));
    assert!(mbr_stdout.contains("\"sector_count\":1024"));
    assert!(mbr_stdout.contains("\"mbr_type\":7"));
    let _ = fs::remove_file(mbr_fixture);

    // GPT fixture
    let gpt_fixture = temporary_fixture_path("img");
    let mut gpt_bytes = vec![0_u8; 2 * 1024 * 1024];
    gpt_bytes[510] = 0x55;
    gpt_bytes[511] = 0xAA;
    gpt_bytes[entry_offset + 4] = 0xEE; // Protective MBR
    gpt_bytes[entry_offset + 8..entry_offset + 12].copy_from_slice(&1_u32.to_le_bytes());
    gpt_bytes[entry_offset + 12..entry_offset + 16].copy_from_slice(&4095_u32.to_le_bytes());

    let header = &mut gpt_bytes[512..1024];
    header[0..8].copy_from_slice(b"EFI PART");
    header[12..16].copy_from_slice(&92_u32.to_le_bytes());
    header[24..32].copy_from_slice(&1_u64.to_le_bytes());
    header[72..80].copy_from_slice(&2_u64.to_le_bytes());
    header[80..84].copy_from_slice(&1_u32.to_le_bytes());
    header[84..88].copy_from_slice(&128_u32.to_le_bytes());

    let entry = &mut gpt_bytes[1024..1024 + 128];
    entry[0..16].copy_from_slice(&[0xaa; 16]); // Type GUID
    entry[16..32].copy_from_slice(&[0xbb; 16]); // Unique GUID
    entry[32..40].copy_from_slice(&40_u64.to_le_bytes());
    entry[40..48].copy_from_slice(&100_u64.to_le_bytes());
    entry[48..56].copy_from_slice(&1_u64.to_le_bytes());
    for (i, unit) in "Photos".encode_utf16().enumerate() {
        let offset = 56 + i * 2;
        entry[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }
    fs::write(&gpt_fixture, gpt_bytes).expect("fixture should be written");

    let gpt_output = worker_cmd()
        .args(["--partition-info", gpt_fixture.to_str().unwrap()])
        .output()
        .expect("worker binary should execute");
    assert!(gpt_output.status.success());
    let gpt_stdout = String::from_utf8_lossy(&gpt_output.stdout);
    assert!(gpt_stdout.contains("\"scheme\":\"gpt\""));
    assert!(gpt_stdout.contains("\"start_lba\":40"));
    assert!(gpt_stdout.contains("\"sector_count\":61"));
    assert!(gpt_stdout.contains("\"name\":\"Photos\""));
    let _ = fs::remove_file(gpt_fixture);
}

#[test]
fn cli_physical_drive_info_arguments_and_errors() {
    // Missing index argument -> exit code 2
    let no_arg = worker_cmd()
        .arg("--physical-drive-info")
        .output()
        .expect("worker binary should execute");
    assert_eq!(no_arg.status.code(), Some(2));
    let no_arg_stderr = String::from_utf8_lossy(&no_arg.stderr);
    assert!(no_arg_stderr.contains("--physical-drive-info requires a disk index"));

    // Invalid non-integer index -> exit code 2
    let invalid_arg = worker_cmd()
        .args(["--physical-drive-info", "not_a_number"])
        .output()
        .expect("worker binary should execute");
    assert_eq!(invalid_arg.status.code(), Some(2));
    let invalid_stderr = String::from_utf8_lossy(&invalid_arg.stderr);
    assert!(invalid_stderr.contains("requires a non-negative integer index"));

    // Non-existent physical drive index -> exit code 1
    let non_existent = worker_cmd()
        .args(["--physical-drive-info", "99999"])
        .output()
        .expect("worker binary should execute");
    assert_eq!(non_existent.status.code(), Some(1));
    let err_stderr = String::from_utf8_lossy(&non_existent.stderr);
    assert!(err_stderr.contains("Failed to inspect PhysicalDrive99999"));
}

#[test]
fn cli_unknown_argument() {
    let output = worker_cmd()
        .arg("--invalid-unknown-option-xyz")
        .output()
        .expect("worker binary should execute");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown argument: --invalid-unknown-option-xyz"));
}
