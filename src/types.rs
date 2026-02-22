use crate::board::ChessMove;

pub type Score = i32;

pub const SCORE_INFINITY: Score = 30_000;
pub const SCORE_MATE: Score = 29_000;
pub const MAX_PLY: usize = 128;
pub const DEFAULT_DEPTH: u8 = 5;
pub const DEFAULT_HASH_MB: usize = 64;
pub const HISTORY_MAX: Score = 16384;

pub struct EngineConfig {
    pub hash_mb: usize,
    pub syzygy_path: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            hash_mb: DEFAULT_HASH_MB,
            syzygy_path: None,
        }
    }
}

pub struct SearchResult {
    pub best_move: Option<ChessMove>,
    pub score: Score,
    pub depth: u8,
    pub nodes: u64,
}

