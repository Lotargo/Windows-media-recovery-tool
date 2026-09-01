# 06 - CI/CD Strategy

## Goal

CI/CD is the primary verification path for this project. Development may proceed without repeated local manual checks, but changes are not considered complete until the relevant automated pipeline passes.

The pipeline must eventually build, test, package, and qualify x86, x64, and ARM64 artifacts from the same source revision.

## Principles

- CI validates correctness; local manual checks are optional development aids.
- Release artifacts are built by automation, not copied from developer machines.
- Architecture-specific behavior is tested explicitly.
- Generic and SIMD implementations are compared for semantic equality.
- Recovery tests operate on synthetic/fixture media and disk images, never on arbitrary runner disks.
- Packaging/signing occurs only after code/test gates pass.
- Signing secrets are stored only in protected CI secret infrastructure.
- Hardware-specific performance claims require real hardware; hosted CI provides regression signals, not final throughput claims.

## Workflow layers

### 1. Repository checks

Fast checks on every pull request and push:

- expected repository structure;
- Rust formatting;
- Rust linting;
- .NET formatting/static checks selected by the project;
- protocol/schema validation;
- documentation/link sanity where practical;
- no signing keys or obvious generated recovery artifacts committed.

### 2. Build matrix

Build native targets:

| Target | .NET RID | Rust target |
| --- | --- | --- |
| x86 | `win-x86` | `i686-pc-windows-msvc` |
| x64 | `win-x64` | `x86_64-pc-windows-msvc` |
| ARM64 | `win-arm64` | `aarch64-pc-windows-msvc` |

The exact runner strategy may use native or cross-compilation where supported, but Release qualification must eventually include native execution on representative x86/x64/ARM64 Windows environments.

### 3. Unit tests

Rust:

- block-source invariants;
- integer/offset boundaries;
- partition parsers;
- NTFS/FAT32/exFAT parser units;
- path reconstruction;
- media signature logic;
- media validators;
- candidate resolver;
- checkpoint/session state;
- resource governor decisions;
- SIMD/generic equality.

.NET:

- view-model/state logic where separated from WinUI;
- protocol client behavior;
- sorting/filtering/paging logic;
- reconnection/session presentation logic;
- installer/update-facing non-UI logic where testable.

### 4. Fixture/integration tests

Maintain deterministic synthetic images for scenarios such as:

- intact NTFS with deleted JPEG entries;
- NTFS with surviving filename/path but partially overwritten content;
- FAT32/exFAT deleted media;
- media with no usable file-system metadata but recoverable signatures;
- duplicate evidence discovered by metadata and carving;
- large offsets beyond 4 GiB;
- sparse/large synthetic source;
- bad/unreadable range simulation;
- removable-device interruption simulation where possible;
- contiguous video;
- fragmented-video fixtures when reconstruction exists.

Expected results should be machine-readable and version-controlled.

## Recovery golden tests

For each fixture, store expected facts rather than only UI snapshots.

Example conceptual expectation:

```json
{
  "candidate_count": 12,
  "candidates": [
    {
      "original_path": "\\DCIM\\100CANON\\IMG_0001.JPG",
      "path_state": "Exact",
      "media_type": "jpeg",
      "recoverable": true
    }
  ]
}
```

Golden tests must focus on deterministic recovery evidence and output hashes where appropriate.

## SIMD differential tests

Every SIMD kernel keeps a reference implementation.

For a given input corpus:

```text
generic(input) == optimized(input)
```

Compare at least:

- hit offsets;
- hit ordering when ordering is part of the contract;
- parsed lengths/types;
- error behavior;
- boundary conditions around chunk edges.

Randomized/property tests should include alignment offsets and buffers whose signatures cross SIMD/chunk boundaries.

## x86-specific gates

The x86 build requires dedicated checks because compilation alone does not prove 32-bit safety.

CI/fixtures should cover:

- source offsets above 4 GiB;
- no truncation from `u64` to pointer-sized values in persisted models;
- bounded candidate paging;
- bounded preview cache;
- large-file streaming without whole-file allocation;
- no accidental AVX/AVX2 baseline requirement.

## Performance regression jobs

Maintain benchmark jobs for deterministic CPU/memory workloads:

- signature scan throughput on fixture buffers;
- parser throughput;
- candidate resolver throughput;
- database insertion/query batches;
- preview decode latency;
- memory high-water mark for selected synthetic scenarios.

Hosted CI performance can be noisy. Use thresholds conservatively and prefer trend reports or explicit review for meaningful regressions instead of brittle hard failures for tiny differences.

## Windows integration tests

Where runner permissions allow, test:

- worker process startup/shutdown;
- Named Pipe connection;
- protocol version negotiation;
- UI client reconnect to worker;
- file-backed block source;
- installer smoke tests on disposable VMs/runners.

Raw physical-drive tests should use dedicated controlled machines or virtual disks and must never modify the runner's boot/system disk.

## Packaging pipeline

After build/test gates:

```text
source tag
   |
   v
build x86/x64/ARM64
   |
   v
run tests
   |
   v
publish self-contained app artifacts
   |
   v
sign binaries
   |
   v
build installers
   |
   v
sign installers
   |
   v
clean-machine smoke tests
   |
   v
checksums + release metadata
   |
   v
release artifacts
```

## Signing separation

Pull-request workflows must not expose release-signing secrets to untrusted code.

Use separate protected release jobs/environments with branch/tag restrictions and required approvals if appropriate.

Release signing should consume already-qualified source/build inputs through a traceable pipeline rather than allowing arbitrary script execution with signing credentials.

## Artifact naming

Proposed convention:

```text
WindowsMediaRecovery-<version>-x86.zip
WindowsMediaRecovery-<version>-x64.zip
WindowsMediaRecovery-<version>-arm64.zip
WindowsMediaRecovery-<version>-x86.msi
WindowsMediaRecovery-<version>-x64.msi
WindowsMediaRecovery-<version>-arm64.msi
WindowsMediaRecoverySetup-<version>.exe
checksums-sha256.txt
```

Names may change with final product branding, but architecture and version must remain unambiguous.

## Branch and release policy

Initial development can remain lightweight on `main`, but CI must run on every push.

Before public releases, prefer:

- tagged release source;
- immutable version for every artifact;
- changelog updated before tag;
- no manual binary replacement after signing/release;
- a new version/tag for any changed public binary.

## Initial CI bootstrap

Before source code exists, CI may validate only repository/planning structure. As implementation stages begin, activate jobs incrementally:

1. repository structure check;
2. Rust/.NET build;
3. unit tests;
4. fixture/golden tests;
5. architecture matrix;
6. integration tests;
7. package smoke tests;
8. protected signing/release pipeline.

Do not create permanently green placeholder jobs that imply unsupported architectures or tests are already validated.

## Definition of done for a change

A code change is complete when:

- relevant CI checks pass;
- deterministic recovery fixtures remain correct;
- new behavior has automated coverage when practical;
- architecture-specific implications are addressed;
- `CHANGELOG.md` is updated for significant user-visible or architectural changes;
- planning docs are updated when the implementation materially changes a documented decision.
