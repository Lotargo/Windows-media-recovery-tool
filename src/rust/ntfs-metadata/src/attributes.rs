use crate::{NtfsFileRecord, read_u16, read_u32, read_u64};
use std::io;

const ATTRIBUTE_END: u32 = 0xffff_ffff;
const RESIDENT_HEADER_BYTES: usize = 24;
const NONRESIDENT_HEADER_BYTES: usize = 64;
const FILE_NAME_VALUE_BYTES: usize = 66;
const STANDARD_INFORMATION_MIN_BYTES: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtfsAttributeType {
    StandardInformation,
    AttributeList,
    FileName,
    ObjectId,
    SecurityDescriptor,
    VolumeName,
    VolumeInformation,
    Data,
    IndexRoot,
    IndexAllocation,
    Bitmap,
    ReparsePoint,
    EaInformation,
    Ea,
    LoggedUtilityStream,
    Unknown(u32),
}

impl NtfsAttributeType {
    pub const fn from_code(code: u32) -> Self {
        match code {
            0x10 => Self::StandardInformation,
            0x20 => Self::AttributeList,
            0x30 => Self::FileName,
            0x40 => Self::ObjectId,
            0x50 => Self::SecurityDescriptor,
            0x60 => Self::VolumeName,
            0x70 => Self::VolumeInformation,
            0x80 => Self::Data,
            0x90 => Self::IndexRoot,
            0xa0 => Self::IndexAllocation,
            0xb0 => Self::Bitmap,
            0xc0 => Self::ReparsePoint,
            0xd0 => Self::EaInformation,
            0xe0 => Self::Ea,
            0x100 => Self::LoggedUtilityStream,
            value => Self::Unknown(value),
        }
    }

