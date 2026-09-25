# poe2-cli

A Rust CLI (`poe2`) for Path of Exile 2 build analysis and trade searches. It
embeds Path of Building (PoB-PoE2) headless on LuaJIT through `mlua`, so every
stat and every piece of game data comes from PoB itself. Nothing is calculated
or hand-maintained here. `skills/poe2` holds the agent skill that teaches
coding agents to use the CLI; it is linked into the agent's skills directory.

## Commands

- Build and test through mise, which pins Rust in `mise.toml`: `mise x -- cargo build`,
  `mise x -- cargo test`, `mise x -- cargo clippy --all-targets`, `mise x -- cargo fmt`.
- Install the binary the user runs: `mise x -- cargo install --path . --root ~/.local`.
  Reinstall after every change that should reach `poe2` on PATH.
- `tests/pob.rs` runs against the real pinned PoB and skips itself when PoB is
  not installed yet. Its fixture, `tests/fixtures/koftespies.pob`, is the
  user's character.

## Layout

- `src/pob/`: embedded PoB.
  - `boot.lua` boots PoB's `HeadlessWrapper.lua`.
  - `api.lua` holds one Lua function per feature, and Rust calls them through
    `Pob::call` in `mod.rs`.
  - `model.rs` mirrors what `api.lua` returns, field for field.
  - `install.rs` downloads the pinned PoB release (`VERSION`) into the local
    data directory.
- `src/source.rs` turns a BUILD argument into build XML.
- `src/ninja.rs` is the poe.ninja profile API; `src/market.rs` is poe.ninja
  prices, cached on disk for an hour.
- `src/trade/` is the pathofexile.com trade API: session storage, and rate
  limits saved to disk across runs.
- `src/main.rs` holds the clap commands, `src/report.rs` the text output and
  `src/shop.rs` the price-aware ranking.

## How to work on it

- Features belong in `api.lua` and should use PoB's own machinery, not new
  calculations. Examples: `build.calcsTab:GetMiscCalculator()` (the calculator
  behind PoB's tooltips) for comparisons, `build.displayStats` for stat lists,
  PoB's `TradeQueryGenerator` and `TradeQueryRequests` for trade.
- The PoB source to read is in `~/.local/share/poe2/pob/v0.23.1/src`.
- For quick Lua experiments, the Podman image `localhost/poe2-pob:v0.23.1`
  runs a script against that directory:
  `podman run --rm --network=none --entrypoint luajit -v ~/.local/share/poe2/pob/v0.23.1:/pob:ro,z -v <dir>:/work:ro,z -w /pob/src localhost/poe2-pob:v0.23.1 /work/x.lua`,
  where the script `dofile`s copies of `boot.lua` and `api.lua`.
- Gotchas:
  - Raise user-facing Lua errors with `error(msg, 0)`.
  - Serialise requests to Lua with `serialize_none_to_null(false)`, or
    `None` arrives as a truthy userdata instead of nil.
  - `Pob::start` changes the working directory, so read user files before it.
  - PoB reports load errors via `launch.promptMsg`; `poe2.load` turns that
    into an error.
- Verify changes against the real build, not only the tests:
  `poe2 stats tests/fixtures/koftespies.pob`, or the live character
  `'Exile#1234/Koftespies'`.

## Conventions

- Commit messages use conventional commits (`feat:`, `fix:`), and carry no
  attribution lines.
- In prose, comments and output, never use em dashes.
- Control flow blocks (`if`, `for`, `match`, ...) get a blank line before and
  after, except as the first or last statement in their block.
- Comment density and naming follow the surrounding code: doc comments explain
  why, not what.
