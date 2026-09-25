# poe2

Path of Exile 2 build analysis on the command line. Characters come from public
poe.ninja profiles; every stat is calculated by
[Path of Building](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
itself, running headless inside this binary on an embedded LuaJIT. No stat is
calculated in this repo.

## Setup

The Rust toolchain is pinned in `mise.toml`, and building also needs a C
compiler for LuaJIT and lua-utf8.

```sh
mise install
cargo build --release
```

The first calculation downloads the pinned PoB release (about 390 MB, 65 MB
unpacked) into `~/.local/share/poe2/pob/<version>`.

## Usage

```sh
poe2 char stats https://poe.ninja/poe2/profile/<account>/<league>/character/<name>
poe2 char stats <url> --json     # every PoB output stat
poe2 char export <url>           # build code for the PoB GUI's "Import from code"
```

## How it works

1. `ninja.rs` fetches the character model from poe.ninja's profile API. It
   includes a PoB build code.
2. `pob/code.rs` decodes the build code to build XML.
3. `pob/mod.rs` boots PoB through its own `HeadlessWrapper.lua`
   (`pob/boot.lua`), loads the build, and reads PoB's calculated output
   straight from its Lua tables.

The PoB version is pinned in `pob/install.rs`. To move to a new PoB release,
bump `VERSION` and run the tests: `tests/pob.rs` checks that headless PoB
reproduces the stats poe.ninja stored in a real build. It skips itself when
the pinned PoB is not installed yet.

## Vendored code

- `vendor/luautf8`: [starwing/luautf8](https://github.com/starwing/luautf8)
  at a47b143, MIT. PoB requires it as `lua-utf8`; `build.rs` compiles it in.
- `vendor/luajit`: the Lua 5.1 API headers from the LuaJIT that `mlua`
  vendors (luajit-src 210.7.3), MIT, needed to compile luautf8.
