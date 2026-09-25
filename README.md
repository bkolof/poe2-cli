# poe2

Path of Exile 2 build analysis on the command line. Every stat is calculated by
[Path of Building](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
itself, running headless inside this binary on an embedded LuaJIT, and game
data (mods, gems, uniques) comes from PoB's data. No stat is calculated and no
game data is kept in this repo.

## Setup

The Rust toolchain is pinned in `mise.toml`, and building also needs a C
compiler for LuaJIT and lua-utf8.

```sh
mise install
cargo build --release
```

To put `poe2` on PATH and give Claude Code the skill in `skills/poe2`:

```sh
cargo install --path . --root ~/.local                  # ~/.local/bin/poe2
ln -sfn "$PWD/skills/poe2" ~/.claude/skills/poe2
```

The symlink keeps the skill versioned with the CLI, and edits take effect
without reinstalling. `~/.claude/skills` is not an exact directory in chezmoi,
so chezmoi leaves the link alone.

The first calculation downloads the pinned PoB release (about 390 MB, 65 MB
unpacked) into `poe2/pob/<version>` under the local data directory:
`~/.local/share` on Linux, `~/Library/Application Support` on macOS,
`%LOCALAPPDATA%` on Windows.

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
poe2 export BUILD                         # build code for PoB's "Import from code"
poe2 chars 'Name#1234'                    # an account's public characters
poe2 mods "movement speed" --base "Silk Slippers"
poe2 gems "falling thunder"
poe2 uniques "atziri"
```

Every command takes `--json`.

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

## Vendored code

- `vendor/luautf8`: [starwing/luautf8](https://github.com/starwing/luautf8)
  at a47b143, MIT. PoB requires it as `lua-utf8`; `build.rs` compiles it in.
- `vendor/luajit`: the Lua 5.1 API headers from the LuaJIT that `mlua`
  vendors (luajit-src 210.7.3), MIT, needed to compile luautf8.
