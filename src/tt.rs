use crate::board::ChessMove;

use crate::types::{Score, SCORE_MATE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TTFlag {
    Exact,
    LowerBound, // Beta cutoff (score >= beta)
    UpperBound, // Failed low (score <= alpha)
}

#[derive(Clone, Copy)]
pub struct TTEntry {
    pub key: u64, // Zobrist hash (full, for collision detection)
    pub depth: u8,
    pub score: Score,
    pub flag: TTFlag,
    pub best_move: Option<ChessMove>,
    pub age: u8, // Search generation for aging
}

impl Default for TTEntry {
    fn default() -> Self {
        Self {
            key: 0,
            depth: 0,
            score: 0,
            flag: TTFlag::Exact,
            best_move: None,
            age: 0,
        }
    }
}

pub struct TranspositionTable {
    entries: Vec<TTEntry>,
    mask: usize, // size - 1 (for fast modulo)
    generation: u8, // Current search generation
}

impl TranspositionTable {
    /// Create new TT with given size in megabytes
    pub fn new(mb: usize) -> Self {
        let entry_size = std::mem::size_of::<TTEntry>();
        let num_entries = (mb * 1024 * 1024) / entry_size;
        // Round down to power of 2
        let size = num_entries.next_power_of_two() / 2;
        let size = size.max(1024); // Minimum 1024 entries

        Self {
            entries: vec![TTEntry::default(); size],
            mask: size - 1,
            generation: 0,
        }
    }

    /// Increment generation counter (call at start of each search)
    pub fn new_search(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Probe TT for given hash
    pub fn probe(&self, hash: u64, _ply: usize) -> Option<&TTEntry> {
        let idx = hash as usize & self.mask;
        let entry = &self.entries[idx];

        if entry.key == hash {
            Some(entry)
        } else {
            None
        }
    }

    /// Retrieve score from TT entry, adjusting mate scores for current ply
    pub fn retrieve_score(entry: &TTEntry, ply: usize, alpha: Score, beta: Score) -> Option<Score> {
        let mut score = entry.score;

        // Adjust mate scores from storage format (relative to root) to current ply
        if score > SCORE_MATE - 100 {
            score -= ply as Score;
        } else if score < -SCORE_MATE + 100 {
            score += ply as Score;
        }

        match entry.flag {
            TTFlag::Exact => Some(score),
            TTFlag::LowerBound => {
                if score >= beta { Some(score) } else { None }
            }
            TTFlag::UpperBound => {
                if score <= alpha { Some(score) } else { None }
            }
        }
    }

    /// Store position in TT
    pub fn store(
        &mut self,
        hash: u64,
        depth: u8,
        mut score: Score,
        flag: TTFlag,
        best_move: Option<ChessMove>,
        ply: usize,
    ) {
        let idx = hash as usize & self.mask;
        let entry = &self.entries[idx];

        // Replacement strategy: depth-preferred with aging
        // Replace if: empty, same position, deeper search, or stale entry
        let should_replace = entry.key == 0
            || entry.key == hash
            || depth >= entry.depth
            || entry.age != self.generation;

        if !should_replace {
            return;
        }

        // Adjust mate scores for storage (make relative to root)
        if score > SCORE_MATE - 100 {
            score += ply as Score;
        } else if score < -SCORE_MATE + 100 {
            score -= ply as Score;
        }

        self.entries[idx] = TTEntry {
            key: hash,
            depth,
            score,
            flag,
            best_move,
            age: self.generation,
        };
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = TTEntry::default();
        }
        self.generation = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SCORE_INFINITY;

    #[test]
    fn test_tt_store_and_probe() {
        let mut tt = TranspositionTable::new(1); // 1 MB
        let hash: u64 = 0x123456789ABCDEF0;

        tt.store(hash, 5, 100, TTFlag::Exact, None, 0);

        let entry = tt.probe(hash, 0);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.depth, 5);
        assert_eq!(entry.score, 100);
        assert_eq!(entry.flag, TTFlag::Exact);
    }

    #[test]
    fn test_tt_miss() {
        let tt = TranspositionTable::new(1);
        let entry = tt.probe(0xDEADBEEF, 0);
        // Default entries have key=0, so 0xDEADBEEF shouldn't match
        assert!(entry.is_none());
    }

    #[test]
    fn test_mate_score_adjustment() {
        let mut tt = TranspositionTable::new(1);
        let hash: u64 = 0xABCDEF;

        // Store mate score at ply 3
        let mate_score = SCORE_MATE - 3;
        tt.store(hash, 10, mate_score, TTFlag::Exact, None, 3);

        // stored score should be adjusted: SCORE_MATE - 3 + 3 = SCORE_MATE
        let entry = tt.probe(hash, 0).unwrap();
        assert_eq!(entry.score, SCORE_MATE);

        // Retrieve at ply 5 should give SCORE_MATE - 5
        let retrieved = TranspositionTable::retrieve_score(entry, 5, -SCORE_INFINITY, SCORE_INFINITY);
        assert_eq!(retrieved, Some(SCORE_MATE - 5));
    }

    #[test]
    fn test_tt_replacement() {
        let mut tt = TranspositionTable::new(1);
        let hash: u64 = 0x12345;

        // Store at depth 3
        tt.store(hash, 3, 50, TTFlag::Exact, None, 0);
        // Overwrite with deeper search
        tt.store(hash, 6, 75, TTFlag::Exact, None, 0);

        let entry = tt.probe(hash, 0).unwrap();
        assert_eq!(entry.depth, 6);
        assert_eq!(entry.score, 75);
    }
}

// TT is hashmap indexed by `zobrist_hash % table_size`. Each entry stores position's hash (for
// collision detection), search depth, score, bound type (exact/lower/upper) and best move found.
// Table uses depth-preferred replacement w/ aging -> deeper searches overwrite shallower ones, and
// stale entries from prev searches are replaced.

// Mate score adjustment: Mate scores are stored relative to the root (ie. mate in 5 from root) but
// need to be adjusted to the current ply when probed (ie. mate in 3 from this node). This is done
// by adding/subtracting ply difference
