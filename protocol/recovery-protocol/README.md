# Recovery Protocol

This directory owns the versioned local contract between the WinUI desktop process and the Rust recovery worker.

## Stage 0 contract

- Protocol version: `1`.
- Planned transport: Windows Named Pipes.
- Planned pipe name: `Lotargo.WindowsMediaRecovery.Worker.v1`.
- Development worker binary: `media-recovery-worker.exe`.
- Packaged worker binary: `MediaRecovery.Worker.exe`.

The Stage 0 worker does not yet host the Named Pipe server. It exposes two process-level probes used by CI and future bootstrap code:

```text
media-recovery-worker.exe --version
media-recovery-worker.exe --capabilities
```

`--capabilities` returns one JSON object containing the protocol version, native architecture, and currently implemented worker capabilities.

## Planned domain operations

The pipe protocol will remain narrow and domain-oriented. Expected operations include:

- worker hello / protocol negotiation;
- enumerate sources;
- start scan;
- pause scan;
- resume scan;
- cancel scan;
- subscribe to scan events;
- query paged candidates;
- request candidate details;
- request preview;
- start recovery;
- query recovery progress;
- query diagnostics/runtime metrics.

Large disk blocks and recovered media payloads are not part of the control protocol.

## Compatibility rule

A breaking protocol change requires an explicit protocol-version increment. The desktop application must fail clearly on an incompatible worker instead of guessing message semantics.
