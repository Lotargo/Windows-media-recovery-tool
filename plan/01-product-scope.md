# 01 - Product Scope

## Product statement

Windows Media Recovery Tool is a native Windows desktop application for recovering deleted and lost **images and videos** from local and removable storage.

The first release is not a generic undelete utility. It is optimized around media discovery, preview, sorting, path reconstruction, and safe long-running recovery on both old and modern Windows PCs.

## Release 1 targets

### Operating system

- Windows only.
- Native x86 build.
- Native x64 build.
- Native ARM64 build.
- No macOS or Linux implementation in Release 1.

### Storage

The application must support media recovery from:

- internal HDD;
- internal SATA SSD;
- NVMe SSD;
- external HDD/SSD;
- USB flash storage;
- SD and microSD cards;
- supported RAW disk-image files.

### File systems

Release 1 targets:

- NTFS;
- FAT32;
- exFAT.

The recovery pipeline must also be able to discover media by signatures when useful file-system metadata is missing.

## Recoverable content

### Images

The exact codec list may evolve during implementation, but Release 1 must cover the major families relevant to Windows PCs, phones, cameras, memory cards, and web media:

- JPEG/JPG;
- PNG;
- WebP;
- GIF;
- BMP;
- TIFF;
- HEIF/HEIC;
- AVIF where practical;
- common camera RAW families such as DNG, CR2/CR3, NEF, ARW, RAF, ORF, and RW2 where reliable identification and extraction are implemented.

### Video

The first release must include video recovery as a primary capability rather than a future plugin. Initial container families include:

- MP4;
- MOV;
- AVI;
- MKV;
- common camera/phone transport or recording containers where reliable recovery is practical.

Video reconstruction must be designed separately from simple image carving because large and fragmented video files have different recovery requirements.

## Required user-facing capabilities

- Detect available physical and removable storage.
- Open source media read-only.
- Run a fast metadata-oriented scan.
- Continue into a deep media-oriented scan.
- Show recovered results while scanning is still active.
- Generate previews without blocking the scan pipeline.
- Display original file name when recoverable.
- Reconstruct original folder hierarchy and path when supported by surviving metadata.
- Clearly distinguish exact/recovered paths from unknown paths.
- Sort and filter by useful media properties.
- Search recovered results.
- Select individual files, groups, folders, or all results for recovery.
- Recover to a destination other than the source media.
- Pause and resume long-running scans.
- Resume persisted scans after UI restart and, where feasible, process or machine restart.
- Provide recovery status/confidence based on evidence rather than vague labels.

## Required sorting and filtering dimensions

The exact UI may evolve, but the data model must support at least:

- original path;
- file name;
- media type;
- file size;
- creation/modification/media timestamp when available;
- image dimensions;
- camera/device metadata when available;
- recovery status;
- recoverability/confidence;
- scan discovery method;
- whether a preview is available.

## Performance goals

Release 1 must be usable on both:

- older x86/x64 Windows PCs with limited CPU cores, RAM, and mechanical disks;
- modern x64/ARM64 systems with SSD/NVMe storage and many logical processors.

The runtime must adapt rather than expose raw worker-count tuning as the primary solution.

Primary performance metrics:

- time to first useful result;
- time to first preview;
- sustained scan throughput;
- CPU utilization;
- memory usage;
- source-device I/O behavior;
- UI responsiveness;
- total scan duration;
- number of media candidates found;
- number of candidates with reconstructed names/paths;
- percentage of selected media successfully recovered and validated.

## Safety requirements

- Never write to the source device during scan or recovery.
- Warn and prevent normal recovery output from targeting the same source storage when that risks overwriting recoverable data.
- Persist recovery metadata and checkpoints separately from the source.
- Never fabricate a path, name, timestamp, or recovery status that is not supported by evidence.
- Treat corrupt/unreadable regions as expected recovery conditions rather than fatal application errors where possible.

## Explicit Release 1 non-goals

The following are intentionally out of scope:

- generic document recovery;
- archive recovery;
- executable recovery;
- database recovery;
- email recovery;
- generic audio recovery;
- AI restoration, AI reconstruction, or AI classification;
- macOS support;
- Linux support;
- RAID reconstruction;
- full forensic-suite functionality;
- speculative plugin architecture for unsupported operating systems or arbitrary file classes.

These items may be considered only after the Windows media-recovery release is stable.

## Release 1 acceptance statement

Release 1 is ready only when a non-technical Windows user can install the application, choose a storage source, start a scan, see useful media previews before the complete scan ends, browse recovered media by reconstructed location and metadata, safely restore selected images/videos, and leave a long scan running without the application monopolizing or destabilizing the machine.
