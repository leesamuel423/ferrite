use std::str::FromStr;
use ferrite::board::{Board, MoveGen};
use ferrite::search::{search, SearchState};
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_search(c: &mut Criterion) {
    ferrite::board::init();
    let board = Board::default();

    c.bench_function("search_depth_3_startpos", |b| {
        b.iter(|| {
            let mut state = SearchState::new();
            state.silent = true;
            search(&board, &mut state, 3)
        })
    });

    let kiwipete = Board::from_str(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
    ).unwrap();

    c.bench_function("search_depth_3_kiwipete", |b| {
        b.iter(|| {
            let mut state = SearchState::new();
            state.silent = true;
            search(&kiwipete, &mut state, 3)
        })
    });

    c.bench_function("search_depth_4_startpos", |b| {
        b.iter(|| {
            let mut state = SearchState::new();
            state.silent = true;
            search(&board, &mut state, 4)
        })
    });
}

fn bench_movegen(c: &mut Criterion) {
    ferrite::board::init();
    let board = Board::default();
    c.bench_function("movegen_startpos", |b| {
        b.iter(|| { let moves: Vec<_> = MoveGen::new_legal(&board).collect(); moves.len() })
    });

    let kiwipete = Board::from_str(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
    ).unwrap();
    c.bench_function("movegen_kiwipete", |b| {
        b.iter(|| { let moves: Vec<_> = MoveGen::new_legal(&kiwipete).collect(); moves.len() })
    });
}

criterion_group!(benches, bench_search, bench_movegen);
criterion_main!(benches);
