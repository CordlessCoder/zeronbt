use InputFile::*;
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use std::{hint::black_box, ops::Range};
use zeronbt::{FsmResult, NbtFsm};

include!("common.rs");

pub struct ChunkedIoSource {
    data: &'static [u8],
    chunk_size: usize,
    view: Range<usize>,
}

pub fn chunk_io(input: InputFile, chunk_size: usize) -> ChunkedIoSource {
    ChunkedIoSource {
        data: input.data(),
        chunk_size,
        view: 0..0,
    }
}

impl ChunkedIoSource {
    #[inline]
    pub fn current_view(&self) -> &'static [u8] {
        &self.data[self.view.clone()]
    }
    #[inline(always)]
    pub fn request_more(&mut self) {
        self.view.end = self
            .view
            .end
            .saturating_add(self.chunk_size)
            .min(self.data.len());
    }
    pub fn consume(&mut self, count: usize) {
        self.view.start = self.view.start.saturating_add(count).min(self.view.end)
    }
    #[inline(always)]
    pub fn is_done(&self) -> bool {
        self.view.start >= self.data.len()
    }
}

#[library_benchmark]
#[bench::bigtest_small_io(chunk_io(BigTest, 16))]
#[bench::bigtest_large_io(chunk_io(BigTest, 1024))]
#[bench::bigtest_complete(chunk_io(BigTest, 1024 * 1024))]
#[bench::chunk_small_io(chunk_io(Chunk, 16))]
#[bench::chunk_large_io(chunk_io(Chunk, 1024))]
#[bench::chunk_complete(chunk_io(Chunk, 1024 * 1024))]
fn parse_zeronbt(io_source: ChunkedIoSource) -> u64 {
    black_box(parse_from_source(black_box(io_source)))
}

fn sink<T>(val: T) {
    _ = black_box(val);
}

fn parse_from_source(mut source: ChunkedIoSource) -> u64 {
    let mut fsm = NbtFsm::new();
    let mut count = 0;
    while !source.is_done() {
        fsm = fsm.with_data(source.current_view());
        match fsm.next_fragment() {
            Err(error) => panic!("Failed to parse NBT: {error}"),
            Ok(FsmResult::Needs(_)) => {
                source.request_more();
            }
            Ok(FsmResult::Found(fragment)) => {
                sink(black_box(fragment));
                count += 1;
            }
        }
        source.consume(fsm.consumed())
    }
    count
}

library_benchmark_group!(
    name = parse_streaming;
    compare_by_id = true;
    benchmarks = parse_zeronbt
);
main!(library_benchmark_groups = parse_streaming);
