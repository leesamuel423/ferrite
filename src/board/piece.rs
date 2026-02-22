use std::ops::Not;

// chess piece type (pawn, knight, bishop, rook, queen, king)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Piece {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl Piece {
    #[inline]
    pub fn to_index(self) -> usize {
        self as usize
    }

    // all 6 piece types in order
    pub const ALL: [Piece; 6] = [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
        Piece::King,
    ];
}

// color: white or black
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Color {
    White,
    Black
}

impl Color {
    #[inline]
    pub fn to_index(self) -> usize {
        self as usize
    }
}

impl Not for Color {
    type Output = Color;

    #[inline]
    fn not(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piece_indices() {
        assert_eq!(Piece::Pawn.to_index(), 0);
        assert_eq!(Piece::King.to_index(), 5);
    }

    #[test]
    fn test_color_flip() {
        assert_eq!(!Color::White, Color::Black);
        assert_eq!(!Color::Black, Color::White);
    }

    #[test]
    fn test_color_index() {
        assert_eq!(Color::White.to_index(), 0);
        assert_eq!(Color::Black.to_index(), 1);
    }
}

// Design Decision: `Piece` uses `#[derive(Copy)]` -> it's a single byte, so copying is free. 
// We'll use `Copy` on all small types (Squares, BitBoard, ChessMove) so don't have to worry abt
// ownership or borrowing for these
