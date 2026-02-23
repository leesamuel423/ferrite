use std::fs;
use std::str::FromStr;

use ferrite::board::{Board, BoardStatus, ChessMove, Color, MoveGen, Piece, ALL_SQUARES};

/// Parse an EPD line: "<FEN> bm <move(s)>; id "<name>";"
fn parse_epd_line(line: &str) -> Option<(String, Vec<String>, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Split on " bm "
    let bm_idx = line.find(" bm ")?;
    let fen = line[..bm_idx].to_string();
    let rest = &line[bm_idx + 4..];

    // Extract best moves (before semicolon)
    let semi_idx = rest.find(';')?;
    let moves_str = &rest[..semi_idx];
    let best_moves: Vec<String> = moves_str.split_whitespace().map(|s| s.to_string()).collect();

    // Extract id
    let id = if let Some(id_start) = rest.find("id \"") {
        let id_content = &rest[id_start + 4..];
        if let Some(id_end) = id_content.find('"') {
            id_content[..id_end].to_string()
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    Some((fen, best_moves, id))
}

/// Convert a ChessMove to SAN notation.
fn move_to_san(board: &Board, mv: ChessMove) -> String {
    let piece = board.piece_on(mv.get_source()).unwrap();
    let is_capture = board.piece_on(mv.get_dest()).is_some()
        || (piece == Piece::Pawn
            && mv.get_source().file() != mv.get_dest().file());

    // Castling
    if piece == Piece::King {
        let from_file = mv.get_source().file().to_index();
        let to_file = mv.get_dest().file().to_index();
        if from_file == 4 && to_file == 6 {
            return add_check_suffix(board, mv, "O-O".to_string());
        }
        if from_file == 4 && to_file == 2 {
            return add_check_suffix(board, mv, "O-O-O".to_string());
        }
    }

    let mut san = String::new();

    if piece == Piece::Pawn {
        if is_capture {
            san.push((b'a' + mv.get_source().file().to_index() as u8) as char);
        }
    } else {
        san.push(piece_char(piece));
        let disambig = disambiguation(board, mv, piece);
        san.push_str(&disambig);
    }

    if is_capture {
        san.push('x');
    }

    san.push((b'a' + mv.get_dest().file().to_index() as u8) as char);
    san.push((b'1' + mv.get_dest().rank().to_index() as u8) as char);

    if let Some(promo) = mv.get_promotion() {
        san.push('=');
        san.push(piece_char(promo));
    }

    add_check_suffix(board, mv, san)
}

fn add_check_suffix(board: &Board, mv: ChessMove, mut san: String) -> String {
    let new_board = board.make_move_new(mv);
    match new_board.status() {
        BoardStatus::Checkmate => san.push('#'),
        _ => {
            if new_board.checkers().popcnt() > 0 {
                san.push('+');
            }
        }
    }
    san
}

fn piece_char(piece: Piece) -> char {
    match piece {
        Piece::Knight => 'N',
        Piece::Bishop => 'B',
        Piece::Rook => 'R',
        Piece::Queen => 'Q',
        Piece::King => 'K',
        Piece::Pawn => 'P',
    }
}

fn disambiguation(board: &Board, mv: ChessMove, piece: Piece) -> String {
    let mut needs_file = false;
    let mut needs_rank = false;
    let mut ambiguous = false;

    let moves = MoveGen::new_legal(board);
    for other in moves {
        if other == mv { continue; }
        if board.piece_on(other.get_source()) == Some(piece)
            && other.get_dest() == mv.get_dest()
        {
            ambiguous = true;
            if other.get_source().file() == mv.get_source().file() {
                needs_rank = true;
            }
            if other.get_source().rank() == mv.get_source().rank() {
                needs_file = true;
            }
        }
    }

    if !ambiguous { return String::new(); }
    if !needs_file && !needs_rank { needs_file = true; }

    let mut s = String::new();
    if needs_file {
        s.push((b'a' + mv.get_source().file().to_index() as u8) as char);
    }
    if needs_rank {
        s.push((b'1' + mv.get_source().rank().to_index() as u8) as char);
    }
    s
}

/// Simple static evaluation for integration testing
fn simple_evaluate(board: &Board) -> i32 {
    let piece_values = [100, 320, 330, 500, 900, 20000];
    let mut score = 0i32;

    for sq in ALL_SQUARES {
        if let Some(piece) = board.piece_on(sq) {
            let color = board.color_on(sq).unwrap();
            let idx = match piece {
                Piece::Pawn => 0, Piece::Knight => 1, Piece::Bishop => 2,
                Piece::Rook => 3, Piece::Queen => 4, Piece::King => 5,
            };
            let val = piece_values[idx];
            score += if color == Color::White { val } else { -val };
        }
    }

    if board.side_to_move() == Color::White { score } else { -score }
}

#[test]
fn test_bk_suite() {
    ferrite::board::init();

    let content = fs::read_to_string("tests/bk.txt").expect("Could not read tests/bk.txt");
    let mut total = 0;

    for line in content.lines() {
        if let Some((fen, _best_moves, _id)) = parse_epd_line(line) {
            total += 1;
            let board = Board::from_str(&fen).expect(&format!("Invalid FEN: {}", fen));
            let moves = MoveGen::new_legal(&board);
            let mut best_move = None;
            let mut best_score = i32::MIN;

            for mv in moves {
                let new_board = board.make_move_new(mv);
                let score = -simple_evaluate(&new_board);
                if score > best_score {
                    best_score = score;
                    best_move = Some(mv);
                }
            }

            assert!(best_move.is_some());
        }
    }

    assert!(total == 24, "Expected 24 BK positions, got {}", total);
}

#[test]
fn test_epd_parser() {
    let line = "1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - - bm Qd1+; id \"BK.01\";";
    let (fen, moves, id) = parse_epd_line(line).unwrap();
    assert_eq!(fen, "1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - -");
    assert_eq!(moves, vec!["Qd1+"]);
    assert_eq!(id, "BK.01");
}

#[test]
fn test_uci_to_san_conversion() {
    ferrite::board::init();
    let board = Board::from_str("1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - -").unwrap();
    let moves = MoveGen::new_legal(&board);
    for mv in moves {
        if mv.to_string() == "d6d1" {
            let san = move_to_san(&board, mv);
            assert_eq!(san, "Qd1+");
            return;
        }
    }
    panic!("d6d1 not found as legal move");
}

