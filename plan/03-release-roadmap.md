# 03 - Release Roadmap

This roadmap describes the implementation order for Release 1. The order is intentionally dependency-driven: later UI features depend on stable recovery data, and advanced optimization comes after a correct baseline exists.

## Stage 0 - Repository and build foundation

**Status:** Planned

### Goals

- Establish repository structure.
- Create Rust workspace and .NET solution.
- Define x86, x64, and ARM64 build targets.
- Add version information shared across UI, worker, protocol, and installer.
- Add CI build matrix.
- Add formatting, linting, and basic test jobs.
- Define release artifact naming.

### Deliverables

- `src/desktop/MediaRecovery.Desktop`.
- Rust workspace under `src/`.
- protocol package/module.
- initial CI workflows.
- packaging directory.

### Exit criteria

- CI produces compilable x86/x64/ARM64 development artifacts.
- Empty desktop application can launch on the supported Windows test environment.
- Worker can start, report version/capabilities, and exit cleanly.

---

## Stage 1 - Windows storage and hardware discovery

**Status:** Planned

### Goals

Build the safe hardware-facing foundation before recovery algorithms.

### Work

- Enumerate physical and removable storage.
- Identify disk/volume relationships.
- Query logical/physical sector sizes and alignment.
- Detect bus/storage characteristics where Windows exposes them.
- Detect seek penalty/TRIM/storage class signals where reliable.
- Collect CPU architecture, logical processor topology, available SIMD features, and memory limits.
- Implement read-only physical-drive source.
- Implement RAW disk-image source.
- Implement bounded synchronous baseline reads first; asynchronous engine follows once correctness is covered.

### Exit criteria

- Worker can enumerate sources without UI code.
- Worker can open supported sources read-only.
- `u64` storage addressing works on x86, x64, and ARM64 builds.
- CI tests synthetic sources larger than 4 GiB without truncating offsets.
- No source-write API is reachable through the normal scan path.

---

## Stage 2 - Partition and file-system metadata recovery

**Status:** Planned

### Goals

Produce useful deleted-media results quickly from surviving file-system metadata.

### Work

- MBR/GPT partition discovery.
- NTFS parsing focused on media recovery requirements.
- FAT32 directory and allocation metadata.
- exFAT directory and allocation metadata.
- Deleted-entry discovery.
- Filename recovery.
- Parent-directory relationships.
- Original-path reconstruction with evidence states.
- Extent/allocation information sufficient for candidate recovery.

### Exit criteria

- Fixtures containing deleted media entries produce deterministic candidates.
- Exact/partial/name-only/unknown path state is represented correctly.
- Re-running the same scan produces the same candidate set.
- UI-independent integration tests validate paths and extents.

---

## Stage 3 - Scan store, sessions, checkpoints, and IPC

**Status:** Planned

### Goals

Make scans durable before adding expensive deep scanning.

### Work

- Embedded scan database.
- Source identity and session records.
- Candidate persistence.
- Checkpoint model.
- Pause/resume/cancel semantics.
- Worker state machine.
- Versioned Named Pipe protocol.
- UI reconnect to an existing worker/session.
- Structured progress and diagnostic events.

### Exit criteria

- A scan can be interrupted after persisted work and resumed without starting from zero.
- Desktop process can restart and reconnect to the worker.
- Candidate queries are paged and do not load the complete scan into UI memory.
- Protocol version mismatch fails cleanly with a user-actionable error.

---

## Stage 4 - Image recovery and progressive previews

**Status:** Planned

### Goals

Deliver the first complete user-visible recovery loop for images.

### Work

- Media signature registry limited to supported image formats.
- Fast signature scanner baseline.
- JPEG structural validation.
- PNG/WebP/GIF/BMP/TIFF family handling.
- HEIF/HEIC and selected RAW-family identification/metadata extraction where practical.
- Metadata extraction useful for sorting.
- Candidate resolver merges file-system and carving evidence.
- Preview engine.
- Embedded thumbnail extraction when available.
- Demand-driven preview queue based on UI visibility/selection.
- Image recovery writer and post-write validation.

### Exit criteria

- User can start a scan and see image candidates before full scan completion.
- Previews appear progressively.
- Metadata and carved discoveries are merged rather than duplicated.
- Selected images can be written to a safe destination and validated.

---

## Stage 5 - Deep scanning and adaptive I/O

**Status:** Planned

### Goals

Increase scan depth and throughput without destroying usability on weak hardware.

### Work

- Sequential HDD-oriented reader.
- SSD/NVMe asynchronous read strategy.
- Bounded producer/consumer pipeline.
- Adaptive read block size.
- Adaptive queue depth.
- Separate I/O and CPU-analysis workers.
- Runtime performance telemetry.
- Backpressure between reader, scanner, database, and preview work.
- Bad-sector/unreadable-range handling.
- Resume checkpoints for deep scans.

### Exit criteria

- HDD workload avoids pathological multi-threaded random seeking.
- SSD/NVMe workload can sustain multiple outstanding requests when beneficial.
- Slow downstream processing does not cause unbounded RAM growth.
- Scan remains recoverable after removable-device interruptions where safe.

---

## Stage 6 - SIMD and CPU-aware execution

**Status:** Planned

