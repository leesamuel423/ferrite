
use std::io::{self, BufRead};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crate::board::{Board, ChessMove, Color, File, Piece, Rank, Square};

use crate::search::{self, SearchState};
use crate::types::{EngineConfig, DEFAULT_DEPTH};

pub fn run() {
    let stdin = io::stdin();

    let mut board = Board::default();
    let mut config = EngineConfig::default();
    let mut search_state: Option<SearchState> = Some(SearchState::new());
    let mut stop_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let mut search_thread: Option<thread::JoinHandle<SearchState>> = None;
    let mut position_history: Vec<u64> = Vec::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0] {
            "uci" => {
                println!("id name chess-engine");
                println!("id author yourname");
                println!("option name Hash type spin default 64 min 1 max 4096");
                println!("option name SyzygyPath type string default <empty>");
                println!("uciok");
            }
            "isready" => {
                wait_for_search(&mut search_thread, &mut search_state);
                println!("readyok");
            }
            "ucinewgame" => {
                wait_for_search(&mut search_thread, &mut search_state);
                board = Board::default();
                position_history.clear();
                if let Some(ref mut ss) = search_state {
                    ss.tt.clear();
                }
            }
            "position" => {
                wait_for_search(&mut search_thread, &mut search_state);
                parse_position(&tokens, &mut board, &mut position_history);
            }
            "go" => {
                wait_for_search(&mut search_thread, &mut search_state);

                let go_params = parse_go(&tokens);
                let max_depth = go_params.depth.unwrap_or(DEFAULT_DEPTH);

                let mut ss = search_state.take().expect("search state missing");
                ss.reset();
                ss.time_limit_ms = go_params.compute_time_ms(board.side_to_move());
                ss.position_history = position_history.clone();

                // Set up shared stop flag
                let flag = Arc::new(AtomicBool::new(false));
                stop_flag = flag.clone();
                ss.stop = flag;

                let board_copy = board;

                search_thread = Some(thread::spawn(move || {
                    let result = search::search(&board_copy, &mut ss, max_depth);

                    let elapsed_ms = ss.start_time.elapsed().as_millis().max(1) as u64;
                    let nps = result.nodes * 1000 / elapsed_ms;
                    let score_str = search::format_score(result.score);
                    println!(
                        "info depth {} {} nodes {} time {} nps {}",
                        result.depth, score_str, result.nodes, elapsed_ms, nps
                    );

                    if let Some(m) = result.best_move {
                        println!("bestmove {}", m);
                    } else {
                        println!("bestmove 0000");
                    }

                    ss
                }));
            }
            "stop" => {
                stop_flag.store(true, Ordering::SeqCst);
                wait_for_search(&mut search_thread, &mut search_state);
            }
            "setoption" => {
                wait_for_search(&mut search_thread, &mut search_state);
                if let Some(ref mut ss) = search_state {
                    parse_setoption(&tokens, &mut config, ss);
                }
            }
            "quit" => {
                stop_flag.store(true, Ordering::SeqCst);
                wait_for_search(&mut search_thread, &mut search_state);
                break;
            }
            "d" | "print" => {
                println!("{}", board);
            }
            _ => {}
        }
    }
}

/// Wait for a running search thread to finish and recover the SearchState.
fn wait_for_search(
    handle: &mut Option<thread::JoinHandle<SearchState>>,
    state: &mut Option<SearchState>,
) {
    if let Some(h) = handle.take() {
        match h.join() {
            Ok(ss) => *state = Some(ss),
            Err(_) => {
                // Search thread panicked — create fresh state
                *state = Some(SearchState::new());
            }
        }
    }
}

/// Parsed `go` command parameters.
struct GoParams {
    depth: Option<u8>,
    movetime: Option<u64>,
    wtime: Option<u64>,
    btime: Option<u64>,
    winc: Option<u64>,
    binc: Option<u64>,
    moves_to_go: Option<u64>,
    infinite: bool,
}

impl GoParams {
    fn new() -> Self {
        Self {
            depth: None,
            movetime: None,
            wtime: None,
            btime: None,
            winc: None,
            binc: None,
            moves_to_go: None,
            infinite: false,
        }
    }

