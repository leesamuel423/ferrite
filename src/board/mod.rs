mod attacks;
mod bitboard;
#[allow(clippy::module_inception)]
mod board;
mod chessmove;
mod magic;
mod movegen;
mod piece;
mod square;
mod zobrist;

#[allow(unused_imports)]
pub use bitboard::{BitBoard, EMPTY};
#[allow(unused_imports)]
pub use board::{Board, BoardStatus};
pub use chessmove::ChessMove;
#[allow(unused_imports)]
pub use movegen::MoveGen;
pub use piece::{Color, Piece};
#[allow(unused_imports)]
pub use square::{File, Rank, Square, ALL_SQUARES};

/// Init attack tables and Zobrist keys. Must be called before any board operations.
pub fn init() {
    attacks::init_attacks();
    zobrist::init_zobrist();
}
