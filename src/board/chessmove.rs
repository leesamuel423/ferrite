use std::fmt;

use super::piece::Piece;
use super::square::Square;

/// Chess move encoded compactly in a u16.
///
/// Layout: `src(6) | dst(6) | promo(2) | is_promo(1) | reserved(1)`
///   - bits 0..5:  source square (0-63)
///   - bits 6..11: destination square (0-63)
///   - bits 12..13: promotion piece (0=Knight, 1=Bishop, 2=Rook, 3=Queen)
///   - bit 14: is_promotion flag
///   - bit 15: reserved
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessMove(u16);

impl ChessMove {
    #[inline]
    pub fn new(src: Square, dst: Square, promotion: Option<Piece>) -> Self {
        let mut bits = (src.to_index() as u16) | ((dst.to_index() as u16) << 6);
        if let Some(p) = promotion {
            let code = match p {
                Piece::Knight => 0,
                Piece::Bishop => 1,
                Piece::Rook => 2,
                Piece::Queen => 3,
                _ => 3, // default to queen for invalid promo pieces
            };
            bits |= code << 12;
            bits |= 1 << 14; // is_promotion flag
        }
        ChessMove(bits)
    }

    #[inline]
    pub fn get_source(self) -> Square {
        Square::new((self.0 & 0x3F) as u8)
    }

    #[inline]
    pub fn get_dest(self) -> Square {
        Square::new(((self.0 >> 6) & 0x3F) as u8)
    }

    #[inline]
    pub fn get_promotion(self) -> Option<Piece> {
        if self.0 & (1 << 14) == 0 {
            None
        } else {
            Some(match (self.0 >> 12) & 3 {
                0 => Piece::Knight,
                1 => Piece::Bishop,
                2 => Piece::Rook,
                _ => Piece::Queen,
            })
        }
    }
}

impl fmt::Display for ChessMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.get_source(), self.get_dest())?;
        if let Some(promo) = self.get_promotion() {
            let c = match promo {
                Piece::Knight => 'n',
                Piece::Bishop => 'b',
                Piece::Rook => 'r',
                Piece::Queen => 'q',
                _ => 'q',
            };
            write!(f, "{}", c)?;
        }
        Ok(())
    }
}

impl fmt::Debug for ChessMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChessMove({})", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::square::{File, Rank};

    #[test]
    fn test_basic_move() {
        let src = Square::make_square(Rank::from_index(1), File::from_index(4)); // e2
        let dst = Square::make_square(Rank::from_index(3), File::from_index(4)); // e4
        let mv = ChessMove::new(src, dst, None);
        assert_eq!(mv.get_source(), src);
        assert_eq!(mv.get_dest(), dst);
        assert_eq!(mv.get_promotion(), None);
        assert_eq!(mv.to_string(), "e2e4");
    }

    #[test]
    fn test_promotion() {
        let src = Square::make_square(Rank::from_index(6), File::from_index(0)); // a7
        let dst = Square::make_square(Rank::from_index(7), File::from_index(0)); // a8
        let mv = ChessMove::new(src, dst, Some(Piece::Queen));
        assert_eq!(mv.get_promotion(), Some(Piece::Queen));
        assert_eq!(mv.to_string(), "a7a8q");
    }

    #[test]
    fn test_all_promotions() {
        let src = Square::new(48); // a7
        let dst = Square::new(56); // a8
        for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
            let mv = ChessMove::new(src, dst, Some(piece));
            assert_eq!(mv.get_promotion(), Some(piece));
        }
    }

    #[test]
    fn test_roundtrip_all_squares() {
        for s in 0..64u8 {
            for d in 0..64u8 {
                if s == d { continue; }
                let mv = ChessMove::new(Square::new(s), Square::new(d), None);
                assert_eq!(mv.get_source().to_index(), s as usize);
                assert_eq!(mv.get_dest().to_index(), d as usize);
            }
        }
    }

    #[test]
    fn test_equality() {
        let a = ChessMove::new(Square::new(12), Square::new(28), None);
        let b = ChessMove::new(Square::new(12), Square::new(28), None);
        let c = ChessMove::new(Square::new(12), Square::new(20), None);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}

// move is packed into 16 bit int for maximum cache efficiency
// so w/ encoding, move lists fit in cache lines and comparisons are single integer comparisons
