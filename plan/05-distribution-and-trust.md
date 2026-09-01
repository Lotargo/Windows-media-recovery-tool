# 05 - Distribution and Trust

## Goal

Installation and Windows trust are Release 1 requirements. A recovery tool that accesses physical storage must look and behave like normal, transparent Windows software from the first public build.

The normal installation flow must never require a user to disable Microsoft Defender, create antivirus exclusions, run PowerShell commands, or bypass warnings as part of documented setup.

## Distribution targets

Release 1 produces native application artifacts for:

- x86;
- x64;
- ARM64.

Preferred distribution paths:

1. Signed direct-download installer.
2. Microsoft Store distribution if the packaging model remains compatible with required privileged recovery behavior.
3. Architecture-specific MSI artifacts for advanced/manual deployment where useful.

## Packaging direction

Use a native Windows installer technology. WiX is the initial preferred direction because it supports conventional Windows install/upgrade/repair/uninstall flows and fits the C#/.NET + native Rust worker architecture.

Suggested artifact shape:

```text
MediaRecoverySetup.exe
    |
    +--> detect native architecture
    +--> install matching signed package

MediaRecovery-x86.msi
MediaRecovery-x64.msi
MediaRecovery-arm64.msi
```

The bootstrapper strategy must be revisited if it harms signing reputation, offline installation, maintenance, or simplicity. Architecture-specific signed installers remain the canonical artifacts.

## Self-contained runtime

The user should not need to install developer/runtime prerequisites manually.

The packaged application should include the required runtime/dependency set for the selected architecture when licensing and deployment constraints allow it, including the .NET/Windows App SDK dependency strategy selected during implementation.

A fresh supported Windows installation should be able to install and launch the application without first visiting a runtime download page.

## Code signing

Every distributed executable component must be signed using the same stable publisher identity where applicable.

At minimum:

- desktop executable;
- recovery worker executable;
- distributed native DLLs where relevant;
- bootstrapper;
- MSI/package artifact;
- updater component if one is introduced later.

Signatures must be timestamped so already-released binaries remain verifiable after certificate rotation/expiration according to the selected signing infrastructure.

Signing keys/certificates are never committed to the repository.

## Stable publisher identity

Publisher identity must remain stable across public releases. Changing publisher identity casually damages trust/reputation continuity and complicates upgrades.

The user-facing installer/UAC experience should show a verified publisher rather than `Unknown Publisher` for signed release builds.

## Privilege model

Do not solve physical-drive access by running the complete desktop application permanently elevated.

Preferred model:

```text
MediaRecovery.Desktop.exe
normal user
        |
        | user initiates operation requiring raw access
        v
Windows elevation
        |
        v
MediaRecovery.Worker.exe
privileged only as required
```

The exact launch/privilege mechanism must be designed so worker ownership, IPC authentication, and session lifecycle remain safe and understandable.

## Antivirus and reputation posture

The application must avoid implementation choices that resemble malware unnecessarily.

Do not use:

- executable packers purely to reduce size;
- executable obfuscation in native recovery binaries;
- process injection;
- self-modifying code;
- hidden PowerShell/script bootstrap logic;
- runtime download and execution of opaque binaries;
- automatic Defender exclusions;
- attempts to disable security features;
- random executable names in temporary directories;
- persistence mechanisms unrelated to the visible product behavior.

Low-level raw disk access is a legitimate product requirement and should be isolated, signed, documented, and invoked through a clear user action.

## SmartScreen and Defender expectations

Direct-download reputation cannot be guaranteed for a brand-new publisher or binary. The release process should reduce unnecessary warnings through stable signing, normal packaging, transparent behavior, and release consistency rather than trying to evade reputation systems.

A confirmed false positive is handled through the appropriate vendor submission/review process. The application must never modify itself merely to evade a detection signature.

## Installation requirements

The installer must support:

- clean install;
- architecture-correct installation;
- upgrade from an older compatible version;
- rollback when installation fails;
- repair where appropriate;
- uninstall through normal Windows mechanisms;
- Start menu/application registration expected of a desktop product;
- clear version and publisher metadata.

Installation must not delete user recovery output, scan databases, or user-created disk images during upgrades/uninstall unless the user explicitly requests data removal.

## Application data layout

Separate immutable program files from mutable user/session data.

Conceptually:

```text
Program Files/
    Windows Media Recovery Tool/
        application binaries

ProgramData/ or LocalAppData/
    Windows Media Recovery Tool/
        settings
        scan databases
        preview cache
        diagnostics
```

The exact location depends on whether state is machine-wide or per-user. Large scan databases and preview caches should be configurable if real-world use demonstrates the need.

Recovered user media should always be written to an explicit destination chosen/approved by the user rather than hidden application data.

## Update strategy

Release 1 must at least support reliable in-place upgrades through the installer.

An automatic updater is optional unless added deliberately to the Release 1 scope later. If implemented, it must:

- verify signed update metadata/artifacts;
- never execute unsigned downloaded binaries;
- preserve active/persisted scan state safely;
- avoid updating a running privileged worker in an unsafe state;
- support rollback or clear recovery from interrupted update.

Do not introduce an updater merely because desktop applications usually have one.

## Clean-machine release tests

Release candidates must be tested on fresh supported Windows environments, not only developer workstations.

Test matrix includes x86/x64/ARM64 wherever available and must cover:

- install;
- first launch;
- worker elevation;
- physical/removable source enumeration;
- application restart;
- upgrade;
- uninstall;
- missing prior runtime assumptions;
- standard Microsoft Defender enabled;
- normal SmartScreen/reputation behavior observation;
- signed publisher verification.

## Release artifact metadata

Each public release should publish or retain:

- semantic/product version;
- git commit/tag;
- target architecture;
- SHA-256 digest;
- signing verification result;
- installer type;
- build workflow/run identifier.

## Release trust gate

A release is blocked if any of the following is true:

- a required binary is unsigned;
- the installer is unsigned;
- publisher identity is inconsistent unexpectedly;
- clean install requires manual runtime installation contrary to package design;
- the application requires security exclusions to function normally;
- uninstall/upgrade can destroy user recovery output unexpectedly;
- an architecture artifact contains the wrong worker/UI architecture;
- release CI cannot reproduce the package from the tagged source.
