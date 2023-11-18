mod mid_scorer;
mod pieces;
mod scorer;

use io::BufRead;
use mid_scorer::mobility_score;
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use shakmaty::{Board, Chess, Move, Position, Setup};
use std::io::BufWriter;
use std::io::Write;
use std::{collections::HashSet, io};

use scorer::board_score;

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

    for line in stdin.lines().take_while(Result::is_ok).map(Result::unwrap) {
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
            dbg_write.flush().unwrap();
            eprintln!("{} score {}", Uci::from_move(game, &m), score);
            send(&format!("bestmove {}", Uci::from_move(game, &m)));
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
    normal: bool,
}

impl SearchState {
    fn with_max_depth(max_depth: u8) -> Self {
        Self {
            depth: max_depth,
            alpha: i32::MIN,
            beta: i32::MAX,
            normal: false,
        }
    }
}

fn do_search(
    pos: &Chess,
    prev_pos: &Chess,
    state: SearchState,
    seen_positions: &HashSet<Board>,
    prev_moves: &mut Vec<String>,
    dbg_write: &mut impl Write,
) -> Option<(Option<Move>, i32)> {
    let color = pos.turn();
    if state.depth == 0 {
        let score = board_score(pos);
        writeln!(dbg_write, "{:?} {}", prev_moves, score).unwrap();
        return Some((None, score));
    }
    if pos.is_checkmate() {
        if state.normal {
            let score = -color.fold(-10000 - state.depth as i32, 10000 + state.depth as i32);
            // writeln!(dbg_write, "{:?} {} mate", prev_moves, score).unwrap();
            return Some((None, score));
        } else {
            let score = color.fold(-10000 - state.depth as i32, 10000 + state.depth as i32);
            // writeln!(dbg_write, "{:?} {} mate", prev_moves, score).unwrap();
            return Some((None, score));
        }
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
    move_pos.sort_by_cached_key(|(_, pos)| board_score(pos) * color.fold(-1, 1));
    for (m, new_pos) in move_pos {
        prev_moves.push(Uci::from_move(pos, &m).to_string());
        let further_move = do_search(
            &new_pos,
            pos,
            SearchState {
                depth: state.depth - 1,
                alpha,
                beta,
                normal: !state.normal,
            },
            seen_positions,
            prev_moves,
            dbg_write,
        );
        prev_moves.pop();
        if let Some((_, score)) = further_move {
            let score = if seen_positions.contains(new_pos.board()) {
                0
            } else {
                score
            };
            let annoyance = if !state.normal && state.depth > 1 {
                mobility_score(pos, &new_pos)
            } else {
                0
            };
            let score = score + annoyance;
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
                alpha = alpha.max(score - annoyance);
                if alpha > beta {
                    break;
                }
            } else {
                beta = beta.min(score - annoyance);
                if beta < alpha {
                    break;
                }
            }
        }
    }

    if let Some((m, score)) = &best {
        let new_pos = pos.clone().play(m.as_ref().unwrap()).unwrap();
        let annoyance = if !state.normal && state.depth > 1 {
            mobility_score(pos, &new_pos)
        } else {
            0
        };
        let minmax = color.fold("max", "min");
        writeln!(
            dbg_write,
            "{:?} {} is {} ({} + {}a) because of next move {:?}",
            prev_moves,
            minmax,
            score,
            score - annoyance,
            annoyance,
            Uci::from_move(pos, m.as_ref().unwrap()).to_string()
        )
        .unwrap();
    } else {
        writeln!(dbg_write, "{:?} none", prev_moves).unwrap();
    }

    best
}

fn send(msg: &str) {
    eprintln!("send {}", &msg);
    println!("{}", msg);
}

// ======
