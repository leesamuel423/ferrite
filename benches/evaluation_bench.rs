use std::str::FromStr;
use ferrite::board::Board;
use ferrite::evaluation::evaluate;
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_evaluation(c: &mut Criterion) {
    ferrite::board::init();
    let positions = vec![
        ("startpos", Board::default()),
        ("middlegame", Board::from_str("r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4").unwrap()),
        ("endgame", Board::from_str("8/5k2/8/8/8/8/4K3/4R3 w - - 0 1").unwrap()),
        ("complex", Board::from_str("r1bq1rk1/pp2ppbp/2np2p1/2n5/P3PP2/N1P2N2/1PB3PP/R1B1QRK1 b - - 0 10").unwrap()),
    ];
    for (name, board) in &positions {
        c.bench_function(&format!("eval_{}", name), |b| {
            b.iter(|| evaluate(board))
        });
    }
}

criterion_group!(benches, bench_evaluation);
criterion_main!(benches);
