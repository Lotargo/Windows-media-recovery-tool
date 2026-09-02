# Changelog

All significant project changes are recorded here.

The project follows the spirit of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning starts with the Stage 0 executable foundation at `0.1.0`.

## Unreleased

### Local Test Infrastructure & Quality Assurance

#### Added
- Comprehensive local test suite across all Rust crates (`recovery-core`, `recovery-protocol`, `hardware-profile`, `partition-table`, `windows-storage`), establishing 76 fast, deterministic automated tests.
- Worker CLI integration test suite (`src/worker/media-recovery-worker/tests/cli_tests.rs`) covering all execution flags (`--version`, `--capabilities`, `--hardware-profile`, `--list-sources`, `--raw-image-info`, `--partition-info`, `--physical-drive-info`), JSON schema contracts, and error exit codes.
- End-to-end partition discovery integration tests (`src/rust/partition-table/tests/partition_discovery_integration.rs`) validating file-backed RAW disk images with MBR and GPT layouts.
- Fault injection test harnesses (`ChunkedSource`, `FailingSource`) verifying graceful error handling and recovery from byte-fragmented reads and bad sectors / I/O failures.
- Fuzzing and pseudo-random stress testing verifying that partition parsers, UTF-16 string decoders, device descriptors, `multi_sz` lists, and CPU topology decoders never panic on corrupt inputs.
- Terabyte-scale addressing validation (up to 20 TiB virtual source and 16 TiB offsets) ensuring 64-bit address calculation safety without 32-bit truncation on x86.
- Live non-destructive Windows storage enumeration unit tests verifying mounted volumes and physical drive paths.

### Stage 2 Partition Discovery & Metadata Recovery

#### Added
- Stage 2 `partition-table` crate with initial MBR and GPT discovery over the shared `BlockSource` contract.
- GPT partition name, type GUID, unique GUID, attributes, LBA range, and byte-range extraction.
- Worker `--partition-info <raw-image-path>` diagnostic.
- Cross-architecture CI smoke test for an MBR RAW image flowing through the worker partition-discovery path.

### Stage 1 Storage Access & Hardware Profiling

#### Added
- Stage 1 `hardware-profile` crate with architecture, pointer width, logical/physical processor topology, processor groups, physical-memory telemetry, and SIMD detection.
- Stage 1 `windows-storage` crate with read-only RAW disk image and physical-drive `BlockSource` implementations.
- Windows physical-drive profiling for disk length, logical/physical sector sizes, alignment, seek penalty, TRIM, bus type, command queueing, and removable-media hints.
- Storage identity extraction for vendor, model, revision, and serial number when Windows exposes the data.
- Storage classification for HDD, SSD, NVMe, removable, SD/MMC, virtual, and unknown devices.
- Physical-drive discovery through `QueryDosDeviceW` without probing arbitrary `PhysicalDriveN` indices.
- Mounted drive discovery and disk-volume mapping through `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`.
- Worker diagnostics for hardware profiling, source enumeration, RAW image inspection, and physical-drive inspection.
- CI runtime checks for CPU topology, memory telemetry, storage discovery, and RAW disk image access on x86, x64, and ARM64.
- Synthetic `BlockSource` coverage proving offsets above 4 GiB remain intact on 32-bit and 64-bit builds.

### Stage 0 Initial Foundation & Architecture

#### Added
- Initial project definition for a Windows-only media recovery application.
- Release 1 scope limited to image and video recovery.
- Native architecture targets: x86, x64, and ARM64.
- C#/.NET/WinUI 3 desktop layer and Rust recovery core direction.
- Read-only source-storage invariant.
- Adaptive HDD/SSD/NVMe scanning requirement.
- Runtime SIMD dispatch requirement.
- Background resource-governed scanning requirement.
- Preview-during-scan and original-path reconstruction as first-class product features.
- Signed Windows installer and CI/CD-based release validation requirements.
- Initial planning documentation under `plan/`.
- Stage 0 Rust workspace with `recovery-core`, `recovery-protocol`, and a standalone recovery worker.
- Stage 0 .NET 10 / WinUI 3 desktop solution using Windows App SDK 2.4.0.
- Native x86, x64, and ARM64 CI build/test matrix.
- Worker `--version` and `--capabilities` probes for CI and bootstrap diagnostics.
- Initial versioned local IPC contract and Windows packaging workspace.
- Synthetic recovery-fixture workspace for deterministic future recovery tests.

#### Changed
- CI now builds real Rust and WinUI targets instead of using implementation placeholders.
- Rust CI uses `--locked` for test, clippy, and release build steps.
- Rust formatting is validated in a dedicated architecture-neutral `Rust format` job rather than being attached to the x64 matrix entry.
- Rust architecture jobs run independently from the formatting job so style failures do not hide x86/x64/ARM64 results.
- Stage 1 capability reporting advertises implemented read-only physical-drive access.
- Development worker binary uses the Cargo-friendly name `media-recovery-worker.exe`; final public naming is owned by the Windows packaging layer.
- Win32 FFI bindings use Rust snake_case names with explicit `link_name` symbols to keep strict linting independent from Windows API naming.

#### Fixed
- Replaced a brittle CI check for the literal phrase `No AI` with checks for the actual Release 1 scope invariants.
- Fixed initial Rust formatting and constant-assertion clippy failures reported by the first architecture-matrix run.
- Fixed storage classification compilation on Rust 1.98 by removing unsupported const evaluation from runtime device classification.

#### Removed
- Nothing yet.
