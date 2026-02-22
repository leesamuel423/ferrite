use arrayvec::ArrayVec;
use crate::board::{BitBoard, Board, ChessMove, MoveGen, Piece, EMPTY};

use crate::pst::MVV_VALUE;
use crate::types::Score;

pub struct ScoredMove {
    pub mv: ChessMove,
    pub score: Score,
}

/// Generates and orders moves for position
/// Priority: (1) Hash move, (2) Captures by MVV-LVA, (3) Killer moves,
/// (4) History heuristic, (5) Remaining quiet moves.
pub fn order_moves(
    board: &Board,
    hash_move: Option<ChessMove>,
    killers: &[Option<ChessMove>; 2],
    history: &[[Score; 64]; 6],
    _ply: usize,
) -> ArrayVec<ScoredMove, 256> {
    let mut scored: ArrayVec<ScoredMove, 256> = ArrayVec::new();

    let mut movegen = MoveGen::new_legal(board);

    // First pass: captures (pieces on enemy squares)
    let targets = board.color_combined(!board.side_to_move());
    movegen.set_iterator_mask(targets);

    for mv in &mut movegen {
        let score = if Some(mv) == hash_move {
            100_000
        } else {
            let mut s: Score = 10_000; // Base capture bonus (above all quiet moves)
            if let Some(victim) = board.piece_on(mv.get_dest()) {
                let victim_idx = piece_to_index(victim);
                let attacker = board.piece_on(mv.get_source()).unwrap();
                let attacker_idx = piece_to_index(attacker);
                // MVV-LVA: high victim value ...  low attacker index = good capture
                s += MVV_VALUE[victim_idx] * 10 - attacker_idx as Score;
            }
            if mv.get_promotion().is_some() {
                s += 9000;
            }
            s
        };
        scored.push(ScoredMove { mv, score });
    }

    // Second pass: en passant captures (destination square is empty, so missed above)
    if let Some(ep_sq) = board.en_passant() {
        movegen.set_iterator_mask(BitBoard::from_square(ep_sq));
        for mv in &mut movegen {
            let score = if Some(mv) == hash_move {
                100_000
            } else {
                // Pawn captures pawn via en passant
                10_000 + MVV_VALUE[0] * 10
            };
            scored.push(ScoredMove { mv, score });
        }
    }

    // Third pass: quiet moves
    movegen.set_iterator_mask(!EMPTY);
    for mv in &mut movegen {
        let score = if Some(mv) == hash_move {
            100_000
        } else if mv.get_promotion().is_some() {
            9000
        } else if Some(mv) == killers[0] {
            // First killer move: just below captures
            8000
        } else if Some(mv) == killers[1] {
            // Second killer move
            7000
        } else {
            // History heuristic score
            if let Some(piece) = board.piece_on(mv.get_source()) {
                let pi = piece_to_index(piece);
                let to = mv.get_dest().to_index();
                history[pi][to]
            } else {
                0
            }
        };
        scored.push(ScoredMove { mv, score });
    }

    scored.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    scored
}

/// Generates only capture moves for quiescence search, ordered by MVV-LVA.
pub fn order_captures(board: &Board) -> ArrayVec<ScoredMove, 256> {
    let mut scored: ArrayVec<ScoredMove, 256> = ArrayVec::new();

    let mut movegen = MoveGen::new_legal(board);
    let targets = board.color_combined(!board.side_to_move());
    movegen.set_iterator_mask(targets);

    for mv in &mut movegen {
        let mut score: Score = 0;
        if let Some(victim) = board.piece_on(mv.get_dest()) {
            let victim_idx = piece_to_index(victim);
            let attacker = board.piece_on(mv.get_source()).unwrap();
            let attacker_idx = piece_to_index(attacker);
            score = MVV_VALUE[victim_idx] * 10 - attacker_idx as Score;
        }
        if mv.get_promotion().is_some() {
            score += 9000;
        }
        scored.push(ScoredMove { mv, score });
    }

    // En passant captures (destination square is empty, so missed above)
    if let Some(ep_sq) = board.en_passant() {
        movegen.set_iterator_mask(BitBoard::from_square(ep_sq));
        for mv in &mut movegen {
            // Pawn captures pawn via en passant
            let score = MVV_VALUE[0] * 10;
            scored.push(ScoredMove { mv, score });
        }
    }

    scored.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    scored
}

fn piece_to_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
}

// Priority order:
// 1. Hash move (from TT) — the move that was best last time we searched this position (100,000)
// 2. Captures by MVV-LVA — Most Valuable Victim, Least Valuable Attacker (10,000+)
// 3. Promotions (+9,000 bonus)
// 4. Killer moves — quiet moves that caused beta cutoffs at the same ply (8,000/7,000)
// 5. History heuristic — quiet moves that frequently cause cutoffs (0-16,384)
// 6. Remaining quiet moves (0)
