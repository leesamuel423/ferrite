use std::sync::LazyLock;

struct ZobristKeys {
    /// Zobrist keys: [piece_type][color][square]
    piece: [[[u64; 64]; 2]; 6],
    /// key XORed when it's black's turn
    side: u64,
    /// keys for each castling rights combination (4 bits → 16 values)
    castling: [u64; 16],
    /// keys for en passant file (0-7). Only active when EP is possible
    ep: [u64; 8],
}

static KEYS: LazyLock<ZobristKeys> = LazyLock::new(|| {
    let mut rng = XorShift64(0x3243F6A8885A308D); // fixed seed

    let mut piece = [[[0u64; 64]; 2]; 6];
    for piece_keys in &mut piece {
        for color_keys in piece_keys {
            for key in color_keys {
                *key = rng.next();
            }
        }
    }

    let side = rng.next();

    let mut castling = [0u64; 16];
    for key in &mut castling {
        *key = rng.next();
    }

    let mut ep = [0u64; 8];
    for key in &mut ep {
        *key = rng.next();
    }

    ZobristKeys { piece, side, castling, ep }
});

/// Force-init all Zobrist keys. Can be called at startup, but keys are also
/// lazily initialized on first access.
pub fn init_zobrist() {
    LazyLock::force(&KEYS);
}

#[inline]
pub fn piece_key(piece: usize, color: usize, sq: usize) -> u64 {
    KEYS.piece[piece][color][sq]
}

#[inline]
pub fn side_key() -> u64 {
    KEYS.side
}

#[inline]
pub fn castling_key(rights: u8) -> u64 {
    KEYS.castling[rights as usize & 0xF]
}

#[inline]
pub fn ep_key(file: usize) -> u64 {
    KEYS.ep[file]
}

/// simple xorshift64 PRNG
struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keys_nonzero() {
        init_zobrist();
        // spot check that keys are non-zero
        assert_ne!(piece_key(0, 0, 0), 0);
        assert_ne!(side_key(), 0);
        assert_ne!(castling_key(0b1111), 0);
        assert_ne!(ep_key(0), 0);
    }

    #[test]
    fn test_keys_unique() {
        init_zobrist();
        // check that different piece/square combos produce different keys
        let k1 = piece_key(0, 0, 0); // White pawn on A1
        let k2 = piece_key(0, 0, 1); // White pawn on B1
        let k3 = piece_key(1, 0, 0); // White knight on A1
        let k4 = piece_key(0, 1, 0); // Black pawn on A1
        assert_ne!(k1, k2);
        assert_ne!(k1, k3);
        assert_ne!(k1, k4);
    }

    #[test]
    fn test_xor_cancellation() {
        init_zobrist();
        // XORing a key twice should cancel out
        let mut hash = 0u64;
        let key = piece_key(3, 0, 28);
        hash ^= key;
        hash ^= key;
        assert_eq!(hash, 0);
    }
}

// zobrist hashing goal is to generate deterministic random keys for position hashing. Eachposition
// gets a near-unique 64-bit hash used for the transposition table and repetition detection.
// 1. pregenerate random 64 bit num for every piece, color, square triple, plus side-to-move,
//    castling rights, and en passant file
// 2. Position's hash = XOR of all applicable keys
// 3. When making move, incrementally update hash: XOR out removed pieces, XOR in placed pieces,
//    XOR the side key
// 4. XOR is its own inverse: `hash ^= key; hash ^= key;` restores original hash
// This all gives O(1) hash updates per move instead of O(# of pieces)

// Fixed seed b/c deterministic init means every run of engine produces same Zobrist keys. This
// makes debugging reproducible and ensures consistent transposition table
