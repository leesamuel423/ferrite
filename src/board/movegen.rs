use super::attacks::{knight_attacks, king_attacks, pawn_attacks, bishop_attacks, rook_attacks};
use super::bitboard::{BitBoard, EMPTY};
use super::board::Board;
use super::chessmove::ChessMove;
use super::piece::{Color, Piece};
use super::square::{Square, Rank, File};

/// Legal move generator with consuming multi-pass iteration
pub struct MoveGen {
    moves: Vec<ChessMove>,
    consumed: Vec<bool>,
    index: usize,
    mask: BitBoard,
}

impl MoveGen {
    /// generate all legal moves for position
    pub fn new_legal(board: &Board) -> Self {
        let pseudo = generate_pseudo_legal(board);

        // filter for legality: make each move and check if our king is safe
        let mut legal_moves = Vec::with_capacity(pseudo.len());
        for mv in &pseudo {
            let new_board = board.make_move_new(*mv);
            // After make_move_new, side_to_move has flipped. Check if PREVIOUS
            // side's king is attacked — but compute_checkers already checks current
            //side's king. Since we flipped, we need to verify opponent (i.e., 
            //side that just moved) is not in check. Do this by checking if side 
            //that just moved left their king in check.
            if !is_king_attacked(&new_board, board.side_to_move()) {
                legal_moves.push(*mv);
            }
        }

        let len = legal_moves.len();
        MoveGen {
            moves: legal_moves,
            consumed: vec![false; len],
            index: 0,
            mask: !EMPTY, // all squares by default
        }
    }

    /// Set iterator mask and reset index
    /// Only moves whose destination matches mask will be yielded
    /// Already-consumed moves are skipped
    pub fn set_iterator_mask(&mut self, mask: BitBoard) {
        self.mask = mask;
        self.index = 0;
    }
}

impl Iterator for MoveGen {
    type Item = ChessMove;

    fn next(&mut self) -> Option<ChessMove> {
        while self.index < self.moves.len() {
            let i = self.index;
            self.index += 1;

            if self.consumed[i] {
                continue;
            }

            let mv = self.moves[i];
            let dest_bb = BitBoard::from_square(mv.get_dest());
            if (dest_bb & self.mask).is_empty() {
                continue;
            }

            self.consumed[i] = true;
            return Some(mv);
        }
        None
    }
}

/// Check if given color's king is attacked in position.
fn is_king_attacked(board: &Board, color: Color) -> bool {
    let king_bb = board.pieces(Piece::King) & board.color_combined(color);
    if king_bb.is_empty() {
        return false;
    }
    let king_sq = Square::new(king_bb.0.trailing_zeros() as u8);
    let occupied = board.combined();
    let enemy = board.color_combined(!color);

    // Check all piece types
    if !(knight_attacks(king_sq) & board.pieces(Piece::Knight) & enemy).is_empty() {
        return true;
    }
    if !(pawn_attacks(color, king_sq) & board.pieces(Piece::Pawn) & enemy).is_empty() {
        return true;
    }
    if !(bishop_attacks(king_sq, occupied)
        & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen))
        & enemy)
        .is_empty()
    {
        return true;
    }
    if !(rook_attacks(king_sq, occupied)
        & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
        & enemy)
        .is_empty()
    {
        return true;
    }
    if !(king_attacks(king_sq) & board.pieces(Piece::King) & enemy).is_empty() {
        return true;
    }

    false
}

/// Generate all pseudo-legal moves (piece rules only, ignoring pins/check).
fn generate_pseudo_legal(board: &Board) -> Vec<ChessMove> {
    let mut moves = Vec::with_capacity(64);
    let us = board.side_to_move();
    let them = !us;
    let our_pieces = board.color_combined(us);
    let their_pieces = board.color_combined(them);
    let occupied = board.combined();
    let empty = !occupied;

    // Pawn moves
    generate_pawn_moves(board, us, our_pieces, their_pieces, occupied, empty, &mut moves);

    // Knight moves
    let knights = board.pieces(Piece::Knight) & our_pieces;
    for sq in knights.iter() {
        let attacks = knight_attacks(sq) & !our_pieces;
        for dst in attacks.iter() {
            moves.push(ChessMove::new(sq, dst, None));
        }
    }

    // Bishop moves
    let bishops = board.pieces(Piece::Bishop) & our_pieces;
    for sq in bishops.iter() {
        let attacks = bishop_attacks(sq, occupied) & !our_pieces;
        for dst in attacks.iter() {
            moves.push(ChessMove::new(sq, dst, None));
        }
    }

    // Rook moves
    let rooks = board.pieces(Piece::Rook) & our_pieces;
    for sq in rooks.iter() {
        let attacks = rook_attacks(sq, occupied) & !our_pieces;
        for dst in attacks.iter() {
            moves.push(ChessMove::new(sq, dst, None));
        }
    }

    // Queen moves
    let queens = board.pieces(Piece::Queen) & our_pieces;
    for sq in queens.iter() {
        let attacks = (bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)) & !our_pieces;
        for dst in attacks.iter() {
            moves.push(ChessMove::new(sq, dst, None));
        }
    }

    // King moves (non-castling)
    let king_bb = board.pieces(Piece::King) & our_pieces;
    if !king_bb.is_empty() {
        let king_sq = Square::new(king_bb.0.trailing_zeros() as u8);
        let attacks = king_attacks(king_sq) & !our_pieces;
        for dst in attacks.iter() {
            moves.push(ChessMove::new(king_sq, dst, None));
        }

        // Castling
        generate_castling(board, king_sq, us, occupied, &mut moves);
    }

    moves
}

