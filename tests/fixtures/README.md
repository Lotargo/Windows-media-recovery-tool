# Recovery Fixtures

This directory will contain deterministic synthetic storage/media fixtures used by CI.

Fixtures must never contain private user data or depend on arbitrary runner disks. Expected future cases include:

- offsets beyond 4 GiB;
- NTFS deleted media entries and parent paths;
- FAT32/exFAT deleted media entries;
- carved media without surviving file-system metadata;
- duplicate evidence found by metadata and carving;
- unreadable-range simulation;
- contiguous and fragmented video cases.

Expected recovery facts should be machine-readable and version-controlled alongside each fixture.
