# 02 - Architecture

## Architectural goal

The application is split into a Windows-native presentation layer and a recovery-critical native engine. The split exists to isolate long-running privileged storage work from UI lifecycle, not to create a distributed system.

```text
+--------------------------------------------------+
| MediaRecovery.Desktop                           |
| C# / .NET / WinUI 3                             |
|                                                  |
| Shell | Gallery | Filters | Preview | Settings  |
+------------------------+-------------------------+
                         |
                         | versioned local IPC
                         | Named Pipes
                         v
+--------------------------------------------------+
| MediaRecovery.Worker                            |
| Rust                                             |
|                                                  |
| Session | Scheduler | DB | Resource Governor    |
+------------------------+-------------------------+
                         |
                         v
+--------------------------------------------------+
| Recovery Core / Rust crates                     |
|                                                  |
| I/O | FS | Scan | Media | SIMD | Restore        |
+------------------------+-------------------------+
                         |
                         v
+--------------------------------------------------+
| Windows APIs / Physical Drives / Disk Images    |
+--------------------------------------------------+
```

## Process model

### Desktop process

`MediaRecovery.Desktop.exe`

Responsibilities:

- WinUI 3 application shell;
- source selection and user workflow;
- scan configuration;
- gallery virtualization;
- recovered folder tree;
- search, sorting, and filters;
- preview presentation;
- taskbar progress and notifications;
- settings and diagnostics;
- connection/reconnection to the worker;
- install/update-facing UX.

The desktop process must not contain recovery algorithms and must not directly parse physical disks.

### Recovery worker

`MediaRecovery.Worker.exe`

Responsibilities:

- privileged physical-device access where required;
- scan session lifecycle;
- source read-only enforcement;
- block scheduling;
- file-system analysis;
- media scanning;
- path reconstruction;
- preview scheduling;
- recovery planning and writing;
- checkpointing and recovery database ownership;
- resource governance;
- structured progress/events exposed to the UI.

A UI crash or restart must not automatically invalidate an active worker session.

## IPC

Use local Windows Named Pipes with an explicitly versioned protocol.

The protocol should remain narrow and domain-oriented. The UI asks for operations such as:

- enumerate sources;
- start/pause/resume/cancel scan;
- subscribe to scan events;
- query paged candidates;
- request candidate details;
- request/generate preview;
- start recovery;
- query recovery progress;
- read diagnostics and runtime metrics.

Do not stream entire disk blocks or large media payloads through ad-hoc UI messages. Large previews may be exposed through controlled cache files or another measured local transport if Named Pipe payloads become inefficient.

## Suggested repository layout

```text
/
├── README.md
├── CHANGELOG.md
├── plan/
├── .github/
│   └── workflows/
├── src/
│   ├── desktop/
│   │   └── MediaRecovery.Desktop/
│   ├── worker/
│   │   └── media-recovery-worker/
│   └── rust/
│       ├── recovery-core/
│       ├── windows-storage/
│       ├── hardware-profile/
│       ├── io-engine/
│       ├── partition/
│       ├── fs-ntfs/
│       ├── fs-fat/
│       ├── fs-exfat/
│       ├── path-recovery/
│       ├── scan-engine/
│       ├── media-image/
│       ├── media-video/
│       ├── preview-engine/
│       ├── simd-kernels/
│       ├── resource-governor/
│       ├── scan-store/
│       └── restore-engine/
├── protocol/
│   └── recovery-protocol/
├── tests/
│   ├── fixtures/
│   ├── images/
│   └── integration/
└── packaging/
    └── windows/
```

The exact crate boundaries may be merged or split after profiling. They are functional boundaries, not mandatory micro-packages.

## Recovery pipeline

```text
Source discovery
      |
      v
Storage profile
      |
      +--> HDD strategy
      +--> SSD strategy
      +--> NVMe strategy
      |
      v
Partition discovery
      |
      v
Fast metadata scan --------------------------+
      |                                      |
      |                                      v
      |                                Results database
      |                                      |
      |                                      +--> UI gallery/tree
      v                                      +--> preview queue
Deep media scan                              |
      |                                      |
      v                                      |
Signature + structure validation ------------+
      |
      v
Candidate resolution
      |
      v
Recovery planning
      |
      v
Write to safe destination
      |
      v
Validation
```

