use chull::ConvexHull;
use io::{BufRead, BufWriter, Write};
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use shakmaty::{
    attacks, Bitboard, Board, Castles, CastlingSide, Chess, Color, Move, MoveList, Position, Rank,
    Role, Setup, Square,
};
use std::{collections::HashSet, io};

fn main() {
    let stdin = io::stdin();
    let stdin = io::BufReader::new(stdin.lock());
    let mut game: Option<Chess> = None;

    dbg!(std::env::current_dir().unwrap());
    let f = match std::fs::File::create("pos.txt") {
        Ok(f) => f,
        Err(e) => panic!("file error: {}", e),
    };
    let mut dbg_write = BufWriter::new(f);

    let mut seen_positions = HashSet::new();

    for line in stdin.lines().filter_map(Result::ok) {
        let line = line.as_str();
        eprintln!("recv {}", &line);
        if line == "uci" {
            send("uciok");
        } else if let Some(pos) = line.strip_prefix("position startpos") {
            let mut parts = pos.split(" moves").skip(1);
            let raw_fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
            let moves = if let Some(moves) = parts.next() {
                moves
                    .split(' ')
                    .skip(1)
                    .map(|m| m.parse::<Uci>().unwrap())
                    .collect::<Vec<Uci>>()
            } else {
                vec![]
            };
            seen_positions = HashSet::new();
            setup_with_fen_and_moves(raw_fen, moves, &mut seen_positions, &mut game);
        } else if let Some(pos) = line.strip_prefix("position fen ") {
            let mut parts = pos.split(" moves");
            let raw_fen = parts.next().unwrap();
            let moves = if let Some(moves) = parts.next() {
                moves
                    .split(' ')
                    .skip(1)
                    .map(|m| m.parse::<Uci>().unwrap())
                    .collect::<Vec<Uci>>()
            } else {
                vec![]
            };
            seen_positions = HashSet::new();
            setup_with_fen_and_moves(raw_fen, moves, &mut seen_positions, &mut game);
        } else if line == "isready" {
            send("readyok");
        } else if line.starts_with("go ") {
            let game = game.as_mut().unwrap();
            let (m, score) = search(game, 6, &seen_positions, &mut dbg_write).unwrap();
            eprintln!("{} score {}", Uci::from_move(game, &m), score);
            send(&format!(
                "bestmove {}",
                Uci::from_move(game, &m).to_string()
            ));
        }
    }
}

fn setup_with_fen_and_moves(
    raw_fen: &str,
    moves: Vec<Uci>,
    seen_positions: &mut HashSet<Board>,
    game: &mut Option<Chess>,
) {
    let setup: Fen = raw_fen.parse().unwrap();
    let mut g: Chess = setup.position().unwrap();
    seen_positions.insert(g.board().clone());
    for m in moves {
        let m = m.to_move(&g).unwrap();
        g = g.play(&m).unwrap();
        seen_positions.insert(g.board().clone());
    }
    *game = Some(g);
}

fn search(
    pos: &Chess,
    depth: u8,
    seen_positions: &HashSet<Board>,
    dbg_write: &mut impl Write,
) -> Option<(Move, i32)> {
    let mut storage = Vec::new();
    do_search(
        pos,
        pos,
        SearchState::with_max_depth(depth),
        seen_positions,
        &mut storage,
        dbg_write,
    )
    .map(|(m, score)| (m.unwrap(), score))
}

struct SearchState {
    depth: u8,
    alpha: i32,
    beta: i32,
}

impl SearchState {
    fn with_max_depth(max_depth: u8) -> Self {
        Self {
            depth: max_depth,
            alpha: i32::MIN,
            beta: i32::MAX,
        }
    }
}

