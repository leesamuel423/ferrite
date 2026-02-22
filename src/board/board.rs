use std::fmt;
use std::str::FromStr;

use super::attacks::{bishop_attacks, rook_attacks, knight_attacks, king_attacks, pawn_attacks};
use super::bitboard::{BitBoard, EMPTY};
use super::chessmove::ChessMove;
use super::piece::{Color, Piece};
use super::square::{Square, Rank, File};
use super::zobrist;

/// Castling rights stored as a 4-bit mask
/// Bit 0: White kingside, Bit 1: White queenside
/// Bit 2: Black kingside, Bit 3: Black queenside
pub const WK: u8 = 1;
pub const WQ: u8 = 2;
pub const BK: u8 = 4;
pub const BQ: u8 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardStatus {
    Ongoing,
    Checkmate,
    Stalemate,
}

/// The board representation. Copy
#[derive(Clone, Copy, Debug)]
pub struct Board {
    pieces: [BitBoard; 6], // per piece type
    colors: [BitBoard; 2], // per color
    side_to_move: Color,
    castling: u8, // 4-bit castling rights
    ep_square: Option<Square>,
    halfmove_clock: u8,
    hash: u64,
    checkers: BitBoard, // cached: enemy pieces giving check
}

impl Board {
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn piece_on(&self, sq: Square) -> Option<Piece> {
        let bb = BitBoard::from_square(sq);
        Piece::ALL.into_iter().find(|&p| !(self.pieces[p.to_index()] & bb).is_empty())
    }

    pub fn color_on(&self, sq: Square) -> Option<Color> {
        let bb = BitBoard::from_square(sq);
        if !(self.colors[0] & bb).is_empty() {
            Some(Color::White)
        } else if !(self.colors[1] & bb).is_empty() {
            Some(Color::Black)
        } else {
            None
        }
    }

    pub fn checkers(&self) -> BitBoard {
        self.checkers
    }

    pub fn en_passant(&self) -> Option<Square> {
        self.ep_square
    }

    pub fn color_combined(&self, color: Color) -> BitBoard {
        self.colors[color.to_index()]
    }

    pub fn pieces(&self, piece: Piece) -> BitBoard {
        self.pieces[piece.to_index()]
    }

    pub fn combined(&self) -> BitBoard {
        self.colors[0] | self.colors[1]
    }

    pub fn get_hash(&self) -> u64 {
        self.hash
    }

    pub fn castling_rights(&self) -> u8 {
        self.castling
    }

    /// Compute board status by checking if any legal move exists
    pub fn status(&self) -> BoardStatus {
        // Quick check: generate pseudo-legal moves and test legality
        if self.has_legal_move() {
            BoardStatus::Ongoing
        } else if !self.checkers.is_empty() {
            BoardStatus::Checkmate
        } else {
            BoardStatus::Stalemate
        }
    }

    /// Check if at least one legal move exists (for status detection)
    fn has_legal_move(&self) -> bool {
        use super::movegen::MoveGen;
        let mut mg = MoveGen::new_legal(self);
        mg.next().is_some()
    }

    /// Check if move is legal in the current position
    pub fn legal(&self, mv: ChessMove) -> bool {
        use super::movegen::MoveGen;
        let mg = MoveGen::new_legal(self);
        for m in mg {
            if m == mv {
                return true;
            }
        }
        false
    }

