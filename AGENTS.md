# poe2-cli

A Rust CLI (`poe2`) for Path of Exile 2 build analysis and trade searches. It
embeds Path of Building (PoB-PoE2) headless on LuaJIT through `mlua`, so every
stat and every piece of game data comes from PoB itself. Nothing is calculated
or hand-maintained here. `skills/poe2` holds the agent skill that teaches
coding agents to use the CLI; users get it as a Claude Code plugin, and a
local checkout links it into the agent's skills directory.

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
  `poe2 stats tests/fixtures/koftespies.pob`, or a live character as
  `'Account#1234/Character'` (ask the user for theirs; trade and price checks
  need a live character for its league).

## Reference

Facts checked against the live services; recheck them when something breaks.

- **Trade API.**
  - Search: `POST /api/trade2/search/poe2/<league>`, returning at most 100
    listing ids. Fetch: `GET /api/trade2/fetch/<ids>?query=<search id>`, 10 ids
    per request. Plain searches work anonymously; weighted ones need a login.
  - Rate limits come from the response headers, one rule set per name in
    `X-Rate-Limit-Rules` (`Ip`, and `Account` when logged in); the limiter
    learns and merges them. Logged in, searches allowed 3 per 5s, 8 per 10s,
    15 per 60s, 60 per 300s and 600 per 3h; fetches 6 and 12 per 4s, 16 per
    12s, 100 per 300s and 1000 per 3h.
  - `statgroup.N` in a sort counts only the weighted-sum groups, not every
    stat group.
  - "Query is too complex" depends on the stats and on how many weighted sums
    a query has, not on a filter count: one sum passed with 89 filters, three
    failed with 14. Searches retry with fewer of PoB's weights.
  - Instant buyout listings carry a hideout token instead of a whisper.
  - The price filter converts currencies at the site's own rates, which
    differ from poe.ninja's; budgets are enforced again after fetching.
- **poe.ninja.** Unique prices come from
  `/poe2/api/economy/stash/current/item/overview?league=<name>&type=UniqueArmours`
  (also `UniqueWeapons`, `UniqueAccessories`, `UniqueJewels`, `UniqueFlasks`,
  `UniqueCharms`), with `primaryValue` in divines. Currency comes from
  `/poe2/api/economy/exchange/current/overview?league=<name>&type=Currency`.
  League names match the trade site's.
- **PoB internals in use.**
  - `CombinedDPS` is damage per use for skills with `skillData.showAverage`
    (the "per use" skills).
  - Skills granted by the tree carry `group.source = "Tree:<id>"`.
  - The weighted-search defaults are `FullDPS` 1.0 and `TotalEHP` 0.5.
  - `output.CharmLimit` is only set in breakdowns; the charm count comes from
    the `CharmLimit` mods, which include quest rewards.
  - Jewel sockets are slots named `Jewel <node id>`, usable when the node is
    in `build.spec.allocNodes`.

## Releases

- `dist` (pinned in `mise.toml`, configured in `dist-workspace.toml`) builds
  Linux, macOS and Windows binaries and shell and PowerShell installers.
  GitHub Actions runs it for a pushed version tag (`.github/workflows/release.yml`,
  generated by `mise x -- dist generate`; regenerate it rather than editing it).
- A release bumps the version in `Cargo.toml` and `.claude-plugin/plugin.json`
  together, then tags `vX.Y.Z` on main.
- `.github/workflows/ci.yml` runs fmt, clippy and the tests, including the PoB
  ones, on Linux, macOS and Windows for every push to main.
- The repo is also a Claude Code plugin marketplace (`.claude-plugin/`) that
  ships `skills/poe2`.

## Conventions

- Commit messages use conventional commits (`feat:`, `fix:`), and carry no
  attribution lines.
- In prose, comments and output, never use em dashes.
- Control flow blocks (`if`, `for`, `match`, ...) get a blank line before and
  after, except as the first or last statement in their block.
- Comment density and naming follow the surrounding code: doc comments explain
  why, not what.
