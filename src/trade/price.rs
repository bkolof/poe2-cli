//! Price checks: listings like an item, found by a search that relaxes until
//! enough match, and an estimate from the cheapest of them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Client, search_url};
use crate::market::Rates;
use crate::pob::model::{PriceItem, PriceMod, WeaponStats};

mod rules;

/// Enough listings to price from; with fewer, the search relaxes.
pub const ENOUGH_LISTINGS: u64 = 10;
/// How many of the cheapest listings the estimate is the median of. Asking
/// prices above the cheapest few are often far above what items sell for,
/// especially where few listings match.
pub const ESTIMATE_FROM: usize = 5;
/// Searches per item at most, one per step of relaxation.
const MAX_SEARCHES: usize = 4;

/// How many of the searched stats each search requires, strictest first: all
/// of them, then one fewer at a time, down to half.
pub fn required_steps(mods: usize) -> Vec<usize> {
    if mods == 0 {
        return vec![0];
    }

    (mods.div_ceil(2)..=mods).rev().take(MAX_SEARCHES).collect()
}

/// Pseudo totals the trade site keeps, which traders price by: where a total
/// comes from does not matter.
const TOTALS: &[(&str, &str)] = &[
    (
        "pseudo.pseudo_total_elemental_resistance",
        "% total Elemental Resistance",
    ),
    (
        "pseudo.pseudo_total_chaos_resistance",
        "% total to Chaos Resistance",
    ),
    ("pseudo.pseudo_total_life", " total maximum Life"),
    ("pseudo.pseudo_total_mana", " total maximum Mana"),
    ("pseudo.pseudo_total_strength", " total to Strength"),
    ("pseudo.pseudo_total_dexterity", " total to Dexterity"),
    ("pseudo.pseudo_total_intelligence", " total to Intelligence"),
];

/// Which totals a mod adds to, and how many times its value: "+10% to all
/// Elemental Resistances" adds 30 to the elemental total.
fn totals_of(text: &str) -> Vec<(usize, f64)> {
    let rest = text.trim_start_matches('+');
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    let [
        elemental,
        chaos,
        life,
        mana,
        strength,
        dexterity,
        intelligence,
    ] = [0, 1, 2, 3, 4, 5, 6];

    match rest {
        "% to Fire Resistance" | "% to Cold Resistance" | "% to Lightning Resistance" => {
            vec![(elemental, 1.0)]
        }
        "% to all Elemental Resistances" => vec![(elemental, 3.0)],
        "% to Fire and Cold Resistances"
        | "% to Fire and Lightning Resistances"
        | "% to Cold and Lightning Resistances" => vec![(elemental, 2.0)],
        "% to Chaos Resistance" => vec![(chaos, 1.0)],
        " to maximum Life" => vec![(life, 1.0)],
        " to maximum Mana" => vec![(mana, 1.0)],
        " to Strength" => vec![(strength, 1.0)],
        " to Dexterity" => vec![(dexterity, 1.0)],
        " to Intelligence" => vec![(intelligence, 1.0)],
        " to all Attributes" => vec![(strength, 1.0), (dexterity, 1.0), (intelligence, 1.0)],
        " to Strength and Dexterity" => vec![(strength, 1.0), (dexterity, 1.0)],
        " to Strength and Intelligence" => vec![(strength, 1.0), (intelligence, 1.0)],
        " to Dexterity and Intelligence" => vec![(dexterity, 1.0), (intelligence, 1.0)],
        _ => Vec::new(),
    }
}

/// Whether a mod adds to an armour piece's own defences, which the search
/// covers through the item's armour, evasion and energy shield.
fn local_defence(text: &str) -> bool {
    let defences = [
        "Armour",
        "Evasion Rating",
        "maximum Energy Shield",
        "Energy Shield",
        "Armour and Evasion",
        "Armour and Energy Shield",
        "Evasion and Energy Shield",
        "Armour, Evasion and Energy Shield",
    ];
    let flat =
        text.starts_with('+') && defences.iter().any(|d| text.ends_with(&format!(" to {d}")));
    let increased = defences
        .iter()
        .any(|d| text.ends_with(&format!("% increased {d}")));
    flat || increased
}