    /// Make move and return resulting board. Does not validate legality
    ///
    /// Handles:
    /// 1. Remove piece from source square
    /// 2. Remove captured piece (if any)
    /// 3. Handle en passant capture (captured pawn is NOT on destination)
    /// 4. Place piece/promoted piece on destination
    /// 5. Handle castling (move the rook too)
    /// 6. Update castling rights via CASTLING_MASK[64]
    /// 7. Set new en passant square (double pawn push)
    /// 8. Update halfmove clock
    /// 9. Flip side to move + update hash
    /// 10. Recompute checkers
    pub fn make_move_new(&self, mv: ChessMove) -> Board {
        let mut b = *self;
        let src = mv.get_source();
        let dst = mv.get_dest();
        let us = self.side_to_move;
        let them = !us;
        let us_idx = us.to_index();
        let them_idx = them.to_index();

        let piece = self.piece_on(src).expect("no piece on source square");
        let captured = self.piece_on(dst);

        // Remove old hash components
        b.hash ^= zobrist::castling_key(b.castling);
        if let Some(ep) = b.ep_square {
            b.hash ^= zobrist::ep_key(ep.file().to_index());
        }

        // Remove piece from source
        let src_bb = BitBoard::from_square(src);
        let dst_bb = BitBoard::from_square(dst);
        b.pieces[piece.to_index()] ^= src_bb;
        b.colors[us_idx] ^= src_bb;
        b.hash ^= zobrist::piece_key(piece.to_index(), us_idx, src.to_index());

        // Handle capture (regular)
        if let Some(cap) = captured {
            b.pieces[cap.to_index()] ^= dst_bb;
            b.colors[them_idx] ^= dst_bb;
            b.hash ^= zobrist::piece_key(cap.to_index(), them_idx, dst.to_index());
        }

        // Handle en passant capture
        let is_ep = piece == Piece::Pawn
            && self.ep_square == Some(dst)
            && captured.is_none();

        if is_ep {
            let ep_pawn_sq = match us {
                Color::White => Square::new(dst.to_index() as u8 - 8),
                Color::Black => Square::new(dst.to_index() as u8 + 8),
            };
            let ep_bb = BitBoard::from_square(ep_pawn_sq);
            b.pieces[Piece::Pawn.to_index()] ^= ep_bb;
            b.colors[them_idx] ^= ep_bb;
            b.hash ^= zobrist::piece_key(Piece::Pawn.to_index(), them_idx, ep_pawn_sq.to_index());
        }

        // Place piece (or promoted piece) on destination
        let placed_piece = mv.get_promotion().unwrap_or(piece);
        b.pieces[placed_piece.to_index()] ^= dst_bb;
        b.colors[us_idx] ^= dst_bb;
        b.hash ^= zobrist::piece_key(placed_piece.to_index(), us_idx, dst.to_index());

        // Handle castling (move the rook)
        if piece == Piece::King {
            let from_file = src.file().to_index();
            let to_file = dst.file().to_index();

            if from_file == 4 && to_file == 6 {
                // Kingside castle
                let rook_src = Square::make_square(src.rank(), File::from_index(7));
                let rook_dst = Square::make_square(src.rank(), File::from_index(5));
                Self::move_piece(&mut b, Piece::Rook, us, rook_src, rook_dst);
            } else if from_file == 4 && to_file == 2 {
                // Queenside castle
                let rook_src = Square::make_square(src.rank(), File::from_index(0));
                let rook_dst = Square::make_square(src.rank(), File::from_index(3));
                Self::move_piece(&mut b, Piece::Rook, us, rook_src, rook_dst);
            }
        }

        // Update castling rights
        b.castling &= CASTLING_MASK[src.to_index()];
        b.castling &= CASTLING_MASK[dst.to_index()];

        // Update en passant square
        b.ep_square = None;
        if piece == Piece::Pawn {
            let src_rank = src.rank().to_index();
            let dst_rank = dst.rank().to_index();
            if src_rank.abs_diff(dst_rank) == 2 {
                let ep_rank = (src_rank + dst_rank) / 2;
                b.ep_square = Some(Square::make_square(
                    Rank::from_index(ep_rank),
                    src.file(),
                ));
            }
        }

        // Update halfmove clock
        if piece == Piece::Pawn || captured.is_some() || is_ep {
            b.halfmove_clock = 0;
        } else {
            b.halfmove_clock = self.halfmove_clock + 1;
        }

        // Hash in new castling + EP
        b.hash ^= zobrist::castling_key(b.castling);
        if let Some(ep) = b.ep_square {
            b.hash ^= zobrist::ep_key(ep.file().to_index());
        }

        // Flip side to move
        b.side_to_move = them;
        b.hash ^= zobrist::side_key();

        // Recompute checkers
        b.checkers = b.compute_checkers();

        b
    }

