---
name: poe2
description: >-
  Analyse Path of Exile 2 characters with the `poe2` CLI, which calculates
  stats with the real Path of Building engine. Use when the user asks about a
  PoE2 character or build: its DPS, defences, resistances, EHP, max hit, what
  their main skill does, or wants a PoB build code. Triggers: a
  poe.ninja/poe2/profile URL, "my character", "my build", "how tanky am I",
  "what's my DPS", "export to PoB".
---

# poe2: PoE2 build analysis

`poe2` fetches a character from a public poe.ninja profile and calculates it
with Path of Building (PoB-PoE2) running headless inside the binary. Its
numbers match what PoB and poe.ninja show. Never estimate stats yourself when
`poe2` can calculate them.

## Commands

```sh
poe2 char stats <url>          # readable summary: DPS, defences, res, EHP, max hit
poe2 char stats <url> --json   # every PoB output stat (about 700) plus mainSkill
poe2 char export <url>         # PoB build code, for the PoB GUI's "Import from code"
```

`<url>` is `https://poe.ninja/poe2/profile/<account>/<league>/character/<name>`.
The account uses `-` where the in-game name has `#` (`Name#1234` is
`Name-1234`). If the user gives only an account and character name, ask for
the league or the poe.ninja URL rather than guessing.

Each call takes about 3 seconds. The very first call downloads PoB (about
390 MB) and takes longer; tell the user when that happens.

## Reading the JSON

Start with the summary. Reach for `--json` when a question needs a stat the
summary leaves out, and pull only what you need with `jq` instead of reading
all 700 keys:

```sh
poe2 char stats <url> --json | jq '.stats | {Life, EnergyShield, TotalEHP, FireResistOverCap}'
poe2 char stats <url> --json | jq '.stats | with_entries(select(.key | test("MaximumHitTaken")))'
```

Useful keys, all under `.stats`:

| Topic | Keys |
|---|---|
| Damage | `CombinedDPS`, `TotalDPS`, `AverageDamage`, `Speed`, `CritChance`, `CritMultiplier`, `HitChance` |
| Pools | `Life`, `EnergyShield`, `Mana`, `Spirit`, `SpiritUnreserved`, `LifeUnreserved` |
| Mitigation | `Armour`, `Evasion`, `AverageEvadeChance`, `DeflectChance`, `BlockChance` |
| Resistances | `FireResist`, `ColdResist`, `LightningResist`, `ChaosResist`, plus `...OverCap` for each |
| Survivability | `TotalEHP`, `PhysicalMaximumHitTaken` and the same for each element and chaos, `StunThreshold` |
| Recovery | `LifeRegenRecovery`, `EnergyShieldRegenRecovery`, `ManaRegenRecovery` |

## Things to keep in mind

- **Stats reflect the build as poe.ninja last saw it**, with the PoB
  configuration stored in its export (enemy settings, buffs, charges). If the
  user says they changed gear recently, the profile may not be refreshed yet.
- **Some PoB keys are PoE1 leftovers** that are always 0 in PoE2, such as
  `SpellSuppressionChance` or the `Dodge` keys. Do not present them as weaknesses.
- **DPS is for the main skill only** (`mainSkill`), the one selected in the
  export. Say which skill a DPS figure belongs to.
- For anything `poe2` cannot calculate yet, such as what-if gear swaps, say so
  and suggest the user check in the PoB GUI with `poe2 char export`.
