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

            let value = (offset & 0xff) as u8;
            buffer[0] = value;
            Ok(1)
        }
    }

    #[test]
    fn source_ranges_support_offsets_beyond_four_gib() {
        let range = SourceRange::new(8 * 1024 * 1024 * 1024, 4096);
        assert_eq!(range.end(), Some(8 * 1024 * 1024 * 1024 + 4096));
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
    fn source_range_overflow_is_detected() {
        let range = SourceRange::new(u64::MAX - 3, 8);
        assert_eq!(range.end(), None);
    }
}
