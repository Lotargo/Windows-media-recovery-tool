use partition_table::{PartitionKind, PartitionScheme, discover_partition_table};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_storage::RawDiskImage;

fn temporary_fixture_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "windows-media-recovery-{name}-{}-{unique}.img",
        std::process::id()
    ))
}

#[test]
fn e2e_mbr_image_discovery() {
    let path = temporary_fixture_path("mbr");
    let mut bytes = vec![0_u8; 8 * 1024 * 1024]; // 8 MiB image

    // Boot signature
    bytes[510] = 0x55;
    bytes[511] = 0xaa;

    // Partition 1: NTFS (0x07), start LBA 2048, count 4096 (2 MiB)
    let p1 = 446;
    bytes[p1 + 4] = 0x07;
    bytes[p1 + 8..p1 + 12].copy_from_slice(&2048_u32.to_le_bytes());
    bytes[p1 + 12..p1 + 16].copy_from_slice(&4096_u32.to_le_bytes());

    // Partition 2: FAT32 (0x0b), start LBA 8192, count 4096 (2 MiB)
    let p2 = 446 + 16;
    bytes[p2 + 4] = 0x0b;
    bytes[p2 + 8..p2 + 12].copy_from_slice(&8192_u32.to_le_bytes());
    bytes[p2 + 12..p2 + 16].copy_from_slice(&4096_u32.to_le_bytes());

    fs::write(&path, bytes).expect("MBR fixture file should be written");

    let image = RawDiskImage::open(&path).expect("RAW disk image should open");
    let table = discover_partition_table(&image)
        .expect("partition table discovery should succeed")
        .expect("MBR table should be found");

    assert_eq!(table.scheme, PartitionScheme::Mbr);
    assert_eq!(table.entries.len(), 2);

    assert_eq!(table.entries[0].index, 0);
    assert_eq!(
        table.entries[0].kind,
        PartitionKind::Mbr { type_code: 0x07 }
    );
    assert_eq!(table.entries[0].start_lba, 2048);
    assert_eq!(table.entries[0].sector_count, 4096);
    assert_eq!(table.entries[0].byte_offset, 2048 * 512);
    assert_eq!(table.entries[0].byte_length, 4096 * 512);

    assert_eq!(table.entries[1].index, 1);
    assert_eq!(
        table.entries[1].kind,
        PartitionKind::Mbr { type_code: 0x0b }
    );
    assert_eq!(table.entries[1].start_lba, 8192);
    assert_eq!(table.entries[1].sector_count, 4096);
    assert_eq!(table.entries[1].byte_offset, 8192 * 512);
    assert_eq!(table.entries[1].byte_length, 4096 * 512);

    drop(image);
    let _ = fs::remove_file(path);
}