fn generate_pawn_moves(
    board: &Board,
    us: Color,
    our_pieces: BitBoard,
    their_pieces: BitBoard,
    _occupied: BitBoard,
    empty: BitBoard,
    moves: &mut Vec<ChessMove>,
) {
    let pawns = board.pieces(Piece::Pawn) & our_pieces;
    let promo_rank = if us == Color::White { 7usize } else { 0usize };

    let (push_dir, start_rank, _double_rank): (i8, usize, usize) = if us == Color::White {
        (8, 1, 3)
    } else {
        (-8, 6, 4)
    };

    for sq in pawns.iter() {
        let sq_idx = sq.to_index() as i8;

        // Single push
        let push_sq_idx = sq_idx + push_dir;
        if (0..64).contains(&push_sq_idx) {
            let push_sq = Square::new(push_sq_idx as u8);
            if !(BitBoard::from_square(push_sq) & empty).is_empty() {
                if push_sq.rank().to_index() == promo_rank {
                    // Promotion: generate all 4 promo moves
                    for p in [Piece::Queen, Piece::Rook, Piece::Bishop, Piece::Knight] {
                        moves.push(ChessMove::new(sq, push_sq, Some(p)));
                    }
                } else {
                    moves.push(ChessMove::new(sq, push_sq, None));

                    // Double push (only if single push was possible)
                    if sq.rank().to_index() == start_rank {
                        let double_idx = sq_idx + push_dir * 2;
                        let double_sq = Square::new(double_idx as u8);
                        if !(BitBoard::from_square(double_sq) & empty).is_empty() {
                            moves.push(ChessMove::new(sq, double_sq, None));
                        }
                    }
                }
            }
        }

        // Captures (using pawn attack table)
        let attacks = pawn_attacks(us, sq) & their_pieces;
        for dst in attacks.iter() {
            if dst.rank().to_index() == promo_rank {
                for p in [Piece::Queen, Piece::Rook, Piece::Bishop, Piece::Knight] {
                    moves.push(ChessMove::new(sq, dst, Some(p)));
                }
            } else {
                moves.push(ChessMove::new(sq, dst, None));
            }
        }

        // En passant
        if let Some(ep_sq) = board.en_passant() {
            let ep_attacks = pawn_attacks(us, sq) & BitBoard::from_square(ep_sq);
            if !ep_attacks.is_empty() {
                moves.push(ChessMove::new(sq, ep_sq, None));
            }
        }
    }
}

fn generate_castling(
    board: &Board,
    king_sq: Square,
    us: Color,
    occupied: BitBoard,
    moves: &mut Vec<ChessMove>,
) {
    use super::board::{WK, WQ, BK, BQ};
    let rights = board.castling_rights();

    let (ks_right, qs_right, rank) = if us == Color::White {
        (WK, WQ, 0usize)
    } else {
        (BK, BQ, 7usize)
    };

    // Kingside
    if rights & ks_right != 0 {
        let f_sq = Square::make_square(Rank::from_index(rank), File::from_index(5));
        let g_sq = Square::make_square(Rank::from_index(rank), File::from_index(6));

        // Squares between king and rook must be empty
        if (BitBoard::from_square(f_sq) | BitBoard::from_square(g_sq)) & occupied == EMPTY {
            // King must not pass through or land on attacked square, and not be in check
            if !is_king_attacked(board, us)
                && !is_square_attacked(board, f_sq, us)
                && !is_square_attacked(board, g_sq, us)
            {
                moves.push(ChessMove::new(king_sq, g_sq, None));
            }
        }
    }

    // Queenside
    if rights & qs_right != 0 {
        let d_sq = Square::make_square(Rank::from_index(rank), File::from_index(3));
        let c_sq = Square::make_square(Rank::from_index(rank), File::from_index(2));
        let b_sq = Square::make_square(Rank::from_index(rank), File::from_index(1));

        // Squares between king and rook must be empty
        if (BitBoard::from_square(d_sq) | BitBoard::from_square(c_sq) | BitBoard::from_square(b_sq))
            & occupied
            == EMPTY
            && !is_king_attacked(board, us)
            && !is_square_attacked(board, d_sq, us)
            && !is_square_attacked(board, c_sq, us)
        {
            moves.push(ChessMove::new(king_sq, c_sq, None));
        }
    }
}

