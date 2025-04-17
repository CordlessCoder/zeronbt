#[derive(Debug, PartialEq, Clone, Default, Hash)]
pub struct Buffer<'s> {
    data: &'s [u8],
    /// How far along we are in the buffer
    // SAFETY: position <= data.len() must always be upheld
    position: usize,
}

impl<'s> Buffer<'s> {
    pub const fn new(data: &'s [u8]) -> Self {
        Buffer { data, position: 0 }
    }

    #[inline]
    pub fn available(&self) -> &'s [u8] {
        // SAFETY: position <= data.len() is an invariant of this data structure
        unsafe { self.data.get_unchecked(self.position..) }
    }

    #[inline]
    pub fn consumed(&self) -> &'s [u8] {
        // SAFETY: position <= data.len() is an invariant of this data structure
        unsafe { self.data.get_unchecked(..self.position) }
    }

    #[inline(always)]
    // SAFETY: If this function returns Some(), then position + count is a valid new value for
    // position
    pub fn peek(&self, count: usize) -> Option<&'s [u8]> {
        self.available().get(..count)
    }

    #[inline]
    pub fn consume(&mut self, count: usize) -> Option<&'s [u8]> {
        let slice = self.peek(count)?;
        // SAFETY: The new value of position must be <= data.len() as otherwise peek()
        // would return None
        self.position += count;
        Some(slice)
    }

    #[inline]
    pub fn peek_arr<const LEN: usize>(&self) -> Option<&'s [u8; LEN]> {
        let slice = self.peek(LEN)?;
        // SAFETY: If peek() returned Some(), then the length of the slice must match the len
        // requested
        Some(unsafe { slice.try_into().unwrap_unchecked() })
    }

    #[inline]
    pub fn consume_arr<const LEN: usize>(&mut self) -> Option<&'s [u8; LEN]> {
        let arr = self.peek_arr()?;
        // SAFETY: If peek_arr() returned Some(), then position + count must be a valid new value
        // for position
        self.position += LEN;
        Some(arr)
    }
}
