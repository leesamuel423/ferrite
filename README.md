# Ferrite

**A UCI-compatible chess engine written in Rust.**

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)
![UCI](https://img.shields.io/badge/Protocol-UCI-blue)
![PeSTO](https://img.shields.io/badge/Eval-PeSTO-green)
![Syzygy](https://img.shields.io/badge/Endgame-Syzygy-purple)

Ferrite implements the core ideas behind modern computer chess: bitboard representation, magic bitboard move generation, alpha-beta search with pruning heuristics, PeSTO tapered evaluation, and Syzygy endgame tablebase support. It communicates via the Universal Chess Interface (UCI) protocol, making it compatible with any standard chess GUI.

---

## Features

- **Bitboard representation** — 8 bitboards (6 piece types + 2 colors) encode the entire position using CPU-native `u64` operations
- **Magic bitboards** — O(1) slider attack lookups via precomputed hash tables with collision-free magic numbers
- **16-bit move encoding** — compact `ChessMove(u16)` for cache-friendly move lists and single-integer comparison
- **Zobrist hashing** — O(1) incremental hash updates for transposition table and repetition detection
- **PeSTO tapered evaluation** — separate midgame/endgame piece-square tables blended by game phase
- **Iterative deepening negamax** — alpha-beta pruning with time-controlled deepening
- **Null Move Pruning (NMP)** — skip-turn heuristic with zugzwang guard
- **Late Move Reductions (LMR)** — search late quiet moves at reduced depth first
- **Quiescence search** — resolve captures at leaf nodes to avoid the horizon effect
- **Move ordering** — hash move, MVV-LVA captures, killer moves, history heuristic
- **Transposition table** — Zobrist-indexed with depth-preferred replacement and aging
- **Syzygy endgame tablebases** — perfect play for positions with 5 or fewer pieces
- **BK tactical test suite** — 24-position test suite with EPD parser and SAN converter

---

## Getting Started

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (edition 2024)

### Build & Run

```bash
git clone https://github.com/yourusername/ferrite.git
cd ferrite

# Debug build
make build

# Optimized release build (LTO + single codegen unit)
make release

# Run the engine (release mode)
make run
```

### Makefile Targets

| Target      | Command                        | Description                       |
| ----------- | ------------------------------ | --------------------------------- |
| `build`     | `cargo build`                  | Debug build                       |
| `release`   | `cargo build --release`        | Optimized release build           |
| `run`       | `cargo run --release`          | Run engine in UCI mode            |
| `test`      | `cargo test`                   | Run all unit + integration tests  |
| `bench`     | `cargo bench --bench ...`      | Run Criterion benchmarks          |
| `clippy`    | `cargo clippy -- -D warnings`  | Lint with Clippy                  |
| `fmt`       | `cargo fmt`                    | Format code                       |
| `fmt-check` | `cargo fmt -- --check`         | Check formatting                  |
| `clean`     | `cargo clean`                  | Remove build artifacts            |
| `ci`        | `fmt-check clippy test bench`  | Full CI pipeline                  |

### Connecting to a GUI

Ferrite speaks UCI. Point any UCI-compatible GUI at the compiled binary:

- **[Arena](http://www.playwitharena.de/)** — Engines > Install New Engine > select `target/release/ferrite`
- **[CuteChess](https://cutechess.com/)** — Tools > Settings > Engines > Add > set command to the binary path
- **[Lichess (via lichess-bot)](https://github.com/lichess-bot-devs/lichess-bot)** — configure `engine.dir` and `engine.name` in `config.yml`

---

## Architecture Overview

### Search Flow

```
UCI "go" command
    │
    ▼
┌──────────────────────┐
│  Iterative Deepening │ ◄── depth 1, 2, 3, ...
│  (search.rs:125)     │     each iteration warms TT for next
└────────┬─────────────┘
         │
         ▼
┌──────────────────────┐     ┌──────────────┐
│  Negamax + α-β       │────►│  TT Probe    │ hit? → return stored score
│  (search.rs:190)     │     └──────────────┘
│                      │     ┌──────────────┐
│                      │────►│  Syzygy      │ ≤5 pieces? → perfect score
│                      │     └──────────────┘
│                      │     ┌──────────────┐
│  if can_null &&      │────►│  Null Move   │ skip turn; still ≥ β? → prune
│  depth ≥ 3           │     │  Pruning     │
│                      │     └──────────────┘
│                      │     ┌──────────────┐
│  for each move:      │────►│  Move Order  │ hash → MVV-LVA → killers
│                      │     │              │ → history → quiet
│  if late + quiet:    │     └──────────────┘
│    LMR (depth-2)     │
│    re-search if >α   │
└────────┬─────────────┘
         │ depth == 0
         ▼
┌──────────────────────┐
│  Quiescence Search   │ ◄── resolve captures to avoid horizon effect
│  (search.rs:374)     │     stand-pat + capture-only search
└────────┬─────────────┘
         │
         ▼
┌──────────────────────┐
│  PeSTO Evaluation    │ ◄── tapered MG/EG piece-square tables
│  (evaluation.rs:32)  │
└──────────────────────┘
```

### Data Flow

```
UCI command  ──►  parse position/FEN  ──►  Board struct (bitboards + Zobrist)
                                              │
"go depth N" ──►  SearchState.reset() ──►  iterative_deepening(1..N)
                                              │
                                              ▼
                                         negamax(board, depth, α, β)
                                              │
                                              ▼
                                         "bestmove e2e4"  ──►  UCI output
```

---

## Technical Deep-Dive

### Board Representation: Why Bitboards?

A chess board has 64 squares. A `u64` has 64 bits. This is not a coincidence — it's the foundation of every competitive chess engine.

**`BitBoard(u64)`** ([`src/board/bitboard.rs`](src/board/bitboard.rs)) wraps a single `u64` where each bit represents a square. Setting bit 0 means "A1 is in this set", bit 63 means "H8 is in this set". This is the **LERF (Little-Endian Rank-File)** mapping:

```
A1=0, B1=1, ... H1=7
A2=8, B2=9, ... H2=15
...
A8=56, B8=57, ... H8=63
```

From any square index: `rank = index >> 3`, `file = index & 7`. Shifting left by 8 moves "one rank up" — a natural fit for pawn pushes.

**Why this matters:** Questions like "Where can this knight move?" become a single bitwise AND:

```rust
let targets = KNIGHT_ATTACKS[sq] & !own_pieces;  // one AND + one NOT on u64s
```

The engine uses **8 bitboards total**: 6 piece types (pawn, knight, bishop, rook, queen, king) + 2 colors (white, black). Any positional query is a combination of bitwise operations on these.

**Brian Kernighan's bit trick** powers the iterator — clearing the lowest set bit in O(1):

```rust
let idx = self.0.trailing_zeros() as u8;  // find lowest set bit
self.0 &= self.0 - 1;                     // clear it
```

This iterates over N set bits in exactly N steps, with no branches beyond the loop termination check.

---

### Move Encoding: 16-bit Compact Moves

**`ChessMove(u16)`** ([`src/board/chessmove.rs`](src/board/chessmove.rs)) packs a full chess move into 16 bits:

```
Bit layout: src(6) | dst(6) | promo(2) | is_promo(1) | reserved(1)
  bits 0..5:   source square (0-63)
  bits 6..11:  destination square (0-63)
  bits 12..13: promotion piece (0=Knight, 1=Bishop, 2=Rook, 3=Queen)
  bit 14:      is_promotion flag
  bit 15:      reserved
```

**Why 16 bits?** Two reasons:

1. **Cache efficiency** — A move list of 256 moves fits in 512 bytes (< 8 cache lines). With 32-bit moves, that doubles. In a search that examines millions of positions, this matters.
2. **Integer comparison** — Equality checks (`a == b`) compile to a single `cmp` instruction. No field-by-field comparison needed.

The `Copy` trait is derived, so moves are passed by value with zero overhead — they're just integers.

---

### Attack Tables & Magic Bitboards

([`src/board/attacks.rs`](src/board/attacks.rs), [`src/board/magic.rs`](src/board/magic.rs))

**Leaper attacks** (knight, king, pawn) are simple: precompute a 64-entry array for each piece type at startup. `KNIGHT_ATTACKS[sq]` gives the attack bitboard for a knight on square `sq` regardless of board state. Knight and king attacks depend only on the piece's square, not on other pieces.

**Slider attacks** (bishop, rook, queen) depend on which squares between the slider and the board edge are occupied by other pieces ("blockers"). A naive implementation ray-traces in each direction until hitting a blocker — O(7) operations per direction. Magic bitboards reduce this to **O(1)** via a precomputed hash table:

```
index = (occupied & mask).wrapping_mul(magic) >> shift
attacks = TABLE[offset + index]
```

The key insight: for a rook on a given square, only the pieces *between* the rook and the edge matter. Pieces on the edge itself don't change the attack (the ray ends there regardless). **Excluding edge squares** from the occupancy mask reduces the number of relevant bits — for example, a rook in the center has 10 relevant bits (not 14), giving a table of 1,024 entries instead of 16,384.

**Finding magic numbers** uses the carry-rippler trick to enumerate all 2^N subsets of a mask (`occ = (occ - mask) & mask`), then tries sparse random candidates from an XorShift64 PRNG until finding one that produces zero collisions:

```rust
let magic = rng.sparse_random();  // rng.next() & rng.next() & rng.next()
// Sparse numbers (few bits set) work better as magic multipliers
```

The PRNG uses a **fixed seed** (`0x12345678_9ABCDEF0`) for deterministic initialization — every run of the engine produces the same magic numbers, making debugging reproducible.

---

### Zobrist Hashing

([`src/board/zobrist.rs`](src/board/zobrist.rs))

Zobrist hashing assigns a random 64-bit key to every (piece, color, square) combination, plus keys for side-to-move, castling rights, and en passant file. A position's hash is the XOR of all applicable keys.

**Why XOR?** It's its own inverse: `hash ^= key; hash ^= key;` restores the original hash. This means:

- **Making a move** = XOR out the piece from source square, XOR it in at destination, XOR the side key. O(1) per move, regardless of board complexity.
- **Unmaking a move** = apply the same XORs (self-inverse).

Components: `piece[6][2][64]` + `side` + `castling[16]` + `ep[8]` = 781 random keys, generated by a XorShift64 PRNG with fixed seed `0x3243F6A8885A308D` for determinism. Keys are lazily initialized via `LazyLock`.

---

### Evaluation: PeSTO Tapered Eval

([`src/evaluation.rs`](src/evaluation.rs), [`src/pst.rs`](src/pst.rs))

The evaluation function uses the **PeSTO (Piece-Square Tables Only)** approach — a well-known evaluation framework that assigns each piece a positional bonus/penalty depending on which square it occupies.

**Material values** differ between midgame and endgame:

| Piece  | Midgame | Endgame |
| ------ | ------- | ------- |
| Pawn   | 82      | 94      |
| Knight | 337     | 281     |
| Bishop | 365     | 297     |
| Rook   | 477     | 512     |
| Queen  | 1025    | 936     |

**Piece-square tables** provide 64-entry bonus/penalty arrays for each piece type, separately for midgame and endgame. For example, knights are rewarded for being centralized in the midgame, while kings are penalized for leaving the back rank.

**Tapered evaluation** blends the two:

```
phase = sum of PHASE_WEIGHT[piece] for all pieces on board
      = 0*pawns + 1*knights + 1*bishops + 2*rooks + 4*queens

score = (mg_score * phase + eg_score * (24 - phase)) / 24
```

With all minor/major pieces present, `phase = 24` (full midgame). As pieces are traded, phase decreases toward 0 (pure endgame). This elegantly handles the transition: king safety dominates in the midgame, while king activity and pawn structure matter in the endgame.

**PST indexing quirk:** PeSTO tables store values with a8=index 0, but Ferrite uses A1=0 (LERF). The fix: White reads `table[sq ^ 56]` (flips rank), Black reads `table[sq]` directly.

---

### Search Algorithm

([`src/search.rs`](src/search.rs))

#### Iterative Deepening

The search starts at depth 1 and increases: 1, 2, 3, ... up to `max_depth`. This seems wasteful, but has two critical benefits:

1. **Time control** — The last *completed* iteration's result is always valid. If time runs out mid-iteration, we use the previous result.
2. **TT warmup** — Each iteration populates the transposition table, making the next iteration dramatically faster (TT hits provide instant score lookups).

A **soft time limit** (50% of allocated time) prevents starting an iteration that likely won't finish.

#### Negamax with Alpha-Beta Pruning

Negamax is a simplification of minimax: instead of alternating between maximizing and minimizing, always maximize from the current player's perspective by negating the child's score.

Alpha-beta pruning skips branches that cannot possibly improve the result. If we've found a move scoring 5, and a sibling branch already guarantees our opponent can force a score of 3 in a different line, we don't need to explore that branch further (**beta cutoff**).

#### Null Move Pruning (NMP)

"If I skip my turn and my position is *still* great, then with a real move it must be even better."

The engine makes a "null move" (passes the turn) and searches at reduced depth (depth - 3). If the reduced search still returns a score >= beta, we assume the real search would too, and prune the whole subtree.

**Zugzwang guard:** NMP is disabled when the side to move has only pawns and a king (king + pawns positions are the most common zugzwang scenarios where being forced to move is a disadvantage).

#### Late Move Reductions (LMR)

Moves are ordered so that the best-looking ones come first. Moves later in the list are statistically less likely to be good. LMR exploits this:

- After searching the first 3 moves at full depth, subsequent quiet (non-capture, non-check) moves are searched at **depth - 2** first.
- If a reduced-depth search finds a score above alpha, the move is **re-searched at full depth**.
- Captures, checks, killer moves, and moves while in check are never reduced.

This typically reduces the search tree by 30-50% with minimal impact on playing strength.

#### Quiescence Search

At leaf nodes (depth 0), simply evaluating the position can be misleading — what if we're about to lose a queen on the next move? This is the **horizon effect**.

Quiescence search extends the search by examining all captures (and all evasions when in check) until the position is "quiet." The **stand-pat** heuristic uses the static evaluation as a lower bound: if the position is already good enough, we don't need to search further captures.

---

### Move Ordering

([`src/movegen.rs`](src/movegen.rs))

Alpha-beta pruning is most effective when the best move is searched first. Move ordering is therefore critical to search performance. The priority hierarchy:

| Priority | Source          | Score   | Rationale                                       |
| -------- | --------------- | ------- | ----------------------------------------------- |
| 1        | Hash move (TT)  | 100,000 | Best move from previous search of this position  |
| 2        | Captures (MVV-LVA) | 10,000+ | Most Valuable Victim, Least Valuable Attacker  |
| 3        | Promotions      | 9,000   | Creating a new queen is almost always good       |
| 4        | Killer move #1  | 8,000   | Quiet move that caused a beta cutoff at this ply |
| 5        | Killer move #2  | 7,000   | Second-best quiet cutoff move at this ply        |
| 6        | History score   | 0-16,384| Quiet moves that frequently cause cutoffs        |
| 7        | Other quiet     | 0       | Remaining moves                                  |

**MVV-LVA (Most Valuable Victim, Least Valuable Attacker):** Captures are scored by `victim_value * 10 - attacker_index`. Capturing a queen with a pawn (QxP: 900*10 - 0 = 9000) scores higher than capturing a pawn with a queen (PxQ: 100*10 - 4 = 996). This encourages winning captures and penalizes trades that lose material.

**Killer heuristic:** Two slots per ply store quiet moves that caused beta cutoffs. When searching a sibling position at the same depth, these "killer moves" are tried before other quiet moves. The intuition: if a move refuted one position, it might refute a nearby position too.

**History heuristic:** A 6x64 table indexed by `[piece][destination_square]` accumulates `depth^2` bonuses whenever a quiet move causes a beta cutoff. This builds a per-search "reputation" for effective quiet moves. The score is capped at 16,384 to prevent overflow.

---

### Transposition Table

([`src/tt.rs`](src/tt.rs))

The transposition table (TT) is a hash map indexed by Zobrist hash, storing the result of previously searched positions. In chess, many different move sequences lead to the same position (transpositions), and the TT avoids re-searching them.

**Entry structure (per position):**

| Field      | Type       | Purpose                                         |
| ---------- | ---------- | ----------------------------------------------- |
| `key`      | `u64`      | Full Zobrist hash for collision detection        |
| `depth`    | `u8`       | Search depth that produced this result           |
| `score`    | `Score`    | Evaluation score                                 |
| `flag`     | `TTFlag`   | Exact, LowerBound (beta cutoff), or UpperBound  |
| `best_move`| `Option<ChessMove>` | Best move found (used for move ordering) |
| `age`      | `u8`       | Search generation for aging                      |

**Sizing:** The table uses power-of-2 sizing so that the index computation is a fast bitwise AND (`hash as usize & mask`) instead of an expensive modulo operation. Default size is 64 MB, configurable via the UCI `Hash` option (1-4096 MB).

**Replacement policy:** Depth-preferred with aging. An entry is replaced if:
- The slot is empty (`key == 0`)
- It's the same position (update with newer data)
- The new search is deeper (`depth >= entry.depth`)
- The existing entry is stale (`entry.age != current_generation`)

**Mate score adjustment:** Mate scores are ply-dependent (mate-in-3 from the root is different from mate-in-3 from ply 5). When storing, scores are adjusted to be relative to the root; when probing, they're adjusted back to the current ply.

---

### Syzygy Endgame Tablebases

([`src/syzygy.rs`](src/syzygy.rs))

Syzygy tablebases contain precomputed perfect-play results for all positions with a given number of pieces (up to 5 in this engine). When the search reaches a position with 5 or fewer pieces, it probes the tablebase for an authoritative Win/Draw/Loss result instead of searching further.

**Bridge implementation:** Ferrite's `Board` type is different from `shakmaty`'s `Chess` type, so the bridge converts via FEN string:

```
Board → FEN string → shakmaty::Fen → shakmaty::Chess → Syzygy probe
```

This FEN round-trip has negligible cost because it only runs for positions with 5 or fewer pieces — by definition, simple positions with short FEN strings.

**WDL scoring:**

| Tablebase Result | Engine Score |
| ---------------- | ------------ |
| Win              | +20,000      |
| Cursed Win       | +100         |
| Draw             | 0            |
| Blessed Loss     | -100         |
| Loss             | -20,000      |

Tablebases are loaded from disk via the UCI `SyzygyPath` option. The engine ships with 3-4-5 piece tables in `endgame/syzgy-3-4-5/`.

---

## Benchmarks

All benchmarks run on a release build (`cargo build --release` with LTO and single codegen unit) using [Criterion.rs](https://github.com/bheisler/criterion.rs).

### Search Performance

| Benchmark                  | Time (mean)  | Description                                    |
| -------------------------- | ------------ | ---------------------------------------------- |
| `search_depth_3_startpos`  | 1.39 ms      | Depth 3 from starting position                 |
| `search_depth_4_startpos`  | 2.66 ms      | Depth 4 from starting position                 |
| `search_depth_3_kiwipete`  | 11.31 ms     | Depth 3 from KiwiPete (48 legal moves)         |

### Move Generation

| Benchmark            | Time (mean) | Description                              |
| -------------------- | ----------- | ---------------------------------------- |
| `movegen_startpos`   | 360 ns      | Generate all legal moves from startpos   |
| `movegen_kiwipete`   | 777 ns      | Generate all legal moves from KiwiPete   |

### Evaluation

| Benchmark         | Time (mean) | Description                                        |
| ----------------- | ----------- | -------------------------------------------------- |
| `eval_startpos`   | 100 ns      | Evaluate starting position                         |
| `eval_middlegame` | 105 ns      | Evaluate complex middlegame (Italian Game)         |
| `eval_endgame`    | 98 ns       | Evaluate K+R vs K endgame                          |
| `eval_complex`    | 110 ns      | Evaluate Sicilian Dragon middlegame                |

### Positions Used

| Name       | FEN                                                                   | Description                    |
| ---------- | --------------------------------------------------------------------- | ------------------------------ |
| Startpos   | `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1`         | Initial position (20 moves)    |
| KiwiPete   | `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -` | 48 legal moves, complex tactics|
| Endgame    | `8/5k2/8/8/8/8/4K3/4R3 w - - 0 1`                                    | K+R vs K                       |
| Complex    | `r1bq1rk1/pp2ppbp/2np2p1/2n5/P3PP2/N1P2N2/1PB3PP/R1B1QRK1 b - -`   | Sicilian Dragon middlegame     |

---

## Testing

Ferrite has **98 tests** across three test targets:

```bash
# Run all tests
make test

# Run only unit tests
cargo test --lib

# Run only the BK tactical suite
cargo test --test bk_suite
```

### Test Coverage by Module

| Module      | Tests | Covers                                                          |
| ----------- | ----- | --------------------------------------------------------------- |
| `bitboard`  | 10    | Construction, popcount, iteration, bitwise ops                  |
| `board`     | 13    | FEN parsing, make/unmake, castling, en passant, promotion, hash |
| `chessmove` | 5     | Encoding/decoding, roundtrip for all 64x64 squares             |
| `attacks`   | 12    | Leaper + slider tables, blockers, all-squares verification      |
| `movegen`   | 11    | Perft depths 1-4, KiwiPete, Position3, iterator masks           |
| `square`    | 4     | LERF mapping, rank/file roundtrip, display                      |
| `piece`     | 3     | Color flip, index mapping                                       |
| `zobrist`   | 3     | Non-zero keys, uniqueness, XOR cancellation                     |
| `evaluation`| 5     | Startpos near-zero, material advantage, endgame phase           |
| `search`    | 11    | Mate-in-1, depth completion, TT speedup, draw detection, PV    |
| `tt`        | 4     | Store/probe, miss, mate adjustment, replacement policy          |
| `syzygy`    | 2     | Invalid path, piece count guard                                 |
| `uci`       | 11    | Position parsing, go params, time allocation, promotions        |
| `bk_suite`  | 3     | EPD parser, SAN conversion, 24-position tactical suite          |

### Perft Verification

Move generation is verified against known perft results (total leaf nodes at a given depth):

| Depth | Startpos     | KiwiPete      | Position 3    |
| ----- | ------------ | ------------- | ------------- |
| 1     | 20           | 48            | 14            |
| 2     | 400          | 2,039         | 191           |
| 3     | 8,902        | 97,862        | 2,812         |
| 4     | 197,281      | —             | —             |

---

## UCI Protocol

### Supported Commands

| Command                           | Description                                    |
| --------------------------------- | ---------------------------------------------- |
| `uci`                             | Identify engine, list options, print `uciok`   |
| `isready`                         | Synchronize; responds `readyok`                |
| `ucinewgame`                      | Reset board, clear TT                          |
| `position startpos [moves ...]`   | Set position from starting position            |
| `position fen <FEN> [moves ...]`  | Set position from FEN string                   |
| `go depth <N>`                    | Search to fixed depth                          |
| `go movetime <ms>`                | Search for fixed time                          |
| `go wtime/btime/winc/binc [...]`  | Search with time control                       |
| `go infinite`                     | Search until `stop`                            |
| `stop`                            | Halt search, return best move found            |
| `setoption name Hash value <MB>`  | Set transposition table size (1-4096 MB)       |
| `setoption name SyzygyPath value <path>` | Load Syzygy tablebases from directory  |
| `d` / `print`                     | Print current board (debug)                    |
| `quit`                            | Exit engine                                    |

### Configuration Options

| Option       | Type   | Default   | Range      | Description                 |
| ------------ | ------ | --------- | ---------- | --------------------------- |
| `Hash`       | spin   | 64        | 1-4096     | TT size in MB               |
| `SyzygyPath` | string | `<empty>` | —          | Path to Syzygy tablebase dir|

### Example Session

```
> uci
< id name chess-engine
< id author yourname
< option name Hash type spin default 64 min 1 max 4096
< option name SyzygyPath type string default <empty>
< uciok

> isready
< readyok

> position startpos moves e2e4 e7e5
> go depth 6
< info depth 1 score cp 5 nodes 27 time 1 nps 27000 pv d2d4
< info depth 2 score cp 20 nodes 178 time 1 nps 178000 pv d2d4 d7d5
< ...
< info depth 6 score cp 15 nodes 45231 time 24 nps 1884625 pv g1f3 b8c6
< bestmove g1f3

> quit
```

## Dependencies

| Crate             | Version | Purpose                                          |
| ----------------- | ------- | ------------------------------------------------ |
| `shakmaty`        | 0.27    | Chess position type for Syzygy bridge            |
| `shakmaty-syzygy` | 0.25    | Syzygy endgame tablebase probing                 |
| `arrayvec`        | 0.7     | Stack-allocated move lists (no heap allocation)  |
| `criterion`       | 0.5     | Benchmarking framework (dev-dependency)          |

---

## License & Credits

- **PeSTO evaluation tables** from [PeSTO's Evaluation Function](https://www.chessprogramming.org/PeSTO%27s_Evaluation_Function) — empirically tuned piece-square tables by Ronald Friederich
- **Syzygy tablebases** by Ronald de Man — endgame truth tables probed via `shakmaty-syzygy`
- **BK test suite** — classic 24-position tactical benchmark by Brat-Ko
