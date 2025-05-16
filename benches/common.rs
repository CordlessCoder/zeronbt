#[derive(Debug, Clone, Copy)]
pub enum InputFile {
    BigTest,
    Chunk,
    Literal(&'static [u8]),
}

impl InputFile {
    pub const fn data(&self) -> &'static [u8] {
        use InputFile::*;
        match self {
            BigTest => include_bytes!("../assets/bigtest.nbt"),
            Chunk => include_bytes!("../assets/chunk_0-0.nbt"),
            Literal(data) => data,
        }
    }
}
