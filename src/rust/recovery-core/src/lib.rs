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

pub struct BoundedBlockSource<'a> {
    source: &'a dyn BlockSource,
    range: SourceRange,
}

impl<'a> BoundedBlockSource<'a> {
    pub fn new(source: &'a dyn BlockSource, range: SourceRange) -> io::Result<Self> {
        let end = range
            .end()
            .ok_or_else(|| io::Error::other("bounded source range overflows u64"))?;
        if end > source.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "bounded source range exceeds the parent source",
            ));
        }

        Ok(Self { source, range })
    }

    pub const fn range(&self) -> SourceRange {
        self.range
    }
}

impl BlockSource for BoundedBlockSource<'_> {
    fn len(&self) -> u64 {
        self.range.length
    }

    fn logical_sector_size(&self) -> u32 {
        self.source.logical_sector_size()
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || offset >= self.range.length {
            return Ok(0);
        }

        let remaining = self.range.length - offset;
        let count = buffer.len().min(remaining.min(usize::MAX as u64) as usize);
        let absolute_offset = self
            .range
            .offset
            .checked_add(offset)
            .ok_or_else(|| io::Error::other("bounded source read offset overflows u64"))?;
        let read = self.source.read_at(absolute_offset, &mut buffer[..count])?;

        if read > count {
            return Err(io::Error::other(
                "parent block source returned more bytes than requested",
            ));
        }

        Ok(read)
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

        let mut empty_buf = [0_u8; 0];
        assert_eq!(source.read_at(0, &mut empty_buf).unwrap(), 0);

        let mut buf = [0_u8; 16];
        assert_eq!(source.read_at(source.len(), &mut buf).unwrap(), 0);
        assert_eq!(source.read_at(source.len() + 1000, &mut buf).unwrap(), 0);

        let mut tail_buf = [0_u8; 8];
        let read = source.read_at(source.len() - 3, &mut tail_buf).unwrap();
        assert_eq!(read, 3);
    }

    #[test]
    fn bounded_source_translates_offsets_and_clamps_reads() {
        let source = SyntheticLargeSource;
        let range = SourceRange::new(4096, 10);
        let bounded = BoundedBlockSource::new(&source, range).unwrap();

        assert_eq!(bounded.len(), 10);
        assert_eq!(bounded.logical_sector_size(), 512);
        assert_eq!(bounded.range(), range);

        let mut buffer = [0_u8; 8];
        let read = bounded.read_at(6, &mut buffer).unwrap();
        assert_eq!(read, 4);
        assert_eq!(&buffer[..4], &[6, 7, 8, 9]);
        assert_eq!(bounded.read_at(10, &mut buffer).unwrap(), 0);
        assert_eq!(bounded.read_at(11, &mut buffer).unwrap(), 0);
    }

    #[test]
    fn bounded_source_rejects_invalid_parent_ranges() {
        let source = SyntheticLargeSource;
        let too_long = SourceRange::new(source.len() - 4, 8);
        assert_eq!(
            BoundedBlockSource::new(&source, too_long)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );

        let overflow = SourceRange::new(u64::MAX, 2);
        assert!(BoundedBlockSource::new(&source, overflow).is_err());
    }

    #[test]
    fn bounded_source_preserves_offsets_beyond_four_gib() {
        let source = SyntheticLargeSource;
        let base = 5 * 1024 * 1024 * 1024 + 0xa0;
        let bounded = BoundedBlockSource::new(&source, SourceRange::new(base, 32)).unwrap();
        let mut buffer = [0_u8; 4];

        let read = bounded.read_at(5, &mut buffer).unwrap();

        assert_eq!(read, 4);
        assert_eq!(buffer, [0xa5, 0xa6, 0xa7, 0xa8]);
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
        let range2 = range1;
        let range3 = Clone::clone(&range1);

        assert_eq!(range1, range2);
        assert_eq!(range1, range3);
        assert_eq!(range1.end(), Some(300));

        let formatted = format!("{range1:?}");
        assert!(formatted.contains("100"));
        assert!(formatted.contains("200"));

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
        let sixteen_tib: u64 = 16 * 1024 * 1024 * 1024 * 1024;
        let one_gib: u64 = 1024 * 1024 * 1024;
        let range = SourceRange::new(sixteen_tib, one_gib);
        assert_eq!(range.end(), Some(sixteen_tib + one_gib));

        let max_range = SourceRange::new(u64::MAX, 0);
        assert_eq!(max_range.end(), Some(u64::MAX));

        let overflow_1 = SourceRange::new(u64::MAX, 1);
        assert_eq!(overflow_1.end(), None);

        let overflow_range = SourceRange::new(u64::MAX - 3, 8);
        assert_eq!(overflow_range.end(), None);
    }
}
