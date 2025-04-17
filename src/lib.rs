#![no_std]
extern crate alloc;
mod buf;
pub mod error;
mod fsm;
pub use fsm::*;
mod tag;
pub mod view;

#[cfg(test)]
mod tests {
    extern crate std;
    use core::ops::Range;
    use std::fmt::Debug;
    use std::prelude::rust_2024::*;
    use std::{dbg, vec};

    use crate::view::BeSlice;
    use crate::{FsmResult, NbtFragment, NbtFsm};

    const INT_BYTES: [u8; 8] = *b"12345678";

    #[derive(Debug)]
    struct FragmentsWithSteamedInput<'i> {
        input: &'i [u8],
        visible: Range<usize>,
        fsm: NbtFsm<'i>,
    }

    impl<'d> FragmentsWithSteamedInput<'d> {
        fn new(input: &'d [u8]) -> Self {
            Self {
                input,
                visible: 0..0,
                fsm: NbtFsm::new(),
            }
        }
    }

    impl<'d> Iterator for FragmentsWithSteamedInput<'d> {
        type Item = NbtFragment<'d>;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                match self
                    .fsm
                    .next_fragment()
                    .expect("NBT Parsing returned an error on valid input")
                {
                    FsmResult::Needs(_) => {
                        if self.visible.end == self.input.len() {
                            return None;
                        };
                        self.visible.start += self.fsm.consumed();
                        self.visible.end += 1;
                        let temp = std::mem::take(&mut self.fsm);
                        self.fsm = temp.with_data(&self.input[self.visible.clone()]);
                    }
                    FsmResult::Found(fragment) => break Some(dbg!(fragment)),
                }
            }
        }
    }

    fn push_name(input: &mut Vec<u8>, name: &[u8]) {
        input.extend_from_slice(&(name.len() as u16).to_be_bytes());
        input.extend_from_slice(name);
    }

    fn expect_name<'f>(mut fragments: impl Iterator<Item = NbtFragment<'f>>, name: &[u8]) {
        let mut pos = 0;
        while pos != name.len() {
            let frame = fragments
                .next()
                .expect("Expected more NameFrame's with the name");
            let NbtFragment::NameFrame(data) = frame else {
                panic!("Found invalid NBT Fragment when parsing name: {frame:?}");
            };
            let rem = &name[pos..];
            assert!(rem.starts_with(data));
            pos += data.len();
        }
        assert_eq!(Some(NbtFragment::NameFrame(&[])), fragments.next())
    }

    enum Expect<'d> {
        Fragment(NbtFragment<'d>),
        Name(&'d [u8]),
    }

    impl<'d> Expect<'d> {
        #[track_caller]
        fn expect<'f>(&self, mut fragments: impl Iterator<Item = NbtFragment<'f>>) {
            match self {
                Expect::Fragment(expected) => {
                    let fragment = fragments.next();
                    assert_eq!(Some(expected), fragment.as_ref());
                }
                Expect::Name(name) => {
                    expect_name(&mut fragments, name);
                }
            }
        }
    }

    #[test]
    fn read_chunk() {
        let data = include_bytes!("../assets/chunk_0-0.nbt");
        let mut fragments = vec![];
        let mut fsm = NbtFsm::new().with_data(data);
        loop {
            match fsm.next_fragment() {
                Err(err) => {
                    panic!("{err}\n{fsm:?}")
                }
                Ok(FsmResult::Needs(_)) => break,
                Ok(FsmResult::Found(f)) => {
                    fragments.push(f);
                }
            }
        }
    }

    #[test]
    fn read_bigtest() {
        let data = include_bytes!("../assets/bigtest.nbt");
        let mut fsm = NbtFsm::new().with_data(data);
        loop {
            match fsm.next_fragment() {
                Err(err) => {
                    panic!("{err}\n{fsm:?}")
                }
                Ok(FsmResult::Needs(_)) => break,
                Ok(FsmResult::Found(_)) => {}
            }
        }
    }

    #[test]
    fn read_numerics() {
        let mut complete_input = vec![0];
        let mut num = |tag, name: &[u8], len| {
            complete_input.push(tag);
            push_name(&mut complete_input, name);
            complete_input.extend_from_slice(&INT_BYTES[..len]);
        };
        num(1, b"BYTE", 1);
        num(2, b"SHORT", 2);
        num(3, b"INT", 4);
        num(4, b"LONG", 8);
        num(5, b"FLOAT", 4);
        num(6, b"DOUBLE", 8);
        let expected = [
            Expect::Fragment(NbtFragment::End),
            Expect::Name(b"BYTE"),
            Expect::Fragment(NbtFragment::Byte(INT_BYTES[0] as i8)),
            Expect::Name(b"SHORT"),
            Expect::Fragment(NbtFragment::Short(i16::from_be_bytes(
                INT_BYTES[..2].try_into().unwrap(),
            ))),
            Expect::Name(b"INT"),
            Expect::Fragment(NbtFragment::Int(i32::from_be_bytes(
                INT_BYTES[..4].try_into().unwrap(),
            ))),
            Expect::Name(b"LONG"),
            Expect::Fragment(NbtFragment::Long(i64::from_be_bytes(
                INT_BYTES[..8].try_into().unwrap(),
            ))),
            Expect::Name(b"FLOAT"),
            Expect::Fragment(NbtFragment::Float(f32::from_be_bytes(
                INT_BYTES[..4].try_into().unwrap(),
            ))),
            Expect::Name(b"DOUBLE"),
            Expect::Fragment(NbtFragment::Double(f64::from_be_bytes(
                INT_BYTES[..8].try_into().unwrap(),
            ))),
        ];

        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);
        for expected in expected {
            expected.expect(&mut fragments);
        }
        assert!(fragments.next().is_none())
    }
    #[test]
    fn read_byte_array() {
        let mut complete_input = vec![7];
        let name = b"testByteArray";
        push_name(&mut complete_input, name);
        let len: i32 = 1024 * 4;
        complete_input.extend_from_slice(&len.to_be_bytes());
        let bytearr: Vec<u8> = ((0..len).map(|n| n as u8)).collect();
        complete_input.extend_from_slice(&bytearr);
        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);

        let expected = [Expect::Name(b"testByteArray")];

        for expected in expected {
            expected.expect(&mut fragments);
        }

        let mut bytearr_position = 0;
        for frame in &mut fragments {
            let NbtFragment::ByteArrayFrame(data) = frame else {
                panic!("Found invalid NBT Fragment when parsing byte array: {frame:?}");
            };
            if data.is_empty() {
                // End of array
                break;
            }
            let rem = &bytearr[bytearr_position..];
            assert!(rem.starts_with(data));
            bytearr_position += data.len();
        }
        assert!(bytearr_position == bytearr.len());
        assert!(fragments.next().is_none());
    }

    #[test]
    fn read_string() {
        let mut complete_input = vec![8];
        let name = b"testString";
        complete_input.extend_from_slice(&(name.len() as u16).to_be_bytes());
        complete_input.extend_from_slice(name);
        let len: u16 = 1024 * 4;
        complete_input.extend_from_slice(&len.to_be_bytes());
        let string_data: Vec<u8> = ('a'..='a')
            .cycle()
            .flat_map(|c| c.to_string().into_bytes())
            .take(len.into())
            .collect();
        complete_input.extend_from_slice(&string_data);
        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);

        let expected = [Expect::Name(b"testString")];

        for expected in expected {
            expected.expect(&mut fragments);
        }
        let mut data_position = 0;
        for frame in &mut fragments {
            let NbtFragment::StringFrame(data) = frame else {
                panic!("Found invalid NBT Fragment when parsing string: {frame:?}");
            };
            if data.is_empty() {
                // End of array
                break;
            }
            let rem = &string_data[data_position..];
            assert!(
                rem.starts_with(data),
                "rem = {rem:?}, data = {data:?}, pos = {data_position}"
            );
            data_position += data.len();
        }
        assert!(data_position == string_data.len());
        assert!(fragments.next().is_none());
    }

    #[test]
    fn read_list() {
        let mut complete_input = vec![9];
        push_name(&mut complete_input, b"testIntList");
        let ints: Vec<i32> = INT_BYTES
            .windows(4)
            .cycle()
            .map(|bytes| i32::from_be_bytes(bytes.try_into().unwrap()))
            .take(128)
            .collect();
        // Int tag
        complete_input.push(3);
        // Len
        complete_input.extend_from_slice(&(ints.len() as i32).to_be_bytes());
        // Body
        for &int in &ints {
            complete_input.extend_from_slice(&int.to_be_bytes());
        }

        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);
        let header = [Expect::Name(b"testIntList")];
        for expect in header {
            expect.expect(&mut fragments);
        }
        for int in ints {
            let bytes = int.to_be_bytes();
            Expect::Fragment(NbtFragment::IntListFrame(BeSlice::new(&bytes).unwrap()))
                .expect(&mut fragments);
        }
        assert!(fragments.next().is_none())
    }

    #[test]
    fn read_intarr() {
        let mut complete_input = vec![11];
        push_name(&mut complete_input, b"testIntArray");
        let ints: Vec<i32> = INT_BYTES
            .windows(4)
            .cycle()
            .map(|bytes| i32::from_be_bytes(bytes.try_into().unwrap()))
            .take(128)
            .collect();
        // Len
        complete_input.extend_from_slice(&(ints.len() as i32).to_be_bytes());
        // Body
        for &int in &ints {
            complete_input.extend_from_slice(&int.to_be_bytes());
        }

        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);
        let header = [Expect::Name(b"testIntArray")];
        for expect in header {
            expect.expect(&mut fragments);
        }
        for int in ints {
            let bytes = int.to_be_bytes();
            Expect::Fragment(NbtFragment::IntListFrame(BeSlice::new(&bytes).unwrap()))
                .expect(&mut fragments);
        }
        assert!(fragments.next().is_none())
    }

    #[test]
    fn read_compound() {
        let mut complete_input = vec![10];
        push_name(&mut complete_input, b"testCompound");

        let mut num = |tag, name: &[u8], len| {
            complete_input.push(tag);
            push_name(&mut complete_input, name);
            complete_input.extend_from_slice(&INT_BYTES[..len]);
        };
        num(1, b"BYTE", 1);
        num(2, b"SHORT", 2);
        num(3, b"INT", 4);
        num(4, b"LONG", 8);
        num(5, b"FLOAT", 4);
        num(6, b"DOUBLE", 8);
        complete_input.push(0);
        let expected = [
            Expect::Fragment(NbtFragment::CompoundTag),
            Expect::Name(b"testCompound"),
            Expect::Name(b"BYTE"),
            Expect::Fragment(NbtFragment::Byte(INT_BYTES[0] as i8)),
            Expect::Name(b"SHORT"),
            Expect::Fragment(NbtFragment::Short(i16::from_be_bytes(
                INT_BYTES[..2].try_into().unwrap(),
            ))),
            Expect::Name(b"INT"),
            Expect::Fragment(NbtFragment::Int(i32::from_be_bytes(
                INT_BYTES[..4].try_into().unwrap(),
            ))),
            Expect::Name(b"LONG"),
            Expect::Fragment(NbtFragment::Long(i64::from_be_bytes(
                INT_BYTES[..8].try_into().unwrap(),
            ))),
            Expect::Name(b"FLOAT"),
            Expect::Fragment(NbtFragment::Float(f32::from_be_bytes(
                INT_BYTES[..4].try_into().unwrap(),
            ))),
            Expect::Name(b"DOUBLE"),
            Expect::Fragment(NbtFragment::Double(f64::from_be_bytes(
                INT_BYTES[..8].try_into().unwrap(),
            ))),
            Expect::Fragment(NbtFragment::End),
        ];
        let mut fragments = FragmentsWithSteamedInput::new(&complete_input);
        for expect in expected {
            expect.expect(&mut fragments);
        }
        assert!(fragments.next().is_none());
    }
}
