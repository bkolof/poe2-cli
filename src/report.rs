//! Text output for people. `--json` bypasses all of this.

use poe2::ninja::{Character, CharacterSummary};
use poe2::pob::model::{
    BuildInfo, GemInfo, Impact, ModInfo, SidebarRow, SkillDps, SlotUpgrades, TreeSuggestion,
    UniqueInfo, WhatIf,
};

use crate::Rank;
use crate::shop::{PricedListing, PricedUnique, SlotSearch};
use poe2::market::Rates;
use poe2::pob::model::TradeStat;
use poe2::trade::price::{ENOUGH_LISTINGS, ESTIMATE_FROM, Offer, PriceCheck};

pub fn header(info: &BuildInfo, character: Option<&Character>) {
    let class = info.ascendancy.as_deref().unwrap_or(&info.class);

    match character {
        Some(c) => println!("{}, level {} {class} ({})", c.name, info.level, c.league),
        None => println!("Level {} {class}", info.level),
    }

    println!(
        "Main skill: {}\n",
        info.main_skill.as_deref().unwrap_or("none")
    );
}

pub fn sidebar(rows: &[SidebarRow]) {
    for row in rows {
        match (&row.label, &row.value) {
            (Some(label), Some(value)) => println!("{label:<32}{value}"),
            (Some(label), None) => println!("{label}"),
            _ => println!(),
        }
    }
}

pub fn skills(mut skills: Vec<SkillDps>) {
    // Per-use skills have no comparable rate, so they sort after the others.
    skills.sort_by(|a, b| {
        a.per_use
            .cmp(&b.per_use)
            .then(total_dps(b).total_cmp(&total_dps(a)))
    });
    println!(
        "  {:<32}{:>12}{:>12}{:>10}{:>12}{:>9}",
        "Skill", "DPS", "Hit DPS", "DoT DPS", "Avg damage", "Speed"
    );

    for skill in &skills {
        let marker = if skill.main { "*" } else { " " };
        let name = match &skill.granted_by {
            Some(source) => format!("{} ({source})", skill.name),
            None => skill.name.clone(),
        };
        let (dps, hit_dps) = if skill.per_use {
            ("per use".to_string(), "-".to_string())
        } else if skill.minion_dps > 0.0 {
            (
                format!("{} (minions)", thousands(skill.minion_dps)),
                "-".to_string(),
            )
        } else {
            (
                thousands_or_dash(skill.combined_dps),
                thousands_or_dash(skill.hit_dps),
            )
        };

        println!(
            "{marker} {name:<32}{dps:>12}{hit_dps:>12}{:>10}{:>12}{:>9}",
            thousands_or_dash(skill.dot_dps),
            thousands_or_dash(skill.average_damage),
            if skill.speed > 0.0 {
                format!("{:.2}/s", skill.speed)
            } else {
                "-".into()
            },
        );
    }

    println!("\n* main skill in the build");

    if skills.iter().any(|s| s.per_use) {
        println!(
            "per use: PoB rates this skill by its damage per use (Avg damage), not per second"
        );
    }
}

pub fn what_if(result: &WhatIf) {
    let passives: Vec<String> = result
        .passives
        .iter()
        .map(|p| {
            let ascendancy = p
                .ascendancy
                .as_ref()
                .map(|a| format!("{a} "))
                .unwrap_or_default();
            let verb = if p.action == "allocate" {
                "allocating"
            } else {
                "unallocating"
            };
            format!(
                "{verb} {} ({ascendancy}id {}, {})",
                p.name,
                p.id,
                plural(p.points, "point")
            )
        })
        .collect();

    for (i, entry) in result.results.iter().enumerate() {
        if i > 0 {
            println!();
        }

        let mut action = Vec::new();

        if let (Some(item), Some(slot)) = (&result.item, &entry.slot) {
            let replacing = entry
                .replacing
                .as_ref()
                .map(|r| format!(", replacing {r}"))
                .unwrap_or_default();
            action.push(format!("equipping {item} in {slot}{replacing}"));
        }

        action.extend(passives.iter().cloned());
        println!("{}:", capitalise(&action.join(" and ")));

        if entry.changes.is_empty() {
            println!("  no stat changes");
        }

        for change in &entry.changes {
            let percent = change
                .percent
                .map(|p| format!(" ({p:+.1}%)"))
                .unwrap_or_default();
            let verdict = if change.better { "better" } else { "worse" };
            println!(
                "  {:>12} {:<32}{} -> {}{percent}  [{verdict}]",
                change.diff, change.label, change.before, change.after
            );
        }
    }
}