#[test]
fn e2e_gpt_image_discovery() {
    let path = temporary_fixture_path("gpt");
    let mut bytes = vec![0_u8; 8 * 1024 * 1024]; // 8 MiB image

    // Protective MBR (LBA 0)
    bytes[510] = 0x55;
    bytes[511] = 0xaa;
    bytes[446 + 4] = 0xee; // Protective MBR type
    bytes[446 + 8..446 + 12].copy_from_slice(&1_u32.to_le_bytes());
    bytes[446 + 12..446 + 16].copy_from_slice(&16383_u32.to_le_bytes());

    // GPT Header (LBA 1, offset 512)
    let header = &mut bytes[512..1024];
    header[0..8].copy_from_slice(b"EFI PART");
    header[12..16].copy_from_slice(&92_u32.to_le_bytes()); // Header size
    header[24..32].copy_from_slice(&1_u64.to_le_bytes()); // Current LBA
    header[72..80].copy_from_slice(&2_u64.to_le_bytes()); // Partition entries starting LBA
    header[80..84].copy_from_slice(&2_u32.to_le_bytes()); // Number of partition entries
    header[84..88].copy_from_slice(&128_u32.to_le_bytes()); // Size of partition entry

    // Partition Entry 1: EFI System Partition (LBA 2, offset 1024)
    let efi_guid: [u8; 16] = [
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9,
        0x3b,
    ];
    let unique_guid1: [u8; 16] = [0x11; 16];
    let entry1 = &mut bytes[1024..1024 + 128];
    entry1[0..16].copy_from_slice(&efi_guid);
    entry1[16..32].copy_from_slice(&unique_guid1);
    entry1[32..40].copy_from_slice(&2048_u64.to_le_bytes()); // first_lba: 2048
    entry1[40..48].copy_from_slice(&4095_u64.to_le_bytes()); // last_lba: 4095 (2048 sectors)
    entry1[48..56].copy_from_slice(&0_u64.to_le_bytes()); // attributes
    for (i, unit) in "EFI System".encode_utf16().enumerate() {
        let offset = 56 + i * 2;
        entry1[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }

    // Partition Entry 2: Microsoft Basic Data (LBA 2 + 128 bytes, offset 1152)
    let ms_data_guid: [u8; 16] = [
        0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99,
        0xc7,
    ];
    let unique_guid2: [u8; 16] = [0x22; 16];
    let entry2 = &mut bytes[1152..1152 + 128];
    entry2[0..16].copy_from_slice(&ms_data_guid);
    entry2[16..32].copy_from_slice(&unique_guid2);
    entry2[32..40].copy_from_slice(&4096_u64.to_le_bytes()); // first_lba: 4096
    entry2[40..48].copy_from_slice(&8191_u64.to_le_bytes()); // last_lba: 8191 (4096 sectors)
    entry2[48..56].copy_from_slice(&0x8000_0000_0000_0000_u64.to_le_bytes()); // read-only attribute
    for (i, unit) in "Recovery Media".encode_utf16().enumerate() {
        let offset = 56 + i * 2;
        entry2[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }

    fs::write(&path, bytes).expect("GPT fixture file should be written");

    let image = RawDiskImage::open(&path).expect("RAW disk image should open");
    let table = discover_partition_table(&image)
        .expect("partition table discovery should succeed")
        .expect("GPT table should be found");

    assert_eq!(table.scheme, PartitionScheme::Gpt);
    assert_eq!(table.entries.len(), 2);

    assert_eq!(table.entries[0].index, 0);
    assert_eq!(
        table.entries[0].kind,
        PartitionKind::Gpt {
            type_guid: efi_guid
        }
    );
    assert_eq!(table.entries[0].unique_guid, Some(unique_guid1));
    assert_eq!(table.entries[0].start_lba, 2048);
    assert_eq!(table.entries[0].sector_count, 2048);
    assert_eq!(table.entries[0].name.as_deref(), Some("EFI System"));

    assert_eq!(table.entries[1].index, 1);
    assert_eq!(
        table.entries[1].kind,
        PartitionKind::Gpt {
            type_guid: ms_data_guid
        }
    );
    assert_eq!(table.entries[1].unique_guid, Some(unique_guid2));
    assert_eq!(table.entries[1].start_lba, 4096);
    assert_eq!(table.entries[1].sector_count, 4096);
    assert_eq!(table.entries[1].attributes, Some(0x8000_0000_0000_0000));
    assert_eq!(table.entries[1].name.as_deref(), Some("Recovery Media"));

    drop(image);
    let _ = fs::remove_file(path);
}

#[test]
fn e2e_empty_image_returns_none() {
    let path = temporary_fixture_path("empty");
    fs::write(&path, vec![0_u8; 1024 * 1024]).expect("empty fixture should be written");

    let image = RawDiskImage::open(&path).expect("RAW image should open");
    let table = discover_partition_table(&image).expect("discovery should succeed");
    assert_eq!(table, None);

    drop(image);
    let _ = fs::remove_file(path);
}
