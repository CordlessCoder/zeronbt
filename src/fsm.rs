use crate::view::{BeRepr, BeSlice};

use super::{buf, error::*, tag::NbtTag};
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NbtFsm<'d> {
    buffer: buf::Buffer<'d>,
    state: TagState,
    namestate: NameState,
    stack: Vec<Nested>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Nested {
    List { tag: NbtTag, len: usize },
    Compound,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
enum NameState {
    NoNameLen,
    Name(usize),
    #[default]
    NameComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
enum TagState {
    #[default]
    Empty,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    ByteArrayNoLength,
    ByteArray(usize),
    StringNoLength,
    String(usize),
    ListNoTag,
    ListNoLength(NbtTag),
    List(NbtTag, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FsmResult<T> {
    /// The buffer must be filled with at least N bytes to continue parsing
    Needs(usize),
    Found(T),
}

impl<T> FsmResult<T> {
    fn on_found(self, cb: impl FnOnce()) -> Self {
        if matches!(self, FsmResult::Found(_)) {
            cb()
        };
        self
    }
    fn map_found<U>(self, cb: impl FnOnce(T) -> U) -> FsmResult<U> {
        match self {
            FsmResult::Found(val) => FsmResult::Found(cb(val)),
            FsmResult::Needs(needs) => FsmResult::Needs(needs),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NbtFragment<'s> {
    End,
    CompoundTag,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ShortListFrame(BeSlice<'s, i16>),
    IntListFrame(BeSlice<'s, i32>),
    LongListFrame(BeSlice<'s, i64>),
    FloatListFrame(BeSlice<'s, f32>),
    DoubleListFrame(BeSlice<'s, f64>),
    /// A tag will be represented by many repeated [TagFrame]s followed by an
    /// empty one
    NameFrame(&'s [u8]),
    /// An array will be represented by many repeated [ByteArrayFrame]s followed by an
    /// empty one
    ByteArrayFrame(&'s [u8]),
    /// A string will be represented by many repeated [StringFrame]s followed by an
    /// empty one
    StringFrame(&'s [u8]),
}

macro_rules! forward_needs {
    ($fsmresult:expr) => {
        match $fsmresult {
            FsmResult::Needs(n) => return FsmResult::Needs(n),
            FsmResult::Found(val) => val,
        }
    };
    (wrap($wrapper:expr), $fsmresult:expr) => {
        match $fsmresult {
            FsmResult::Needs(n) => return $wrapper(FsmResult::Needs(n)),
            FsmResult::Found(val) => val,
        }
    };
}

macro_rules! impl_list {
    ($t:ty, $frame:ident, $state:ident, $self:ident, $len:ident) => {{
        let view = $self.read_array::<$t>($len);
        if view.is_empty() {
            return Ok(FsmResult::Needs(<$t>::BYTES));
        }
        $self.state = TagState::List(NbtTag::$state, $len - view.len());
        return Ok(FsmResult::Found(NbtFragment::$frame(view)));
    }};
}

impl<'d> NbtFsm<'d> {
    pub const fn new() -> Self {
        Self {
            buffer: buf::Buffer::new(&[]),
            state: TagState::Empty,
            namestate: NameState::NameComplete,
            stack: Vec::new(),
        }
    }
    pub fn with_data<'new>(self, data: &'new [u8]) -> NbtFsm<'new> {
        let Self {
            stack,
            state,
            namestate,
            ..
        } = self;
        NbtFsm {
            buffer: buf::Buffer::new(data),
            state,
            stack,
            namestate,
        }
    }
    pub fn consumed(&self) -> usize {
        self.buffer.consumed().len()
    }
    #[inline]
    fn push_state(&mut self) {
        let TagState::List(tag, len) = self.state else {
            return;
        };
        self.stack.push(Nested::List { tag, len });
    }
    fn pop_outer(&mut self) {
        let Some(Nested::List { tag, len }) = self.stack.pop() else {
            self.state = TagState::Empty;
            return;
        };
        self.state = TagState::List(tag, len);
    }
    #[inline(always)]
    fn read_array<T: BeRepr>(&mut self, len: usize) -> BeSlice<'d, T> {
        let has = self.buffer.available().len() / T::BYTES;
        let len = len.min(has);
        unsafe {
            // SAFETY: The .available() call above guarantees that we can consume this many bytes
            let data = self.buffer.consume(len * T::BYTES).unwrap_unchecked();
            // SAFETY: BeSlice::new requires that the length of the slice is divisble by the T::BYTES,
            // which we just guaranteed
            BeSlice::new(data).unwrap_unchecked()
        }
    }
    #[inline(always)]
    pub fn next_fragment(&mut self) -> NbtResult<FsmResult<NbtFragment<'d>>> {
        'name: loop {
            match self.namestate {
                NameState::NameComplete => (),
                NameState::NoNameLen => {
                    let len = forward_needs!(wrap(Ok), self.capture_short());
                    let len = len as usize;
                    self.namestate = NameState::Name(len);
                    continue;
                }
                NameState::Name(len) => {
                    if len == 0 {
                        self.namestate = NameState::NameComplete;
                        break Ok(FsmResult::Found(NbtFragment::NameFrame(&[])));
                    }
                    let frame = len.min(self.buffer.available().len());
                    if frame == 0 {
                        break Ok(FsmResult::Needs(1));
                    }
                    let bytes = self.buffer.consume(frame).unwrap();
                    let len = len - frame;
                    self.namestate = NameState::Name(len);
                    break Ok(FsmResult::Found(NbtFragment::NameFrame(bytes)));
                }
            };
            loop {
                match self.state {
                    TagState::Empty => {
                        let tag = forward_needs!(wrap(Ok), self.capture_tag()?);
                        let state = match tag {
                            NbtTag::End => {
                                self.pop_outer();
                                return Ok(FsmResult::Found(NbtFragment::End));
                            }
                            NbtTag::Compound => {
                                self.stack.push(Nested::Compound);
                                self.state = TagState::Empty;
                                self.namestate = NameState::NoNameLen;
                                return Ok(FsmResult::Found(NbtFragment::CompoundTag));
                            }
                            NbtTag::Byte => TagState::Byte,
                            NbtTag::Short => TagState::Short,
                            NbtTag::Int => TagState::Int,
                            NbtTag::Long => TagState::Long,
                            NbtTag::Float => TagState::Float,
                            NbtTag::Double => TagState::Double,
                            NbtTag::ByteArray => TagState::ByteArrayNoLength,
                            NbtTag::String => TagState::StringNoLength,
                            NbtTag::List => TagState::ListNoTag,
                            NbtTag::IntArray => TagState::ListNoLength(NbtTag::Int),
                            NbtTag::LongArray => TagState::ListNoLength(NbtTag::Long),
                        };
                        self.state = state;
                        self.namestate = NameState::NoNameLen;
                        continue 'name;
                    }
                    TagState::ListNoTag => {
                        let tag = forward_needs!(wrap(Ok), self.capture_tag()?);
                        self.state = TagState::ListNoLength(tag);
                        continue;
                    }
                    TagState::ListNoLength(NbtTag::ByteArray) => {
                        self.state = TagState::ByteArrayNoLength;
                        continue;
                    }
                    TagState::ListNoLength(NbtTag::End) => {
                        self.pop_outer();
                        return Ok(FsmResult::Found(NbtFragment::End));
                    }
                    TagState::ListNoLength(tag) => {
                        let len = forward_needs!(wrap(Ok), self.capture_int());
                        let Ok(len) = usize::try_from(len) else {
                            return Err(NbtParseError::InvalidLen(len));
                        };
                        self.state = TagState::List(tag, len);
                        continue;
                    }
                    TagState::List(_, 0) => {
                        self.pop_outer();
                        continue;
                    }
                    TagState::List(NbtTag::End | NbtTag::Byte, _) => {
                        unreachable!()
                    }
                    TagState::List(NbtTag::String, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::StringNoLength;
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::ByteArray, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::ByteArrayNoLength;
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::IntArray, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::ListNoLength(NbtTag::Int);
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::LongArray, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::ListNoLength(NbtTag::Long);
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::List, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::ListNoTag;
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::Compound, ref mut len) => {
                        *len -= 1;
                        self.push_state();
                        self.state = TagState::Empty;
                        self.stack.push(Nested::Compound);
                        self.namestate = NameState::NameComplete;
                        continue;
                    }
                    TagState::List(NbtTag::Short, len) => {
                        impl_list!(i16, ShortListFrame, Short, self, len)
                    }
                    TagState::List(NbtTag::Int, len) => {
                        impl_list!(i32, IntListFrame, Int, self, len)
                    }
                    TagState::List(NbtTag::Long, len) => {
                        impl_list!(i64, LongListFrame, Long, self, len)
                    }
                    TagState::List(NbtTag::Float, len) => {
                        impl_list!(f32, FloatListFrame, Float, self, len)
                    }
                    TagState::List(NbtTag::Double, len) => {
                        impl_list!(f64, DoubleListFrame, Double, self, len)
                    }
                    TagState::StringNoLength => {
                        let len = forward_needs!(wrap(Ok), self.capture_short());
                        let len = len as usize;
                        self.state = TagState::String(len);
                        continue;
                    }
                    TagState::String(len) => {
                        if len == 0 {
                            self.pop_outer();
                            return Ok(FsmResult::Found(NbtFragment::StringFrame(&[])));
                        }
                        let view = self.read_array::<u8>(len).raw_bytes();
                        if view.is_empty() {
                            return Ok(FsmResult::Needs(1));
                        }
                        self.state = TagState::String(len - view.len());
                        return Ok(FsmResult::Found(NbtFragment::StringFrame(view)));
                    }
                    TagState::ByteArrayNoLength => {
                        let len = forward_needs!(wrap(Ok), self.capture_int());
                        let Ok(len) = usize::try_from(len) else {
                            return Err(NbtParseError::InvalidLen(len));
                        };
                        self.state = TagState::ByteArray(len);
                        continue;
                    }
                    TagState::ByteArray(len) => {
                        if len == 0 {
                            self.pop_outer();
                            return Ok(FsmResult::Found(NbtFragment::ByteArrayFrame(&[])));
                        }
                        let view = self.read_array::<u8>(len).raw_bytes();
                        if view.is_empty() {
                            return Ok(FsmResult::Needs(1));
                        }
                        self.state = TagState::ByteArray(len - view.len());
                        return Ok(FsmResult::Found(NbtFragment::ByteArrayFrame(view)));
                    }
                    TagState::Byte => {
                        return Ok(self
                            .capture_byte()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Byte));
                    }
                    TagState::Short => {
                        return Ok(self
                            .capture_short()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Short));
                    }
                    TagState::Int => {
                        return Ok(self
                            .capture_int()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Int));
                    }
                    TagState::Long => {
                        return Ok(self
                            .capture_long()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Long));
                    }
                    TagState::Float => {
                        return Ok(self
                            .capture_float()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Float));
                    }
                    TagState::Double => {
                        return Ok(self
                            .capture_double()
                            .on_found(|| self.pop_outer())
                            .map_found(NbtFragment::Double));
                    }
                };
            }
        }
    }
    #[inline(always)]
    fn consume_arr<const LEN: usize>(&mut self) -> FsmResult<&'d [u8; LEN]> {
        match self.buffer.consume_arr() {
            Some(data) => FsmResult::Found(data),
            None => FsmResult::Needs(LEN),
        }
    }
    #[inline(always)]
    fn capture_double(&mut self) -> FsmResult<f64> {
        let &be = forward_needs!(self.consume_arr());
        FsmResult::Found(f64::from_be_bytes(be))
    }
    #[inline(always)]
    fn capture_float(&mut self) -> FsmResult<f32> {
        let &be = forward_needs!(self.consume_arr());
        FsmResult::Found(f32::from_be_bytes(be))
    }
    #[inline(always)]
    fn capture_long(&mut self) -> FsmResult<i64> {
        let &be = forward_needs!(self.consume_arr());
        FsmResult::Found(i64::from_be_bytes(be))
    }
    #[inline(always)]
    fn capture_int(&mut self) -> FsmResult<i32> {
        let &be = forward_needs!(self.consume_arr());
        FsmResult::Found(i32::from_be_bytes(be))
    }
    #[inline(always)]
    fn capture_short(&mut self) -> FsmResult<i16> {
        let &be = forward_needs!(self.consume_arr());
        FsmResult::Found(i16::from_be_bytes(be))
    }
    #[inline(always)]
    fn capture_byte(&mut self) -> FsmResult<i8> {
        let &[byte] = forward_needs!(self.consume_arr());
        FsmResult::Found(byte as i8)
    }
    #[inline(always)]
    fn capture_tag(&mut self) -> NbtResult<FsmResult<NbtTag>> {
        let Some(&[tag]) = self.buffer.consume_arr() else {
            return Ok(FsmResult::Needs(1));
        };
        let tag = NbtTag::try_from(tag)?;
        Ok(FsmResult::Found(tag))
    }
}