The fast metadata pass and deep media scan are not separate user products. They are phases of one progressively useful scan session.

## Evidence model

A media candidate may be supported by multiple independent pieces of evidence:

- file-system record;
- parent-directory relationship;
- original filename;
- allocation chain/extents;
- media signature;
- container structure;
- embedded metadata;
- expected length or dimensions;
- readable/unreadable ranges;
- fragment relationships.

The candidate resolver merges evidence into one logical result rather than exposing duplicates from each detector.

Example conceptual model:

```text
MediaCandidate
├── id
├── media_type
├── original_name
├── original_path
├── path_evidence
├── timestamps
├── media_metadata
├── source_extents[]
├── discovery_methods[]
├── preview_state
├── integrity_state
├── recovery_confidence
└── recovery_notes[]
```

`recovery_confidence` must be derived from deterministic evidence. It is not an AI score.

## Path reconstruction

Paths are first-class data.

For NTFS, path reconstruction should use surviving MFT parent relationships and related metadata. FAT32/exFAT reconstruction should use surviving directory entries and directory relationships where available.

A candidate path state should distinguish at least:

- `Exact` - complete path reconstructed from file-system evidence;
- `Partial` - only part of the hierarchy survives;
- `NameOnly` - filename survives but parent path does not;
- `Unknown` - media was found through carving without reliable path evidence.

The UI may group unknown-path media into a synthetic view, but the synthetic UI group must never be persisted as the candidate's original path.

## Storage abstraction

The recovery engine needs a small source abstraction rather than a cross-platform operating-system abstraction.

Conceptually:

```rust
trait BlockSource {
    fn len(&self) -> u64;
    fn logical_sector_size(&self) -> u32;
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize>;
}
```

Implementations for Release 1:

- `WindowsPhysicalDrive`;
- `WindowsVolume` only if required by a specific scan path;
- `RawDiskImage`.

Asynchronous implementations may expose a different internal interface. The important invariant is that persisted offsets are `u64` and source access remains read-only.

## Architecture targets

### x86

- 32-bit process address-space constraints are treated as a first-class runtime limit.
- Buffers, preview caches, and queues must be bounded aggressively.
- Storage offsets remain 64-bit.
- Baseline SIMD must not assume AVX.

### x64

- Primary desktop target.
- Runtime SIMD dispatch may use SSE/AVX families where available and proven beneficial.
- High-core-count scheduling must remain bounded by storage throughput and memory pressure.

### ARM64

- Native UI and native Rust worker.
- No dependency on x64 emulation for recovery-critical work.
- ARM SIMD kernels should use architecture-appropriate intrinsics where profiling demonstrates value.

## Recovery database

Use a local embedded database, expected to be SQLite unless implementation evidence justifies another choice.

Suggested logical groups:

- scan session;
- source identity/profile;
- partition/file-system metadata;
- discovered file-system entries;
- directory relationships;
- media candidates;
- candidate extents/fragments;
- media metadata;
- preview state/cache index;
- checkpoints;
- recovery attempts/results;
- runtime metrics and diagnostics.

The database allows discovery to be separated from later interpretation and presentation. Large scans must not require all candidates to stay resident in RAM.

## Privilege boundary

The UI should normally run unelevated. Elevation is requested only for the worker or helper path that requires privileged physical-drive access.

The installer must not solve runtime privilege needs by permanently running the entire application as administrator.

## Error model

The recovery domain expects partial failure:

- unreadable sector;
- disappearing USB device;
- corrupt file-system structure;
- malformed media container;
- truncated file;
- failed preview decode;
- insufficient destination space.

These conditions should normally degrade a candidate/session rather than crash the process.

Fatal errors are reserved for conditions where session consistency can no longer be maintained.

## Deliberate non-abstractions

Release 1 should not introduce interfaces solely for hypothetical:

- Linux block devices;
- macOS device APIs;
- arbitrary generic file classes;
- cloud recovery;
- RAID controllers;
- forensic evidence formats beyond current requirements.

If future releases add a real second implementation, refactor toward the common abstraction then.