pub fn tree(suggestions: &[TreeSuggestion], by: Rank) {
    if suggestions.is_empty() {
        println!("No passive within reach improves the build.");
        return;
    }

    println!(
        "Ranked by {} per point. Totals include the path to each passive.\n",
        rank_name(by)
    );

    for suggestion in suggestions {
        println!(
            "{:>6.1}/pt {}  {} ({}, {}, id {})",
            score(&suggestion.impact, by),
            impact_columns(&suggestion.impact),
            suggestion.name,
            suggestion.kind,
            plural(suggestion.impact.points, "point"),
            suggestion.id,
        );

        if !suggestion.stats.is_empty() {
            println!("{:>52}{}", "", suggestion.stats.join("; "));
        }
    }
}

pub fn upgrades(upgrades: &SlotUpgrades, by: Rank) {
    println!(
        "{} in {} (item level {}): the best tier of each mod it can roll, one at a time, mid roll",
        upgrades.item, upgrades.slot, upgrades.item_level
    );
    println!(
        "Free affixes: {} prefix, {} suffix{}\n",
        upgrades.free_prefixes,
        upgrades.free_suffixes,
        if upgrades.corrupted {
            " (corrupted: the item cannot be modified)"
        } else {
            ""
        }
    );

    if upgrades.upgrades.is_empty() {
        println!("No mod improves the build.");
        return;
    }

    println!("Ranked by {}:\n", rank_name(by));

    for upgrade in &upgrades.upgrades {
        let fits = if upgrade.fits {
            ""
        } else {
            ", needs a free slot"
        };
        println!(
            "{:>6.1}  {}  {} ({}, level {}{fits})",
            score(&upgrade.impact, by),
            impact_columns(&upgrade.impact),
            upgrade.text,
            upgrade.affix,
            upgrade.level
        );
    }
}

pub fn characters(account: &str, characters: &[CharacterSummary]) {
    for c in characters {
        let current = if c.is_current { " (current)" } else { "" };
        println!(
            "{:<24}{:>4}  {:<22}{:<20}{}",
            format!("{}{current}", c.name),
            c.level,
            c.class_name.as_deref().unwrap_or("?"),
            c.league,
            c.profile_url(account)
        );
    }
}

pub fn mods(mut mods: Vec<ModInfo>) {
    if mods.is_empty() {
        println!("No matching mods.");
        return;
    }

    mods.sort_by(|a, b| (&a.kind, &a.group, a.level).cmp(&(&b.kind, &b.group, b.level)));
    let mut last_group = None;

    for m in &mods {
        let group = (m.kind.as_str(), m.group.as_deref());

        if last_group != Some(group) {
            let affix = m.affix.as_deref().unwrap_or("");
            println!(
                "\n{} {} {affix} [{}]",
                m.kind,
                m.group.as_deref().unwrap_or(&m.id),
                m.tags.join(", ")
            );
            last_group = Some(group);
        }

        let name = m.name.as_deref().filter(|n| !n.is_empty()).unwrap_or("-");
        println!(
            "  level {:>3}  {:<28}{}",
            m.level.unwrap_or(0),
            name,
            m.lines.join(" / ")
        );
    }
}

pub fn gems(mut gems: Vec<GemInfo>) {
    if gems.is_empty() {
        println!("No matching gems.");
        return;
    }

    gems.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, gem) in gems.iter().enumerate() {
        if i > 0 {
            println!();
        }

        let kind = if gem.support {
            "Support"
        } else {
            gem.kind.as_deref().unwrap_or("Skill")
        };
        let r = &gem.requirements;
        println!(
            "{} ({kind}): {}",
            gem.name,
            gem.tags.as_deref().unwrap_or("")
        );
        println!(
            "  requires str {} / dex {} / int {}, max level {}",
            r.str,
            r.dex,
            r.int,
            gem.max_level.unwrap_or(0)
        );

        if let Some(description) = &gem.description {
            println!("  {description}");
        }
    }
}