    pub const fn code(self) -> u32 {
        match self {
            Self::StandardInformation => 0x10,
            Self::AttributeList => 0x20,
            Self::FileName => 0x30,
            Self::ObjectId => 0x40,
            Self::SecurityDescriptor => 0x50,
            Self::VolumeName => 0x60,
            Self::VolumeInformation => 0x70,
            Self::Data => 0x80,
            Self::IndexRoot => 0x90,
            Self::IndexAllocation => 0xa0,
            Self::Bitmap => 0xb0,
            Self::ReparsePoint => 0xc0,
            Self::EaInformation => 0xd0,
            Self::Ea => 0xe0,
            Self::LoggedUtilityStream => 0x100,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtfsAttributeForm {
    Resident {
        value_length: u32,
        value_offset: u16,
    },
    NonResident {
        lowest_vcn: u64,
        highest_vcn: u64,
        mapping_pairs_offset: u16,
        allocated_length: u64,
        file_size: u64,
        valid_data_length: u64,
        total_allocated: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsAttributeHeader {
    pub attribute_type: NtfsAttributeType,
    pub record_length: u32,
    pub name: Option<String>,
    pub flags: u16,
    pub instance: u16,
    pub form: NtfsAttributeForm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsFileReference {
    pub record_number: u64,
    pub sequence_number: u16,
}

impl NtfsFileReference {
    pub const fn from_raw(raw: u64) -> Self {
        Self {
            record_number: raw & 0x0000_ffff_ffff_ffff,
            sequence_number: (raw >> 48) as u16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsFileName {
    pub parent: NtfsFileReference,
    pub creation_time: u64,
    pub modification_time: u64,
    pub mft_change_time: u64,
    pub access_time: u64,
    pub allocated_size: u64,
    pub real_size: u64,
    pub file_flags: u32,
    pub reparse_value: u32,
    pub namespace: u8,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsStandardInformation {
    pub creation_time: u64,
    pub modification_time: u64,
    pub mft_change_time: u64,
    pub access_time: u64,
    pub file_attributes: u32,
    pub maximum_versions: u32,
    pub version_number: u32,
    pub class_id: u32,
    pub owner_id: Option<u32>,
    pub security_id: Option<u32>,
    pub quota_charged: Option<u64>,
    pub usn: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsDataRun {
    pub vcn_start: u64,
    pub cluster_count: u64,
    pub lcn_start: Option<u64>,
}

impl NtfsDataRun {
    pub fn vcn_end(self) -> Option<u64> {
        self.vcn_start.checked_add(self.cluster_count)
    }
}

#[derive(Debug)]
pub struct NtfsAttribute<'a> {
    pub offset: usize,
    pub header: NtfsAttributeHeader,
    record: &'a [u8],
}

impl NtfsAttribute<'_> {
    pub fn resident_value(&self) -> io::Result<Option<&[u8]>> {
        let NtfsAttributeForm::Resident {
            value_length,
            value_offset,
        } = self.header.form
        else {
            return Ok(None);
        };

        let start = usize::from(value_offset);
        let length = usize::try_from(value_length)
            .map_err(|_| io::Error::other("resident attribute value is too large"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| io::Error::other("resident attribute value range overflow"))?;
        let value = self.record.get(start..end).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "resident attribute value lies outside its record",
            )
        })?;
        Ok(Some(value))
    }

    pub fn mapping_pairs(&self) -> io::Result<Option<&[u8]>> {
        let NtfsAttributeForm::NonResident {
            mapping_pairs_offset,
            ..
        } = self.header.form
        else {
            return Ok(None);
        };

        let start = usize::from(mapping_pairs_offset);
        if start >= self.record.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS mapping-pairs offset lies outside its attribute record",
            ));
        }
        Ok(Some(&self.record[start..]))
    }

    pub fn file_name(&self) -> io::Result<Option<NtfsFileName>> {
        if self.header.attribute_type != NtfsAttributeType::FileName {
            return Ok(None);
        }
        let value = self.resident_value()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "$FILE_NAME attribute must be resident",
            )
        })?;
        parse_file_name(value).map(Some)
    }

    pub fn standard_information(&self) -> io::Result<Option<NtfsStandardInformation>> {
        if self.header.attribute_type != NtfsAttributeType::StandardInformation {
            return Ok(None);
        }
        let value = self.resident_value()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "$STANDARD_INFORMATION attribute must be resident",
            )
        })?;
        parse_standard_information(value).map(Some)
    }

    pub fn data_runs(&self) -> io::Result<Option<Vec<NtfsDataRun>>> {
        if self.header.attribute_type != NtfsAttributeType::Data {
            return Ok(None);
        }
        let NtfsAttributeForm::NonResident {
            lowest_vcn,
            highest_vcn,
            ..
        } = self.header.form
        else {
            return Ok(None);
        };
        let mapping_pairs = self.mapping_pairs()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "nonresident $DATA has no mapping pairs",
            )
        })?;
        parse_mapping_pairs(mapping_pairs, lowest_vcn, highest_vcn).map(Some)
    }
}

impl NtfsFileRecord {
    pub fn attributes(&self) -> io::Result<Vec<NtfsAttribute<'_>>> {
        parse_attributes(self)
    }

    pub fn file_names(&self) -> io::Result<Vec<NtfsFileName>> {
        let mut names = Vec::new();
        for attribute in self.attributes()? {
            if let Some(name) = attribute.file_name()? {
                names.push(name);
            }
        }
        Ok(names)
    }
}