fn do_search(
    pos: &Chess,
    prev_pos: &Chess,
    state: SearchState,
    seen_positions: &HashSet<Board>,
    storage: &mut Vec<Vec<i32>>,
    dbg_write: &mut impl Write,
) -> Option<(Option<Move>, i32)> {
    let color = pos.turn();
    if state.depth == 0 {
        let score = mobility_score(prev_pos, pos);
        // writeln!(dbg_write, "{:?} {}", prev_moves, score).unwrap();
        return Some((None, score));
    }
    if pos.is_checkmate() {
        let score = color.fold(-10000 - state.depth as i32, 10000 + state.depth as i32);
        // writeln!(dbg_write, "{:?} {} mate", prev_moves, score).unwrap();
        return Some((None, score));
    }
    if pos.is_stalemate() {
        // writeln!(dbg_write, "{:?} 0.0, stale", prev_moves).unwrap();
        return Some((None, 0));
    }

    let mut alpha = state.alpha;
    let mut beta = state.beta;
    let mut best: Option<(Option<Move>, i32)> = None;

    let legals = pos.legals();

    let mut move_pos: Vec<(Move, Chess)> = legals
        .into_iter()
        .map(|m| {
            let chess = pos.clone().play(&m).unwrap();
            (m, chess)
        })
        .collect();
    move_pos.sort_by_cached_key(|(_, pos)| {
        if color.is_white() {
            -mobility_score(prev_pos, pos)
        } else {
            mobility_score(prev_pos, pos)
        }
    });
    for (m, new_pos) in move_pos {
        // prev_moves.push(Uci::from_move(pos, &m).to_string());
        let further_move = do_search(
            &new_pos,
            pos,
            SearchState {
                depth: state.depth - 1,
                alpha,
                beta,
            },
            seen_positions,
            storage,
            dbg_write,
        );
        // prev_moves.pop();
        if let Some((_, score)) = further_move {
            let score = if seen_positions.contains(new_pos.board()) {
                0
            } else {
                score
            };
            let new_best = best
                .as_ref()
                .map(|(_, best)| {
                    color.fold(score > *best, score < *best) || score == *best && rand::random()
                })
                .unwrap_or(true);
            if new_best {
                best = Some((Some(m), score));
            }
            if color.is_white() {
                alpha = alpha.max(score);
                if alpha > beta {
                    break;
                }
            } else {
                beta = beta.min(score);
                if beta < alpha {
                    break;
                }
            }
        }
    }

    // if let Some((_, score)) = best {
    //     writeln!(dbg_write, "{:?} {}", prev_moves, score).unwrap();
    // } else {
    //     writeln!(dbg_write, "{:?} none", prev_moves).unwrap();
    // }

    best
}

fn board_value(board: &Board) -> i32 {
    let white_pawns = board.pawns() & board.white();
    let white_value = (board.queens() & board.white()).count() * 9
        + (board.rooks() & board.white()).count() * 5
        + (board.bishops() & board.white()).count() * 3
        + (board.knights() & board.white()).count() * 3
        + (white_pawns & Bitboard::rank(Rank::Third)).count()
        + (white_pawns & Bitboard::rank(Rank::Fourth)).count()
        + (white_pawns & Bitboard::rank(Rank::Fifth)).count()
        + (white_pawns & Bitboard::rank(Rank::Sixth)).count() * 2
        + (white_pawns & Bitboard::rank(Rank::Seventh)).count() * 2;
    let black_pawns = board.pawns() & board.black();
    let black_value = (board.queens() & board.black()).count() * 9
        + (board.rooks() & board.black()).count() * 5
        + (board.bishops() & board.black()).count() * 3
        + (board.knights() & board.black()).count() * 3
        + (black_pawns & Bitboard::rank(Rank::Sixth)).count()
        + (black_pawns & Bitboard::rank(Rank::Fifth)).count()
        + (black_pawns & Bitboard::rank(Rank::Fourth)).count()
        + (black_pawns & Bitboard::rank(Rank::Third)).count() * 2
        + (black_pawns & Bitboard::rank(Rank::Second)).count() * 2;

    white_value as i32 - black_value as i32
}

fn mobility_score(prev_pos: &Chess, new_pos: &Chess) -> i32 {
    let mut move_list = MoveList::new();
    legal_moves_ignoreing_check(prev_pos, &mut move_list);
    let prev_moves = move_list.len() as i32;
    move_list.clear();
    legal_moves_ignoreing_check(new_pos, &mut move_list);
    let moves = move_list.len() as i32;

    let board_value = board_value(new_pos.board());

    new_pos.turn().fold(moves - prev_moves, prev_moves - moves) + board_value * 2
}

fn mobility_space_score(prev_pos: &Chess, new_pos: &Chess, storage: &mut Vec<Vec<i32>>) -> i32 {
    let mut move_list = MoveList::new();

    move_list.clear();
    legal_moves_ignoreing_check(prev_pos, &mut move_list);
    let prev_moves = move_list.len() as i32;
    move_list.clear();
    legal_moves_ignoreing_check(new_pos, &mut move_list);
    let moves = move_list.len() as i32;

    // let board_value = board_value(new_pos.board());

    let white_coords = new_pos
        .board()
        .pieces()
        .filter(|(_, piece)| piece.color.is_white())
        .map(|(square, _)| vec![i32::from(square.file()), i32::from(square.rank())]);
    storage.clear();
    storage.extend(white_coords);
    let white_hull = ConvexHull::try_new(&storage, 0, None)
        .map(|hull| hull.volume())
        .unwrap_or(0);
    let black_coords = new_pos
        .board()
        .pieces()
        .filter(|(_, piece)| piece.color.is_black())
        .map(|(square, _)| vec![i32::from(square.file()), i32::from(square.rank())]);
    storage.clear();
    storage.extend(black_coords);
    let black_hull = ConvexHull::try_new(&storage, 0, None)
        .map(|hull| hull.volume())
        .unwrap_or(0);
    let volume_val = white_hull - black_hull;

    (new_pos.turn().fold(moves - prev_moves, prev_moves - moves) as f32 * 1.5) as i32 + volume_val
}