pub fn uniques(uniques: &[UniqueInfo], limit: usize) {
    if uniques.is_empty() {
        println!("No matching uniques.");
        return;
    }

    for (i, unique) in uniques.iter().take(limit).enumerate() {
        if i > 0 {
            println!();
        }

        println!("{}", unique.text);
    }

    if uniques.len() > limit {
        let rest: Vec<&str> = uniques[limit..].iter().map(|u| u.name.as_str()).collect();
        println!("\n{} more: {}", rest.len(), rest.join(", "));
    }
}

pub fn uniques_for(slot: &str, league: &str, uniques: &[PricedUnique], rates: &Rates, by: Rank) {
    println!(
        "Uniques for {slot}, ranked by {}, priced by poe.ninja in {league}:\n",
        rank_name(by)
    );

    if uniques.is_empty() {
        println!("No unique improves the build within the budget.");
        return;
    }

    for unique in uniques {
        let c = &unique.candidate;
        let price = match (unique.divines, unique.listings) {
            (Some(divines), Some(listings)) => {
                format!("{} ({listings} listed)", rates.format(divines))
            }
            _ => "no price".into(),
        };
        let level = if unique.equippable {
            String::new()
        } else {
            format!(", needs level {}", c.level_required)
        };
        println!(
            "{:>6.1}  {}  {:<24}{} ({}{level})",
            score(&c.impact, by),
            impact_columns(&c.impact),
            price,
            c.name,
            c.base
        );
    }
}