fn parse_attributes(record: &NtfsFileRecord) -> io::Result<Vec<NtfsAttribute<'_>>> {
    let used_size = usize::try_from(record.header.used_size)
        .map_err(|_| io::Error::other("NTFS FILE used size does not fit memory indexing"))?;
    let bytes = record.bytes.get(..used_size).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS FILE used size exceeds the record buffer",
        )
    })?;
    let mut offset = usize::from(record.header.first_attribute_offset);
    let mut attributes = Vec::new();

    while offset < bytes.len() {
        let type_code = read_u32(bytes, offset)?;
        if type_code == ATTRIBUTE_END {
            return Ok(attributes);
        }

        let record_length = usize::try_from(read_u32(bytes, offset + 4)?)
            .map_err(|_| io::Error::other("NTFS attribute length does not fit memory indexing"))?;
        let form_code = *bytes.get(offset + 8).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS attribute form code is truncated",
            )
        })?;
        let name_length = *bytes.get(offset + 9).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS attribute name length is truncated",
            )
        })?;
        let name_offset = read_u16(bytes, offset + 10)?;
        let flags = read_u16(bytes, offset + 12)?;
        let instance = read_u16(bytes, offset + 14)?;

        let minimum_length = match form_code {
            0 => RESIDENT_HEADER_BYTES,
            1 => NONRESIDENT_HEADER_BYTES,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NTFS attribute form code is invalid",
                ));
            }
        };
        if record_length < minimum_length || record_length % 8 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS attribute length is invalid or not quadword aligned",
            ));
        }
        let end = offset
            .checked_add(record_length)
            .ok_or_else(|| io::Error::other("NTFS attribute end offset overflow"))?;
        if end > bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS attribute extends beyond the FILE record used area",
            ));
        }
        let attribute_record = &bytes[offset..end];
        let name = parse_attribute_name(attribute_record, name_length, name_offset)?;

        let form = if form_code == 0 {
            let value_length = read_u32(attribute_record, 16)?;
            let value_offset = read_u16(attribute_record, 20)?;
            validate_resident_value(attribute_record, value_length, value_offset)?;
            NtfsAttributeForm::Resident {
                value_length,
                value_offset,
            }
        } else {
            let lowest_vcn = read_u64(attribute_record, 16)?;
            let highest_vcn = read_u64(attribute_record, 24)?;
            let mapping_pairs_offset = read_u16(attribute_record, 32)?;
            if lowest_vcn > highest_vcn {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NTFS nonresident attribute has an inverted VCN range",
                ));
            }
            if usize::from(mapping_pairs_offset) < NONRESIDENT_HEADER_BYTES
                || usize::from(mapping_pairs_offset) >= attribute_record.len()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NTFS mapping-pairs offset is invalid",
                ));
            }

            NtfsAttributeForm::NonResident {
                lowest_vcn,
                highest_vcn,
                mapping_pairs_offset,
                allocated_length: read_u64(attribute_record, 40)?,
                file_size: read_u64(attribute_record, 48)?,
                valid_data_length: read_u64(attribute_record, 56)?,
                total_allocated: if attribute_record.len() >= 72 {
                    Some(read_u64(attribute_record, 64)?)
                } else {
                    None
                },
            }
        };

        attributes.push(NtfsAttribute {
            offset,
            header: NtfsAttributeHeader {
                attribute_type: NtfsAttributeType::from_code(type_code),
                record_length: record_length as u32,
                name,
                flags,
                instance,
                form,
            },
            record: attribute_record,
        });
        offset = end;
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "NTFS FILE record does not contain an attribute terminator",
    ))
}

fn parse_attribute_name(record: &[u8], length: u8, offset: u16) -> io::Result<Option<String>> {
    if length == 0 {
        return Ok(None);
    }

    let start = usize::from(offset);
    let bytes_length = usize::from(length)
        .checked_mul(2)
        .ok_or_else(|| io::Error::other("NTFS attribute name length overflow"))?;
    let end = start
        .checked_add(bytes_length)
        .ok_or_else(|| io::Error::other("NTFS attribute name range overflow"))?;
    let bytes = record.get(start..end).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS attribute name lies outside its record",
        )
    })?;
    decode_utf16_le(bytes).map(Some)
}

fn validate_resident_value(record: &[u8], length: u32, offset: u16) -> io::Result<()> {
    let start = usize::from(offset);
    if start < RESIDENT_HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS resident value offset overlaps the attribute header",
        ));
    }
    let length = usize::try_from(length)
        .map_err(|_| io::Error::other("NTFS resident value length is too large"))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| io::Error::other("NTFS resident value range overflow"))?;
    if end > record.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS resident value lies outside its attribute record",
        ));
    }
    Ok(())
}