### Goals

Optimize proven hot paths without changing semantics.

### Work

- Generic reference scanner.
- x86/x64 runtime feature detection.
- SSE-family kernels where beneficial.
- AVX2 kernels for measured hot paths.
- AVX-512 only if profiling shows a real gain on supported CPUs.
- ARM64 SIMD/NEON kernels where beneficial.
- CPU topology-aware worker limits.
- Differential tests between generic and SIMD implementations.

### Exit criteria

- Every optimized kernel matches the generic implementation exactly on fixtures and randomized inputs.
- Unsupported CPUs fall back safely.
- x86 build does not accidentally require modern ISA extensions.
- Performance benchmarks demonstrate a measurable gain before an optimized path is kept.

---

## Stage 7 - Video recovery

**Status:** Planned

### Goals

Treat video as a first-class recovery domain, not as a large binary file with a magic header.

### Work

- MP4/MOV container parsing relevant to recovery.
- AVI/MKV and selected additional container support.
- Structural validation.
- Large-file extent handling.
- Fragment candidate tracking.
- Recoverable contiguous video path.
- Fragment reconstruction strategies for supported containers where evidence is sufficient.
- Preview/thumbnail extraction where practical.
- Recovery validation that distinguishes structurally valid, partial, and unusable output.

### Exit criteria

- Supported contiguous videos can be discovered, previewed where feasible, recovered, and validated.
- Supported fragmented-video fixtures exercise reconstruction paths.
- Large videos do not require loading complete file contents into RAM.

---

## Stage 8 - Resource governor and background operation

**Status:** Planned

### Goals

Make multi-hour and multi-day scans safe and polite on real Windows machines.

### Work

- Runtime modes: Maximum, Balanced, Background, Gentle.
- Balanced as default.
- CPU worker throttling.
- I/O queue throttling.
- Memory-pressure response.
- Preview-work deprioritization under pressure.
- Windows background processing/QoS integration where appropriate.
- Storage-temperature feedback when reliably exposed.
- Conservative fallback when telemetry is unavailable.
- Power/sleep/resume behavior.

### Exit criteria

- Background mode leaves foreground Windows interaction usable on low-end test profiles.
- Memory usage remains bounded over long scans.
- Worker can reduce load in response to configured pressure thresholds.
- Sleep/resume and UI restart do not silently corrupt session state.

---

## Stage 9 - Complete desktop UX

**Status:** Planned

### Goals

Turn the recovery engine into a consumer-usable Windows application.

### Work

- Device/source selection.
- Clear source safety messaging.
- Scan setup and progress.
- Progressive gallery.
- Original folder tree.
- Search, sorting, and filters.
- Recovery-confidence/evidence details.
- Selected-item preview.
- Multi-select and bulk recovery.
- Destination safety checks.
- Pause/resume/background controls.
- Clear handling for disconnected devices and corrupted media.
- Accessibility, DPI scaling, keyboard navigation, localization readiness.

### Exit criteria

- A non-technical user can complete the primary recovery flow without logs or command-line tools.
- UI never requires the full candidate set in memory.
- Large galleries remain responsive through virtualization/paging.

---

## Stage 10 - Installer, trust, and release automation

**Status:** Planned

### Goals

Ship trusted Windows software rather than a development artifact.

### Work

- WiX-based Windows packaging unless implementation evidence favors another native installer technology.
- Architecture-specific x86/x64/ARM64 application packages.
- Universal bootstrapper or equivalent simple architecture selection path.
- Self-contained .NET/Windows App SDK dependencies where appropriate.
- Authenticode signing and timestamping.
- Stable publisher identity.
- Installer signing.
- Upgrade, rollback, repair, uninstall.
- Clean-machine validation.
- Defender/SmartScreen release checks.
- Microsoft Store-compatible distribution path if retained for Release 1.

### Exit criteria

- Clean supported Windows machines can install without developer runtimes or manual scripts.
- Installed publisher identity is verified.
- Normal installation never instructs the user to disable Defender or create exclusions.
- x86/x64/ARM64 artifacts are produced by CI/CD.
- Uninstall removes product files without deleting user recovery output or scan data unexpectedly.

---

## Stage 11 - Release qualification

**Status:** Planned

### Goals

Prove the complete system on representative hardware and recovery fixtures.

### Qualification profiles

At minimum:

- low-memory x86/older x64 + HDD profile;
- mainstream x64 + SATA SSD profile;
- high-core-count x64 + NVMe profile;
- ARM64 Windows profile;
- SD/microSD and USB removable-storage scenarios;
- large-capacity source and large-video scenarios;
- bad/unreadable-region scenarios.

### Exit criteria

- Release acceptance criteria from `01-product-scope.md` are satisfied.
- No release-blocking CI failures.
- Signed installers and checksums are generated from the release pipeline.
- Recovery correctness fixtures remain deterministic across all architecture targets.
- Performance regressions against the maintained benchmark baseline are reviewed before release.

## Post-Release 1

Only after Release 1 is stable should the project evaluate:

- additional file classes;
- additional file systems;
- RAID or advanced forensic modes;
- more disk-image formats;
- macOS/Linux;
- AI-assisted restoration or classification.

None of these are dependencies for Release 1.
