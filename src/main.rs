mod report;
mod shop;

use std::fs;
use std::io::{self, IsTerminal, Read};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use poe2::market::{self, Price, Rates};
use poe2::ninja::{self, Character};
use poe2::pob::model::{TradeQueryRequest, WhatIfRequest};
use poe2::pob::{self, Pob};
use poe2::source::{LoadedBuild, Source};
use poe2::trade::price;
use poe2::trade::query::{self, Additions, Count, Filter, Require, Search, Sort, Sum};
use poe2::trade::{self, session};
use serde::Serialize;
use shop::SlotSearch;

/// Path of Exile 2 build analysis, backed by headless Path of Building.
///
/// Wherever a command takes a BUILD, it accepts a poe.ninja profile URL,
/// `account/character` (as in `Name#1234/Character`), a link to a build site
/// PoB imports from (pobb.in, poe.ninja/poe2/pob, Maxroll, ...), a file with
/// a build code or PoB XML, `-` for stdin, or a build code.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Print JSON instead of text
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

// The commands are parsed once per run, so their size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Command {
    /// PoB's sidebar stats for a build
    Stats {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
    },
    /// DPS of every active skill, each calculated as the main skill
    Skills {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
    },
    /// Stat changes from equipping an item or (un)allocating passives
    #[command(visible_alias = "what-if")]
    Whatif {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// Item text as copied in game with Ctrl+C: a file, or `-` for stdin
        #[arg(long)]
        item: Option<String>,
        /// Only compare the item in this slot (e.g. "Ring 1"), not every slot it fits
        #[arg(long, requires = "item")]
        slot: Option<String>,
        /// Allocate a passive and the path to it, by name or node id (repeatable)
        #[arg(long)]
        allocate: Vec<String>,
        /// Unallocate a passive and the passives depending on it, by name or node id (repeatable)
        #[arg(long)]
        unallocate: Vec<String>,
    },
    /// The best unallocated passives within reach, including their paths
    Tree {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// What to rank by, per passive point spent
        #[arg(long, value_enum, default_value_t = Rank::Balanced)]
        by: Rank,
        /// Only consider passives at most this many points away
        #[arg(long, default_value_t = 4)]
        distance: u32,
        #[arg(long, default_value_t = 15)]
        limit: usize,
    },
    /// The mods that would help most on the item in a slot
    Upgrades {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// The item slot, e.g. "Boots", "Ring 1", "Weapon 1"
        #[arg(long)]
        slot: String,
        /// What to rank by
        #[arg(long, value_enum, default_value_t = Rank::Balanced)]
        by: Rank,
        #[arg(long, default_value_t = 15)]
        limit: usize,
    },
    /// The build's PoB build code, for the PoB GUI's "Import from code"
    Export {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
    },
    /// The public characters on a poe.ninja account (`Name#1234`)
    Chars { account: String },
    /// Search item mods by text
    Mods {
        query: String,
        /// Only mods that can roll on this item base, e.g. "Silk Slippers"
        #[arg(long)]
        base: Option<String>,
    },
    /// Search skill and support gems by name or tag
    Gems { query: String },
    /// Search unique items by name or text
    Uniques {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// The uniques that would improve a slot, with poe.ninja prices
    UniquesFor {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// The item slot, e.g. "Boots", "Ring 1", "Weapon 1"
        #[arg(long)]
        slot: String,
        /// The most to spend: `5` (divines), `5div`, `300ex` or `20c`
        #[arg(long)]
        budget: Option<Price>,
        /// What to rank by
        #[arg(long, value_enum, default_value_t = Rank::Balanced)]
        by: Rank,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        /// The league to price in (default: the character's, or the current league)
        #[arg(long)]
        league: Option<String>,
    },
    /// Trade site searches for the best items for a build (needs `trade login`)
    #[command(subcommand)]
    Trade(TradeCommand),
    /// Price check an item on the trade site, or every item a build has equipped
    ///
    /// Searches listings like the item (a unique by name; anything else by its
    /// mods at a tolerance, requiring fewer of them until enough listings match)
    /// and estimates the price from the cheapest. Needs no login.
    Price {
        /// The build whose equipped items to price: a poe.ninja URL, account/character,
        /// build site link, file, `-` or build code
        #[arg(required_unless_present = "item", conflicts_with = "item")]
        build: Option<String>,
        /// Item text as copied in game with Ctrl+C: a file, or `-` for stdin
        #[arg(long)]
        item: Option<String>,
        /// How far below the item's values a listing's mods may be, in percent
        #[arg(long, default_value_t = 10.0)]
        tolerance: f64,
        /// Which listings to include
        #[arg(long, value_enum, default_value_t = Status::Available)]
        status: Status,
        /// How many of the cheapest listings to fetch per item
        #[arg(long, default_value_t = 10)]
        fetch: usize,
        /// The league (default: the character's, or the current league)
        #[arg(long)]
        league: Option<String>,
        /// Open the search on the trade site (with --item)
        #[arg(long, requires = "item")]
        open: bool,
    },
    /// Currency exchange rates from poe.ninja
    Prices {
        /// The league (default: the current challenge league)
        #[arg(long)]
        league: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum TradeCommand {
    /// Store your pathofexile.com session (the POESESSID cookie), read from stdin
    Login,
    /// Check whether the stored session still works
    Status,
    /// Delete the stored session
    Logout,
    /// Find the best value items for a slot: PoB weights the search, then
    /// calculates every listing it fetches
    ///
    /// Stats are given by their trade site text or id (see `trade stats`).
    Search {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// The item slot, e.g. "Boots", "Ring 1", "Weapon 1"; a jewel socket as
        /// "Jewel <node id>", or "jewel" for an empty allocated one
        #[arg(long)]
        slot: String,
        #[command(flatten)]
        trade: TradeArgs,
        /// Search for a unique by name instead of PoB's weighted search
        #[arg(long)]
        name: Option<String>,
        /// Search for an item base, e.g. "Silk Slippers", instead of PoB's weighted search
        #[arg(long)]
        base: Option<String>,
        /// The jewels to search for in a jewel socket
        #[arg(long, value_enum, default_value_t = JewelType::Base)]
        jewel_type: JewelType,
        /// A stat the item must have, as `stat=min`, `stat=min..max` or `stat=..max`,
        /// e.g. "movement speed=25" (repeatable)
        #[arg(long)]
        require: Vec<Require>,
        /// A stat the item must not have (repeatable)
        #[arg(long)]
        exclude: Vec<String>,
        /// At least N of some stats, as `N: stat, stat, ...` (repeatable)
        #[arg(long)]
        count: Vec<Count>,
        /// A weighted sum of stats with an optional minimum, as `[min:] stat=weight, ...`,
        /// e.g. "60: fire resistance=1, cold resistance=1" (repeatable)
        #[arg(long)]
        sum: Vec<Sum>,
        /// An item filter, as `name=value` or `name=min..max`, e.g. ilvl=80, es=150..,
        /// rune_sockets=2, corrupted=false, indexed=1week, rarity=any (repeatable)
        #[arg(long)]
        filter: Vec<Filter>,
        /// Which listings come first, and so get calculated: pob (PoB's weighted sum,
        /// the default), price (cheapest; the default for --name and --base),
        /// sum:N (the Nth --sum) or stat:TEXT
        #[arg(long)]
        sort: Option<Sort>,
        /// The minimum for PoB's weighted sum (default: half of what the current item scores)
        #[arg(long)]
        min_weight: Option<f64>,
        /// Trade site query JSON to merge into the search: a file, or `-` for stdin
        #[arg(long)]
        query: Option<String>,
        /// Print the search's query instead of running it
        #[arg(long)]
        show_query: bool,
        /// Open the search on the trade site
        #[arg(long)]
        open: bool,
    },
    /// The best value upgrades across the gear slots: one weighted search per slot
    Scan {
        /// The build: a poe.ninja URL, account/character, build site link, file, `-` or build code
        build: String,
        /// A slot to search (repeatable; default: weapons, armour, jewellery and belt)
        #[arg(long)]
        slot: Vec<String>,
        #[command(flatten)]
        trade: TradeArgs,
    },
    /// Look up trade site stats by text, for the stats in `trade search`
    Stats {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

/// What every trade search takes.
#[derive(Args)]
struct TradeArgs {
    /// The most to spend: `5` (divines), `5div`, `300ex` or `20c`
    #[arg(long)]
    budget: Option<Price>,
    /// What to optimise for
    #[arg(long, value_enum, default_value_t = Rank::Balanced)]
    by: Rank,
    /// Which listings to include
    #[arg(long, value_enum, default_value_t = Status::Available)]
    status: Status,
    /// How many of the best matching listings to calculate with PoB, per search
    #[arg(long, default_value_t = 30)]
    fetch: usize,
    /// The league (default: the character's, or the current league)
    #[arg(long)]
    league: Option<String>,
}

/// The slots `trade scan` searches by default.
const SCAN_SLOTS: &[&str] = &[
    "Weapon 1",
    "Weapon 2",
    "Helmet",
    "Body Armour",
    "Gloves",
    "Boots",
    "Amulet",
    "Ring 1",
    "Ring 2",
    "Belt",
];

#[derive(Clone, Copy, ValueEnum)]
enum JewelType {
    /// Jewels without a radius
    Base,
    /// Radius jewels, such as Time-Lost ones
    Radius,
}

/// What a trade search looks for.
enum Target {
    /// PoB's weighted search, with the jewel type for a jewel socket.
    Weighted(JewelType),
    /// A unique by name, an item base, or both.
    Item {
        name: Option<String>,
        base: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Status {
    /// Instant buyout only
    Securable,
    /// Instant buyout, or a seller who is online
    Available,
    /// Sellers who are online
    Online,
    /// Every listing
    Any,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Rank {
    /// DPS and EHP percentage changes added together
    Balanced,
    Dps,
    Ehp,
}

fn main() -> Result<()> {
    // Exit quietly when the reader goes away (`poe2 ... | head`) instead of
    // panicking on the failed write.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();
    let json = cli.json;

    match cli.command {
        Command::Stats { build } => {
            let (pob, loaded) = open(&build)?;
            let info = pob.info()?;

            if json {
                let stats = pob.output()?;
                return print_json(&serde_json::json!({ "build": info, "stats": stats }));
            }

            report::header(&info, loaded.character.as_ref());
            report::sidebar(&pob.sidebar()?);
        }
        Command::Skills { build } => {
            let (pob, _) = open(&build)?;
            let skills = pob.skills()?;

            if json {
                return print_json(&skills);
            }

            report::skills(skills);
        }
        Command::Whatif {
            build,
            item,
            slot,
            allocate,
            unallocate,
        } => {
            if build == "-" && item.as_deref() == Some("-") {
                anyhow::bail!("the build and the item cannot both come from stdin");
            }

            let item = item.map(|path| read_input(&path)).transpose()?;
            let request = WhatIfRequest {
                item,
                slot,
                allocate,
                unallocate,
            };

            if request.item.is_none()
                && request.allocate.is_empty()
                && request.unallocate.is_empty()
            {
                anyhow::bail!("nothing to compare: pass --item, --allocate or --unallocate");
            }

            let (pob, _) = open(&build)?;
            let result = pob.what_if(&request)?;

            if json {
                return print_json(&result);
            }

            report::what_if(&result);
        }
        Command::Tree {
            build,
            by,
            distance,
            limit,
        } => {
            let (pob, _) = open(&build)?;
            let mut suggestions = pob.tree_suggestions(distance)?;
            suggestions.retain(|s| report::score(&s.impact, by) > 0.0);
            suggestions.sort_by(|a, b| {
                report::score(&b.impact, by).total_cmp(&report::score(&a.impact, by))
            });
            suggestions.truncate(limit);

            if json {
                return print_json(&suggestions);
            }

            report::tree(&suggestions, by);
        }
        Command::Upgrades {
            build,
            slot,
            by,
            limit,
        } => {
            let (pob, _) = open(&build)?;
            let mut upgrades = pob.slot_upgrades(&slot)?;
            upgrades
                .upgrades
                .retain(|u| report::score(&u.impact, by) > 0.0);
            upgrades.upgrades.sort_by(|a, b| {
                report::score(&b.impact, by).total_cmp(&report::score(&a.impact, by))
            });
            upgrades.upgrades.truncate(limit);

            if json {
                return print_json(&upgrades);
            }

            report::upgrades(&upgrades, by);
        }
        Command::Export { build } => {
            let source = Source::read(&build)?;

            if let Source::Profile(reference) = &source {
                println!(
                    "{}",
                    ninja::fetch_character(reference)?.path_of_building_export
                );
                return Ok(());
            }

            let pob = Pob::start(&pob::ensure_installed()?)?;
            println!("{}", pob::encode_build_code(&source.fetch(&pob)?.xml)?);
        }
        Command::Chars { account } => {
            let characters = ninja::list_characters(&account)?;

            if json {
                return print_json(&characters);
            }

            report::characters(&account, &characters);
        }
        Command::Mods { query, base } => {
            let pob = Pob::start(&pob::ensure_installed()?)?;
            let mods = pob.search_mods(&query, base.as_deref())?;

            if json {
                return print_json(&mods);
            }

            report::mods(mods);
        }
        Command::Gems { query } => {
            let pob = Pob::start(&pob::ensure_installed()?)?;
            let gems = pob.search_gems(&query)?;

            if json {
                return print_json(&gems);
            }

            report::gems(gems);
        }
        Command::Uniques { query, limit } => {
            let pob = Pob::start(&pob::ensure_installed()?)?;
            let mut uniques = pob.search_uniques(&query)?;
            uniques.sort_by(|a, b| a.name.cmp(&b.name));

            if json {
                return print_json(&uniques);
            }

            report::uniques(&uniques, limit);
        }
        Command::UniquesFor {
            build,
            slot,
            budget,
            by,
            limit,
            league,
        } => {
            let (pob, loaded) = open(&build)?;
            let league = shop::league(league, loaded.character.as_ref())?;
            let rates = market::currency_rates(&league)?;
            let budget = shop::budget_in_divines(budget.as_ref(), &rates)?;
            let level = pob.info()?.level;
            let slot = pob.uniques_for_slot(&slot)?;
            let prices = market::unique_prices(&league)?;
            let mut uniques = shop::rank_uniques(slot.candidates, &prices, budget, level, by);
            uniques.truncate(limit);

            if json {
                return print_json(&uniques);
            }

            report::uniques_for(&slot.slot, &league, &uniques, &rates, by);
        }
        Command::Trade(command) => trade_command(command, json)?,
        Command::Price {
            build,
            item,
            tolerance,
            status,
            fetch,
            league,
            open,
        } => {
            let (character, items) = match (build, item) {
                (Some(build), _) => {
                    let (pob, loaded) = self::open(&build)?;
                    (loaded.character, pob.equipped_for_price()?)
                }
                (None, Some(path)) => {
                    // Read before PoB starts, which changes the working directory.
                    let text = read_input(&path)?;
                    let pob = Pob::start(&pob::ensure_installed()?)?;
                    (None, vec![pob.price_item(&text)?])
                }
                (None, None) => unreachable!("clap requires a build or --item"),
            };
            let league = shop::league(league, character.as_ref())?;
            let rates = market::currency_rates(&league)?;
            let uniques = if items.iter().any(|i| i.unique) {
                market::unique_prices(&league)?
            } else {
                Vec::new()
            };
            let client = trade::Client::new()?;
            let mut checks = Vec::new();

            for item in items {
                let mut check = price::check(
                    &client,
                    &league,
                    &rates,
                    item,
                    status_id(status),
                    tolerance / 100.0,
                    fetch,
                )?;

                if check.item.unique {
                    check.ninja =
                        market::unique_price(&uniques, &check.item.name, &check.item.base)
                            .map(|p| p.divines);
                }

                checks.push(check);
            }

            if open {
                open::that(&checks[0].url)
                    .with_context(|| format!("cannot open {}", checks[0].url))?;
            }

            if json {
                return print_json(&serde_json::json!({ "league": league, "items": checks }));
            }

            report::price_checks(&checks, &league, &rates);
        }
        Command::Prices { league, limit } => {
            let league = league.map_or_else(market::current_league, Ok)?;
            let rates = market::currency_rates(&league)?;

            if json {
                return print_json(&rates);
            }

            report::prices(&rates, limit);
        }
    }

    Ok(())
}

fn trade_command(command: TradeCommand, json: bool) -> Result<()> {
    match command {
        TradeCommand::Login => {
            let input = if io::stdin().is_terminal() {
                rpassword::prompt_password("Paste your POESESSID cookie: ")?
            } else {
                read_input("-")?
            };
            let value = session::normalise(&input)?;

            if !trade::Client::with_session(Some(value.clone())).session_valid()? {
                anyhow::bail!(
                    "pathofexile.com did not accept this session; log in again and copy a fresh POESESSID"
                );
            }

            session::save(&value)?;
            println!("Logged in. The session is stored for trade searches.");
        }
        TradeCommand::Status => {
            let client = trade::Client::new()?;

            if !client.has_session() {
                println!("Not logged in: run `poe2 trade login`.");
            } else if client.session_valid()? {
                println!("Logged in; the session works.");
            } else {
                println!("The stored session expired: run `poe2 trade login`.");
            }
        }
        TradeCommand::Logout => {
            if session::delete()? {
                println!("Deleted the stored session.");
            } else {
                println!("No session was stored.");
            }
        }
        TradeCommand::Search {
            build,
            slot,
            trade,
            name,
            base,
            jewel_type,
            require,
            exclude,
            count,
            sum,
            filter,
            sort,
            min_weight,
            query,
            show_query,
            open,
        } => {
            let raw = query
                .map(|path| -> Result<serde_json::Value> {
                    serde_json::from_str(&read_input(&path)?)
                        .with_context(|| format!("{path} is not JSON"))
                })
                .transpose()?;
            let additions = Additions {
                require,
                exclude,
                count,
                sum,
                filter,
                sort,
                min_weight,
                raw,
            };
            let (pob, loaded) = self::open(&build)?;
            let market = TradeMarket::new(&trade, loaded.character.as_ref())?;
            let target = if name.is_some() || base.is_some() {
                Target::Item { name, base }
            } else {
                Target::Weighted(jewel_type)
            };
            let search = market.query(&pob, &slot, &trade, &target, &additions)?;

            if show_query {
                return print_json(&search.1.query);
            }

            let client = trade::Client::new()?;
            let found = market.search(&pob, &client, search, trade.by, trade.fetch)?;

            if open {
                open::that(&found.url).with_context(|| format!("cannot open {}", found.url))?;
            }

            if json {
                return print_json(
                    &serde_json::json!({ "league": market.league, "search": found }),
                );
            }

            report::trade_search(&found, &market.league, &market.rates, trade.by);
        }
        TradeCommand::Scan { build, slot, trade } => {
            let slots = if slot.is_empty() {
                SCAN_SLOTS.iter().map(|s| s.to_string()).collect()
            } else {
                slot
            };
            let (pob, loaded) = open(&build)?;
            let market = TradeMarket::new(&trade, loaded.character.as_ref())?;
            let client = trade::Client::new()?;
            let mut found = Vec::new();
            let mut skipped = Vec::new();

            for slot in &slots {
                // A slot PoB cannot search is skipped; a failing search stops the scan.
                let target = Target::Weighted(JewelType::Base);

                match market.query(&pob, slot, &trade, &target, &Additions::default()) {
                    Ok(search) => {
                        found.push(market.search(&pob, &client, search, trade.by, trade.fetch)?)
                    }
                    Err(error) => skipped.push((slot.clone(), error.to_string())),
                }
            }

            if json {
                let skipped: Vec<_> = skipped
                    .iter()
                    .map(|(slot, reason)| serde_json::json!({ "slot": slot, "reason": reason }))
                    .collect();
                return print_json(&serde_json::json!({
                    "league": market.league,
                    "searches": found,
                    "skipped": skipped,
                }));
            }

            report::trade_scan(
                &found,
                &skipped,
                &market.league,
                &market.rates,
                market.budget,
                trade.by,
            );
        }
        TradeCommand::Stats { query, limit } => {
            let pob = Pob::start(&pob::ensure_installed()?)?;
            let stats = pob.trade_stats(&query)?;

            if json {
                return print_json(&stats);
            }

            report::trade_stats(&stats, limit);
        }
    }

    Ok(())
}

/// Prices and the budget for trade searches in one league.
struct TradeMarket {
    league: String,
    rates: Rates,
    /// In divines.
    budget: Option<f64>,
}

impl TradeMarket {
    fn new(trade: &TradeArgs, character: Option<&Character>) -> Result<Self> {
        let league = shop::league(trade.league.clone(), character)?;
        let rates = market::currency_rates(&league)?;
        let budget = shop::budget_in_divines(trade.budget.as_ref(), &rates)?;
        Ok(Self {
            league,
            rates,
            budget,
        })
    }

    /// The query for a slot, PoB's weighted one or one by name or base, with
    /// the user's additions.
    fn query(
        &self,
        pob: &Pob,
        slot: &str,
        trade: &TradeArgs,
        target: &Target,
        additions: &Additions,
    ) -> Result<(String, Search)> {
        let slot = pob.trade_slot(slot)?;
        let exalted = self
            .rates
            .divines("exalted")
            .context("poe.ninja has no exalted orb rate")?;
        // Without a budget, a very high cap still leaves out unpriced listings.
        let max_exalted = self.budget.map_or(1e7, |divines| divines / exalted);
        let resolve = |text: &str| {
            pob.trade_stats(text)?
                .into_iter()
                .next()
                .with_context(|| format!("no trade site stat matches '{text}'"))
        };

        let search = match target {
            Target::Weighted(jewel_type) => {
                let generated = pob.trade_query(&TradeQueryRequest {
                    slot: slot.clone(),
                    by: rank_id(trade.by).into(),
                    status: status_id(trade.status).into(),
                    max_exalted,
                    jewel_type: jewel_type_id(*jewel_type).into(),
                })?;
                additions.apply(&generated.query, generated.weights, resolve)?
            }
            Target::Item { name, base } => {
                let unique = name.as_deref().map(|n| pob.find_unique(n)).transpose()?;

                if let (Some(unique), Some(base)) = (&unique, base)
                    && !unique.base.eq_ignore_ascii_case(base)
                {
                    anyhow::bail!("{} is a {}, not a {base}", unique.name, unique.base);
                }

                let query = query::item_query(
                    status_id(trade.status),
                    unique.as_ref().map(|u| u.name.as_str()),
                    unique.as_ref().map(|u| u.base.as_str()).or(base.as_deref()),
                    max_exalted,
                    pob.info()?.level,
                );
                additions.apply(&query, Vec::new(), resolve)?
            }
        };
        Ok((slot, search))
    }

    /// Run a search, then calculate and rank the best matches it finds.
    fn search(
        &self,
        pob: &Pob,
        client: &trade::Client,
        (slot, mut search): (String, Search),
        by: Rank,
        fetch: usize,
    ) -> Result<SlotSearch> {
        // What a search costs the site depends on its stats and groups, so a
        // search it finds too complex is retried with fewer of PoB's weights.
        let result = loop {
            match client.search(&self.league, &search.query) {
                Err(error) if error.is::<trade::TooComplex>() && search.weights.len() > 1 => {
                    search.keep_weights(search.weights.len() * 2 / 3);
                }
                result => break result,
            }
        };
        let result = result.map_err(|error| {
            if client.has_session() {
                error
            } else {
                error.context("weighted searches need a session: run `poe2 trade login`")
            }
        })?;
        let ids: Vec<String> = result.result.iter().take(fetch).cloned().collect();
        let bodies = client.fetch(&result.id, &ids)?;
        let listings = shop::rank_listings(
            pob.evaluate_listings(&slot, &bodies)?,
            &self.rates,
            self.budget,
            by,
        );
        Ok(SlotSearch {
            url: trade::search_url(&self.league, &result.id),
            slot,
            total: result.total,
            search,
            listings,
        })
    }
}

fn rank_id(rank: Rank) -> &'static str {
    match rank {
        Rank::Balanced => "balanced",
        Rank::Dps => "dps",
        Rank::Ehp => "ehp",
    }
}

fn jewel_type_id(jewel_type: JewelType) -> &'static str {
    match jewel_type {
        JewelType::Base => "Base",
        JewelType::Radius => "Radius",
    }
}

fn status_id(status: Status) -> &'static str {
    match status {
        Status::Securable => "securable",
        Status::Available => "available",
        Status::Online => "online",
        Status::Any => "any",
    }
}

/// Read the build source, start PoB and load the build into it.
fn open(build: &str) -> Result<(Pob, LoadedBuild)> {
    let source = Source::read(build)?;
    let pob = Pob::start(&pob::ensure_installed()?)?;
    let loaded = source.fetch(&pob)?;
    pob.load(&loaded.xml)?;
    Ok((pob, loaded))
}

/// A file's contents, or stdin for `-`.
fn read_input(path: &str) -> Result<String> {
    if path == "-" {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        return Ok(text);
    }

    fs::read_to_string(path).with_context(|| format!("cannot read {path}"))
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
