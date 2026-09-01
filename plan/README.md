# Project Plan

This directory is the implementation map for Windows Media Recovery Tool.

Release 1 is deliberately narrow: **Windows-only image and video recovery**, native on **x86, x64, and ARM64**, with no AI and no generic file-recovery scope.

## Documents

1. [`01-product-scope.md`](01-product-scope.md) - product boundaries, supported scenarios, non-goals, and Release 1 acceptance criteria.
2. [`02-architecture.md`](02-architecture.md) - process boundaries, C#/.NET UI, Rust worker/core, recovery pipeline, and persistence model.
3. [`03-release-roadmap.md`](03-release-roadmap.md) - ordered implementation stages and exit criteria.
4. [`04-performance-and-runtime.md`](04-performance-and-runtime.md) - HDD/SSD/NVMe strategies, SIMD, threading, resource governance, previews, and long-running scans.
5. [`05-distribution-and-trust.md`](05-distribution-and-trust.md) - installer, code signing, Windows trust, update/uninstall behavior, and clean-machine requirements.
6. [`06-ci-cd.md`](06-ci-cd.md) - build matrix, automated checks, fixtures, packaging, signing gates, and release automation.

## Release 1 principles

- Do one job well: recover images and videos.
- Windows is the only operating system target.
- x86, x64, and ARM64 are first-class release targets.
- No AI is required or planned for the first release.
- Source storage is always treated as read-only.
- The UI must become useful before a deep scan finishes.
- Original file paths are recovered only from evidence; missing paths are never invented.
- Performance adapts to the actual CPU, memory pressure, and storage class.
- A scan may run for many hours or days and must remain pausable, resumable, and resource-aware.
- SIMD changes speed, never semantics.
- CI/CD is the primary verification path; release artifacts must be produced from repeatable automated builds.
- Installation quality and Windows trust are product requirements, not post-release packaging work.

## Planning status vocabulary

Each roadmap stage uses the following states:

- **Planned** - requirements are defined but implementation has not started.
- **In progress** - implementation work is active.
- **Blocked** - a dependency or design decision prevents completion.
- **Complete** - exit criteria are satisfied by CI and the required artifacts exist.

## Future expansion policy

Possible future work includes generic file recovery, additional file systems, RAID/forensic workflows, macOS, and Linux. These possibilities must not introduce speculative abstraction into Release 1. Generalize an interface only when the current Windows media-recovery implementation benefits from it or when a second real implementation exists.
