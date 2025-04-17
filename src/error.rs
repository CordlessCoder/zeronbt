use thiserror::Error;

pub type NbtResult<T> = Result<T, NbtParseError>;

#[derive(Debug, Clone, Error, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NbtParseError {
    #[error("Found invalid tag byte {0} while parsing NBT.")]
    InvalidTag(u8),
    #[error("Found invalid length {0} while parsing NBT.")]
    InvalidLen(i32),
}
