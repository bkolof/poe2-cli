mod report;

use std::fs;
use std::io::{self, Read};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use poe2::ninja;
use poe2::pob::model::WhatIfRequest;
use poe2::pob::{self, Pob};
use poe2::source::{LoadedBuild, Source};
use serde::Serialize;

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

            report::what_if(&request, &result);
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
    }

    Ok(())
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
