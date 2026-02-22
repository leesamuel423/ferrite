use std::sync::Once;

use super::bitboard::BitBoard;
use super::piece::Color;
use super::square::Square;
use super::magic::{BISHOP_BITS, ROOK_BITS, MagicEntry, Rng, find_magic};

// --- static tables ---

static INIT: Once = Once::new();

static mut KNIGHT_ATTACKS: [BitBoard; 64] = [BitBoard(0); 64];
static mut KING_ATTACKS: [BitBoard; 64] = [BitBoard(0); 64];
static mut PAWN_ATTACKS: [[BitBoard; 64]; 2] = [[BitBoard(0); 64]; 2];

static mut BISHOP_TABLE: Vec<BitBoard> = Vec::new();
static mut ROOK_TABLE: Vec<BitBoard> = Vec::new();
static mut BISHOP_ENTRIES: [MagicEntry; 64] = unsafe { std::mem::zeroed() };
static mut ROOK_ENTRIES: [MagicEntry; 64] = unsafe { std::mem::zeroed() };

/// Init all attack tables. Must be called once before any lookup.
pub fn init_attacks() {
    INIT.call_once(|| {
        unsafe {
            init_knight_attacks();
            init_king_attacks();
            init_pawn_attacks();
            init_slider_attacks();
        }
    });
}

// --- Leaper lookup functions ---

#[inline]
pub fn knight_attacks(sq: Square) -> BitBoard {
    unsafe { KNIGHT_ATTACKS[sq.to_index()] }
}

#[inline]
pub fn king_attacks(sq: Square) -> BitBoard {
    unsafe { KING_ATTACKS[sq.to_index()] }
}

#[inline]
pub fn pawn_attacks(color: Color, sq: Square) -> BitBoard {
    unsafe { PAWN_ATTACKS[color.to_index()][sq.to_index()] }
}

// --- Slider lookup functions ---

#[inline]
pub fn bishop_attacks(sq: Square, occupied: BitBoard) -> BitBoard {
    unsafe {
        let entry = &BISHOP_ENTRIES[sq.to_index()];
        let idx = magic_index(entry, occupied);
        BISHOP_TABLE[idx]
    }
}

#[inline]
pub fn rook_attacks(sq: Square, occupied: BitBoard) -> BitBoard {
    unsafe {
        let entry = &ROOK_ENTRIES[sq.to_index()];
        let idx = magic_index(entry, occupied);
        ROOK_TABLE[idx]
    }
}

#[inline]
#[allow(dead_code)] // Public API, used in tests
pub fn queen_attacks(sq: Square, occupied: BitBoard) -> BitBoard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

/// Compute magic table index for a given occupancy.
#[inline]
fn magic_index(entry: &MagicEntry, occupied: BitBoard) -> usize {
    let blockers = occupied & entry.mask;
    let hash = blockers.0.wrapping_mul(entry.magic);
    entry.offset as usize + (hash >> entry.shift) as usize
}

// --- Initialization ---

unsafe fn init_knight_attacks() {
    unsafe {
        let offsets: [(i8, i8); 8] = [
            (-2, -1), (-2, 1), (-1, -2), (-1, 2),
            (1, -2), (1, 2), (2, -1), (2, 1),
        ];
        for sq in 0..64u8 {
            let r = (sq >> 3) as i8;
            let f = (sq & 7) as i8;
            let mut bb = 0u64;
            for (dr, df) in offsets {
                let nr = r + dr;
                let nf = f + df;
                if (0..8).contains(&nr) && (0..8).contains(&nf) {
                    bb |= 1u64 << (nr * 8 + nf);
                }
            }
            KNIGHT_ATTACKS[sq as usize] = BitBoard(bb);
        }
    }
}

unsafe fn init_king_attacks() {
    unsafe {
        let offsets: [(i8, i8); 8] = [
            (-1, -1), (-1, 0), (-1, 1),
            (0, -1),           (0, 1),
            (1, -1),  (1, 0),  (1, 1),
        ];
        for sq in 0..64u8 {
            let r = (sq >> 3) as i8;
            let f = (sq & 7) as i8;
            let mut bb = 0u64;
            for (dr, df) in offsets {
                let nr = r + dr;
                let nf = f + df;
                if (0..8).contains(&nr) && (0..8).contains(&nf) {
                    bb |= 1u64 << (nr * 8 + nf);
                }
            }
            KING_ATTACKS[sq as usize] = BitBoard(bb);
        }
    }
}

