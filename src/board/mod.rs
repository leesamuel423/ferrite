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

pub use bitboard::{BitBoard, EMPTY};
pub use board::{Board, BoardStatus};
pub use chessmove::ChessMove;
pub use movegen::MoveGen;
pub use piece::{Color, Piece};
pub use square::{File, Rank, Square, ALL_SQUARES};

/// Init attack tables and Zobrist keys. Must be called before any board operations.
pub fn init() {
    attacks::init_attacks();
    zobrist::init_zobrist();
}
