//! Text output for people. `--json` bypasses all of this.

use poe2::ninja::{Character, CharacterSummary};
use poe2::pob::model::{
    BuildInfo, GemInfo, Impact, ModInfo, SidebarRow, SkillDps, SlotUpgrades, TreeSuggestion,
    UniqueInfo, WhatIf, WhatIfRequest,
};

use crate::Rank;

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
    skills.sort_by(|a, b| total_dps(b).total_cmp(&total_dps(a)));
    println!(
        "  {:<28}{:>12}{:>12}{:>12}{:>9}",
        "Skill", "DPS", "Hit DPS", "DoT DPS", "Speed"
    );

    for skill in &skills {
        let marker = if skill.main { "*" } else { " " };
        let dps = if skill.minion_dps > 0.0 {
            format!("{} (minions)", thousands(skill.minion_dps))
        } else {
            thousands_or_dash(skill.combined_dps)
        };

        println!(
            "{marker} {:<28}{:>12}{:>12}{:>12}{:>9}",
            skill.name,
            dps,
            thousands_or_dash(skill.hit_dps),
            thousands_or_dash(skill.dot_dps),
            if skill.speed > 0.0 {
                format!("{:.2}/s", skill.speed)
            } else {
                "-".into()
            },
        );
    }

    println!("\n* main skill in the build");
}

pub fn what_if(request: &WhatIfRequest, result: &WhatIf) {
    let mut passives = Vec::new();

    if !request.allocate.is_empty() {
        passives.push(format!(
            "allocating {} ({})",
            request.allocate.join(", "),
            plural(result.points.added, "point")
        ));
    }

    if !request.unallocate.is_empty() {
        passives.push(format!(
            "unallocating {} ({})",
            request.unallocate.join(", "),
            plural(result.points.removed, "point")
        ));
    }

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
