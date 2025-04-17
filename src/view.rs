use core::{fmt::Debug, marker::PhantomData, mem::MaybeUninit};

#[derive(Debug)]
pub struct BeSlice<'s, T: BeRepr> {
    data: &'s [u8],
    _repr: PhantomData<T>,
}

impl<'s, T: BeRepr> PartialEq for BeSlice<'s, T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<'s, T: BeRepr> Clone for BeSlice<'s, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'s, T: BeRepr> Copy for BeSlice<'s, T> {}

pub trait BeRepr: Sized + Clone + Copy + Debug {
    const BYTES: usize = core::mem::size_of::<Self>();

    fn copy_slice(data: &[u8], dst: &mut [MaybeUninit<Self>]) -> usize {
        let mut written = 0;
        for (src, dst) in data.chunks_exact(Self::BYTES).zip(dst) {
            let val = unsafe { Self::unaligned_be_read(src.as_ptr()) };
            dst.write(val);
            written += 1;
        }
        written
    }

    /// # Safety
    /// The range [ptr, ptr + Self::BYTES] must be valid for reading
    unsafe fn unaligned_be_read(ptr: *const u8) -> Self;
}

macro_rules! basic_be_impl {
    ($($t:ty),*) => {
        $(impl BeRepr for $t {
            unsafe fn unaligned_be_read(ptr: *const u8) -> Self {
                <$t>::from_be_bytes(unsafe { core::ptr::read_unaligned(ptr.cast()) })
            }
        })*
    };
}
basic_be_impl!(i8, i16, i32, i64, i128);
basic_be_impl!(u8, u16, u32, u64, u128);
basic_be_impl!(f32, f64);

impl<'s, T: BeRepr> BeSlice<'s, T> {
    #[inline(always)]
    pub fn new(data: &'s [u8]) -> Option<Self> {
        if data.len() % T::BYTES != 0 {
            return None;
        }
        Some(BeSlice {
            data,
            _repr: PhantomData,
        })
    }

    pub const fn len(&self) -> usize {
        self.data.len() / T::BYTES
    }

    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub const fn raw_bytes(&self) -> &'s [u8] {
        self.data
    }

    #[inline(always)]
    /// # Safety
    /// Calling this function with an out-of-bounds index is undefined behavior
    ///
    /// An index is in-bounds if idx < self.len() holds.
    pub unsafe fn get_unchecked(&self, idx: usize) -> T {
        let offset = T::BYTES * idx;
        let data = unsafe { self.data.get_unchecked(offset..offset + T::BYTES) };
        unsafe { T::unaligned_be_read(data.as_ptr()) }
    }
    #[inline]
    pub fn get(&self, idx: usize) -> Option<T> {
        if self.len() <= idx {
            return None;
        }
        Some(unsafe { self.get_unchecked(idx) })
    }

    #[inline]
    pub fn iter(&self) -> BeIterator<'s, T> {
        BeIterator(*self)
    }
}

pub struct BeIterator<'s, T: BeRepr>(BeSlice<'s, T>);

impl<'s, T: BeRepr> Iterator for BeIterator<'s, T> {
    type Item = T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.0.get(0)?;
        self.0.data = unsafe { self.0.data.get_unchecked(T::BYTES..) };
        Some(next)
    }
    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.data.len() / T::BYTES;
        (len, Some(len))
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let next = self.0.get(n);
        let consumed = (n + 1).min(self.len());
        self.0.data = unsafe { self.0.data.get_unchecked(consumed * T::BYTES..) };
        next
    }
}
impl<'s, T: BeRepr> ExactSizeIterator for BeIterator<'s, T> {
    fn len(&self) -> usize {
        self.0.data.len() / T::BYTES
    }
}
impl<'s, T: BeRepr> DoubleEndedIterator for BeIterator<'s, T> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        let last = self.len().checked_sub(1)?;
        let val = unsafe { self.0.get_unchecked(last) };
        self.0.data = unsafe { self.0.data.get_unchecked(..last * T::BYTES) };
        Some(val)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let idx = self.len().checked_sub(n).and_then(|n| n.checked_sub(1));
        let val = idx.map(|idx| unsafe { self.0.get_unchecked(idx) });
        let end = idx.unwrap_or(0).saturating_sub(1);
        self.0.data = unsafe { self.0.data.get_unchecked(..end * T::BYTES) };
        val
    }
}
