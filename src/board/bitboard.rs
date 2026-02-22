use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use super::square::Square;

/// Bitboard is set of squares represented as 64-bit integer.
/// Each bit corresponds to square (bit 0 = A1, bit 63 = H8).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct BitBoard(pub u64);

/// empty bitboard.
pub const EMPTY: BitBoard = BitBoard(0);

impl BitBoard {
    /// create a bitboard with a single square set.
    #[inline]
    pub fn from_square(sq: Square) -> Self {
        BitBoard(1u64 << sq.to_index())
    }

    /// population count (number of set bits).
    #[inline]
    pub fn popcnt(self) -> u32 {
        self.0.count_ones()
    }

    /// returns true if no bits are set.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// iterate over set squares (yields each Square whose bit is 1).
    #[inline]
    pub fn iter(self) -> BitBoardIter {
        BitBoardIter(self.0)
    }
}

/// iterator over the set bits of a BitBoard.
pub struct BitBoardIter(u64);

impl Iterator for BitBoardIter {
    type Item = Square;

    #[inline]
    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            let idx = self.0.trailing_zeros() as u8;
            self.0 &= self.0 - 1; // clear lowest set bit w/ Brian Kernighan's bit trick
            Some(Square::new(idx))
        }
    }
}

// --- operator impls ---
impl BitAnd for BitBoard {
    type Output = BitBoard;
    #[inline]
    fn bitand(self, rhs: BitBoard) -> BitBoard {
        BitBoard(self.0 & rhs.0)
    }
}

impl BitAndAssign for BitBoard {
    #[inline]
    fn bitand_assign(&mut self, rhs: BitBoard) {
        self.0 &= rhs.0;
    }
}

impl BitOr for BitBoard {
    type Output = BitBoard;
    #[inline]
    fn bitor(self, rhs: BitBoard) -> BitBoard {
        BitBoard(self.0 | rhs.0)
    }
}

impl BitOrAssign for BitBoard {
    #[inline]
    fn bitor_assign(&mut self, rhs: BitBoard) {
        self.0 |= rhs.0;
    }
}

impl BitXor for BitBoard {
    type Output = BitBoard;
    #[inline]
    fn bitxor(self, rhs: BitBoard) -> BitBoard {
        BitBoard(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for BitBoard {
    #[inline]
    fn bitxor_assign(&mut self, rhs: BitBoard) {
        self.0 ^= rhs.0;
    }
}

impl Not for BitBoard {
    type Output = BitBoard;
    #[inline]
    fn not(self) -> BitBoard {
        BitBoard(!self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::square::{File, Rank};

    #[test]
    fn test_from_square() {
        let sq = Square::new(0); // A1
        let bb = BitBoard::from_square(sq);
        assert_eq!(bb.0, 1);
    }

    #[test]
    fn test_popcnt() {
        assert_eq!(BitBoard(0xFF).popcnt(), 8);
        assert_eq!(EMPTY.popcnt(), 0);
    }

    #[test]
    fn test_iter() {
        let bb = BitBoard(0b1010_0001); // bits 0, 5, 7
        let squares: Vec<usize> = bb.iter().map(|sq| sq.to_index()).collect();
        assert_eq!(squares, vec![0, 5, 7]);
    }

    #[test]
    fn test_empty_iter() {
        let squares: Vec<Square> = EMPTY.iter().collect();
        assert!(squares.is_empty());
    }

    #[test]
    fn test_bitwise_ops() {
        let a = BitBoard(0xFF00);
        let b = BitBoard(0x00FF);
        assert_eq!((a | b).0, 0xFFFF);
        assert_eq!((a & b).0, 0);
        assert_eq!((a ^ b).0, 0xFFFF);
    }

    #[test]
    fn test_not() {
        let bb = BitBoard(0);
        assert_eq!((!bb).0, !0u64);
    }

    #[test]
    fn test_is_empty() {
        assert!(EMPTY.is_empty());
        assert!(!BitBoard(1).is_empty());
    }

    #[test]
    fn test_from_square_h8() {
        let sq = Square::make_square(Rank::from_index(7), File::from_index(7));
        let bb = BitBoard::from_square(sq);
        assert_eq!(bb.0, 1u64 << 63);
    }
}

// BitBoard data structure to represent state of board. Each bit represents square. Setting bit 0
// means "A1 is in this set", 63 means "H8 is in this set".

// "Where can the knight move?" -> `KNIGHT_ATTACKS[sq] & !own_pieces`
// ^^ single AND + NOT on 64 bit integers
