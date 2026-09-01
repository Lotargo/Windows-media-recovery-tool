# Changelog

All significant project changes are recorded here.

The project follows the spirit of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning starts with the Stage 0 executable foundation at `0.1.0`.

## Unreleased

### Added

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
- Stage 1 `hardware-profile` crate with native architecture, pointer-width, logical-processor, and baseline SIMD detection.
- Stage 1 `windows-storage` crate with read-only RAW disk image support and Windows physical-drive read-only handle primitives.
- Worker `--hardware-profile` and `--raw-image-info` diagnostics.
- CI runtime checks for CPU architecture/pointer width and RAW disk image access on x86, x64, and ARM64.
- Windows physical-drive profiling for disk length, logical/physical sector sizes, seek penalty, TRIM, bus type, and removable-media hints.
- `WindowsPhysicalDrive` implementation of the shared read-only `BlockSource` contract.
- Worker `--physical-drive-info <index>` diagnostic for controlled Windows hardware testing.

### Changed

- CI now builds real Rust and WinUI targets instead of using implementation placeholders.
- Rust CI now uses `--locked` for test, clippy, and release build steps.
- Stage 1 capability reporting now advertises implemented read-only physical-drive access.
- Development worker binary uses the Cargo-friendly name `media-recovery-worker.exe`; final public naming is owned by the Windows packaging layer.

### Fixed

- Replaced a brittle CI check for the literal phrase `No AI` with checks for the actual Release 1 scope invariants.
- Fixed initial Rust formatting and constant-assertion clippy failures reported by the first architecture-matrix run.

### Removed

- Nothing yet.