/// Whether a mod adds to an attack weapon's own damage or attack speed, which
/// the search covers through its DPS. Critical hit chance is not in the DPS
/// filters, so it is judged like any other mod.
fn local_weapon(text: &str) -> bool {
    let adds = text.starts_with("Adds ")
        && ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
            .iter()
            .any(|t| {
                text.ends_with(&format!(" {t} Damage")) || text.ends_with(&format!(" {t} damage"))
            });
    adds || text.ends_with("% increased Physical Damage")
        || text.ends_with("% increased Attack Speed")
}

/// A mod a price check leaves out, and why.
#[derive(Debug, Clone, Serialize)]
pub struct LeftOut {
    pub text: String,
    pub reason: String,
}

/// What a price check searches for, and what it leaves out.
#[derive(Debug, Default)]
pub struct Selection {
    pub stats: Vec<PriceMod>,
    pub left_out: Vec<LeftOut>,
}

/// The stats a price check searches for, compared the way traders compare
/// items. Requiring each mod on its own finds only items at least as good in
/// every one, which are few and dearer, so:
/// - resistance, life, mana and attribute mods are summed into the site's
///   pseudo totals;
/// - an armour piece's defence mods, and an attack weapon's damage, attack
///   speed and critical hit mods, are left to its defences and DPS;
/// - the rest are judged by `rules::judge`: stats that set prices are kept,
///   filler is left out.
pub fn select(item: &PriceItem) -> Selection {
    let armour = !item.defences.is_empty();
    let weapon = item.weapon.as_ref().is_some_and(|w| w.total_dps > 0.0);
    let mut totals = [0.0; TOTALS.len()];
    let mut selection = Selection::default();
    let mut leave_out = |text: &str, reason: &str| {
        selection.left_out.push(LeftOut {
            text: text.into(),
            reason: reason.into(),
        })
    };
    let mut stats = Vec::new();

    for m in &item.mods {
        // An aggregated mod reads "A, B"; its parts are the same stat.
        let first = m.text.split(", ").next().unwrap_or(&m.text);
        let folds = totals_of(first);

        if armour && local_defence(first) {
            leave_out(&m.text, "counted in the item's defences");
            continue;
        }

        if weapon && local_weapon(first) {
            leave_out(&m.text, "counted in the weapon's DPS");
            continue;
        }

        match m.value {
            Some(value) if !folds.is_empty() && !m.invert && !m.option => {
                for (total, times) in folds {
                    totals[total] += value * times;
                }
            }
            _ => match rules::judge(m, item) {
                Ok(()) => stats.push(m.clone()),
                Err(reason) => leave_out(&m.text, &reason),
            },
        }
    }

    for (index, total) in totals.iter().enumerate() {
        if *total > 0.0 {
            let (id, label) = TOTALS[index];
            stats.push(PriceMod {
                text: format!("+{total}{label}"),
                ids: vec![id.into()],
                value: Some(*total),
                kind: "pseudo".into(),
                ..Default::default()
            });
        }
    }

    selection.stats = stats;
    selection
}

/// The weapon DPS a search requires: physical or elemental DPS when one makes
/// up most of the damage, and the total when no one kind does, or when a
/// physical weapon also has a real share of elemental damage.
fn weapon_filters(weapon: &WeaponStats) -> Vec<(&'static str, f64)> {
    let share = |dps: f64| dps / weapon.total_dps;
    let mut filters = Vec::new();

    if share(weapon.physical_dps) >= rules::MAIN_DAMAGE_SHARE {
        filters.push(("pdps", weapon.physical_dps));
    } else if share(weapon.elemental_dps) >= rules::MAIN_DAMAGE_SHARE {
        filters.push(("edps", weapon.elemental_dps));
    }

    if filters.is_empty() || share(weapon.elemental_dps) >= rules::MINOR_DAMAGE_SHARE {
        filters.push(("dps", weapon.total_dps));
    }

    filters
}

