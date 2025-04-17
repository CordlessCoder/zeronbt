use crate::error::NbtParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum NbtTag {
    End = 0,
    Byte = 1,
    Short = 2,
    Int = 3,
    Long = 4,
    Float = 5,
    Double = 6,
    ByteArray = 7,
    String = 8,
    List = 9,
    Compound = 10,
    IntArray = 11,
    LongArray = 12,
}

impl TryFrom<u8> for NbtTag {
    type Error = NbtParseError;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => NbtTag::End,
            1 => NbtTag::Byte,
            2 => NbtTag::Short,
            3 => NbtTag::Int,
            4 => NbtTag::Long,
            5 => NbtTag::Float,
            6 => NbtTag::Double,
            7 => NbtTag::ByteArray,
            8 => NbtTag::String,
            9 => NbtTag::List,
            10 => NbtTag::Compound,
            11 => NbtTag::IntArray,
            12 => NbtTag::LongArray,
            invalid => return Err(NbtParseError::InvalidTag(invalid)),
        })
    }
}
