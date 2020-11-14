# Mobility chess

A UCI… inspired? – it seems to work, okay – chess engine.

The UCI frontend I'm using is https://github.com/fohristiwhirl/nibbler

## Features / Problems

- Alpha-beta pruning minimax game tree search to depth 6
- Sadly single-core 'cause alpha-beta makes parallelism hard
- Makes moves in a few seconds to a few minutes depending on board state

## The Heuristic

…is just

```text
  <valid moves for light pieces ignoring check>
- <valid moves for dark pieces ignoring check>
+ <material value of light pieces>
- <material value of dark pieces>
```

plus a little incentive to move the pawns forward.

That means that the engine sometimes ~~blunders~~ gambits minor pieces or even the queen if that
means it then has equivalently more valid moves.

Valid moves are counted ignoring checks because otherwise the engine would blunder anything just so
that the opponent does not get the opportunity to give some irrational check.

## Build or install

Requires [rust](https://rustup.rs/) to be installed.

```sh
git clone https://github.com/iyzana/mobility-chess
cd mobility-chess
cargo build --release
```

or install

```sh
cargo install https://github.com/iyzana/mobility-chess
```
