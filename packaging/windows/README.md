# Windows Packaging

Release 1 packaging targets:

- x86 installer payload;
- x64 installer payload;
- ARM64 installer payload;
- signed WiX-based MSI packages unless implementation evidence justifies another native Windows installer technology;
- a simple architecture-selection bootstrapper if it improves direct-download installation;
- Authenticode signing and timestamping in protected release jobs;
- clean install, upgrade, rollback, repair, and uninstall behavior.

Packaging is intentionally separated from the recovery worker and WinUI projects. Development binaries may keep build-system-friendly names; the packaging layer owns final public executable and installer names.

No private signing material belongs in this repository.