    /// Compute the time limit for this search in milliseconds.
    fn compute_time_ms(&self, side: Color) -> u64 {
        if self.infinite {
            return 0;
        }
        if let Some(mt) = self.movetime {
            return mt;
        }

        let (my_time, my_inc) = if side == Color::White {
            (self.wtime.unwrap_or(0), self.winc.unwrap_or(0))
        } else {
            (self.btime.unwrap_or(0), self.binc.unwrap_or(0))
        };

        if my_time == 0 {
            return 0; // No time control = infinite (depth-limited)
        }

        let moves_left = self.moves_to_go.unwrap_or(30);
        let base = my_time / moves_left.max(1);
        let inc_bonus = my_inc * 3 / 4;
        let allocated = base + inc_bonus;

        // Don't use more than 80% of remaining time
        allocated.min(my_time * 4 / 5)
    }
}

fn parse_go(tokens: &[&str]) -> GoParams {
    let mut params = GoParams::new();
    let mut i = 1;

    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                i += 1;
                if i < tokens.len() {
                    params.depth = tokens[i].parse().ok();
                }
            }
            "movetime" => {
                i += 1;
                if i < tokens.len() {
                    params.movetime = tokens[i].parse().ok();
                }
            }
            "wtime" => {
                i += 1;
                if i < tokens.len() {
                    params.wtime = tokens[i].parse().ok();
                }
            }
            "btime" => {
                i += 1;
                if i < tokens.len() {
                    params.btime = tokens[i].parse().ok();
                }
            }
            "winc" => {
                i += 1;
                if i < tokens.len() {
                    params.winc = tokens[i].parse().ok();
                }
            }
            "binc" => {
                i += 1;
                if i < tokens.len() {
                    params.binc = tokens[i].parse().ok();
                }
            }
            "movestogo" => {
                i += 1;
                if i < tokens.len() {
                    params.moves_to_go = tokens[i].parse().ok();
                }
            }
            "infinite" => {
                params.infinite = true;
            }
            _ => {}
        }
        i += 1;
    }

    params
}

fn parse_position(tokens: &[&str], board: &mut Board, history: &mut Vec<u64>) {
    if tokens.len() < 2 {
        return;
    }

    let mut idx = 1;

    if tokens[idx] == "startpos" {
        *board = Board::default();
        idx += 1;
    } else if tokens[idx] == "fen" {
        idx += 1;
        // Collect FEN fields up to "moves" keyword or end of tokens
        let mut fen_parts: Vec<&str> = Vec::new();
        while idx < tokens.len() && tokens[idx] != "moves" && fen_parts.len() < 6 {
            fen_parts.push(tokens[idx]);
            idx += 1;
        }
        if fen_parts.len() >= 4 {
            let fen_str = fen_parts.join(" ");
            match Board::from_str(&fen_str) {
                Ok(b) => *board = b,
                Err(_) => return,
            }
        } else {
            return;
        }
    } else {
        return;
    }

    // Build position history for draw detection
    history.clear();
    history.push(board.get_hash());

    // Parse moves
    if idx < tokens.len() && tokens[idx] == "moves" {
        idx += 1;
        for &move_str in &tokens[idx..] {
            if let Some(m) = parse_uci_move(board, move_str) {
                *board = board.make_move_new(m);
                history.push(board.get_hash());
            }
        }
    }
}

/// Parse a UCI move string directly into squares + optional promotion piece.
/// Zero heap allocations (no move generation/string comparison).
fn parse_uci_move(board: &Board, move_str: &str) -> Option<ChessMove> {
    if move_str.len() < 4 {
        return None;
    }
    let bytes = move_str.as_bytes();

    let src_file = bytes[0].wrapping_sub(b'a');
    let src_rank = bytes[1].wrapping_sub(b'1');
    let dst_file = bytes[2].wrapping_sub(b'a');
    let dst_rank = bytes[3].wrapping_sub(b'1');

    if src_file >= 8 || src_rank >= 8 || dst_file >= 8 || dst_rank >= 8 {
        return None;
    }

    let src = Square::make_square(
        Rank::from_index(src_rank as usize),
        File::from_index(src_file as usize),
    );
    let dst = Square::make_square(
        Rank::from_index(dst_rank as usize),
        File::from_index(dst_file as usize),
    );

    let promo = if move_str.len() >= 5 {
        match bytes[4] {
            b'q' => Some(Piece::Queen),
            b'r' => Some(Piece::Rook),
            b'b' => Some(Piece::Bishop),
            b'n' => Some(Piece::Knight),
            _ => None,
        }
    } else {
        None
    };

    let mv = ChessMove::new(src, dst, promo);
    if board.legal(mv) {
        Some(mv)
    } else {
        None
    }
}