fn legal_moves_ignoreing_check(pos: &Chess, moves: &mut MoveList) {
    let king = pos
        .board()
        .king_of(pos.turn())
        .expect("king in standard chess");

    let has_ep = gen_en_passant(pos.board(), pos.turn(), pos.ep_square(), moves);

    {
        let target = !pos.us();
        gen_non_king(pos, target, moves);
        gen_safe_king(pos, king, target, moves);
        gen_castling_moves(pos, &pos.castles(), king, CastlingSide::KingSide, moves);
        gen_castling_moves(pos, &pos.castles(), king, CastlingSide::QueenSide, moves);
    }

    let blockers = slider_blockers(pos.board(), pos.them(), king);
    if blockers.any() || has_ep {
        let mut i = 0;
        while i < moves.len() {
            if is_safe(pos, king, &moves[i], blockers) {
                i += 1;
            } else {
                moves.swap_remove(i);
            }
        }
    }
}

fn send(msg: &str) {
    eprintln!("send {}", &msg);
    println!("{}", msg);
}

// ======

fn push_promotions(moves: &mut MoveList, from: Square, to: Square, capture: Option<Role>) {
    moves.push(Move::Normal {
        role: Role::Pawn,
        from,
        capture,
        to,
        promotion: Some(Role::Queen),
    });
    moves.push(Move::Normal {
        role: Role::Pawn,
        from,
        capture,
        to,
        promotion: Some(Role::Rook),
    });
    moves.push(Move::Normal {
        role: Role::Pawn,
        from,
        capture,
        to,
        promotion: Some(Role::Bishop),
    });
    moves.push(Move::Normal {
        role: Role::Pawn,
        from,
        capture,
        to,
        promotion: Some(Role::Knight),
    });
}

fn gen_en_passant(
    board: &Board,
    turn: Color,
    ep_square: Option<Square>,
    moves: &mut MoveList,
) -> bool {
    let mut found = false;

    if let Some(to) = ep_square {
        for from in board.pawns() & board.by_color(turn) & attacks::pawn_attacks(!turn, to) {
            moves.push(Move::EnPassant { from, to });
            found = true;
        }
    }

    found
}

fn slider_blockers(board: &Board, enemy: Bitboard, king: Square) -> Bitboard {
    let snipers = (attacks::rook_attacks(king, Bitboard(0)) & board.rooks_and_queens())
        | (attacks::bishop_attacks(king, Bitboard(0)) & board.bishops_and_queens());

    let mut blockers = Bitboard(0);

    for sniper in snipers & enemy {
        let b = attacks::between(king, sniper) & board.occupied();

        if !b.more_than_one() {
            blockers.add(b);
        }
    }

    blockers
}

fn is_safe<P: Position>(pos: &P, king: Square, m: &Move, blockers: Bitboard) -> bool {
    match *m {
        Move::Normal { from, to, .. } => {
            !blockers.contains(from) || attacks::aligned(from, to, king)
        }
        Move::EnPassant { from, to } => {
            let mut occupied = pos.board().occupied();
            occupied.toggle(from);
            occupied.toggle(to.with_rank_of(from)); // captured pawn
            occupied.add(to);

            (attacks::rook_attacks(king, occupied) & pos.them() & pos.board().rooks_and_queens())
                .is_empty()
                && (attacks::bishop_attacks(king, occupied)
                    & pos.them()
                    & pos.board().bishops_and_queens())
                .is_empty()
        }
        _ => true,
    }
}

fn gen_non_king<P: Position>(pos: &P, target: Bitboard, moves: &mut MoveList) {
    gen_pawn_moves(pos, target, moves);
    KnightTag::gen_moves(pos, target, moves);
    BishopTag::gen_moves(pos, target, moves);
    RookTag::gen_moves(pos, target, moves);
    QueenTag::gen_moves(pos, target, moves);
}

fn gen_safe_king<P: Position>(pos: &P, king: Square, target: Bitboard, moves: &mut MoveList) {
    for to in attacks::king_attacks(king) & target {
        if pos
            .board()
            .attacks_to(to, !pos.turn(), pos.board().occupied())
            .is_empty()
        {
            moves.push(Move::Normal {
                role: Role::King,
                from: king,
                capture: pos.board().role_at(to),
                to,
                promotion: None,
            });
        }
    }
}

