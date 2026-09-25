use anyhow::Result;
use clap::{Parser, Subcommand};
use poe2::ninja;
use poe2::pob::{self, CalcResult, Pob};

/// Path of Exile 2 build analysis, backed by headless Path of Building.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Characters on public poe.ninja profiles
    #[command(subcommand)]
    Char(CharCommand),
}

#[derive(Subcommand)]
enum CharCommand {
    /// Calculate a character's stats with PoB
    Stats {
        /// https://poe.ninja/poe2/profile/<account>/<league>/character/<name>
        url: String,
        /// Print every PoB output stat as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print the character's PoB build code, for the PoB GUI's "Import from code"
    Export {
        /// https://poe.ninja/poe2/profile/<account>/<league>/character/<name>
        url: String,
    },
}

enum Format {
    Count,
    Rate,
    Percent,
    Decimal1Percent,
    Multiplier,
}

/// (label, PoB output keys, format). Several keys share one row, joined by " / ".
const SUMMARY: &[(&str, &[&str], Format)] = &[
    ("DPS", &["CombinedDPS"], Format::Count),
    ("Average hit", &["AverageDamage"], Format::Count),
    ("Speed", &["Speed"], Format::Rate),
    ("Crit chance", &["CritChance"], Format::Decimal1Percent),
    ("Crit multiplier", &["CritMultiplier"], Format::Multiplier),
    ("Life", &["Life"], Format::Count),
    ("Energy shield", &["EnergyShield"], Format::Count),
    ("Mana", &["Mana"], Format::Count),
    ("Spirit unreserved", &["SpiritUnreserved"], Format::Count),
    ("Evasion", &["Evasion"], Format::Count),
    ("Armour", &["Armour"], Format::Count),
    (
        "Fire / cold / lightning / chaos res",
        &["FireResist", "ColdResist", "LightningResist", "ChaosResist"],
        Format::Percent,
    ),
    ("EHP", &["TotalEHP"], Format::Count),
    (
        "Max hit (phys / chaos)",
        &["PhysicalMaximumHitTaken", "ChaosMaximumHitTaken"],
        Format::Count,
    ),
];

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Char(CharCommand::Stats { url, json }) => char_stats(&url, json),
        Command::Char(CharCommand::Export { url }) => {
            let character = ninja::fetch_character(&ninja::parse_profile_url(&url)?)?;
            println!("{}", character.path_of_building_export);
            Ok(())
        }
    }
}

fn char_stats(url: &str, json: bool) -> Result<()> {
    let character = ninja::fetch_character(&ninja::parse_profile_url(url)?)?;
    let build_xml = pob::decode_build_code(&character.path_of_building_export)?;
    let result = Pob::start(&pob::ensure_installed()?)?.calculate(&build_xml)?;

    if json {
        let output = serde_json::json!({ "mainSkill": result.main_skill, "stats": result.stats });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!(
        "{}, level {} {} ({})",
        character.name, character.level, character.class, character.league
    );
    println!(
        "Main skill: {}\n",
        result.main_skill.as_deref().unwrap_or("unknown")
    );
    print_summary(&result);
    Ok(())
}

fn print_summary(result: &CalcResult) {
    for (label, keys, format) in SUMMARY {
        let values: Option<Vec<String>> = keys
            .iter()
            .map(|key| result.number(key).map(|value| format_value(value, format)))
            .collect();

        if let Some(values) = values {
            println!("{label:<38}{}", values.join(" / "));
        }
    }
}

fn format_value(value: f64, format: &Format) -> String {
    match format {
        Format::Count => thousands(value.round() as i64),
        Format::Rate => format!("{value:.2}/s"),
        Format::Percent => format!("{value:.0}%"),
        Format::Decimal1Percent => format!("{value:.1}%"),
        Format::Multiplier => format!("{value:.2}x"),
    }
}

fn thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::new();

    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }

        grouped.push(digit);
    }

    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(15106), "15,106");
        assert_eq!(thousands(-1234567), "-1,234,567");
    }
}