fn parse_file_name(value: &[u8]) -> io::Result<NtfsFileName> {
    if value.len() < FILE_NAME_VALUE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "$FILE_NAME value is truncated",
        ));
    }

    let name_length = usize::from(value[64]);
    let name_bytes = name_length
        .checked_mul(2)
        .ok_or_else(|| io::Error::other("$FILE_NAME length overflow"))?;
    let name_end = FILE_NAME_VALUE_BYTES
        .checked_add(name_bytes)
        .ok_or_else(|| io::Error::other("$FILE_NAME range overflow"))?;
    let name = decode_utf16_le(value.get(FILE_NAME_VALUE_BYTES..name_end).ok_or_else(|| {
        io::Error::new(io::ErrorKind::UnexpectedEof, "$FILE_NAME text is truncated")
    })?)?;

    Ok(NtfsFileName {
        parent: NtfsFileReference::from_raw(read_u64(value, 0)?),
        creation_time: read_u64(value, 8)?,
        modification_time: read_u64(value, 16)?,
        mft_change_time: read_u64(value, 24)?,
        access_time: read_u64(value, 32)?,
        allocated_size: read_u64(value, 40)?,
        real_size: read_u64(value, 48)?,
        file_flags: read_u32(value, 56)?,
        reparse_value: read_u32(value, 60)?,
        namespace: value[65],
        name,
    })
}

fn parse_standard_information(value: &[u8]) -> io::Result<NtfsStandardInformation> {
    if value.len() < STANDARD_INFORMATION_MIN_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "$STANDARD_INFORMATION value is truncated",
        ));
    }

    Ok(NtfsStandardInformation {
        creation_time: read_u64(value, 0)?,
        modification_time: read_u64(value, 8)?,
        mft_change_time: read_u64(value, 16)?,
        access_time: read_u64(value, 24)?,
        file_attributes: read_u32(value, 32)?,
        maximum_versions: read_u32(value, 36)?,
        version_number: read_u32(value, 40)?,
        class_id: read_u32(value, 44)?,
        owner_id: (value.len() >= 52)
            .then(|| read_u32(value, 48))
            .transpose()?,
        security_id: (value.len() >= 56)
            .then(|| read_u32(value, 52))
            .transpose()?,
        quota_charged: (value.len() >= 64)
            .then(|| read_u64(value, 56))
            .transpose()?,
        usn: (value.len() >= 72)
            .then(|| read_u64(value, 64))
            .transpose()?,
    })
}

fn parse_mapping_pairs(
    bytes: &[u8],
    lowest_vcn: u64,
    highest_vcn: u64,
) -> io::Result<Vec<NtfsDataRun>> {
    let mut cursor = 0_usize;
    let mut current_vcn = lowest_vcn;
    let mut current_lcn = 0_i64;
    let mut runs = Vec::new();

    loop {
        let header = *bytes.get(cursor).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS mapping-pairs stream is unterminated",
            )
        })?;
        cursor += 1;
        if header == 0 {
            break;
        }

        let length_bytes = usize::from(header & 0x0f);
        let offset_bytes = usize::from(header >> 4);
        if length_bytes == 0 || length_bytes > 8 || offset_bytes > 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS mapping-pairs field width is invalid",
            ));
        }

        let length_slice = bytes.get(cursor..cursor + length_bytes).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NTFS mapping-pairs run length is truncated",
            )
        })?;
        let cluster_count = read_unsigned_le(length_slice);
        cursor += length_bytes;
        if cluster_count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTFS mapping-pairs run has zero clusters",
            ));
        }

        let lcn_start = if offset_bytes == 0 {
            None
        } else {
            let offset_slice = bytes.get(cursor..cursor + offset_bytes).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "NTFS mapping-pairs LCN delta is truncated",
                )
            })?;
            let delta = read_signed_le(offset_slice);
            cursor += offset_bytes;
            current_lcn = current_lcn
                .checked_add(delta)
                .ok_or_else(|| io::Error::other("NTFS mapping-pairs LCN overflow"))?;
            if current_lcn < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NTFS mapping-pairs produced a negative LCN",
                ));
            }
            Some(current_lcn as u64)
        };

        let next_vcn = current_vcn
            .checked_add(cluster_count)
            .ok_or_else(|| io::Error::other("NTFS mapping-pairs VCN overflow"))?;
        runs.push(NtfsDataRun {
            vcn_start: current_vcn,
            cluster_count,
            lcn_start,
        });
        current_vcn = next_vcn;
    }

    let expected_end = highest_vcn
        .checked_add(1)
        .ok_or_else(|| io::Error::other("NTFS highest VCN overflow"))?;
    if current_vcn != expected_end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS mapping-pairs coverage does not match the attribute VCN range",
        ));
    }

    Ok(runs)
}

