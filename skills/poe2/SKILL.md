---
name: poe2
description: >-
  Analyse and improve Path of Exile 2 builds with the `poe2` CLI, which runs
  the real Path of Building engine. Use when the user asks about a PoE2
  character or build (DPS, defences, EHP, max hit, skills), whether an item is
  an upgrade, which passives to take next, which mods to look for on a slot,
  or about PoE2 mods, gems and uniques, or wants to buy upgrades on the trade
  site or know prices. Triggers: a poe.ninja/poe2 URL, a PoB build code or
  pobb.in link, pasted item text, "my character", "my build", "is this an
  upgrade", "what should I craft", "which passive next", "find me boots",
  "best upgrade for my budget", "what is a divine worth".
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
| Which uniques are worth buying? | `poe2 uniques-for BUILD --slot Boots [--budget 2]` |
| Find the best items to buy | `poe2 trade search BUILD --slot Boots --budget 5 [--require "movement speed=25"] [--open]` |
| Price a specific unique or base, calculated | `poe2 trade search BUILD --slot Boots --name "Atziri's Step"` or `--base "Silk Slippers"` |
| Find a jewel | `poe2 trade search BUILD --slot jewel [--jewel-type radius]` |
| Best upgrade in every slot | `poe2 trade scan BUILD --budget 5 [--slot Boots --slot Gloves]` |
| Find a trade site stat | `poe2 trade stats "fire resistance"` |
| What is this item worth? | `poe2 price --item -` with the item text on stdin |
| What is my gear worth? | `poe2 price BUILD` |
| Currency prices | `poe2 prices` |
| PoB code for the GUI | `poe2 export BUILD` |
| Mods, gems, uniques | `poe2 mods "movement speed" [--base "Silk Slippers"]`, `poe2 gems NAME`, `poe2 uniques NAME` |

Every command takes `--json`. Build commands take a few seconds each; the very
first run downloads PoB (about 390 MB), so tell the user when that happens.

Slots are named `Weapon 1`, `Weapon 2`, `Helmet`, `Body Armour`, `Gloves`,
`Boots`, `Amulet`, `Ring 1`, `Ring 2`, `Belt`, `Charm 1` to `3`, `Flask 1`
and `2`. Jewel sockets are `Jewel <node id>`; for trade searches, `jewel`
picks an empty allocated socket, or the error lists the filled ones.

### Buying

Budgets are in divines unless marked: `5`, `5div`, `300ex`, `20c`. Prices come
from poe.ninja, for the character's league.

- `uniques-for` needs no login: it calculates every unique that fits the slot
  and joins poe.ninja prices. Try it before a trade search for slots where
  uniques are common.
- `trade search` lets PoB weight a trade site search by what each stat is
  worth to the build, fetches the best matches (`--fetch`, default 30) and
  calculates every one with PoB. It lists the best value at each price: each
  step up in price is a real improvement. Items the character lacks the
  attributes for are left out of that list.
- PoB only weighs stats that change DPS or EHP. For stats a player still
  wants, such as movement speed on boots, add `--require "movement speed=25"`
  (a text to find the trade site stat by, then a minimum). Ask the user about
  this for boots.
- `--open` opens the search on the trade site in the user's browser, where
  they can buy. Only listings from sellers who trade in person carry a
  whisper; instant buyout listings are bought on the site.
- Searches count against the trade site's rate limits; `poe2` waits when
  needed. Do not loop over many searches: use `trade scan` for several slots.

- `--name` (a unique) or `--base` (an item base) replaces PoB's weighted
  search: it finds those items, cheapest first, and still calculates each in
  the slot. The advanced options below apply too, except `--min-weight` and
  `--sort pob`.
- In a jewel socket, `--jewel-type radius` searches radius jewels (Time-Lost)
  instead of base jewels. Search both: they weigh different mods.

#### Advanced searches

PoB's weighted sum stays the core of every search. On top of it (all
repeatable; a stat is a text to find it by or a trade id from `trade stats`):

