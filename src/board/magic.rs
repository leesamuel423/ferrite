use super::bitboard::BitBoard;

/// magic bitboard entry for one square
pub struct MagicEntry {
    pub mask: BitBoard,
    pub magic: u64,
    pub shift: u8,
    pub offset: u32,
}

/// number of relevant bits for bishop occupancy at each square
pub static BISHOP_BITS: [u8; 64] = [
    6, 5, 5, 5, 5, 5, 5, 6,
    5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 7, 7, 7, 7, 5, 5,
    5, 5, 7, 9, 9, 7, 5, 5,
    5, 5, 7, 9, 9, 7, 5, 5,
    5, 5, 7, 7, 7, 7, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5,
    6, 5, 5, 5, 5, 5, 5, 6,
];

/// number of relevant bits for rook occupancy at each square
pub static ROOK_BITS: [u8; 64] = [
    12, 11, 11, 11, 11, 11, 11, 12,
    11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11,
    12, 11, 11, 11, 11, 11, 11, 12,
];

/// simple xorshift64 PRNG for magic number generation
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// generate a sparse random number (few bits set) ... good magic candidates
    pub fn sparse_random(&mut self) -> u64 {
        self.next() & self.next() & self.next()
    }
}

/// find a magic number for given square, mask, bits, and slow attack generator
/// returns magic number that produces a collision-free hash
pub fn find_magic(
    mask: u64,
    bits: u8,
    slow_attacks: &dyn Fn(u64) -> u64,
    rng: &mut Rng,
) -> u64 {
    let shift = 64 - bits;
    let table_size = 1usize << bits;

    // enumerate all occupancy subsets of the mask using Carry-Rippler trick.
    // for a mask with N bits set, this generates all 2^N subsets.
    let num_subsets = 1usize << mask.count_ones();
    let mut occupancies = Vec::with_capacity(num_subsets);
    let mut attacks = Vec::with_capacity(num_subsets);

    let mut occ = 0u64;
    loop {
        occupancies.push(occ);
        attacks.push(slow_attacks(occ));
        // Carry-Rippler: generates the next subset of `mask`
        occ = occ.wrapping_sub(mask) & mask;
        if occ == 0 {
            break;
        }
    }

    // Try random magic numbers until a collision-free one
    // A "collision" means two different occupancies map to same index
    // but have different attack sets
    let mut table = vec![0u64; table_size];
    loop {
        let magic = rng.sparse_random();
        if magic == 0 {
            continue;
        }

        // quick reject: check that magic spreads bits well enough
        if (mask.wrapping_mul(magic) & 0xFF00_0000_0000_0000).count_ones() < 6 {
            continue;
        }

        // clear table
        for entry in table.iter_mut() {
            *entry = 0;
        }

        let mut ok = true;
        for i in 0..occupancies.len() {
            let idx = (occupancies[i].wrapping_mul(magic) >> shift) as usize;
            if table[idx] == 0 {
                table[idx] = attacks[i];
            } else if table[idx] != attacks[i] {
                ok = false;
                break;
            }
        }

        if ok {
            return magic;
        }
    }
}

// Instead of tracing rays at runtime, which is slow, pre-compute attack bitboards for every piece
// on every square. For leapers (knight, kings, pawns), simple lookup table. For sliders (bishops,
// rooks, queens), use magic bitboard (hashtable indexed by `(occupancy * magic_number) >> shift`)

// For a rook on D4, attack dependson which squares between D4 and board edges are occupied
// (blockers). There are ~12 relevant bits in occupancy mask, giving 2^12 = 4096 possible occupancy
// patterns. Magic number constant, when multiplied by occupancy and right-shifted, maps each
// pattern to unique index in compact table. These magic numbers found w/ trial error w/ PRNG.
// xorshift64 PRNG to generate candidate magic numbers and then test for collisions

// Carry-Rippler trick:`occ = (occ - mask) & mask` enumerates all subsets of `mask`. This is a 
// well-known bit manipulation technique — it "carries" through the bits of the mask, generating 
// each subset exactly once.