/// The search for listings like the item, cheapest first: a unique by name,
/// anything else by category, defences and `required` of the searched stats,
/// each at `1 - tolerance` of its value or more.
pub fn query(
    item: &PriceItem,
    stats: &[PriceMod],
    status: &str,
    tolerance: f64,
    required: usize,
) -> Value {
    let mut query = json!({
        "query": { "status": { "option": status }, "stats": [], "filters": {} },
        "sort": { "price": "asc" },
    });
    let q = &mut query["query"];
    q["filters"]["misc_filters"]["filters"]["corrupted"] =
        json!({ "option": item.corrupted.to_string() });

    if item.unique {
        q["name"] = json!(item.name);

        if let Some(base) = &item.trade_base {
            q["type"] = json!(base);
        }

        return query;
    }

    // Uniques and other rarities with the same stats are priced differently.
    q["filters"]["type_filters"]["filters"]["rarity"] =
        json!({ "option": item.rarity.to_lowercase() });

    if let Some(category) = &item.category {
        q["filters"]["type_filters"]["filters"]["category"] = json!({ "option": category });
    }

    // Normal and magic items are worth their base more than their mods, and
    // crafting bases their item level.
    if let Some(base) = &item.trade_base
        && (item.rarity == "NORMAL" || item.rarity == "MAGIC")
    {
        q["type"] = json!(base);
    }

    if let Some(level) = rules::base_item_level(item) {
        q["filters"]["type_filters"]["filters"]["ilvl"] = json!({ "min": level });
    }

    for (id, value) in &item.defences {
        q["filters"]["equipment_filters"]["filters"][id] =
            json!({ "min": (value * (1.0 - tolerance)).floor() });
    }

    if let Some(weapon) = item.weapon.as_ref().filter(|w| w.total_dps > 0.0) {
        for (id, dps) in weapon_filters(weapon) {
            q["filters"]["equipment_filters"]["filters"][id] =
                json!({ "min": (dps * (1.0 - tolerance)).floor() });
        }
    }

    let searched = stats;
    let stats = q["stats"].as_array_mut().expect("stats is an array");

    if required == searched.len() {
        let single: Vec<Value> = searched
            .iter()
            .filter(|m| m.ids.len() == 1)
            .map(|m| stat_filter(m, &m.ids[0], tolerance))
            .collect();

        if !single.is_empty() {
            stats.push(json!({ "type": "and", "filters": single }));
        }

        // A text matching several trade stats needs any one of them.
        for m in searched.iter().filter(|m| m.ids.len() > 1) {
            let filters: Vec<Value> = m
                .ids
                .iter()
                .map(|id| stat_filter(m, id, tolerance))
                .collect();
            stats.push(json!({ "type": "count", "value": { "min": 1 }, "filters": filters }));
        }
    } else {
        let filters: Vec<Value> = searched
            .iter()
            .flat_map(|m| m.ids.iter().map(move |id| stat_filter(m, id, tolerance)))
            .collect();
        stats.push(json!({ "type": "count", "value": { "min": required }, "filters": filters }));
    }

    query
}

fn stat_filter(m: &PriceMod, id: &str, tolerance: f64) -> Value {
    let value = match m.value {
        Some(value) if m.option => json!({ "min": value, "max": value }),
        Some(value) => {
            let bound = ((value - value.abs() * tolerance) * 100.0).floor() / 100.0;

            if m.invert {
                json!({ "max": -bound })
            } else {
                json!({ "min": bound })
            }
        }
        None => json!({}),
    };
    json!({ "id": id, "value": value })
}

/// A listing's asking price.
#[derive(Debug, Serialize)]
pub struct Offer {
    pub name: String,
    pub amount: f64,
    pub currency: String,
    /// In divines, when poe.ninja knows the currency.
    pub divines: Option<f64>,
    pub seller: Option<String>,
    /// When it was listed.
    pub indexed: Option<String>,
}