fn gen_castling_moves<P: Position>(
    pos: &P,
    castles: &Castles,
    king: Square,
    side: CastlingSide,
    moves: &mut MoveList,
) {
    if let Some(rook) = castles.rook(pos.turn(), side) {
        let path = castles.path(pos.turn(), side);
        if (path & pos.board().occupied()).any() {
            return;
        }

        let king_to = side.king_to(pos.turn());
        let king_path = attacks::between(king, king_to).with(king);
        for sq in king_path {
            if pos
                .king_attackers(sq, !pos.turn(), pos.board().occupied() ^ king)
                .any()
            {
                return;
            }
        }

        if pos
            .king_attackers(
                king_to,
                !pos.turn(),
                pos.board().occupied() ^ king ^ rook ^ side.rook_to(pos.turn()),
            )
            .any()
        {
            return;
        }

        moves.push(Move::Castle { king, rook });
    }
}

fn gen_pawn_moves<P: Position>(pos: &P, target: Bitboard, moves: &mut MoveList) {
    let seventh = pos.our(Role::Pawn) & Bitboard::relative_rank(pos.turn(), Rank::Seventh);

    for from in pos.our(Role::Pawn) & !seventh {
        for to in attacks::pawn_attacks(pos.turn(), from) & pos.them() & target {
            moves.push(Move::Normal {
                role: Role::Pawn,
                from,
                capture: pos.board().role_at(to),
                to,
                promotion: None,
            });
        }
    }

    for from in seventh {
        for to in attacks::pawn_attacks(pos.turn(), from) & pos.them() & target {
            push_promotions(moves, from, to, pos.board().role_at(to));
        }
    }

    let single_moves = pos.our(Role::Pawn).relative_shift(pos.turn(), 8) & !pos.board().occupied();

    let double_moves = single_moves.relative_shift(pos.turn(), 8)
        & Bitboard::relative_rank(pos.turn(), Rank::Fourth)
            .with(Bitboard::relative_rank(pos.turn(), Rank::Third))
        & !pos.board().occupied();

    for to in single_moves & target & !Bitboard::BACKRANKS {
        if let Some(from) = to.offset(pos.turn().fold(-8, 8)) {
            moves.push(Move::Normal {
                role: Role::Pawn,
                from,
                capture: None,
                to,
                promotion: None,
            });
        }
    }

    for to in single_moves & target & Bitboard::BACKRANKS {
        if let Some(from) = to.offset(pos.turn().fold(-8, 8)) {
            push_promotions(moves, from, to, None);
        }
    }

    for to in double_moves & target {
        if let Some(from) = to.offset(pos.turn().fold(-16, 16)) {
            moves.push(Move::Normal {
                role: Role::Pawn,
                from,
                capture: None,
                to,
                promotion: None,
            });
        }
    }
}

trait Stepper {
    const ROLE: Role;

    fn attacks(from: Square) -> Bitboard;

    fn gen_moves<P: Position>(pos: &P, target: Bitboard, moves: &mut MoveList) {
        for from in pos.our(Self::ROLE) {
            for to in Self::attacks(from) & target {
                moves.push(Move::Normal {
                    role: Self::ROLE,
                    from,
                    capture: pos.board().role_at(to),
                    to,
                    promotion: None,
                });
            }
        }
    }
}

trait Slider {
    const ROLE: Role;
    fn attacks(from: Square, occupied: Bitboard) -> Bitboard;

    fn gen_moves<P: Position>(pos: &P, target: Bitboard, moves: &mut MoveList) {
        for from in pos.our(Self::ROLE) {
            for to in Self::attacks(from, pos.board().occupied()) & target {
                moves.push(Move::Normal {
                    role: Self::ROLE,
                    from,
                    capture: pos.board().role_at(to),
                    to,
                    promotion: None,
                });
            }
        }
    }
}

enum KnightTag {}
enum BishopTag {}
enum RookTag {}
enum QueenTag {}

impl Stepper for KnightTag {
    const ROLE: Role = Role::Knight;
    fn attacks(from: Square) -> Bitboard {
        attacks::knight_attacks(from)
    }
}

impl Slider for BishopTag {
    const ROLE: Role = Role::Bishop;
    fn attacks(from: Square, occupied: Bitboard) -> Bitboard {
        attacks::bishop_attacks(from, occupied)
    }
}

impl Slider for RookTag {
    const ROLE: Role = Role::Rook;
    fn attacks(from: Square, occupied: Bitboard) -> Bitboard {
        attacks::rook_attacks(from, occupied)
    }
}

impl Slider for QueenTag {
    const ROLE: Role = Role::Queen;
    fn attacks(from: Square, occupied: Bitboard) -> Bitboard {
        attacks::queen_attacks(from, occupied)
    }
}