    /// Helper to move a piece (for castling rook moves)
    fn move_piece(b: &mut Board, piece: Piece, color: Color, from: Square, to: Square) {
        let from_bb = BitBoard::from_square(from);
        let to_bb = BitBoard::from_square(to);
        let ci = color.to_index();
        let pi = piece.to_index();
        b.pieces[pi] ^= from_bb | to_bb;
        b.colors[ci] ^= from_bb | to_bb;
        b.hash ^= zobrist::piece_key(pi, ci, from.to_index());
        b.hash ^= zobrist::piece_key(pi, ci, to.to_index());
    }

    /// Null move: flip side, clear EP. Returns None if in check
    pub fn null_move(&self) -> Option<Board> {
        if !self.checkers.is_empty() {
            return None;
        }

        let mut b = *self;

        // Remove old EP from hash
        if let Some(ep) = b.ep_square {
            b.hash ^= zobrist::ep_key(ep.file().to_index());
        }

        b.ep_square = None;
        b.side_to_move = !b.side_to_move;
        b.hash ^= zobrist::side_key();
        b.checkers = b.compute_checkers();

        Some(b)
    }

    /// Compute which enemy pieces are giving check to the current side's king
    fn compute_checkers(&self) -> BitBoard {
        let us = self.side_to_move;
        let them = !us;
        let our_king_bb = self.pieces[Piece::King.to_index()] & self.colors[us.to_index()];
        if our_king_bb.is_empty() {
            return EMPTY;
        }
        let king_sq = Square::new(our_king_bb.0.trailing_zeros() as u8);
        let occupied = self.combined();
        let enemy = self.colors[them.to_index()];

        let mut checkers = EMPTY;

        // Knight checks
        checkers |= knight_attacks(king_sq) & self.pieces[Piece::Knight.to_index()] & enemy;
        // Pawn checks
        checkers |= pawn_attacks(us, king_sq) & self.pieces[Piece::Pawn.to_index()] & enemy;
        // Bishop/Queen checks (diagonal)
        checkers |= bishop_attacks(king_sq, occupied)
            & (self.pieces[Piece::Bishop.to_index()] | self.pieces[Piece::Queen.to_index()])
            & enemy;
        // Rook/Queen checks (straight)
        checkers |= rook_attacks(king_sq, occupied)
            & (self.pieces[Piece::Rook.to_index()] | self.pieces[Piece::Queen.to_index()])
            & enemy;
        // King checks (theoretically impossible but keep for consistency)
        checkers |= king_attacks(king_sq) & self.pieces[Piece::King.to_index()] & enemy;

        checkers
    }

    /// Compute hash from scratch (for FEN parsing)
    fn compute_hash(&self) -> u64 {
        let mut h = 0u64;
        for piece in Piece::ALL {
            for color in [Color::White, Color::Black] {
                let mut bb = self.pieces[piece.to_index()] & self.colors[color.to_index()];
                while !bb.is_empty() {
                    let sq_idx = bb.0.trailing_zeros() as usize;
                    h ^= zobrist::piece_key(piece.to_index(), color.to_index(), sq_idx);
                    bb.0 &= bb.0 - 1;
                }
            }
        }
        if self.side_to_move == Color::Black {
            h ^= zobrist::side_key();
        }
        h ^= zobrist::castling_key(self.castling);
        if let Some(ep) = self.ep_square {
            h ^= zobrist::ep_key(ep.file().to_index());
        }
        h
    }
}

