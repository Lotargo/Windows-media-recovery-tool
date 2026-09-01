# 04 - Performance and Runtime Strategy

## Goal

Performance is a Release 1 product feature. The application must scale down to old Windows PCs and scale up to modern x64/ARM64 systems without requiring the user to understand thread counts, queue depth, SIMD instruction sets, or storage internals.

The runtime optimizes for useful recovery throughput while preserving source safety, bounded memory usage, UI responsiveness, and long-running stability.

## Primary optimization metric

Raw MB/s is not the only target.

The runtime should measure and optimize a set of user-relevant metrics:

- time to first deleted-media result;
- time to first preview;
- sustained source-read throughput;
- candidates processed per second;
- path reconstruction rate;
- preview latency for visible items;
- CPU utilization;
- memory pressure;
- I/O latency and queue behavior;
- total scan duration;
- foreground-system responsiveness.

A higher disk-read rate is not an improvement if it starves candidate parsing, exhausts RAM, destroys HDD seek behavior, or makes Windows unusable.

## Hardware profiler

At session startup the worker creates a runtime profile from available Windows and CPU information.

### Storage profile

Collect, where reliably exposed:

- device class and bus type;
- capacity;
- logical and physical sector sizes;
- alignment requirements;
- seek-penalty information;
- TRIM support;
- removable/fixed status;
- available storage-health/temperature telemetry;
- small read-only calibration results when necessary.

Do not trust any single USB bridge or device-reported property blindly. Profiles are hints used by an adaptive scheduler.

### CPU profile

Collect:

- architecture: x86/x64/ARM64;
- physical/logical processor topology where available;
- logical processor count;
- cache information where useful;
- available SIMD features;
- memory limits and current memory pressure.

## Storage-specific execution plans

### HDD

Primary rule: preserve sequential access.

```text
Physical HDD
    |
    v
Sequential / low-depth reader
    |
    v
Bounded chunk queue
    |
    +--> parser worker
    +--> signature worker(s)
    +--> metadata worker
    +--> persistence queue
```

Avoid assigning many independent disk-reader threads to different regions of the same mechanical drive. Parallel CPU analysis should operate on data already read into bounded buffers.

Potential adaptive controls:

- large sequential read chunks;
- low outstanding I/O depth;
- limited read-ahead;
- seek-aware scheduling for metadata regions;
- background-mode duty cycling.

### SATA SSD

Use asynchronous/sequential or mixed I/O where it improves measured throughput.

Potential controls:

- moderate outstanding I/O depth;
- larger CPU analysis pool than HDD mode;
- adaptive chunk size;
- backpressure from scan/persistence stages.

### NVMe

Allow higher parallelism while keeping the pipeline bounded.

Potential controls:

- deeper asynchronous I/O queue;
- multiple in-flight regions where beneficial;
- more CPU scanner workers;
- aggressive backpressure when candidate parsing or persistence becomes the bottleneck.

The scheduler must not assume that every NVMe device benefits from maximum queue depth.

### USB / SD / microSD

Treat removable media as its own reliability dimension.

- Expect disconnects and stalls.
- Avoid excessive retries that freeze the entire session.
- Persist checkpoints frequently enough to preserve long work.
- Keep conservative fallbacks for devices with misleading storage properties.

## Pipeline and backpressure

The scan engine is a bounded pipeline, not an unbounded task fan-out.

```text
Reader
  |
  v
Read queue
  |
  v
Scanner / FS analysis
  |
  v
Candidate resolver
  |
  +--> scan database
  +--> preview scheduler
  +--> UI event stream
```

Every queue has a capacity and measurable pressure. When downstream stages are saturated, upstream stages slow down rather than allocating indefinitely.

## Threading policy

Do not equate `logical_cpu_count` with the number of scanner workers.

Worker allocation depends on:

- storage throughput;
- CPU architecture and topology;
- current queue pressure;
- memory pressure;
- preview workload;
- selected runtime mode;
- foreground/background state.

Example conceptual profiles:

```text
Old x86/x64 + HDD
reader: 1
scan workers: 1-2
preview: opportunistic
small queues

Mainstream x64 + SSD
reader/async I/O: moderate
scan workers: several
preview: 1-2
bounded medium queues

High-end x64/ARM64 + NVMe
async I/O: deeper
scan workers: many but bounded
preview: responsive priority lane
DB writer: isolated/batched
```

These are strategies, not fixed constants.

## SIMD strategy

SIMD is planned from the start but introduced only on measured hot paths.

