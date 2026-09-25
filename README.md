# poe2

Path of Exile 2 build analysis on the command line. Every stat is calculated by
[Path of Building](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
itself, running headless inside this binary on an embedded LuaJIT, and game
data (mods, gems, uniques) comes from PoB's data. No stat is calculated and no
game data is kept in this repo.

## Install

**macOS and Linux:**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bkolof/poe2-cli/releases/latest/download/poe2-installer.sh | sh
```

**Windows** (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/bkolof/poe2-cli/releases/latest/download/poe2-installer.ps1 | iex"
```

The installers put `poe2` in `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on
Windows) and add it to PATH; open a new terminal afterwards. The binaries are
also on the [releases page](https://github.com/bkolof/poe2-cli/releases).

The binaries are not code-signed. The installers avoid the warnings, but a
binary downloaded in a browser is blocked the first time: on macOS run
`xattr -d com.apple.quarantine poe2` once, and on Windows choose "More info",
then "Run anyway".

The first calculation downloads the pinned Path of Building release (about
390 MB, 65 MB unpacked) into `poe2/pob/<version>` under the local data
directory: `~/.local/share` on Linux, `~/Library/Application Support` on
macOS, `%LOCALAPPDATA%` on Windows.

### Claude Code

The `poe2` skill teaches Claude Code to use the CLI. Install it as a plugin:

```
/plugin marketplace add bkolof/poe2-cli
/plugin install poe2@poe2-cli
```

Then ask Claude about your character, for example "what should I upgrade on
`Name#1234/MyCharacter` for 2 divines?".

### Codex

The same skill works in Codex (CLI, IDE and desktop app). Type this into a
Codex chat, not into a terminal:

```
$skill-installer install https://github.com/bkolof/poe2-cli/tree/main/skills/poe2
```

Or install it from a terminal. On Windows, in PowerShell:

```powershell
$dir = "$HOME\.agents\skills\poe2"; New-Item -ItemType Directory -Force $dir | Out-Null; Invoke-WebRequest https://raw.githubusercontent.com/bkolof/poe2-cli/main/skills/poe2/SKILL.md -OutFile "$dir\SKILL.md"
```

On macOS and Linux:

```sh
mkdir -p ~/.agents/skills/poe2 && curl -LsSf https://raw.githubusercontent.com/bkolof/poe2-cli/main/skills/poe2/SKILL.md -o ~/.agents/skills/poe2/SKILL.md
```

Codex picks up new skills automatically; restart it if the skill does not show
up.

### Trade site login

Weighted trade searches need your pathofexile.com session: run
`poe2 trade login` and paste the `POESESSID` cookie (browser developer tools,
Storage or Application, Cookies, pathofexile.com). It is stored in a file only
you can read, and never printed. Price checks and plain searches work without
it.

## Usage

A BUILD is a poe.ninja profile URL, `account/character` (`'Name#1234/Char'`),
a link to a build site PoB imports from (pobb.in, Maxroll, ...), a file with a
build code or PoB XML, `-` for stdin, or a build code.

```sh
poe2 stats BUILD                          # PoB's sidebar
poe2 skills BUILD                         # DPS of every active skill
poe2 whatif BUILD --item item.txt         # item text copied with Ctrl+C; `-` reads stdin
poe2 whatif BUILD --allocate "Imbibed Power" --unallocate "For the Jugular"
poe2 tree BUILD --by balanced|dps|ehp     # best passives within reach, per point
poe2 upgrades BUILD --slot Boots          # best mods to add to a slot's item
poe2 uniques-for BUILD --slot Boots --budget 2   # uniques that help, priced by poe.ninja
poe2 trade login                          # store your POESESSID (read from stdin)
poe2 trade search BUILD --slot Boots --budget 5 --require "movement speed=25" --open
poe2 trade search BUILD --slot Boots --name "Atziri's Step"    # a unique, calculated
poe2 trade search BUILD --slot jewel --jewel-type radius       # jewels for a socket
poe2 trade scan BUILD --budget 5          # the best upgrade in every gear slot
poe2 trade stats "fire resistance"        # trade site stats, for --require and friends
poe2 price --item item.txt                # price check an item, like Sidekick
poe2 price BUILD                          # price every equipped item
poe2 prices                               # currency rates from poe.ninja
poe2 export BUILD                         # build code for PoB's "Import from code"
poe2 chars 'Name#1234'                    # an account's public characters
poe2 mods "movement speed" --base "Silk Slippers"
poe2 gems "falling thunder"
poe2 uniques "atziri"
```

Every command takes `--json`. Budgets are in divines unless marked: `5`,
`5div`, `300ex`, `20c`.

`trade search` lets PoB generate a weighted trade site search (its own trade
query generator, weighting each stat by what it is worth to the build),
fetches the best matches and calculates every one with PoB, then lists the
best value at each price. On top of PoB's weights, a search can require
stats within ranges (`--require`), exclude stats (`--exclude`), ask for some
of a set (`--count`), add weighted sums of its own (`--sum`), set any item
filter (`--filter ilvl=80`, `es=150..`, `corrupted=false`, ...), sort by price,
a sum or a stat (`--sort`), and merge in raw query JSON (`--query`);
`--show-query` prints the query without searching. `--name` and `--base`
search for a unique or an item base instead of PoB's weights, and jewel
sockets (`--slot jewel`) are searched for base or radius jewels. `trade scan` runs one
weighted search per gear slot and ranks the upgrades side by side.

`price` checks what an item is worth the way traders do: a unique by name,
anything else by the stats that set its price (pseudo totals, defences, weapon
DPS, and the best tiers of its other mods, with filler left out), and
estimates from the median of the cheapest listings. It needs no login.

Weighted searches need a logged-in session; the
POESESSID is stored in the config directory, readable only by you. Requests
stay within the trade site's rate limits, which are tracked on disk across
runs.

## How it works

1. `source.rs` turns the BUILD argument into PoB build XML, fetching from
   poe.ninja (`ninja.rs`) or a build site where needed. Build site links are
   resolved with PoB's own list of sites.
2. `pob/mod.rs` boots PoB through its `HeadlessWrapper.lua` (`pob/boot.lua`)
   and loads `pob/api.lua`, which holds one Lua function per feature.
3. Those functions use PoB's own machinery: the calculator behind its item and
   passive tooltips for every comparison, its sidebar for stats, and its data
   tables for searches. `report.rs` formats the results for people.

The PoB version is pinned in `pob/install.rs`. To move to a new PoB release,
bump `VERSION` and run the tests: `tests/pob.rs` checks that headless PoB
reproduces the stats poe.ninja stored in a real build. It skips itself when
the pinned PoB is not installed yet.

## Development

The Rust toolchain and `dist` are pinned in `mise.toml`, and building also
needs a C compiler for LuaJIT and lua-utf8.

```sh
mise install
cargo build --release
cargo test        # the PoB tests run once `poe2` has downloaded PoB
```

To use a local build and edit the skill in place:

```sh
cargo install --path . --root ~/.local                  # ~/.local/bin/poe2
ln -sfn "$PWD/skills/poe2" ~/.claude/skills/poe2
```

Releases are built by GitHub Actions with
[dist](https://github.com/axodotdev/cargo-dist): bump the version in
`Cargo.toml` and `.claude-plugin/plugin.json`, commit, and push a tag such as
`v0.3.0`. CI runs the tests on Linux, macOS and Windows on every push.

## Vendored code

- `vendor/luautf8`: [starwing/luautf8](https://github.com/starwing/luautf8)
  at a47b143, MIT. PoB requires it as `lua-utf8`; `build.rs` compiles it in.
- `vendor/luajit`: the Lua 5.1 API headers from the LuaJIT that `mlua`
  vendors (luajit-src 210.7.3), MIT, needed to compile luautf8.
