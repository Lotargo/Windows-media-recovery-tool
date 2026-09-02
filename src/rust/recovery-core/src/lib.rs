use std::io;

pub trait BlockSource {
    fn len(&self) -> u64;
    fn logical_sector_size(&self) -> u32;
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub offset: u64,
    pub length: u64,
}

impl SourceRange {
    pub const fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    pub fn end(self) -> Option<u64> {
        self.offset.checked_add(self.length)
    }
}

pub const ENGINE_NAME: &str = "Windows Media Recovery Engine";

#[cfg(test)]
mod tests {
    use super::*;

    struct SyntheticLargeSource;

    impl BlockSource for SyntheticLargeSource {
        fn len(&self) -> u64 {
            8 * 1024 * 1024 * 1024
        }

        fn logical_sector_size(&self) -> u32 {
            512
        }

        fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
            if offset >= self.len() || buffer.is_empty() {
                return Ok(0);
            }

            let remaining = self.len() - offset;
            let count = buffer.len().min(remaining.min(usize::MAX as u64) as usize);
            for (i, byte) in buffer[..count].iter_mut().enumerate() {
                *byte = ((offset + i as u64) & 0xff) as u8;
            }
            Ok(count)
        }
    }

    struct EmptySource;

    impl BlockSource for EmptySource {
        fn len(&self) -> u64 {
            0
        }

        fn logical_sector_size(&self) -> u32 {
            512
        }

        fn read_at(&self, _offset: u64, _buffer: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn engine_name_is_valid_and_non_empty() {
        assert_eq!(ENGINE_NAME, "Windows Media Recovery Engine");
    }

    #[test]
    fn block_source_is_empty_checks_length() {
        let empty = EmptySource;
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let large = SyntheticLargeSource;
        assert!(!large.is_empty());
        assert_eq!(large.len(), 8 * 1024 * 1024 * 1024);
        assert_eq!(large.logical_sector_size(), 512);
    }

    #[test]
    fn block_source_read_at_boundary_conditions() {
        let source = SyntheticLargeSource;

        // 0-byte read buffer returns Ok(0)
        let mut empty_buf = [0_u8; 0];
        assert_eq!(source.read_at(0, &mut empty_buf).unwrap(), 0);

        // Read at offset >= len returns Ok(0)
        let mut buf = [0_u8; 16];
        assert_eq!(source.read_at(source.len(), &mut buf).unwrap(), 0);
        assert_eq!(source.read_at(source.len() + 1000, &mut buf).unwrap(), 0);

        // Partial read near the very end of source
        let mut tail_buf = [0_u8; 8];
        let read = source.read_at(source.len() - 3, &mut tail_buf).unwrap();
        assert_eq!(read, 3);
    }

    #[test]
    fn source_ranges_support_offsets_beyond_four_gib() {
        let range = SourceRange::new(8 * 1024 * 1024 * 1024, 4096);
        assert_eq!(range.offset, 8 * 1024 * 1024 * 1024);
        assert_eq!(range.length, 4096);
        assert_eq!(range.end(), Some(8 * 1024 * 1024 * 1024 + 4096));
    }

    #[test]
    fn source_range_properties_and_traits() {
        let range1 = SourceRange::new(100, 200);
        let range2 = range1; // Copy trait
        let range3 = Clone::clone(&range1);

        assert_eq!(range1, range2); // PartialEq, Eq
        assert_eq!(range1, range3);
        assert_eq!(range1.end(), Some(300));

        let formatted = format!("{range1:?}"); // Debug trait
        assert!(formatted.contains("100"));
        assert!(formatted.contains("200"));

        // Zero-length range
        let zero_range = SourceRange::new(500, 0);
        assert_eq!(zero_range.end(), Some(500));
    }

    #[test]
    fn block_sources_preserve_offsets_beyond_four_gib() {
        let source = SyntheticLargeSource;
        let offset = 5 * 1024 * 1024 * 1024 + 0xa5;
        let mut buffer = [0_u8; 1];

        let read = source.read_at(offset, &mut buffer).unwrap();

        assert_eq!(read, 1);
        assert_eq!(buffer[0], 0xa5);
        assert_eq!(source.len(), 8 * 1024 * 1024 * 1024);
    }

    #[test]
    fn source_range_terabyte_scale_and_overflow_protection() {
        // 16 TiB offset + 1 GiB length
        let sixteen_tib: u64 = 16 * 1024 * 1024 * 1024 * 1024;
        let one_gib: u64 = 1024 * 1024 * 1024;
        let range = SourceRange::new(sixteen_tib, one_gib);
        assert_eq!(range.end(), Some(sixteen_tib + one_gib));

        // u64::MAX boundary
        let max_range = SourceRange::new(u64::MAX, 0);
        assert_eq!(max_range.end(), Some(u64::MAX));

        let overflow_1 = SourceRange::new(u64::MAX, 1);
        assert_eq!(overflow_1.end(), None);

        let overflow_range = SourceRange::new(u64::MAX - 3, 8);
        assert_eq!(overflow_range.end(), None);
    }
}
