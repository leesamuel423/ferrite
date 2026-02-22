use std::fmt;

/// a square on the chess board, 0..63 (A1=0, H8=63).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Square(u8);

impl Square {
    #[inline]
    pub fn new(index: u8) -> Self {
        debug_assert!(index < 64);
        Square(index)
    }

    #[inline]
    pub fn make_square(rank: Rank, file: File) -> Self {
        Square(rank.0 * 8 + file.0)
    }

    #[inline]
    pub fn to_index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub fn rank(self) -> Rank {
        Rank(self.0 >> 3)
    }

    #[inline]
    pub fn file(self) -> File {
        File(self.0 & 7)
    }

}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file_char = (b'a' + self.file().0) as char;
        let rank_char = (b'1' + self.rank().0) as char;
        write!(f, "{}{}", file_char, rank_char)
    }
}

/// a rank (row) on the chess board, 0..7 (Rank 1 = 0, Rank 8 = 7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Rank(pub(crate) u8);

impl Rank {
    #[inline]
    pub fn from_index(i: usize) -> Self {
        debug_assert!(i < 8);
        Rank(i as u8)
    }

    #[inline]
    pub fn to_index(self) -> usize {
        self.0 as usize
    }
}

/// a file (column) on the chess board, 0..7 (A=0, H=7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct File(pub(crate) u8);

impl File {
    #[inline]
    pub fn from_index(i: usize) -> Self {
        debug_assert!(i < 8);
        File(i as u8)
    }

    #[inline]
    pub fn to_index(self) -> usize {
        self.0 as usize
    }
}

/// all 64 squares in order A1, B1, ..., H8.
pub const ALL_SQUARES: [Square; 64] = {
    let mut arr = [Square(0); 64];
    let mut i = 0u8;
    while i < 64 {
        arr[i as usize] = Square(i);
        i += 1;
    }
    arr
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a1_is_zero() {
        let sq = Square::make_square(Rank::from_index(0), File::from_index(0));
        assert_eq!(sq.to_index(), 0);
    }

    #[test]
    fn test_h8_is_63() {
        let sq = Square::make_square(Rank::from_index(7), File::from_index(7));
        assert_eq!(sq.to_index(), 63);
    }

    #[test]
    fn test_rank_file_roundtrip() {
        for i in 0..64u8 {
            let sq = Square::new(i);
            let reconstructed = Square::make_square(sq.rank(), sq.file());
            assert_eq!(sq, reconstructed);
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(Square::new(0).to_string(), "a1");
        assert_eq!(Square::new(63).to_string(), "h8");
        assert_eq!(Square::new(4).to_string(), "e1");
    }
}

// `Square` is a single board position, stored as a u8 from 0 to 63. Using LERF mapping
// (Little-Endian Rank-File): A1 = 0, B1 = 1, ....H8 = 63

// This means `square_index = rank * 8 + file`, rank = `index >> 3`, file = `index & 7`
//
// Using LERF because it makes bitwise shift operations correspond naturally to moving "up" the
// board (shifting left by 8 = moving one rank up)

// Using bitwise here (where each bit corresponds to a square) b/c can use CPU bitwise operations
// to answer questions like "which squares can this knight attack?" in single instruction