fn parse_setoption(tokens: &[&str], config: &mut EngineConfig, state: &mut SearchState) {
    let name_idx = tokens.iter().position(|&t| t == "name");
    let value_idx = tokens.iter().position(|&t| t == "value");

    if let (Some(ni), Some(vi)) = (name_idx, value_idx) {
        let name: String = tokens[ni + 1..vi].join(" ");
        let value: String = tokens[vi + 1..].join(" ");

        match name.to_lowercase().as_str() {
            "hash" => {
                if let Ok(mb) = value.parse::<usize>() {
                    config.hash_mb = mb.clamp(1, 4096);
                    state.resize_tt(config.hash_mb);
                }
            }
            "syzygypath" => {
                if value.is_empty() || value == "<empty>" {
                    config.syzygy_path = None;
                    state.syzygy = None;
                } else {
                    config.syzygy_path = Some(value.clone());
                    state.load_syzygy(&value);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_position_startpos() {
        let mut board = Board::default();
        let mut history = Vec::new();
        let tokens = vec!["position", "startpos"];
        parse_position(&tokens, &mut board, &mut history);
        assert_eq!(board, Board::default());
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_parse_position_startpos_with_moves() {
        let mut board = Board::default();
        let mut history = Vec::new();
        let tokens = vec!["position", "startpos", "moves", "e2e4", "e7e5"];
        parse_position(&tokens, &mut board, &mut history);
        assert_ne!(board, Board::default());
        // startpos + 2 moves = 3 entries in history
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_parse_position_fen() {
        let mut board = Board::default();
        let mut history = Vec::new();
        let tokens = vec![
            "position", "fen",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR",
            "b", "KQkq", "e3", "0", "1",
        ];
        parse_position(&tokens, &mut board, &mut history);
        assert_ne!(board, Board::default());
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_parse_go_depth() {
        let tokens = vec!["go", "depth", "6"];
        let params = parse_go(&tokens);
        assert_eq!(params.depth, Some(6));
    }

    #[test]
    fn test_parse_go_time() {
        let tokens = vec!["go", "wtime", "60000", "btime", "60000", "winc", "1000", "binc", "1000"];
        let params = parse_go(&tokens);
        assert_eq!(params.wtime, Some(60000));
        assert_eq!(params.btime, Some(60000));
        assert_eq!(params.winc, Some(1000));
        assert_eq!(params.binc, Some(1000));
    }

    #[test]
    fn test_compute_time_ms() {
        let mut params = GoParams::new();
        params.wtime = Some(60000);
        params.winc = Some(1000);
        let time = params.compute_time_ms(Color::White);
        assert!(time > 0 && time <= 48000, "Time allocation {} out of range", time);
    }

    #[test]
    fn test_parse_setoption_hash() {
        let mut config = EngineConfig::default();
        let mut state = SearchState::new();
        let tokens = vec!["setoption", "name", "Hash", "value", "128"];
        parse_setoption(&tokens, &mut config, &mut state);
        assert_eq!(config.hash_mb, 128);
    }

    #[test]
    fn test_parse_uci_move_basic() {
        let board = Board::default();
        let mv = parse_uci_move(&board, "e2e4");
        assert!(mv.is_some(), "e2e4 should be a legal move from startpos");
    }

    #[test]
    fn test_parse_uci_move_invalid() {
        let board = Board::default();
        let mv = parse_uci_move(&board, "e2e5"); // Not legal from startpos
        assert!(mv.is_none(), "e2e5 should not be legal from startpos");
    }

    #[test]
    fn test_parse_uci_move_promotion() {
        let board = Board::from_str("8/P7/8/8/8/8/8/K6k w - - 0 1").unwrap();
        let mv = parse_uci_move(&board, "a7a8q");
        assert!(mv.is_some(), "a7a8q should be a legal promotion");
        assert_eq!(mv.unwrap().get_promotion(), Some(Piece::Queen));
    }

    #[test]
    fn test_position_history_tracking() {
        let mut board = Board::default();
        let mut history = Vec::new();
        // Play moves that return to a similar structure
        let tokens = vec!["position", "startpos", "moves", "g1f3", "g8f6", "f3g1", "f6g8"];
        parse_position(&tokens, &mut board, &mut history);
        // startpos + 4 moves = 5 entries
        assert_eq!(history.len(), 5);
        // First and last positions should have the same hash (repetition)
        assert_eq!(history[0], history[4], "Position after Nf3 Nf6 Ng1 Ng8 should repeat startpos");
    }
}
// search runs in sep thread with an `Arc<AtomicBool>` stop flag shared w/ main thread.