unsafe fn init_pawn_attacks() {
    unsafe {
        for sq in 0..64u8 {
            let r = (sq >> 3) as i8;
            let f = (sq & 7) as i8;
            let mut white = 0u64;
            let mut black = 0u64;

            // white: rank+1
            if r + 1 < 8 {
                if f > 0 { white |= 1u64 << ((r + 1) * 8 + (f - 1)); }
                if f + 1 < 8 { white |= 1u64 << ((r + 1) * 8 + (f + 1)); }
            }
            // black: rank-1
            if r > 0 {
                if f > 0 { black |= 1u64 << ((r - 1) * 8 + (f - 1)); }
                if f + 1 < 8 { black |= 1u64 << ((r - 1) * 8 + (f + 1)); }
            }

            PAWN_ATTACKS[0][sq as usize] = BitBoard(white);
            PAWN_ATTACKS[1][sq as usize] = BitBoard(black);
        }
    }
}

unsafe fn init_slider_attacks() {
    unsafe {
        let mut rng = Rng(0x12345678_9ABCDEF0); // fixed seed for deterministic init

        // Compute total table sizes
        let mut bishop_total = 0usize;
        let mut rook_total = 0usize;
        for sq in 0..64 {
            bishop_total += 1 << BISHOP_BITS[sq];
            rook_total += 1 << ROOK_BITS[sq];
        }

        BISHOP_TABLE = vec![BitBoard(0); bishop_total];
        ROOK_TABLE = vec![BitBoard(0); rook_total];

        // Init bishop entries ... find magic for each square
        let mut offset = 0u32;
        for sq in 0..64 {
            let mask = bishop_mask(sq);
            let bits = BISHOP_BITS[sq];
            let shift = 64 - bits;

            let magic = find_magic(mask, bits, &|occ| bishop_attacks_slow(sq, occ), &mut rng);

            BISHOP_ENTRIES[sq] = MagicEntry {
                mask: BitBoard(mask),
                magic,
                shift,
                offset,
            };

            // Fill table for all occupancy subsets
            let mut occ = 0u64;
            loop {
                let idx = offset as usize
                    + ((occ.wrapping_mul(magic)) >> shift) as usize;
                BISHOP_TABLE[idx] = BitBoard(bishop_attacks_slow(sq, occ));

                occ = occ.wrapping_sub(mask) & mask;
                if occ == 0 { break; }
            }

            offset += 1u32 << bits;
        }

        // Init rook entries ... find magic for each square
        offset = 0;
        for sq in 0..64 {
            let mask = rook_mask(sq);
            let bits = ROOK_BITS[sq];
            let shift = 64 - bits;

            let magic = find_magic(mask, bits, &|occ| rook_attacks_slow(sq, occ), &mut rng);

            ROOK_ENTRIES[sq] = MagicEntry {
                mask: BitBoard(mask),
                magic,
                shift,
                offset,
            };

            let mut occ = 0u64;
            loop {
                let idx = offset as usize
                    + ((occ.wrapping_mul(magic)) >> shift) as usize;
                ROOK_TABLE[idx] = BitBoard(rook_attacks_slow(sq, occ));

                occ = occ.wrapping_sub(mask) & mask;
                if occ == 0 { break; }
            }

            offset += 1u32 << bits;
        }
    }
}

// --- Reference (slow) ray-trace generators used during init only ---