/// The priced listings in fetch responses, in the order fetched.
pub fn offers(bodies: &[String], rates: &Rates) -> Result<Vec<Offer>> {
    #[derive(Deserialize)]
    struct Body {
        result: Vec<Option<Entry>>,
    }

    #[derive(Deserialize)]
    struct Entry {
        listing: ListingInfo,
        item: ItemInfo,
    }

    #[derive(Deserialize)]
    struct ListingInfo {
        price: Option<PriceInfo>,
        account: Option<Account>,
        indexed: Option<String>,
    }

    #[derive(Deserialize)]
    struct PriceInfo {
        amount: f64,
        currency: String,
    }

    #[derive(Deserialize)]
    struct Account {
        name: String,
    }

    #[derive(Deserialize)]
    struct ItemInfo {
        #[serde(default)]
        name: String,
        #[serde(rename = "typeLine")]
        type_line: String,
    }

    let mut offers = Vec::new();

    for body in bodies {
        let body: Body =
            serde_json::from_str(body).context("unexpected listings from the trade site")?;

        for entry in body.result.into_iter().flatten() {
            let Some(price) = entry.listing.price else {
                continue;
            };
            let name = [entry.item.name.as_str(), entry.item.type_line.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            offers.push(Offer {
                name,
                divines: rates.to_divines(price.amount, &price.currency),
                amount: price.amount,
                currency: price.currency,
                seller: entry.listing.account.map(|a| a.name),
                indexed: entry.listing.indexed,
            });
        }
    }

    Ok(offers)
}

/// The median of the cheapest few offers poe.ninja can price, so that one
/// unusually cheap listing does not set the price.
pub fn estimate(offers: &[Offer]) -> Option<f64> {
    let mut prices: Vec<f64> = offers.iter().filter_map(|o| o.divines).collect();

    if prices.is_empty() {
        return None;
    }

    prices.sort_by(f64::total_cmp);
    prices.truncate(ESTIMATE_FROM);
    let middle = prices.len() / 2;

    if prices.len().is_multiple_of(2) {
        Some((prices[middle - 1] + prices[middle]) / 2.0)
    } else {
        Some(prices[middle])
    }
}

/// An item's price check.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCheck {
    pub item: PriceItem,
    /// The stats searched for, from its mods.
    pub searched: Vec<PriceMod>,
    /// Mods not searched for, and why.
    pub left_out: Vec<LeftOut>,
    /// How many of them the listings have, at the tolerance or better.
    pub required: usize,
    pub tolerance: f64,
    pub total: Option<u64>,
    pub url: String,
    pub offers: Vec<Offer>,
    /// In divines.
    pub estimate: Option<f64>,
    /// poe.ninja's price for a unique, in divines.
    pub ninja: Option<f64>,
    /// The search that found the listings.
    pub query: Value,
}

