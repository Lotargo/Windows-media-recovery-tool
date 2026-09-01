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

    #[test]
    fn source_ranges_support_offsets_beyond_four_gib() {
        let range = SourceRange::new(8 * 1024 * 1024 * 1024, 4096);
        assert_eq!(range.end(), Some(8 * 1024 * 1024 * 1024 + 4096));
    }

    #[test]
    fn source_range_overflow_is_detected() {
        let range = SourceRange::new(u64::MAX - 3, 8);
        assert_eq!(range.end(), None);
    }
}