| Option | Meaning |
|---|---|
| `--require "stat=25"`, `"stat=25..35"`, `"stat=..10"` | must have the stat in that range |
| `--exclude "stat"` | must not have the stat |
| `--count "2: stat, stat, stat"` | at least that many of the stats |
| `--sum "[min:] stat=weight, ..."` | an extra weighted sum, e.g. `"60: fire resistance=1, cold resistance=1, lightning resistance=1"` |
| `--filter name=value` | an item filter: `ilvl`, `quality`, `es`, `ar`, `ev`, `spirit`, `rune_sockets`, `dps`, `pdps`, `edps`, `aps`, `crit`, `block`, `lvl`, `str`/`dex`/`int` (ranges like `80`, `80..`, `..82`); `rarity` (`rare`, `unique`, `any`, ...), `corrupted`, `fractured_item`, `desecrated`, `sanctified`, `mirrored`, ... (`true`/`false`); `indexed` (`1day`, `1week`, ...); `account`; `collapse=true` |
| `--sort pob\|price\|sum:N\|stat:TEXT` | which listings come first, and so get calculated |
| `--min-weight N` | the minimum for PoB's sum; the output shows PoB's default |
| `--query FILE` | raw trade query JSON merged in last |
| `--show-query` | print the query without searching (needs no login) |

- `--sort price` returns the cheapest listings whose PoB sum reaches the
  minimum. PoB's default minimum is half of what the current item scores, and
  the sum only approximates what PoB then calculates, so most of the cheapest
  listings are not upgrades. Raise `--min-weight` well above the default to
  skip them; the default sort (`pob`) usually finds more real upgrades.
- Stat texts match the most specific trade stat, preferring pseudo totals
  ("fire resistance" is "+#% total to Fire Resistance"). Check with
  `trade stats` when a text is ambiguous.
- The site limits how costly a search is, and extra weighted sums cost the
  most. When it refuses a search, `poe2` retries with fewer of PoB's least
  important weights and says how many it left out.

### Price checks

`poe2 price` works like Sidekick's price check, without adjusting filters by
hand, and needs no login. A unique is searched by name, with poe.ninja's price
shown too. Anything else is searched by its category, its defences and its
mods, each within `--tolerance` percent of the item's value (default 10). When
fewer than 10 listings have every mod, it searches again for listings with
any N of them, one fewer each time, down to half. The estimate is the median of
the cheapest 10 listings, so a single fake cheap listing does not set it.

- Say how many mods the listings had to match; an estimate from a relaxed
  search, or from few listings, is rough.
- Rune lines are left out, as runes can be swapped. Mods the trade site has no
  stat for are listed as not searchable.
- Gloves turned into Fists of Stone (the Martial Artist's ascendancy) are
  priced as the gloves they were: each transformed mod is traced back to the
  mod it came from, rolled as high, on any gloves base. Lines several mods
  turn into cannot be traced and are left out.
- `poe2 price BUILD` prices every equipped item, one to four searches each,
  and takes a minute or two within the rate limits. Items nobody lists (some
  quest bases) show `?`.

### Trade login

Weighted searches need the user's pathofexile.com session, the POESESSID
cookie. It is as sensitive as a password: never print it, never put it on a
command line, never keep it in the conversation.

- `poe2 trade status` says whether a session is stored and still works.
- The user can run `poe2 trade login` in their own terminal and paste the
  cookie (browser developer tools, Storage or Application, Cookies,
  pathofexile.com). The prompt hides it.
- Or, with the Playwright browser tools: open https://www.pathofexile.com/login
  in a visible browser, let the user log in, then run code in the browser
  context that reads the POESESSID cookie from `page.context().cookies()` and
  passes it on stdin to `poe2 trade login` through `child_process`, returning
  only whether the login succeeded. Never return or log the cookie value.

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
example), including the weapon swap set when it has a weapon. Name a slot such
as `Weapon 1 Swap` to compare there anyway; a swap slot is compared with that
set active. Charms are compared only in the charm slots the build has.

A jewel can go into a socket that is not allocated yet by allocating it in the
same command: `--allocate 21984 --slot "Jewel 21984"`. Find a nearby socket
with `whatif BUILD --allocate "Jewel Socket"`, which names the closest one.

#### Screenshots

When the user shares a screenshot of an item tooltip instead of its text:

1. Transcribe it into Ctrl+C item text: `Item Class`, `Rarity`, the name and
   base on their own lines, then every mod line exactly as shown, with its
   numbers. Keep implicit, rune and enchant lines, and `Corrupted` if shown.
2. Show the transcription to the user and ask them to check it before
   running anything. Digits in screenshots are easy to misread, and one wrong
   number changes the result.
3. Run `whatif` with the confirmed text. If PoB does not recognise the item,
   the base name is usually wrong; check it with `poe2 uniques` or
   `poe2 mods --base`.

Say that the numbers come from a transcription. Ctrl+C text, when the user can
copy it, is always more reliable.

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
