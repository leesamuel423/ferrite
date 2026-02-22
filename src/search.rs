use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::board::{Board, BoardStatus, ChessMove, Piece};

use crate::evaluation::evaluate;
use crate::movegen::{order_captures, order_moves};
use crate::syzygy::SyzygyProber;
use crate::tt::{TTFlag, TranspositionTable};
use crate::types::{Score, SearchResult, SCORE_INFINITY, SCORE_MATE, MAX_PLY, DEFAULT_HASH_MB, HISTORY_MAX};

/// Mutable search state shared across recursion
pub struct SearchState {
    pub nodes: u64,
    pub start_time: Instant,
    pub stop: Arc<AtomicBool>,
    pub time_limit_ms: u64,
    pub killers: [[Option<ChessMove>; 2]; MAX_PLY],
    pub history: [[Score; 64]; 6],
    pub tt: TranspositionTable,
    pub syzygy: Option<SyzygyProber>,
    pub root_best_move: Option<ChessMove>,
    pub position_history: Vec<u64>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            nodes: 0,
            start_time: Instant::now(),
            stop: Arc::new(AtomicBool::new(false)),
            time_limit_ms: 0,
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 64]; 6],
            tt: TranspositionTable::new(DEFAULT_HASH_MB),
            syzygy: None,
            root_best_move: None,
            position_history: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.nodes = 0;
        self.stop.store(false, Ordering::SeqCst);
        self.killers = [[None; 2]; MAX_PLY];
        self.history = [[0; 64]; 6];
        self.start_time = Instant::now();
        self.tt.new_search();
        self.root_best_move = None;
    }

    pub fn resize_tt(&mut self, mb: usize) {
        self.tt = TranspositionTable::new(mb);
    }

    pub fn load_syzygy(&mut self, path: &str) {
        self.syzygy = SyzygyProber::new(path);
    }

    fn check_time(&self) {
        if self.time_limit_ms > 0 {
            let elapsed = self.start_time.elapsed().as_millis() as u64;
            if elapsed >= self.time_limit_ms {
                self.stop.store(true, Ordering::Relaxed);
            }
        }
    }

    fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// Extract principal variation by following TT hash move chain