### Rules

- Maintain a generic reference implementation.
- Detect ISA support at runtime.
- Never require a modern instruction set merely because the build runs on a modern CI host.
- Differential-test every optimized implementation against the generic version.
- Remove an optimized kernel if its complexity is not justified by a measurable gain.

### x86/x64

Possible paths:

- baseline/generic;
- SSE-family paths where useful;
- AVX2 for proven byte-scanning hot paths;
- AVX-512 only after profiling on real supported hardware.

### ARM64

- native ARM64 worker;
- architecture-appropriate vector intrinsics/NEON where useful;
- no reliance on x64 emulation for recovery-critical loops.

### Suitable SIMD candidates

- media-signature scanning;
- file-system marker scanning;
- byte-pattern matching;
- zero/non-zero block classification;
- bitmap processing;
- selected checksums or validation primitives.

Complex file-system semantics should remain clear scalar code unless profiling demonstrates a real hotspot.

## x86 memory discipline

x86 support is a Release 1 requirement, not a build checkbox.

Rules:

- all persistent storage offsets are fixed-width 64-bit values;
- pointer-sized integers are not used for disk positions;
- preview caches are aggressively bounded;
- candidate data is paged from the scan database;
- large disk images are never assumed to fit in address space;
- large videos are streamed through bounded buffers;
- memory mapping is used only where safe for 32-bit address-space limits.

## Preview scheduler

Preview generation must optimize perceived speed without stealing resources from recovery.

Priority classes:

1. selected candidate - critical;
2. currently visible gallery items - high;
3. near-future viewport items - medium;
4. background cache population - idle/opportunistic.

Prefer cheap embedded thumbnails/preview metadata before full media decode when possible.

A scan can report hundreds of thousands of candidates without pre-decoding every item.

## Resource governor

The `ResourceGovernor` continuously adjusts runtime behavior.

### Inputs

- CPU utilization;
- worker saturation;
- queue pressure;
- memory pressure;
- read latency;
- sustained throughput;
- preview demand;
- storage temperature/health telemetry when available;
- configured runtime mode;
- power/sleep lifecycle signals where relevant.

### Outputs

- scanner worker count;
- I/O queue depth;
- block size/read-ahead policy;
- preview concurrency;
- database batching;
- background duty cycle;
- Windows scheduling/QoS hints where appropriate.

## Runtime modes

### Maximum

Prioritize total scan time. Still respects hard safety and memory limits.

### Balanced - default

Use available hardware aggressively enough to feel fast while preserving normal foreground Windows usability.

### Background

Prefer low-impact CPU/I/O scheduling, reduced queue depth, and reduced preview/background work so the user can continue using the PC during multi-hour scans.

### Gentle

For old machines, unstable removable media, questionable disks, or explicit user preference. Uses conservative queues, fewer workers, and lower duty cycle.

Runtime modes are policy presets; the governor remains adaptive inside each mode.

## Temperature behavior

Storage-temperature telemetry is optional because many USB bridges and devices do not expose reliable data.

When reliable temperature information exists, the governor may reduce I/O pressure before warning/critical thresholds are reached.

When telemetry is absent, the runtime relies on conservative queue behavior, latency, throughput, and selected runtime mode. The application must never require a temperature sensor to operate safely.

## Scan persistence

Long scans must not depend on process lifetime.

Persist at least:

- current scan phase;
- completed source ranges;
- relevant parser/file-system checkpoints;
- candidate/evidence records;
- unresolved work queues in reconstructable form;
- source identity sufficient to detect a changed/replaced device;
- runtime/version compatibility information.

The worker should be able to reject a resume when the source no longer matches safely rather than continuing against the wrong device.

## Benchmark suite

CI and release qualification should maintain deterministic synthetic/fixture benchmarks for:

- sequential signature scanning;
- metadata parsing;
- candidate persistence;
- preview generation;
- large-offset access;
- generic vs SIMD equality;
- x86 memory-bound scenarios;
- fragmented/large-video processing.

Hardware-specific throughput claims require real-hardware measurements and should not be inferred from hosted CI alone.

## Performance anti-patterns

Do not introduce:

- one disk-reader thread per CPU core;
- unbounded task queues;
- pre-decoding every preview;
- loading all candidates into RAM;
- mapping entire multi-terabyte sources as one required strategy;
- compile-time ISA assumptions that break older CPUs;
- artificial cross-platform abstractions in hot I/O paths;
- busy loops used as background throttling.
