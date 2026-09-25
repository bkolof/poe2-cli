---
name: poe2
description: >-
  Analyse and improve Path of Exile 2 builds with the `poe2` CLI, which runs
  the real Path of Building engine. Use when the user asks about a PoE2
  character or build (DPS, defences, EHP, max hit, skills), whether an item is
  an upgrade, which passives to take next, which mods to look for on a slot,
  or about PoE2 mods, gems and uniques. Triggers: a poe.ninja/poe2 URL, a PoB
  build code or pobb.in link, pasted item text, "my character", "my build",
  "is this an upgrade", "what should I craft", "which passive next".
---

# poe2: PoE2 build analysis

`poe2` calculates builds with Path of Building (PoB-PoE2) running headless
inside the binary, and answers game-data questions from PoB's data. Its numbers
match PoB and poe.ninja. Never estimate stats or state mod, gem or unique facts
from memory when `poe2` can calculate or look them up.

## Naming a build

Every build command takes a BUILD, which can be:

- a poe.ninja profile URL, `https://poe.ninja/poe2/profile/<account>/<league>/character/<name>`
- `account/character`, such as `Name#1234/Koftespies` (quote it in the shell)
- a link to a build site PoB imports from: pobb.in, poe.ninja/poe2/pob, Maxroll, poe2db, pastebin
- a file with a build code or PoB XML, `-` for stdin, or a raw build code

`poe2 chars 'Name#1234'` lists an account's public characters. If the user
names a character but not the account, ask for it.

## Commands

| Question | Command |
|---|---|
| What are my stats? | `poe2 stats BUILD` (PoB's sidebar) |
| Which skills do damage? | `poe2 skills BUILD` |
| Is this item an upgrade? | `poe2 whatif BUILD --item - [--slot "Ring 1"]` with the item text on stdin |
| What if I (un)allocate a passive? | `poe2 whatif BUILD --allocate NAME --unallocate NAME` (repeatable, names or node ids) |
| Which passives next? | `poe2 tree BUILD [--by balanced\|dps\|ehp] [--distance 4] [--limit 15]` |
| What should I craft or buy for a slot? | `poe2 upgrades BUILD --slot Boots [--by ...]` |
| PoB code for the GUI | `poe2 export BUILD` |
| Mods, gems, uniques | `poe2 mods "movement speed" [--base "Silk Slippers"]`, `poe2 gems NAME`, `poe2 uniques NAME` |

Every command takes `--json`. Build commands take a few seconds each; the very
first run downloads PoB (about 390 MB), so tell the user when that happens.

Slots are named `Weapon 1`, `Weapon 2`, `Helmet`, `Body Armour`, `Gloves`,
`Boots`, `Amulet`, `Ring 1`, `Ring 2`, `Belt`, `Charm 1` to `3`, `Flask 1`
and `2`.

### Items

Items are compared with the text the game copies with Ctrl+C (in game or on
the trade site). Pass it through a heredoc so nothing needs escaping:

```sh
poe2 whatif 'Name#1234/Char' --item - <<'EOF'
Item Class: Boots
Rarity: Rare
...
EOF
```

Without `--slot`, the item is compared in every slot it fits (both rings, for
example).

## Reading the results

- `whatif` lists only stats that change, PoB-formatted, with `[better]` or
  `[worse]`. Summarise the trade-off; do not just repeat the list.
- `tree` totals include the path to each passive, and ranks per point spent.
  `balanced` adds DPS % and EHP %. Passives can share a name; use the node id
  from the output with `whatif --allocate` to check a specific one.
- `upgrades` adds the best tier of each mod the slot's item can roll, at a
  middle roll, and shows the item's free prefixes and suffixes (recognised from
  its mod text). Mods marked "needs a free slot" mean replacing a mod, so check
  with `whatif` against the item as it would be.
- `skills` calculates each skill as if it were the main skill. PoB rates some
  skills (cooldowns, combos) by damage per use rather than per second; those
  show "per use" and must not be compared with DPS figures. DPS in `stats`,
  `whatif`, `tree` and `upgrades` is for the main skill only; say so.
- With `--json`, extract what you need with `jq` rather than reading it all:
  `poe2 stats BUILD --json | jq '.stats | {Life, EnergyShield, TotalEHP}'`.

## Caveats

- `whatif` names the passives it resolved (with node ids and points); check
  they are the ones the user meant. Passives on paths that grant "+5 to any
  Attribute" count as worth nothing until allocated, so paths through them look
  slightly worse than they are.
- The free affixes in `upgrades` are recognised from mod text, so treat them as
  a good guess, especially with essence, desecrated or hybrid mods.

- Numbers use the PoB configuration saved with the build (enemy type, buffs,
  charges, flask uptime). poe.ninja builds use poe.ninja's defaults.
- poe.ninja only refreshes a character every so often. If the user just
  changed gear, the numbers may be behind.
- Some PoB keys are PoE1 leftovers that are always 0 in PoE2, such as
  `SpellSuppressionChance`. Do not present them as weaknesses.