fn read_unsigned_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .fold(0_u64, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        })
}

fn read_signed_le(bytes: &[u8]) -> i64 {
    let unsigned = read_unsigned_le(bytes);
    let bits = bytes.len() * 8;
    if bits == 64 {
        unsigned as i64
    } else {
        let sign_bit = 1_u64 << (bits - 1);
        if unsigned & sign_bit == 0 {
            unsigned as i64
        } else {
            (unsigned | (!0_u64 << bits)) as i64
        }
    }
}

fn decode_utf16_le(bytes: &[u8]) -> io::Result<String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS UTF-16 value has an odd byte length",
        ));
    }
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair));
    String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "NTFS UTF-16 value contains invalid data",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resident_attribute(type_code: u32, value: &[u8]) -> Vec<u8> {
        let value_offset = 24_usize;
        let record_length = (value_offset + value.len() + 7) & !7;
        let mut record = vec![0_u8; record_length];
        record[0..4].copy_from_slice(&type_code.to_le_bytes());
        record[4..8].copy_from_slice(&(record_length as u32).to_le_bytes());
        record[8] = 0;
        record[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
        record[20..22].copy_from_slice(&(value_offset as u16).to_le_bytes());
        record[value_offset..value_offset + value.len()].copy_from_slice(value);
        record
    }

    #[test]
    fn parses_file_name_value_and_parent_reference() {
        let mut value = vec![0_u8; 66 + "photo.jpg".encode_utf16().count() * 2];
        let parent_raw = (7_u64 << 48) | 42;
        value[0..8].copy_from_slice(&parent_raw.to_le_bytes());
        value[40..48].copy_from_slice(&4096_u64.to_le_bytes());
        value[48..56].copy_from_slice(&1234_u64.to_le_bytes());
        value[64] = 9;
        value[65] = 1;
        for (index, unit) in "photo.jpg".encode_utf16().enumerate() {
            let offset = 66 + index * 2;
            value[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }

        let parsed = parse_file_name(&value).unwrap();
        assert_eq!(parsed.parent.record_number, 42);
        assert_eq!(parsed.parent.sequence_number, 7);
        assert_eq!(parsed.name, "photo.jpg");
        assert_eq!(parsed.namespace, 1);
        assert_eq!(parsed.real_size, 1234);
    }

    #[test]
    fn parses_mapping_pairs_with_fragmented_and_sparse_runs() {
        let bytes = [0x11, 0x03, 0x64, 0x11, 0x02, 0x0a, 0x01, 0x04, 0x00];
        let runs = parse_mapping_pairs(&bytes, 0, 8).unwrap();

        assert_eq!(runs.len(), 3);
        assert_eq!(
            runs[0],
            NtfsDataRun {
                vcn_start: 0,
                cluster_count: 3,
                lcn_start: Some(100)
            }
        );
        assert_eq!(
            runs[1],
            NtfsDataRun {
                vcn_start: 3,
                cluster_count: 2,
                lcn_start: Some(110)
            }
        );
        assert_eq!(
            runs[2],
            NtfsDataRun {
                vcn_start: 5,
                cluster_count: 4,
                lcn_start: None
            }
        );
    }

    #[test]
    fn resident_value_bounds_are_enforced() {
        let mut record = resident_attribute(0x30, &[1, 2, 3, 4]);
        record[20..22].copy_from_slice(&0xfff0_u16.to_le_bytes());
        assert!(validate_resident_value(&record, 4, 0xfff0).is_err());
    }

    #[test]
    fn invalid_utf16_is_rejected() {
        assert!(decode_utf16_le(&[0x00]).is_err());
        assert!(decode_utf16_le(&[0x00, 0xd8]).is_err());
    }

    #[test]
    fn attribute_type_codes_round_trip() {
        let kinds = [
            NtfsAttributeType::StandardInformation,
            NtfsAttributeType::FileName,
            NtfsAttributeType::Data,
            NtfsAttributeType::Unknown(0x1234),
        ];
        for kind in kinds {
            assert_eq!(NtfsAttributeType::from_code(kind.code()), kind);
        }
    }
}