pub fn trade_search(found: &SlotSearch, league: &str, rates: &Rates, by: Rank) {
    let search = &found.search;
    let min = search
        .min_weight
        .map_or(String::new(), |min| format!(", sum at least {min:.1}"));
    let dropped = match search.dropped_weights {
        0 => String::new(),
        n => format!(" ({n} less important ones left out to fit the site's limits)"),
    };
    if search.weights.is_empty() {
        let item = &search.query["query"];
        let wanted: Vec<&str> = [&item["name"], &item["type"]]
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        println!("{} in {league}: {}", found.slot, wanted.join(", "));
    } else {
        println!(
            "{} in {league}: PoB weighted {} stats{min}{dropped}",
            found.slot,
            search.weights.len()
        );
    }

    for group in &search.groups {
        let stats = |weighted: bool| -> String {
            group
                .stats
                .iter()
                .map(|s| match (weighted, s.weight) {
                    (true, Some(weight)) => format!("{} x{weight}", s.text),
                    _ => format!("{} {}", s.text, s.range).trim_end().to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        match group.kind {
            "and" => println!("  requires {}", stats(false)),
            "not" => println!("  excludes {}", stats(false)),
            "count" => println!(
                "  at least {} of: {}",
                group.range.min.unwrap_or(1.0),
                stats(false)
            ),
            "weight" => println!("  weighted sum {}: {}", group.range, stats(true)),
            _ => {}
        }
    }

    if !search.filters.is_empty() {
        println!("  filters: {}", search.filters.join(", "));
    }

    println!(
        "{} listings match, sorted by {}; calculated the best {} with PoB, ranked by {}.\n",
        found.total.map_or("Some".into(), |t| t.to_string()),
        search.sort,
        found.listings.len(),
        rank_name(by)
    );

    let best: Vec<&PricedListing> = found.listings.iter().filter(|l| l.best_value).collect();

    if best.is_empty() {
        println!("No listing improves the build.");
    } else {
        println!("Best value at each price:\n");
    }

    for listing in &best {
        println!("{}", listing_line(listing, rates));

        if let Some(whisper) = &listing.listing.whisper {
            println!("{:>19}{whisper}", "");
        }
    }

    let unwearable = found
        .listings
        .iter()
        .filter(|l| !l.listing.meets_requirements)
        .count();

    if unwearable > 0 {
        println!("\n{unwearable} more would need higher attributes than the build has.");
    }

    println!("\nTrade site: {}", found.url);
}

/// Each slot's best value listings, the slots with the biggest upgrade first.
pub fn trade_scan(
    found: &[SlotSearch],
    skipped: &[(String, String)],
    league: &str,
    rates: &Rates,
    budget: Option<f64>,
    by: Rank,
) {
    let within = budget.map_or(String::new(), |b| format!(" within {}", rates.format(b)));
    println!(
        "Best upgrades per slot in {league}{within}, ranked by {}.",
        rank_name(by)
    );
    println!("Each slot's biggest upgrade comes first, then cheaper ones.\n");

    fn best(search: &SlotSearch) -> Vec<&PricedListing> {
        let mut best: Vec<&PricedListing> =
            search.listings.iter().filter(|l| l.best_value).collect();
        best.reverse();
        best
    }

    let mut slots: Vec<&SlotSearch> = found.iter().collect();
    slots.sort_by(|a, b| {
        let top = |s: &SlotSearch| best(s).first().map_or(0.0, |l| l.score);
        top(b).total_cmp(&top(a))
    });

    for search in &slots {
        let listings = best(search);

        if listings.is_empty() {
            println!("{:<12} no upgrade found", search.slot);
        }

        for (i, listing) in listings.iter().enumerate() {
            let slot = if i == 0 { search.slot.as_str() } else { "" };
            println!("{slot:<12}{}", listing_line(listing, rates));
        }
    }

    for (slot, reason) in skipped {
        println!("{slot:<12} skipped: {reason}");
    }

    println!("\nTrade site searches:");

    for search in &slots {
        println!("  {:<12}{}", search.slot, search.url);
    }
}

pub fn trade_stats(stats: &[TradeStat], limit: usize) {
    if stats.is_empty() {
        println!("No trade site stat matches.");
    }

    for stat in stats.iter().take(limit) {
        println!(
            "{:<9} {:<45} {}",
            stat.kind,
            stat.id,
            stat.text.replace('\n', " / ")
        );
    }

    if stats.len() > limit {
        println!("... and {} more", stats.len() - limit);
    }
}

pub fn price_checks(checks: &[PriceCheck], league: &str, rates: &Rates) {
    match checks {
        [check] => price_check(check, league, rates),
        _ => price_summary(checks, league, rates),
    }
}

fn price_check(check: &PriceCheck, league: &str, rates: &Rates) {
    let item = &check.item;
    println!(
        "{}, {} ({}) in {league}",
        item.name,
        item.base,
        item.rarity.to_lowercase()
    );
    let within = format!("{}%", (check.tolerance * 100.0).round());

    if let Some(note) = &item.note {
        println!("{note}.");
    }

    if item.unique {
        println!("Searched by name{}.", corrupted(item.corrupted));
    } else if item.mods.is_empty() {
        println!(
            "Searched by category and defences{}.",
            corrupted(item.corrupted)
        );
    } else if check.required == item.mods.len() {
        println!(
            "Searched for all {} mods, each within {within} of its value{}:",
            item.mods.len(),
            corrupted(item.corrupted)
        );
    } else {
        println!(
            "Too few listings had every mod, so searched for any {} of these {}, each within {within} of its value{}:",
            check.required,
            item.mods.len(),
            corrupted(item.corrupted)
        );
    }

    for m in &item.mods {
        println!("  {}", m.text);
    }

    for (id, value) in &item.defences {
        println!(
            "  {id} at least {}",
            (value * (1.0 - check.tolerance)).floor()
        );
    }

    if !item.unsearchable.is_empty() {
        println!("Not on the trade site: {}", item.unsearchable.join(", "));
    }

    let total = check.total.map_or("Some".into(), |t| t.to_string());

    if check.offers.is_empty() {
        println!("\n{total} listings match.");
    } else {
        println!("\n{total} listings match. The cheapest:\n");
    }

    for offer in &check.offers {
        println!(
            "{:>10}  {}{}",
            offer_price(offer, rates),
            offer.name,
            offer.indexed.as_deref().map_or(String::new(), |i| format!(
                ", listed {}",
                i.split('T').next().unwrap_or(i)
            ))
        );
    }

    println!();

    match check.estimate {
        Some(estimate) => println!(
            "Estimate: about {}, the median of the cheapest {}.",
            rates.format(estimate),
            check
                .offers
                .iter()
                .filter(|o| o.divines.is_some())
                .count()
                .min(ESTIMATE_FROM)
        ),
        None => println!("No priced listings to estimate from."),
    }

    if let Some(ninja) = check.ninja {
        println!("poe.ninja: {}.", rates.format(ninja));
    }

    if check.total.unwrap_or(0) < ENOUGH_LISTINGS {
        println!("Few listings match, so the estimate is rough.");
    }

    println!("\nTrade site: {}", check.url);
}

fn price_summary(checks: &[PriceCheck], league: &str, rates: &Rates) {
    println!("Equipped items priced on the trade site in {league}:\n");
    println!("{:<14}{:>10}  {:>8}  Item", "Slot", "Estimate", "Listings");

    for check in checks {
        let item = &check.item;
        let estimate = check.estimate.map_or("?".into(), |e| rates.format(e));
        let matched = if !item.unique && check.required < item.mods.len() {
            format!(" ({} of {} mods)", check.required, item.mods.len())
        } else {
            String::new()
        };
        println!(
            "{:<14}{estimate:>10}  {:>8}  {}, {}{matched}",
            item.slot,
            check.total.map_or("?".into(), |t| t.to_string()),
            item.name,
            item.base
        );
    }

    let total: f64 = checks.iter().filter_map(|c| c.estimate).sum();
    let unpriced = checks.iter().filter(|c| c.estimate.is_none()).count();
    println!("\nTotal: about {}", rates.format(total));

    match unpriced {
        0 => {}
        1 => println!("1 item had no priced listings and is left out."),
        n => println!("{n} items had no priced listings and are left out."),
    }

    println!("Items with few listings, or with some mods left out, have rough estimates.");
}

fn corrupted(corrupted: bool) -> &'static str {
    if corrupted { ", corrupted" } else { "" }
}

fn offer_price(offer: &Offer, rates: &Rates) -> String {
    match offer.divines {
        Some(divines) => rates.format(divines),
        None => format!("{} {}", offer.amount, offer.currency),
    }
}

fn listing_line(listing: &PricedListing, rates: &Rates) -> String {
    let price = listing.divines.map_or("?".into(), |d| rates.format(d));
    format!(
        "{price:>9}  {:>6.1}  {}  {}",
        listing.score,
        impact_columns(&listing.listing.impact),
        listing.listing.name
    )
}

pub fn prices(rates: &Rates, limit: usize) {
    println!("Currency in {}, from poe.ninja:\n", rates.league);
    let exalted = rates.divines("exalted");

    for currency in rates.currencies.iter().take(limit) {
        let in_exalted = exalted
            .map(|e| format!("{:>12.2} ex", currency.divines / e))
            .unwrap_or_default();
        println!(
            "{:<32}{:>12.4} div{in_exalted}",
            currency.name, currency.divines
        );
    }
}

/// The ranking score per passive point.
pub fn score(impact: &Impact, by: Rank) -> f64 {
    let total = match by {
        Rank::Balanced => impact.dps_percent + impact.ehp_percent,
        Rank::Dps => impact.dps_percent,
        Rank::Ehp => impact.ehp_percent,
    };
    total / f64::from(impact.points.max(1))
}

fn impact_columns(impact: &Impact) -> String {
    format!(
        "{:>7.1}% DPS {:>7.1}% EHP {:>7} life {:>6} ES",
        impact.dps_percent,
        impact.ehp_percent,
        signed(impact.life),
        signed(impact.energy_shield)
    )
}

fn rank_name(by: Rank) -> &'static str {
    match by {
        Rank::Balanced => "DPS % + EHP %",
        Rank::Dps => "DPS %",
        Rank::Ehp => "EHP %",
    }
}

fn total_dps(skill: &SkillDps) -> f64 {
    skill.combined_dps.max(skill.minion_dps)
}

fn signed(value: f64) -> String {
    let rounded = value.round() as i64;
    if rounded > 0 {
        format!("+{}", thousands(value))
    } else {
        thousands(value)
    }
}

fn thousands_or_dash(value: f64) -> String {
    if value.abs() < 0.5 {
        "-".into()
    } else {
        thousands(value)
    }
}

fn thousands(value: f64) -> String {
    let rounded = value.round() as i64;
    let digits = rounded.unsigned_abs().to_string();
    let mut grouped = String::new();

    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }

        grouped.push(digit);
    }

    if rounded < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn plural(count: u32, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0.0), "0");
        assert_eq!(thousands(999.4), "999");
        assert_eq!(thousands(15106.07), "15,106");
        assert_eq!(thousands(-1234567.0), "-1,234,567");
    }

    #[test]
    fn scores_per_point() {
        let impact = Impact {
            points: 2,
            dps_percent: 10.0,
            ehp_percent: -4.0,
            life: 0.0,
            energy_shield: 0.0,
        };

        assert_eq!(score(&impact, Rank::Balanced), 3.0);
        assert_eq!(score(&impact, Rank::Dps), 5.0);
        assert_eq!(score(&impact, Rank::Ehp), -2.0);
    }
}