// --- Castling rights update mask ---
// When piece moves from or to square, AND castling rights with this mask
// This handles rook captures and king/rook moves
const CASTLING_MASK: [u8; 64] = {
    let mut mask = [0xFFu8; 64];
    // A1 (index 0) = White queenside rook
    mask[0] = 0xFF ^ WQ;
    // H1 (index 7) = White kingside rook
    mask[7] = 0xFF ^ WK;
    // E1 (index 4) = White king
    mask[4] = 0xFF ^ (WK | WQ);
    // A8 (index 56) = Black queenside rook
    mask[56] = 0xFF ^ BQ;
    // H8 (index 63) = Black kingside rook
    mask[63] = 0xFF ^ BK;
    // E8 (index 60) = Black king
    mask[60] = 0xFF ^ (BK | BQ);
    mask
};

// --- Default (starting position) ---

impl Default for Board {
    fn default() -> Self {
        Board::from_str("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("invalid starting FEN")
    }
}

impl PartialEq for Board {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
            && self.colors == other.colors
            && self.side_to_move == other.side_to_move
            && self.castling == other.castling
            && self.ep_square == other.ep_square
    }
}

impl Eq for Board {}

// --- FEN parsing ---

impl FromStr for Board {
    type Err = String;

    fn from_str(fen: &str) -> Result<Self, String> {
        super::attacks::init_attacks();
        super::zobrist::init_zobrist();

        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(format!("FEN needs at least 4 fields, got {}", parts.len()));
        }

        let mut pieces = [EMPTY; 6];
        let mut colors = [EMPTY; 2];

        // Parse piece placement (rank 8 first, rank 1 last)
        let mut rank = 7i8;
        let mut file = 0i8;
        for ch in parts[0].chars() {
            if ch == '/' {
                rank -= 1;
                file = 0;
                continue;
            }
            if let Some(skip) = ch.to_digit(10) {
                file += skip as i8;
                continue;
            }

            let color = if ch.is_uppercase() { Color::White } else { Color::Black };
            let piece = match ch.to_ascii_lowercase() {
                'p' => Piece::Pawn,
                'n' => Piece::Knight,
                'b' => Piece::Bishop,
                'r' => Piece::Rook,
                'q' => Piece::Queen,
                'k' => Piece::King,
                _ => return Err(format!("Invalid piece char: {}", ch)),
            };

            if !(0..=7).contains(&rank) || !(0..=7).contains(&file) {
                return Err("FEN rank/file out of bounds".to_string());
            }

            let sq = Square::make_square(Rank::from_index(rank as usize), File::from_index(file as usize));
            let bb = BitBoard::from_square(sq);
            pieces[piece.to_index()] |= bb;
            colors[color.to_index()] |= bb;
            file += 1;
        }

        // Side to move
        let side_to_move = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(format!("Invalid side to move: {}", parts[1])),
        };

        // Castling rights
        let mut castling = 0u8;
        for ch in parts[2].chars() {
            match ch {
                'K' => castling |= WK,
                'Q' => castling |= WQ,
                'k' => castling |= BK,
                'q' => castling |= BQ,
                '-' => {}
                _ => return Err(format!("Invalid castling char: {}", ch)),
            }
        }

        // En passant square
        let ep_square = if parts[3] == "-" {
            None
        } else {
            let bytes = parts[3].as_bytes();
            if bytes.len() >= 2 {
                let f = bytes[0].wrapping_sub(b'a');
                let r = bytes[1].wrapping_sub(b'1');
                if f < 8 && r < 8 {
                    Some(Square::make_square(Rank::from_index(r as usize), File::from_index(f as usize)))
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Halfmove clock (optional)
        let halfmove_clock = if parts.len() > 4 {
            parts[4].parse().unwrap_or(0)
        } else {
            0
        };

        let mut board = Board {
            pieces,
            colors,
            side_to_move,
            castling,
            ep_square,
            halfmove_clock,
            hash: 0,
            checkers: EMPTY,
        };

        board.hash = board.compute_hash();
        board.checkers = board.compute_checkers();

        Ok(board)
    }
}

// --- FEN output (Display) ---

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Piece placement
        for rank in (0..8).rev() {
            let mut empty_count = 0;
            for file in 0..8 {
                let sq = Square::make_square(Rank::from_index(rank), File::from_index(file));
                if let Some(piece) = self.piece_on(sq) {
                    if empty_count > 0 {
                        write!(f, "{}", empty_count)?;
                        empty_count = 0;
                    }
                    let color = self.color_on(sq).unwrap();
                    let ch = piece_to_char(piece, color);
                    write!(f, "{}", ch)?;
                } else {
                    empty_count += 1;
                }
            }
            if empty_count > 0 {
                write!(f, "{}", empty_count)?;
            }
            if rank > 0 {
                write!(f, "/")?;
            }
        }

        // Side to move
        write!(f, " {}", if self.side_to_move == Color::White { "w" } else { "b" })?;

        // Castling
        write!(f, " ")?;
        if self.castling == 0 {
            write!(f, "-")?;
        } else {
            if self.castling & WK != 0 { write!(f, "K")?; }
            if self.castling & WQ != 0 { write!(f, "Q")?; }
            if self.castling & BK != 0 { write!(f, "k")?; }
            if self.castling & BQ != 0 { write!(f, "q")?; }
        }

        // En passant
        write!(f, " ")?;
        if let Some(ep) = self.ep_square {
            write!(f, "{}", ep)?;
        } else {
            write!(f, "-")?;
        }

        // Halfmove clock and fullmove number
        write!(f, " {} 1", self.halfmove_clock)?;

        Ok(())
    }
}