/// Check if specific square is attacked by opponent of `color`
fn is_square_attacked(board: &Board, sq: Square, color: Color) -> bool {
    let enemy = board.color_combined(!color);
    let occupied = board.combined();

    if !(knight_attacks(sq) & board.pieces(Piece::Knight) & enemy).is_empty() {
        return true;
    }
    if !(pawn_attacks(color, sq) & board.pieces(Piece::Pawn) & enemy).is_empty() {
        return true;
    }
    if !(bishop_attacks(sq, occupied)
        & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen))
        & enemy)
        .is_empty()
    {
        return true;
    }
    if !(rook_attacks(sq, occupied)
        & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
        & enemy)
        .is_empty()
    {
        return true;
    }
    if !(king_attacks(sq) & board.pieces(Piece::King) & enemy).is_empty() {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn init() {
        super::super::init();
    }

    fn perft(board: &Board, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let mg = MoveGen::new_legal(board);
        let mut count = 0u64;
        for mv in mg {
            let new_board = board.make_move_new(mv);
            count += perft(&new_board, depth - 1);
        }
        count
    }

    #[test]
    fn test_startpos_moves() {
        init();
        let board = Board::default();
        let moves: Vec<_> = MoveGen::new_legal(&board).collect();
        assert_eq!(moves.len(), 20, "Starting position should have 20 legal moves, got {}", moves.len());
    }

    #[test]
    fn test_perft_depth1() {
        init();
        let board = Board::default();
        assert_eq!(perft(&board, 1), 20);
    }

    #[test]
    fn test_perft_depth2() {
        init();
        let board = Board::default();
        assert_eq!(perft(&board, 2), 400);
    }

    #[test]
    fn test_perft_depth3() {
        init();
        let board = Board::default();
        assert_eq!(perft(&board, 3), 8902);
    }

    #[test]
    fn test_perft_depth4() {
        init();
        let board = Board::default();
        assert_eq!(perft(&board, 4), 197_281);
    }

    #[test]
    fn test_kiwipete_depth1() {
        init();
        let board = Board::from_str("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        assert_eq!(perft(&board, 1), 48);
    }

    #[test]
    fn test_kiwipete_depth2() {
        init();
        let board = Board::from_str("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        assert_eq!(perft(&board, 2), 2039);
    }

    #[test]
    fn test_kiwipete_depth3() {
        init();
        let board = Board::from_str("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        assert_eq!(perft(&board, 3), 97_862);
    }

    #[test]
    fn test_position3_depth1() {
        init();
        let board = Board::from_str("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1").unwrap();
        assert_eq!(perft(&board, 1), 14);
    }

    #[test]
    fn test_position3_depth2() {
        init();
        let board = Board::from_str("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1").unwrap();
        assert_eq!(perft(&board, 2), 191);
    }

    #[test]
    fn test_position3_depth3() {
        init();
        let board = Board::from_str("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1").unwrap();
        assert_eq!(perft(&board, 3), 2812);
    }

    #[test]
    fn test_iterator_mask() {
        init();
        let board = Board::default();
        let mut mg = MoveGen::new_legal(&board);

        // First pass: only captures (none from startpos)
        let targets = board.color_combined(!board.side_to_move());
        mg.set_iterator_mask(targets);
        let captures: Vec<_> = mg.by_ref().collect();
        assert_eq!(captures.len(), 0);

        // Second pass: all remaining
        mg.set_iterator_mask(!EMPTY);
        let remaining: Vec<_> = mg.collect();
        assert_eq!(remaining.len(), 20);
    }

    #[test]
    fn test_no_duplicate_moves() {
        init();
        let board = Board::from_str("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1").unwrap();
        let mut mg = MoveGen::new_legal(&board);

        // Pass 1: captures
        mg.set_iterator_mask(board.color_combined(!board.side_to_move()));
        let pass1: Vec<_> = mg.by_ref().collect();

        // Pass 2: EP
        if let Some(ep_sq) = board.en_passant() {
            mg.set_iterator_mask(BitBoard::from_square(ep_sq));
            let _: Vec<_> = mg.by_ref().collect();
        }

        // Pass 3: all remaining
        mg.set_iterator_mask(!EMPTY);
        let pass3: Vec<_> = mg.collect();

        // Captures should not appear in remaining
        for mv in &pass1 {
            assert!(!pass3.contains(mv), "Move {} yielded in both passes", mv);
        }
    }
}

// Pseudo-legal + filter approach. Generate moves that obey piece movmenet rules but might leave
// the king in check (Pseudo-legal). Then filter... for each candidate move, make it on a copy of
// the board and check if king is attacked. A bit slower than pin-aware generation, but simple

// Multi-pass iteration -> `MoceGen` struct supports 3-pass iteration w/ `set_iterator_mask()`.
// Lets move ordering request captures first -> EP -> quite moves, w/o generating them separately

// perft is gold standard. These numbers (20, 400, 8902, 197281 for startpos; 48, 2039, 97862 for
// Kiwipete) are agreed upon. If perft numbers match, board representation and move gen are
// correct. If they don't match, something wrong
