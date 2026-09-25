//! Price checks: listings like an item, found by a search that relaxes until
//! enough match, and an estimate from the cheapest of them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Client, search_url};
use crate::market::Rates;
use crate::pob::model::{PriceItem, PriceMod};

/// Enough listings to price from; with fewer, the search relaxes.
pub const ENOUGH_LISTINGS: u64 = 10;
/// Searches per item at most, one per step of relaxation.
const MAX_SEARCHES: usize = 4;

/// How many of the item's mods each search requires, strictest first: all of
/// them, then one fewer at a time, down to half.
pub fn required_steps(mods: usize) -> Vec<usize> {
    if mods == 0 {
        return vec![0];
    }

    (mods.div_ceil(2)..=mods).rev().take(MAX_SEARCHES).collect()
}

/// The search for listings like the item, cheapest first: a unique by name,
/// anything else by category, defences and `required` of its mods, each at
/// `1 - tolerance` of its value or more.
pub fn query(item: &PriceItem, status: &str, tolerance: f64, required: usize) -> Value {
    let mut query = json!({
        "query": { "status": { "option": status }, "stats": [], "filters": {} },
        "sort": { "price": "asc" },
    });
    let q = &mut query["query"];
    q["filters"]["misc_filters"]["filters"]["corrupted"] =
        json!({ "option": item.corrupted.to_string() });

    if item.unique {
        q["name"] = json!(item.name);
        q["type"] = json!(item.base);
        return query;
    }

    if let Some(category) = &item.category {
        q["filters"]["type_filters"]["filters"]["category"] = json!({ "option": category });
    }

    // Normal and magic items are worth their base more than their mods.
    if item.rarity == "NORMAL" || item.rarity == "MAGIC" {
        q["type"] = json!(item.base);
    }

    for (id, value) in &item.defences {
        q["filters"]["equipment_filters"]["filters"][id] =
            json!({ "min": (value * (1.0 - tolerance)).floor() });
    }

    let stats = q["stats"].as_array_mut().expect("stats is an array");

    if required == item.mods.len() {
        let single: Vec<Value> = item
            .mods
            .iter()
            .filter(|m| m.ids.len() == 1)
            .map(|m| stat_filter(m, &m.ids[0], tolerance))
            .collect();

        if !single.is_empty() {
            stats.push(json!({ "type": "and", "filters": single }));
        }

        // A text matching several trade stats needs any one of them.
        for m in item.mods.iter().filter(|m| m.ids.len() > 1) {
            let filters: Vec<Value> = m
                .ids
                .iter()
                .map(|id| stat_filter(m, id, tolerance))
                .collect();
            stats.push(json!({ "type": "count", "value": { "min": 1 }, "filters": filters }));
        }
    } else {
        let filters: Vec<Value> = item
            .mods
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

/// The median of the cheapest offers poe.ninja can price, so that one
/// unusually cheap listing does not set the price.
pub fn estimate(offers: &[Offer]) -> Option<f64> {
    let mut prices: Vec<f64> = offers
        .iter()
        .filter_map(|o| o.divines)
        .take(ENOUGH_LISTINGS as usize)
        .collect();

    if prices.is_empty() {
        return None;
    }

    prices.sort_by(f64::total_cmp);
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
    /// How many of its mods the listings have, at the tolerance or better.
    pub required: usize,
    pub tolerance: f64,
    pub total: Option<u64>,
    pub url: String,
    pub offers: Vec<Offer>,
    /// In divines.
    pub estimate: Option<f64>,
    /// poe.ninja's price for a unique, in divines.
    pub ninja: Option<f64>,
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
    let mut found = None;

    for required in required_steps(item.mods.len()) {
        let result = client.search(league, &query(&item, status, tolerance, required))?;
        let enough = result.total.unwrap_or(result.result.len() as u64) >= ENOUGH_LISTINGS;
        found = Some((required, result));

        if enough {
            break;
        }
    }

    let (required, result) = found.expect("there is always a first search");
    let ids: Vec<String> = result.result.iter().take(fetch).cloned().collect();
    let offers = if ids.is_empty() {
        Vec::new()
    } else {
        offers(&client.fetch(&result.id, &ids)?, rates)?
    };
    Ok(PriceCheck {
        url: search_url(league, &result.id),
        total: result.total,
        estimate: estimate(&offers),
        ninja: None,
        item,
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
            option: false,
        };
        PriceItem {
            slot: "Boots".into(),
            name: "Grim Pace".into(),
            base: "Quickslip Shoes".into(),
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
        let q = query(&item(), "available", 0.1, 4);

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
        let q = query(&item(), "available", 0.1, 3);
        let stats = q["query"]["stats"].as_array().unwrap();

        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0]["type"], "count");
        assert_eq!(stats[0]["value"]["min"], 3);
        assert_eq!(stats[0]["filters"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn searches_uniques_by_name() {
        let mut unique = item();
        unique.unique = true;
        unique.name = "Atziri's Step".into();
        unique.base = "Cinched Boots".into();
        let q = query(&unique, "available", 0.1, 0);

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
    }
}
