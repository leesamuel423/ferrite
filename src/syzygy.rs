use std::path::Path;

use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess};
use shakmaty_syzygy::{Tablebase, Wdl, SyzygyError};

use crate::types::Score;

pub struct SyzygyProber {
    tablebase: Tablebase<Chess>,
}

impl SyzygyProber {
    /// Create a new prober by loading tablebases from given directory
    /// Returns None if path doesn't exist or contains no valid tables
    pub fn new(path: &str) -> Option<Self> {
        if !Path::new(path).is_dir() {
            return None;
        }

        let mut tablebase = Tablebase::new();
        if tablebase.add_directory(path).is_err() {
            return None;
        }

        Some(Self { tablebase })
    }

    /// Probe WDL for a position given as a `crate::board::Board`.
    /// Returns a score: positive for win, negative for loss, 0 for draw.
    /// Only valid for positions with 5 or fewer pieces.
    pub fn probe_wdl(&self, board: &crate::board::Board) -> Option<Score> {
        let piece_count = board.combined().popcnt();
        if piece_count > 5 {
            return None;
        }

        // Convert crate::board::Board → FEN string → shakmaty::Chess
        let fen_str = format!("{}", board);
        let fen: Fen = fen_str.parse().ok()?;
        let pos: Chess = fen.into_position(CastlingMode::Standard).ok()?;

        match self.tablebase.probe_wdl_after_zeroing(&pos) {
            Ok(wdl) => Some(wdl_to_score(wdl)),
            Err(SyzygyError::MissingTable { .. }) => None,
            Err(_) => None,
        }
    }
}

fn wdl_to_score(wdl: Wdl) -> Score {
    match wdl {
        Wdl::Win => 20_000,
        Wdl::CursedWin => 100,
        Wdl::Draw => 0,
        Wdl::BlessedLoss => -100,
        Wdl::Loss => -20_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syzygy_prober_invalid_path() {
        let prober = SyzygyProber::new("/nonexistent/path");
        assert!(prober.is_none());
    }

    #[test]
    fn test_syzygy_prober_too_many_pieces() {
        if let Some(prober) = SyzygyProber::new("endgame/syzgy-3-4-5") {
            let board = crate::board::Board::default();
            assert!(prober.probe_wdl(&board).is_none());
        }
    }
}

// Syzygy bridge converts board to a FEN string, parses it with `shakmaty`, then probes tablebase.
// The FEN round-trip has negligible cost since we only probe with <= 5 pieces on board.