fn piece_to_char(piece: Piece, color: Color) -> char {
    let ch = match piece {
        Piece::Pawn => 'p',
        Piece::Knight => 'n',
        Piece::Bishop => 'b',
        Piece::Rook => 'r',
        Piece::Queen => 'q',
        Piece::King => 'k',
    };
    if color == Color::White { ch.to_ascii_uppercase() } else { ch }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        super::super::init();
    }

    #[test]
    fn test_default_board() {
        init();
        let board = Board::default();
        assert_eq!(board.side_to_move(), Color::White);
        assert_eq!(board.castling, WK | WQ | BK | BQ);
        assert!(board.en_passant().is_none());
    }

    #[test]
    fn test_fen_roundtrip_startpos() {
        init();
        let board = Board::default();
        let fen = board.to_string();
        assert!(fen.starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -"));
    }

    #[test]
    fn test_fen_parse_complex() {
        init();
        let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        let board = Board::from_str(fen).unwrap();
        assert_eq!(board.side_to_move(), Color::White);
        assert_eq!(board.castling, WK | WQ | BK | BQ);
    }

    #[test]
    fn test_piece_on() {
        init();
        let board = Board::default();
        let e1 = Square::make_square(Rank::from_index(0), File::from_index(4));
        assert_eq!(board.piece_on(e1), Some(Piece::King));
        assert_eq!(board.color_on(e1), Some(Color::White));
    }

    #[test]
    fn test_make_move_basic() {
        init();
        let board = Board::default();
        let e2 = Square::make_square(Rank::from_index(1), File::from_index(4));
        let e4 = Square::make_square(Rank::from_index(3), File::from_index(4));
        let mv = ChessMove::new(e2, e4, None);
        let new_board = board.make_move_new(mv);

        assert_eq!(new_board.piece_on(e4), Some(Piece::Pawn));
        assert_eq!(new_board.piece_on(e2), None);
        assert_eq!(new_board.side_to_move(), Color::Black);
        assert!(new_board.en_passant().is_some()); // e3
    }

    #[test]
    fn test_make_move_capture() {
        init();
        let fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
        let board = Board::from_str(fen).unwrap();
        let e4 = Square::make_square(Rank::from_index(3), File::from_index(4));
        let d5 = Square::make_square(Rank::from_index(4), File::from_index(3));
        let mv = ChessMove::new(e4, d5, None);
        let new_board = board.make_move_new(mv);
        assert_eq!(new_board.piece_on(d5), Some(Piece::Pawn));
        assert_eq!(new_board.color_on(d5), Some(Color::White));
    }

    #[test]
    fn test_castling_kingside() {
        init();
        let fen = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";
        let board = Board::from_str(fen).unwrap();
        let e1 = Square::make_square(Rank::from_index(0), File::from_index(4));
        let g1 = Square::make_square(Rank::from_index(0), File::from_index(6));
        let mv = ChessMove::new(e1, g1, None);
        let new_board = board.make_move_new(mv);
        assert_eq!(new_board.piece_on(g1), Some(Piece::King));
        let f1 = Square::make_square(Rank::from_index(0), File::from_index(5));
        assert_eq!(new_board.piece_on(f1), Some(Piece::Rook));
    }

    #[test]
    fn test_en_passant_capture() {
        init();
        // White pawn on e5, Black pawn just moved d7-d5
        let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1";
        let board = Board::from_str(fen).unwrap();
        let e5 = Square::make_square(Rank::from_index(4), File::from_index(4));
        let d6 = Square::make_square(Rank::from_index(5), File::from_index(3));
        let mv = ChessMove::new(e5, d6, None);
        let new_board = board.make_move_new(mv);
        assert_eq!(new_board.piece_on(d6), Some(Piece::Pawn));
        assert_eq!(new_board.color_on(d6), Some(Color::White));
        let d5 = Square::make_square(Rank::from_index(4), File::from_index(3));
        assert_eq!(new_board.piece_on(d5), None); // captured pawn removed
    }

    #[test]
    fn test_promotion() {
        init();
        let fen = "8/P7/8/8/8/8/8/K6k w - - 0 1";
        let board = Board::from_str(fen).unwrap();
        let a7 = Square::make_square(Rank::from_index(6), File::from_index(0));
        let a8 = Square::make_square(Rank::from_index(7), File::from_index(0));
        let mv = ChessMove::new(a7, a8, Some(Piece::Queen));
        let new_board = board.make_move_new(mv);
        assert_eq!(new_board.piece_on(a8), Some(Piece::Queen));
    }

    #[test]
    fn test_null_move() {
        init();
        let board = Board::default();
        let null = board.null_move();
        assert!(null.is_some());
        let null_board = null.unwrap();
        assert_eq!(null_board.side_to_move(), Color::Black);
    }

    #[test]
    fn test_null_move_in_check() {
        init();
        // White king in check
        let fen = "rnbqkbnr/pppp1ppp/4p3/8/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
        let board = Board::from_str(fen).unwrap();
        assert!(board.null_move().is_none());
    }

    #[test]
    fn test_hash_changes_on_move() {
        init();
        let board = Board::default();
        let e2 = Square::make_square(Rank::from_index(1), File::from_index(4));
        let e4 = Square::make_square(Rank::from_index(3), File::from_index(4));
        let new_board = board.make_move_new(ChessMove::new(e2, e4, None));
        assert_ne!(board.get_hash(), new_board.get_hash());
    }

    #[test]
    fn test_hash_consistency() {
        init();
        // Hash computed incrementally should match hash computed from scratch
        let board = Board::default();
        let e2 = Square::make_square(Rank::from_index(1), File::from_index(4));
        let e4 = Square::make_square(Rank::from_index(3), File::from_index(4));
        let new_board = board.make_move_new(ChessMove::new(e2, e4, None));
        let expected_hash = new_board.compute_hash();
        assert_eq!(new_board.get_hash(), expected_hash, "Incremental hash should match recomputed hash");
    }
}
// Board is a `Copy` type. Instead of a 64-element array of pieces, use bitboards: 6 `BitBoard`s
// for piece types and 2 for colors. To find on what's on a square, check which bitboards have that
// bit set. Slightly slower for single-square queries, but much faster for pattern matching.

// CASTLING_MASK -> instead of checking "did king or rook move" w/ compex conditionals, can use
// 64-element lookup table. When ANY piece moves from or to square X, AND the castling rights with
// `CASTLING_MASK[X]`. Most entries are 0xFF (no change), but king and rook starting squares have
// specific bits cleared. This also handles case where rook is captured (destination square clears
// the opponent's castling rights).