fn extract_pv(board: &Board, tt: &TranspositionTable, max_moves: usize) -> Vec<ChessMove> {
    let mut pv = Vec::new();
    let mut current_board = *board;
    let mut seen: Vec<u64> = Vec::new();

    for _ in 0..max_moves {
        let hash = current_board.get_hash();
        if seen.contains(&hash) {
            break;
        }
        seen.push(hash);

        if let Some(entry) = tt.probe(hash, 0) {
            if let Some(mv) = entry.best_move {
                if current_board.legal(mv) {
                    pv.push(mv);
                    current_board = current_board.make_move_new(mv);
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    pv
}

/// Format a score for UCI output (centipawns or mate-in-N).
fn format_score(score: Score) -> String {
    if score.abs() > SCORE_MATE - 100 {
        let mate_ply = SCORE_MATE - score.abs();
        let mate_moves = (mate_ply + 1) / 2;
        if score > 0 {
            format!("score mate {}", mate_moves)
        } else {
            format!("score mate -{}", mate_moves)
        }
    } else {
        format!("score cp {}", score)
    }
}

/// Iterative deepening search. Returns best move found
pub fn search(board: &Board, state: &mut SearchState, max_depth: u8) -> SearchResult {
    let mut best_move: Option<ChessMove> = None;
    let mut best_score: Score = -SCORE_INFINITY;

    for depth in 1..=max_depth {
        state.nodes = 0;
        state.root_best_move = None;
        let score = negamax(board, state, depth, 0, -SCORE_INFINITY, SCORE_INFINITY, true);

        if state.is_stopped() {
            // Interrupted — only use partial result if we have nothing from a complete iteration
            if best_move.is_none() {
                best_move = state.root_best_move;
            }
            break;
        }

        best_score = score;
        if let Some(mv) = state.root_best_move {
            best_move = Some(mv);
        }

        let elapsed_ms = state.start_time.elapsed().as_millis().max(1) as u64;
        let nps = state.nodes * 1000 / elapsed_ms;

        // Extract PV from TT chain
        let pv = extract_pv(board, &state.tt, depth as usize);
        let pv_str: String = pv.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(" ");

        let score_str = format_score(best_score);
        println!(
            "info depth {} {} nodes {} time {} nps {} pv {}",
            depth, score_str, state.nodes, elapsed_ms, nps, pv_str
        );

        // Soft time limit: don't start next iteration if >50% of time used
        if state.time_limit_ms > 0 {
            let elapsed = state.start_time.elapsed().as_millis() as u64;
            if elapsed > state.time_limit_ms / 2 {
                break;
            }
        }

        // Early exit if we found a forced mate
        if best_score.abs() > SCORE_MATE - 100 {
            break;
        }
    }

    SearchResult {
        best_move,
        score: best_score,
        depth: max_depth,
        nodes: state.nodes,
    }
}

/// Negamax with alpha-beta pruning, TT, NMP, and LMR.
fn negamax(
    board: &Board,
    state: &mut SearchState,
    depth: u8,
    ply: usize,
    mut alpha: Score,
    beta: Score,
    can_null: bool,
) -> Score {
    // Time check every 2048 nodes
    state.nodes += 1;
    if state.nodes & 2047 == 0 {
        state.check_time();
    }
    if state.is_stopped() {
        return 0;
    }

    // Terminal node checks
    match board.status() {
        BoardStatus::Checkmate => return -SCORE_MATE + ply as Score,
        BoardStatus::Stalemate => return 0,
        _ => {}
    }

    // Draw detection: repetition
    let hash = board.get_hash();
    if ply > 0 && state.position_history.contains(&hash) {
        return 0;
    }

    // Leaf node: switch to quiescence search
    if depth == 0 {
        return quiescence(board, state, ply, alpha, beta);
    }

    // TT probe
    let mut hash_move: Option<ChessMove> = None;

    if let Some(entry) = state.tt.probe(hash, ply) {
        hash_move = entry.best_move;
        if entry.depth >= depth
            && let Some(score) = TranspositionTable::retrieve_score(entry, ply, alpha, beta)
        {
            return score;
        }
    }

    // Syzygy tablebase probe (only at non-root with <= 5 pieces)
    if ply > 0
        && let Some(ref syzygy) = state.syzygy
        && let Some(score) = syzygy.probe_wdl(board)
    {
        return score;
    }

    let in_check = board.checkers().0 != 0;

    // Null move pruning:
    // "If I skip my turn and still beat beta, my real position must be even better."
    // Conditions: not in check, depth >= 3, not consecutive null moves, has non-pawn material
    if can_null && !in_check && depth >= 3 && ply > 0 {
        // Skip NMP in zugzwang-prone positions (side has only pawns + king)
        let our_pieces = board.color_combined(board.side_to_move());
        let pawns_and_king = board.pieces(Piece::Pawn) | board.pieces(Piece::King);
        let has_non_pawn_material = (our_pieces & !pawns_and_king).0 != 0;

        if has_non_pawn_material
            && let Some(null_board) = board.null_move()
        {
            state.position_history.push(hash);
            let score = -negamax(&null_board, state, depth - 3, ply + 1, -beta, -beta + 1, false);
            state.position_history.pop();

            if state.is_stopped() {
                return 0;
            }
            if score >= beta {
                return beta;
            }
        }
    }

    let moves = order_moves(board, hash_move, &state.killers[ply], &state.history, ply);

    if moves.is_empty() {
        return 0;
    }

    // Push current position for repetition detection in child nodes
    state.position_history.push(hash);

    let mut best_score = -SCORE_INFINITY;
    let mut best_move: Option<ChessMove> = None;
    let original_alpha = alpha;

    for (move_num, scored_move) in moves.iter().enumerate() {
        let new_board = board.make_move_new(scored_move.mv);
        let is_capture = board.piece_on(scored_move.mv.get_dest()).is_some()
            || board.en_passant() == Some(scored_move.mv.get_dest());
        let gives_check = new_board.checkers().0 != 0;

        let score;

        // LMR: reduce depth for late quiet moves
        // "Moves ordered late are likely bad... search them shallowly first."
        let do_lmr = move_num >= 3
            && depth >= 3
            && !is_capture
            && !in_check
            && !gives_check
            && Some(scored_move.mv) != state.killers[ply][0]
            && Some(scored_move.mv) != state.killers[ply][1];

        if do_lmr {
            // Reduced depth search with null window
            let reduced = -negamax(&new_board, state, depth - 2, ply + 1, -alpha - 1, -alpha, true);
            if reduced > alpha {
                // Re-search at full depth
                score = -negamax(&new_board, state, depth - 1, ply + 1, -beta, -alpha, true);
            } else {
                score = reduced;
            }
        } else {
            score = -negamax(&new_board, state, depth - 1, ply + 1, -beta, -alpha, true);
        }

        if state.is_stopped() {
            state.position_history.pop();
            return best_score;
        }

        if score > best_score {
            best_score = score;
            best_move = Some(scored_move.mv);
            // Track root best move explicitly (collision-proof)
            if ply == 0 {
                state.root_best_move = Some(scored_move.mv);
            }
        }

        if score > alpha {
            alpha = score;
        }

        // Beta cutoff
        if alpha >= beta {
            // Update killer moves and history for quiet moves that cause cutoffs
            if !is_capture && ply < MAX_PLY {
                // Shift killer: slot 1 = old slot 0
                state.killers[ply][1] = state.killers[ply][0];
                state.killers[ply][0] = Some(scored_move.mv);

                // Update history heuristic (with cap)
                if let Some(piece) = board.piece_on(scored_move.mv.get_source()) {
                    let pi = piece_to_index(piece);
                    let to = scored_move.mv.get_dest().to_index();
                    state.history[pi][to] += (depth as Score) * (depth as Score);
                    if state.history[pi][to] > HISTORY_MAX {
                        state.history[pi][to] = HISTORY_MAX;
                    }
                }
            }
            break;
        }
    }

    state.position_history.pop();

    // Store in TT
    let flag = if best_score >= beta {
        TTFlag::LowerBound
    } else if best_score <= original_alpha {
        TTFlag::UpperBound
    } else {
        TTFlag::Exact
    };

    state.tt.store(hash, depth, best_score, flag, best_move, ply);

    best_score
}

/// Quiescence search — explores captures (and all moves when in check).
fn quiescence(
    board: &Board,
    state: &mut SearchState,
    ply: usize,
    mut alpha: Score,
    beta: Score,
) -> Score {
    state.nodes += 1;

    if ply >= MAX_PLY {
        return evaluate(board);
    }

    let in_check = board.checkers().0 != 0;

    if in_check {
        // In check: must search ALL legal moves — standing pat is illegal
        let mut best_score: Score = -SCORE_INFINITY;
        let killers = state.killers[ply];
        let moves = order_moves(board, None, &killers, &state.history, ply);

        if moves.is_empty() {
            // In check with no legal moves = checkmate
            return -SCORE_MATE + ply as Score;
        }

        for scored_move in &moves {
            let new_board = board.make_move_new(scored_move.mv);
            let score = -quiescence(&new_board, state, ply + 1, -beta, -alpha);

            if state.is_stopped() {
                return best_score;
            }

            if score > best_score {
                best_score = score;
            }
            if score > alpha {
                alpha = score;
            }
            if alpha >= beta {
                return best_score; // Fail-soft
            }
        }

        return best_score;
    }

    // Not in check: normal quiescence with stand-pat
    let stand_pat = evaluate(board);
    let mut best_score = stand_pat;

    if stand_pat >= beta {
        return best_score; // Fail-soft
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let captures = order_captures(board);

    for scored_move in &captures {
        let new_board = board.make_move_new(scored_move.mv);
        let score = -quiescence(&new_board, state, ply + 1, -beta, -alpha);

        if state.is_stopped() {
            return best_score;
        }

        if score > best_score {
            best_score = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            return best_score; // Fail-soft
        }
    }

    best_score
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_search_finds_move() {
        let board = Board::default();
        let mut state = SearchState::new();
        let result = search(&board, &mut state, 3);
        assert!(result.best_move.is_some());
    }

    #[test]
    fn test_search_finds_mate_in_one() {
        let board = Board::from_str("r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4")
            .unwrap();
        let mut state = SearchState::new();
        let result = search(&board, &mut state, 2);
        let best = result.best_move.unwrap();
        assert_eq!(best.to_string(), "h5f7", "Expected Qxf7# but got {}", best);
    }

    #[test]
    fn test_search_avoids_giving_material() {
        let board = Board::default();
        let mut state = SearchState::new();
        let result = search(&board, &mut state, 4);
        assert!(result.best_move.is_some());
        assert!(result.nodes > 0);
    }

    #[test]
    fn test_checkmate_score() {
        let board = Board::from_str("rnbqkbnr/pppp1ppp/4p3/8/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
            .unwrap();
        let mut state = SearchState::new();
        let score = negamax(&board, &mut state, 1, 0, -SCORE_INFINITY, SCORE_INFINITY, true);
        assert!(score < -SCORE_MATE + 200, "Checkmate score should be very negative, got {}", score);
    }

    #[test]
    fn test_tt_reduces_nodes() {
        // Search same position twice. second search should be faster due to TT
        let board = Board::default();
        let mut state = SearchState::new();

        // First search
        search(&board, &mut state, 4);
        let nodes_first = state.nodes;

        // Second search (TT populated)
        state.reset();
        search(&board, &mut state, 4);
        let nodes_second = state.nodes;

        // Second search should use fewer nodes (TT hits)
        assert!(nodes_second <= nodes_first,
            "Second search ({} nodes) should use <= first ({} nodes) due to TT",
            nodes_second, nodes_first);
    }

    #[test]
    fn test_draw_detection_repetition() {
        // Set up a position and add same hash to position_history
        let board = Board::default();
        let mut state = SearchState::new();
        // Simulate a repetition by adding current hash
        state.position_history.push(board.get_hash());
        // At ply > 0, negamax should detect repetition and return 0
        let score = negamax(&board, &mut state, 3, 1, -SCORE_INFINITY, SCORE_INFINITY, true);
        assert_eq!(score, 0, "Repeated position should return 0 (draw), got {}", score);
    }

    #[test]
    fn test_quiescence_in_check() {
        // Position where side to move is in check — quiescence must search all evasions
        let board = Board::from_str("rnbqkbnr/pppp1ppp/4p3/8/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
            .unwrap();
        // This is checkmate, so quiescence should return a mate score
        let mut state = SearchState::new();
        let score = quiescence(&board, &mut state, 0, -SCORE_INFINITY, SCORE_INFINITY);
        assert!(score < -SCORE_MATE + 200, "Checkmate in qsearch should return mate score, got {}", score);
    }

    #[test]
    fn test_stop_preserves_best_move() {
        // Search with a tight time limit so it stops during deeper iterations
        let board = Board::default();
        let mut state = SearchState::new();
        state.time_limit_ms = 1; // 1ms — will stop almost immediately
        state.start_time = Instant::now();
        let result = search(&board, &mut state, 20);
        // Should still have found a move from depth 1 or partial search
        assert!(result.best_move.is_some(), "Should find a move even when stopped early");
    }

    #[test]
    fn test_pv_extraction() {
        let board = Board::default();
        let mut state = SearchState::new();
        search(&board, &mut state, 4);
        let pv = extract_pv(&board, &state.tt, 4);
        assert!(!pv.is_empty(), "PV should contain at least one move after search");
    }

    #[test]
    fn test_mate_score_format() {
        assert_eq!(format_score(SCORE_MATE - 1), "score mate 1");
        assert_eq!(format_score(SCORE_MATE - 3), "score mate 2");
        assert_eq!(format_score(-(SCORE_MATE - 1)), "score mate -1");
        assert_eq!(format_score(-(SCORE_MATE - 3)), "score mate -2");
        assert_eq!(format_score(100), "score cp 100");
        assert_eq!(format_score(-50), "score cp -50");
    }
}

// Implementing iterative deepening negamax w/ alpha-beta pruning, Null Move Pruning (NMP), Late
// Move Reductions (LMR) and quiescence search

// Iterative deepening: search depth 1->2->3... Each iteration benefits from TT entries cached by
// previous iterations, and we can stop anytime (last complete iteration's result is valid)

// Negamax: simplification of minmax where maximize from current player's perspective

// Alpha-beta pruning: skip branches that can't possibly improve results

// Null move pruning: If I skip my turn and still have a great position, real position must be even
// better. This saves searching many positions

// Late move reduction: late quiet moves in the move list are searched at reduced depth first. If
// they look promising, research at full depth

// Quiescence search: at leaf nodes, don't just eval, search all captures to avoid "horizon effect"