/// Bishop relevant occupancy mask (excludes edges)
fn bishop_mask(sq: usize) -> u64 {
    let mut mask = 0u64;
    let r = (sq / 8) as i8;
    let f = (sq % 8) as i8;

    for &(dr, df) in &[(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
        let mut nr = r + dr;
        let mut nf = f + df;
        while nr > 0 && nr < 7 && nf > 0 && nf < 7 {
            mask |= 1u64 << (nr * 8 + nf);
            nr += dr;
            nf += df;
        }
    }
    mask
}

/// Rook relevant occupancy mask (excludes edges unless on that edge)
fn rook_mask(sq: usize) -> u64 {
    let mut mask = 0u64;
    let r = (sq / 8) as i8;
    let f = (sq % 8) as i8;

    // Rank (horizontal),  exclude edge files
    for nf in 1..7i8 {
        if nf != f {
            mask |= 1u64 << (r * 8 + nf);
        }
    }
    // File (vertical), exclude edge ranks
    for nr in 1..7i8 {
        if nr != r {
            mask |= 1u64 << (nr * 8 + f);
        }
    }
    mask
}

/// Reference bishop attack generation: trace rays until hitting a blocker
fn bishop_attacks_slow(sq: usize, occupied: u64) -> u64 {
    let mut attacks = 0u64;
    let r = (sq / 8) as i8;
    let f = (sq % 8) as i8;

    for &(dr, df) in &[(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
        let mut nr = r + dr;
        let mut nf = f + df;
        while (0..8).contains(&nr) && (0..8).contains(&nf) {
            let bit = 1u64 << (nr * 8 + nf);
            attacks |= bit;
            if occupied & bit != 0 { break; }
            nr += dr;
            nf += df;
        }
    }
    attacks
}

/// Reference rook attack generation: trace rays until hitting a blocker
fn rook_attacks_slow(sq: usize, occupied: u64) -> u64 {
    let mut attacks = 0u64;
    let r = (sq / 8) as i8;
    let f = (sq % 8) as i8;

    for &(dr, df) in &[(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
        let mut nr = r + dr;
        let mut nf = f + df;
        while (0..8).contains(&nr) && (0..8).contains(&nf) {
            let bit = 1u64 << (nr * 8 + nf);
            attacks |= bit;
            if occupied & bit != 0 { break; }
            nr += dr;
            nf += df;
        }
    }
    attacks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::square::{Rank, File};

    fn sq(r: usize, f: usize) -> Square {
        Square::make_square(Rank::from_index(r), File::from_index(f))
    }

    #[test]
    fn test_knight_attacks_corner() {
        init_attacks();
        let attacks = knight_attacks(sq(0, 0)); // A1
        assert_eq!(attacks.popcnt(), 2); // B3, C2
    }

    #[test]
    fn test_knight_attacks_center() {
        init_attacks();
        let attacks = knight_attacks(sq(3, 3)); // D4
        assert_eq!(attacks.popcnt(), 8);
    }

    #[test]
    fn test_king_attacks_corner() {
        init_attacks();
        let attacks = king_attacks(sq(0, 0)); // A1
        assert_eq!(attacks.popcnt(), 3);
    }

    #[test]
    fn test_king_attacks_center() {
        init_attacks();
        let attacks = king_attacks(sq(3, 3)); // D4
        assert_eq!(attacks.popcnt(), 8);
    }

    #[test]
    fn test_pawn_attacks_white() {
        init_attacks();
        let attacks = pawn_attacks(Color::White, sq(1, 4)); // E2
        assert_eq!(attacks.popcnt(), 2); // D3, F3
    }

    #[test]
    fn test_pawn_attacks_edge() {
        init_attacks();
        let attacks = pawn_attacks(Color::White, sq(1, 0)); // A2
        assert_eq!(attacks.popcnt(), 1); // B3 only
    }

    #[test]
    fn test_rook_attacks_empty_board() {
        init_attacks();
        let attacks = rook_attacks(sq(3, 3), BitBoard(0)); // D4 on empty board
        assert_eq!(attacks.popcnt(), 14); // full rank + file minus self
    }

    #[test]
    fn test_bishop_attacks_empty_board() {
        init_attacks();
        let attacks = bishop_attacks(sq(3, 3), BitBoard(0)); // D4 on empty board
        assert_eq!(attacks.popcnt(), 13);
    }

    #[test]
    fn test_queen_equals_bishop_or_rook() {
        init_attacks();
        let occ = BitBoard(0x0000_0010_0800_0000); // some blockers
        let s = sq(4, 4); // E5
        let q = queen_attacks(s, occ);
        let b = bishop_attacks(s, occ);
        let r = rook_attacks(s, occ);
        assert_eq!(q, b | r);
    }

    #[test]
    fn test_rook_attacks_with_blockers() {
        init_attacks();
        // Rook on A1, blocker on A4 and D1
        let blocker = BitBoard::from_square(sq(3, 0)) | BitBoard::from_square(sq(0, 3));
        let attacks = rook_attacks(sq(0, 0), blocker);
        // Should reach A2, A3, A4 (blocked), B1, C1, D1 (blocked) = 6 squares
        assert_eq!(attacks.popcnt(), 6);
    }

    #[test]
    fn test_bishop_attacks_with_blockers() {
        init_attacks();
        // Bishop on D4, blocker on F6
        let blocker = BitBoard::from_square(sq(5, 5)); // F6
        let attacks = bishop_attacks(sq(3, 3), blocker);
        // NE: E5, F6 (blocked) = 2
        // NW: C5, B6, A7 = 3
        // SE: E3, F2, G1 = 3
        // SW: C3, B2, A1 = 3
        assert_eq!(attacks.popcnt(), 11);
    }

    #[test]
    fn test_rook_attacks_all_squares_empty_board() {
        init_attacks();
        // Every square on an empty board should have exactly 14 rook attacks
        for r in 0..8 {
            for f in 0..8 {
                let attacks = rook_attacks(sq(r, f), BitBoard(0));
                assert_eq!(attacks.popcnt(), 14,
                    "Rook on ({},{}) should have 14 attacks on empty board, got {}",
                    r, f, attacks.popcnt());
            }
        }
    }
}

// precompute attack tables for every piece type on every squre, stored in static arrays
// initialized once via `std::sync::Once`

// Excluding edges from the mask... for magic bitboards, only care abt relevant blockers (pieces
// btwn slider and edge). Piece on edge doesn't affect ray (ray ends there regardless), so exclude
// edges from occupancy mask. This reduces # of bits and makes lookup table smaller
