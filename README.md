# Windows Media Recovery Tool

A Windows-native desktop application focused exclusively on recovering deleted and lost **images and videos** from local and removable storage.

The first release is intentionally narrow: it is not a universal data-recovery suite and it does not use AI. The project prioritizes recovery depth, scan speed, early previews, original-path reconstruction, predictable resource usage, and native Windows integration.

## Release 1 scope

### Platform

- Windows only.
- Native builds for **x86**, **x64**, and **ARM64**.
- No macOS or Linux support in Release 1.

### Recoverable content

- Images.
- Videos.
- Camera and mobile media formats that can be reliably identified, validated, previewed, and reconstructed.
- Documents, archives, executables, audio, databases, email, and generic file recovery are explicitly out of scope for Release 1.

### Storage and file systems

Primary targets:

- Internal HDDs.
- SATA SSDs.
- NVMe SSDs.
- External HDDs and SSDs.
- USB flash drives.
- SD and microSD cards.
- RAW disk images.

Initial file systems:

- NTFS.
- FAT32.
- exFAT.

### Product priorities

1. Fast time-to-first-result.
2. Preview recovered media while the deep scan is still running.
3. Reconstruct original file names, directory relationships, and paths whenever metadata survives.
4. Deep media-specific scanning and validation.
5. Adaptive behavior for HDD, SSD, NVMe, old PCs, and modern high-core-count systems.
6. Architecture-specific SIMD with deterministic fallback paths.
7. Long-running background scans that remain resumable and do not monopolize the machine.
8. Safe read-only access to source storage.
9. A trusted Windows installation and update experience.

## Technology direction

```text
C# / .NET / WinUI 3
        |
        | Named Pipes / narrow IPC protocol
        v
Rust Recovery Worker
        |
        v
Rust Recovery Core
        |
        v
Windows APIs / Physical Storage / Disk Images
```

### C# / .NET / WinUI 3

Responsible for the Windows desktop experience:

- application shell and lifecycle;
- gallery and previews;
- sorting, filtering, search, and recovered folder tree;
- taskbar progress, notifications, settings, accessibility, DPI, and localization;
- installer-facing integration and user-visible diagnostics.

### Rust

Responsible for all recovery-critical work:

- raw storage access;
- asynchronous and adaptive I/O;
- partition and file-system parsing;
- deleted-entry discovery;
- original-path reconstruction;
- signature scanning and media carving;
- media validation and reconstruction;
- SIMD dispatch;
- scan scheduling, checkpoints, resource governance, and recovery database;
- deterministic recovery and integrity verification.

The recovery worker is designed as a separate process so the UI can restart without destroying a long-running scan.

## Core invariants

- Source media is opened read-only.
- Disk offsets and persisted storage positions use fixed-width 64-bit values, never pointer-sized values.
- SIMD implementations may change performance but must never change recovery results.
- Recovery evidence and confidence must be explainable; the program must not invent missing file paths or metadata.
- Scan state is persisted so multi-hour or multi-day work can pause, resume, or survive UI restarts.
- Preview generation is demand-driven and must not block scanning.
- Release 1 remains media-specific even if internal primitives could later support other file types.

## Planning

Project planning lives in [`plan/`](plan/README.md).

The planning documents distinguish current Release 1 requirements from possible future expansion so architectural flexibility does not become premature complexity.

## Distribution goal

The application must install and run like normal trusted Windows software:

- signed application binaries;
- signed recovery worker;
- signed installer;
- stable publisher identity and timestamping;
- self-contained runtime dependencies;
- x86, x64, and ARM64 packages;
- clean install, upgrade, rollback, and uninstall paths;
- CI/CD validation on clean Windows environments;
- no requirement to disable Microsoft Defender, add exclusions, run scripts manually, or bypass SmartScreen warnings as a normal installation step.

## Status

Planning and architecture definition are in progress. See [`CHANGELOG.md`](CHANGELOG.md) for project-level changes and [`plan/`](plan/README.md) for the implementation roadmap.
