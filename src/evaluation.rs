use crate::board::{Board, Color, Piece, ALL_SQUARES};

use crate::pst::{self, EG_TABLE, MG_TABLE, MG_PIECE_VALUE, EG_PIECE_VALUE, PHASE_WEIGHT, TOTAL_PHASE};
use crate::types::Score;

/// Maps a Piece to our PST index (0-5)
fn piece_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => pst::PAWN,
        Piece::Knight => pst::KNIGHT,
        Piece::Bishop => pst::BISHOP,
        Piece::Rook => pst::ROOK,
        Piece::Queen => pst::QUEEN,
        Piece::King => pst::KING,
    }
}

/// Converts a Square (A1=0, H8=63) to our PST index
/// PeSTO tables are stored with a8=0, h1=63 (rank 8 first, rank 1 last)
/// For White: need to flip rank → index = sq ^ 56 (maps rank 1→8, 2→7, etc.)
/// For Black: use square index directly (already mirrors White's perspective)
fn pst_index_white(sq: crate::board::Square) -> usize {
    sq.to_index() ^ 56
}

fn pst_index_black(sq: crate::board::Square) -> usize {
    sq.to_index()
}

/// Evaluates board position using PeSTO tapered evaluation
/// Returns score from perspective of side to move
pub fn evaluate(board: &Board) -> Score {
    let mut mg_score: [Score; 2] = [0, 0]; // [white, black]
    let mut eg_score: [Score; 2] = [0, 0];
    let mut phase: i32 = 0;

    for sq in ALL_SQUARES {
        if let Some(piece) = board.piece_on(sq) {
            let color = board.color_on(sq).unwrap();
            let idx = piece_index(piece);
            let side = color.to_index(); // White=0, Black=1

            // Material value
            mg_score[side] += MG_PIECE_VALUE[idx];
            eg_score[side] += EG_PIECE_VALUE[idx];

            // Positional value from PST
            let pst_idx = if color == Color::White {
                pst_index_white(sq)
            } else {
                pst_index_black(sq)
            };

            mg_score[side] += MG_TABLE[idx][pst_idx];
            eg_score[side] += EG_TABLE[idx][pst_idx];

            // Accumulate game phase
            phase += PHASE_WEIGHT[idx];
        }
    }

    // Clamp phase to TOTAL_PHASE (shouldn't exceed, but be safe)
    if phase > TOTAL_PHASE {
        phase = TOTAL_PHASE;
    }

    // Compute scores relative to white
    let mg = mg_score[0] - mg_score[1];
    let eg = eg_score[0] - eg_score[1];

    // Tapered eval: blend mg and eg based on phase
    // phase = TOTAL_PHASE means full midgame, phase = 0 means full endgame
    let mg_phase = phase;
    let eg_phase = TOTAL_PHASE - phase;
    let score = (mg * mg_phase + eg * eg_phase) / TOTAL_PHASE;

    // Return from side-to-move perspective
    if board.side_to_move() == Color::White {
        score
    } else {
        -score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_startpos_near_zero() {
        let board = Board::default();
        let score = evaluate(&board);
        // Starting position should be close to 0 (slight white advantage from tempo)
        assert!(score.abs() < 100, "Startpos score {} is too far from 0", score);
    }

    #[test]
    fn test_white_up_queen() {
        // White has an extra queen
        let board = Board::from_str("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .unwrap();
        let score = evaluate(&board);
        assert!(score > 800, "White up a queen should score high, got {}", score);
    }

    #[test]
    fn test_black_up_queen() {
        // Black has an extra queen (White missing queen)
        let board = Board::from_str("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR b KQkq - 0 1")
            .unwrap();
        let score = evaluate(&board);
        // Score is from side-to-move (black), so black advantage = positive
        assert!(score > 800, "Black up a queen (black to move) should be positive, got {}", score);
    }

    #[test]
    fn test_symmetric_position() {
        // Perfectly symmetric position, white to move
        let board = Board::default();
        let score = evaluate(&board);
        // starting position IS symmetric in material, but PSTs give White a small edge
        assert!(score.abs() < 50, "Symmetric position should be near 0, got {}", score);
    }

    #[test]
    fn test_endgame_phase() {
        // King + pawn endgame: should heavily weight endgame tables
        let board = Board::from_str("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let score = evaluate(&board);
        // White has a pawn advantage, score should be positive
        assert!(score > 0, "White with extra pawn should be positive, got {}", score);
    }
}
// tapered evals -> compute separate midgame and endgame scores, blend them based on how much
// material is left ("game phase")

// With all pieces on board, `phase=24` (full midgame)
// Phase decreases toward 0 as pieces traded. Score blends: `(mg_score * phase + eg_score *
// (24-phase)) / 24` -> king safe in midgame, but active in endgame