/// Search for listings like the item, relaxing until enough match, and
/// estimate its price from the cheapest `fetch` of them.
pub fn check(
    client: &Client,
    league: &str,
    rates: &Rates,
    item: PriceItem,
    status: &str,
    tolerance: f64,
    fetch: usize,
) -> Result<PriceCheck> {
    let selection = if item.unique {
        Selection::default()
    } else {
        select(&item)
    };
    let searched = selection.stats;
    let mut found = None;

    for required in required_steps(searched.len()) {
        let query = query(&item, &searched, status, tolerance, required);
        let result = client.search(league, &query)?;
        let enough = result.total.unwrap_or(result.result.len() as u64) >= ENOUGH_LISTINGS;
        found = Some((required, result, query));

        if enough {
            break;
        }
    }

    let (required, result, query) = found.expect("there is always a first search");
    let ids: Vec<String> = result.result.iter().take(fetch).cloned().collect();
    let mut offers = if ids.is_empty() {
        Vec::new()
    } else {
        offers(&client.fetch(&result.id, &ids)?, rates)?
    };
    // The site sorts by its own exchange rates; these are poe.ninja's.
    offers.sort_by(|a, b| {
        let price = |o: &Offer| o.divines.unwrap_or(f64::INFINITY);
        price(a).total_cmp(&price(b))
    });
    Ok(PriceCheck {
        url: search_url(league, &result.id),
        total: result.total,
        estimate: estimate(&offers),
        ninja: None,
        query,
        item,
        searched,
        left_out: selection.left_out,
        required,
        tolerance,
        offers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Currency;

    fn item() -> PriceItem {
        let m = |text: &str, ids: &[&str], value: f64, invert: bool| PriceMod {
            text: text.into(),
            ids: ids.iter().map(|id| id.to_string()).collect(),
            value: Some(value),
            invert,
            kind: "explicit".into(),
            ..Default::default()
        };
        PriceItem {
            slot: "Boots".into(),
            name: "Grim Pace".into(),
            base: "Quickslip Shoes".into(),
            trade_base: Some("Quickslip Shoes".into()),
            note: None,
            rarity: "RARE".into(),
            unique: false,
            category: Some("armour.boots".into()),
            corrupted: false,
            mods: vec![
                m("+100 to maximum Life", &["explicit.life"], 100.0, false),
                m(
                    "30% increased Movement Speed",
                    &["explicit.ms"],
                    30.0,
                    false,
                ),
                m(
                    "10% reduced Charm Charges used",
                    &["explicit.charm"],
                    10.0,
                    true,
                ),
                m(
                    "+20 to Spirit",
                    &["explicit.spirit_a", "explicit.spirit_b"],
                    20.0,
                    false,
                ),
            ],
            unsearchable: Vec::new(),
            defences: [("es".to_string(), 205.0)].into(),
            item_level: Some(80),
            weapon: None,
        }
    }

    #[test]
    fn relaxes_down_to_half_the_mods() {
        assert_eq!(required_steps(0), [0]);
        assert_eq!(required_steps(1), [1]);
        assert_eq!(required_steps(4), [4, 3, 2]);
        assert_eq!(required_steps(9), [9, 8, 7, 6]);
    }

    #[test]
    fn requires_every_mod_first() {
        let q = query(&item(), &item().mods, "available", 0.1, 4);

        assert_eq!(q["sort"], json!({ "price": "asc" }));
        assert_eq!(
            q["query"]["filters"]["equipment_filters"]["filters"]["es"],
            json!({ "min": 184.0 })
        );
        let stats = &q["query"]["stats"];
        assert_eq!(stats[0]["type"], "and");
        assert_eq!(
            stats[0]["filters"][0],
            json!({ "id": "explicit.life", "value": { "min": 90.0 } })
        );
        assert_eq!(stats[0]["filters"][2]["value"], json!({ "max": -9.0 }));
        assert_eq!(stats[1]["type"], "count");
        assert_eq!(stats[1]["filters"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn then_counts_how_many_match() {
        let q = query(&item(), &item().mods, "available", 0.1, 3);
        let stats = q["query"]["stats"].as_array().unwrap();

        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0]["type"], "count");
        assert_eq!(stats[0]["value"]["min"], 3);
        assert_eq!(stats[0]["filters"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn folds_mods_into_totals_and_defences() {
        let m = |text: &str, value: f64| PriceMod {
            text: text.into(),
            ids: vec![format!("explicit.{text}")],
            value: Some(value),
            kind: "explicit".into(),
            ..Default::default()
        };
        let mut armour = item();
        armour.mods = vec![
            m("5% increased Movement Speed", 5.0),
            m("+133 to Evasion Rating, +5 to Evasion Rating", 138.0),
            m("+41 to maximum Energy Shield", 41.0),
            m("76% increased Evasion and Energy Shield", 76.0),
            m("+16 to Dexterity", 16.0),
            m("+21% to Fire Resistance", 21.0),
            m("+37% to Lightning Resistance", 37.0),
            m("+10% to all Elemental Resistances", 10.0),
            m("+5 to all Attributes", 5.0),
        ];

        let texts: Vec<String> = select(&armour).stats.into_iter().map(|s| s.text).collect();
        assert_eq!(
            texts,
            [
                "5% increased Movement Speed",
                "+88% total Elemental Resistance",
                "+5 total to Strength",
                "+21 total to Dexterity",
                "+5 total to Intelligence",
            ]
        );

        // Without defences of its own, an item's defence mods are searched.
        armour.defences.clear();
        assert!(
            select(&armour)
                .stats
                .iter()
                .any(|s| s.text.contains("Evasion Rating"))
        );
    }

    #[test]
    fn compares_attack_weapons_by_dps() {
        let weapon = |physical: f64, elemental: f64| WeaponStats {
            physical_dps: physical,
            elemental_dps: elemental,
            total_dps: physical + elemental,
            crit_chance: 10.0,
            attack_rate: 1.4,
        };
        assert_eq!(weapon_filters(&weapon(300.0, 20.0)), [("pdps", 300.0)]);
        assert_eq!(
            weapon_filters(&weapon(10.0, 300.0)),
            [("edps", 300.0), ("dps", 310.0)]
        );
        assert_eq!(
            weapon_filters(&weapon(211.0, 97.0)),
            [("pdps", 211.0), ("dps", 308.0)]
        );
        assert_eq!(weapon_filters(&weapon(180.0, 120.0)), [("dps", 300.0)]);

        assert!(local_weapon("Adds 33 to 55 Cold Damage"));
        assert!(local_weapon("32% increased Physical Damage"));
        assert!(local_weapon("16% increased Attack Speed"));
        assert!(!local_weapon("+2.5% to Critical Hit Chance"));
        assert!(!local_weapon("Adds 14 to 23 Cold damage to Attacks"));
        assert!(!local_weapon("86% increased Elemental Damage with Attacks"));

        let mut staff = item();
        staff.defences.clear();
        staff.weapon = Some(weapon(180.0, 120.0));
        staff.mods.push(PriceMod {
            text: "Adds 33 to 55 Cold Damage".into(),
            ids: vec!["explicit.cold".into()],
            value: Some(33.0),
            ..Default::default()
        });
        let selection = select(&staff);
        assert!(
            selection
                .left_out
                .iter()
                .any(|l| l.reason == "counted in the weapon's DPS")
        );
        let q = query(
            &staff,
            &selection.stats,
            "available",
            0.1,
            selection.stats.len(),
        );
        assert_eq!(
            q["query"]["filters"]["equipment_filters"]["filters"]["dps"],
            json!({ "min": 270.0 })
        );
    }

    #[test]
    fn searches_uniques_by_name() {
        let mut unique = item();
        unique.unique = true;
        unique.name = "Atziri's Step".into();
        unique.base = "Cinched Boots".into();
        unique.trade_base = Some("Cinched Boots".into());
        let q = query(&unique, &[], "available", 0.1, 0);

        assert_eq!(q["query"]["name"], "Atziri's Step");
        assert_eq!(q["query"]["type"], "Cinched Boots");
        assert!(q["query"]["stats"].as_array().unwrap().is_empty());
        assert_eq!(
            q["query"]["filters"]["misc_filters"]["filters"]["corrupted"]["option"],
            "false"
        );
    }

    #[test]
    fn estimates_the_median_of_the_cheapest() {
        let rates = Rates {
            league: "Test".into(),
            currencies: vec![Currency {
                id: "exalted".into(),
                name: "Exalted Orb".into(),
                divines: 0.01,
            }],
        };
        let listing = |amount: f64, currency: &str| {
            json!({
                "listing": { "price": { "amount": amount, "currency": currency }, "account": { "name": "x" } },
                "item": { "name": "", "typeLine": "Quickslip Shoes" }
            })
        };
        let body = json!({ "result": [
            listing(1.0, "exalted"),
            listing(20.0, "exalted"),
            listing(30.0, "exalted"),
            listing(5.0, "unknown"),
            { "listing": { "price": null }, "item": { "typeLine": "Quickslip Shoes" } },
        ] })
        .to_string();

        let offers = offers(&[body], &rates).unwrap();
        assert_eq!(offers.len(), 4);
        assert_eq!(offers[0].name, "Quickslip Shoes");
        assert_eq!(estimate(&offers), Some(0.2));
        assert_eq!(estimate(&[]), None);

        // A thin market's high asks do not pull the estimate up.
        let asks: Vec<Offer> = [70.0, 1.0, 45.0, 2.0, 1.0, 24.0, 3.0, 15.0, 2.0, 5.0]
            .iter()
            .map(|&divines| Offer {
                name: String::new(),
                amount: divines,
                currency: "divine".into(),
                divines: Some(divines),
                seller: None,
                indexed: None,
            })
            .collect();
        assert_eq!(estimate(&asks), Some(2.0));
    }
}
