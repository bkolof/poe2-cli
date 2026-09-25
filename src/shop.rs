//! Buying advice: PoB's verdict on an item joined with what it costs.

use anyhow::{Context, Result};
use poe2::market::{self, Price, Rates};
use poe2::ninja::Character;
use poe2::pob::model::{Listing, UniqueCandidate};
use poe2::trade::query::Search;
use serde::Serialize;

use crate::Rank;
use crate::report::score;

/// The league to price in: the one given, the character's own, or the
/// current challenge league.
pub fn league(given: Option<String>, character: Option<&Character>) -> Result<String> {
    match (given, character) {
        (Some(league), _) => Ok(league),
        (None, Some(character)) => Ok(character.league.clone()),
        (None, None) => market::current_league(),
    }
}

pub fn budget_in_divines(budget: Option<&Price>, rates: &Rates) -> Result<Option<f64>> {
    budget
        .map(|b| {
            rates
                .to_divines(b.amount, &b.currency)
                .with_context(|| format!("poe.ninja has no rate for {}", b.currency))
        })
        .transpose()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedUnique {
    #[serde(flatten)]
    pub candidate: UniqueCandidate,
    /// poe.ninja's price in divines, if it has one.
    pub divines: Option<f64>,
    pub listings: Option<u32>,
    /// Whether the character's level is high enough to equip it.
    pub equippable: bool,
}

/// The uniques that improve the build, affordable within the budget, best
/// first. Without a budget, uniques poe.ninja has no price for are kept.
pub fn rank_uniques(
    candidates: Vec<UniqueCandidate>,
    prices: &[market::UniquePrice],
    budget: Option<f64>,
    level: u32,
    by: Rank,
) -> Vec<PricedUnique> {
    let mut priced: Vec<PricedUnique> = candidates
        .into_iter()
        .filter(|c| score(&c.impact, by) > 0.0)
        .map(|candidate| {
            let price = market::unique_price(prices, &candidate.name, &candidate.base);
            PricedUnique {
                divines: price.map(|p| p.divines),
                listings: price.map(|p| p.listing_count),
                equippable: candidate.level_required <= level,
                candidate,
            }
        })
        .filter(|u| match (budget, u.divines) {
            (Some(budget), Some(price)) => price <= budget,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .collect();

    priced
        .sort_by(|a, b| score(&b.candidate.impact, by).total_cmp(&score(&a.candidate.impact, by)));
    priced
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricedListing {
    #[serde(flatten)]
    pub listing: Listing,
    /// The asking price in divines, when poe.ninja knows the currency.
    pub divines: Option<f64>,
    pub score: f64,
    /// No other listing is both at least as good and cheaper.
    pub best_value: bool,
}

/// A trade search for one slot and the listings it found, calculated.
#[derive(Debug, Serialize)]
pub struct SlotSearch {
    pub slot: String,
    /// The search on the trade site, where the listings can be bought.
    pub url: String,
    /// How many listings match, as far as the site counts.
    pub total: Option<u64>,
    #[serde(flatten)]
    pub search: Search,
    pub listings: Vec<PricedListing>,
}

/// Price the listings and mark the best value ones: going up in price, each
/// must beat every cheaper listing. Listings the character cannot wear, and
/// ones that do not improve the build, are never best value. The trade site
/// converts prices at its own rates, so listings over the budget at
/// poe.ninja's rates are left out here.
pub fn rank_listings(
    listings: Vec<Listing>,
    rates: &Rates,
    budget: Option<f64>,
    by: Rank,
) -> Vec<PricedListing> {
    let mut priced: Vec<PricedListing> = listings
        .into_iter()
        .map(|listing| PricedListing {
            divines: match (listing.amount, &listing.currency) {
                (Some(amount), Some(currency)) => rates.to_divines(amount, currency),
                _ => None,
            },
            score: score(&listing.impact, by),
            best_value: false,
            listing,
        })
        .filter(|l| match (budget, l.divines) {
            (Some(budget), Some(price)) => price <= budget,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .collect();

    priced.sort_by(|a, b| {
        let price = |p: &PricedListing| p.divines.unwrap_or(f64::INFINITY);
        price(a)
            .total_cmp(&price(b))
            .then(b.score.total_cmp(&a.score))
    });
    let mut best_so_far = 0.0;

    for listing in &mut priced {
        let eligible = listing.divines.is_some() && listing.listing.meets_requirements;

        if eligible && listing.score > best_so_far {
            listing.best_value = true;
            best_so_far = listing.score;
        }
    }

    priced
}

#[cfg(test)]
mod tests {
    use super::*;
    use poe2::market::Currency;
    use poe2::pob::model::Impact;

    fn listing(id: &str, exalted: f64, dps_percent: f64, meets_requirements: bool) -> Listing {
        Listing {
            id: id.into(),
            name: id.into(),
            amount: Some(exalted),
            currency: Some("exalted".into()),
            seller: None,
            whisper: None,
            item_text: String::new(),
            meets_requirements,
            impact: Impact {
                points: 1,
                dps_percent,
                ehp_percent: 0.0,
                life: 0.0,
                energy_shield: 0.0,
            },
        }
    }

    #[test]
    fn marks_the_best_value_at_each_price() {
        let rates = Rates {
            league: "Test".into(),
            currencies: vec![Currency {
                id: "exalted".into(),
                name: "Exalted Orb".into(),
                divines: 0.01,
            }],
        };
        let listings = vec![
            listing("cheap", 10.0, 2.0, true),
            listing("worse and dearer", 20.0, 1.0, true),
            listing("better", 50.0, 5.0, true),
            listing("unwearable", 5.0, 9.0, false),
            listing("downgrade", 1.0, -3.0, true),
            listing("over budget", 80.0, 9.0, true),
        ];

        let ranked = rank_listings(listings, &rates, Some(0.6), Rank::Dps);

        let best: Vec<&str> = ranked
            .iter()
            .filter(|l| l.best_value)
            .map(|l| l.listing.id.as_str())
            .collect();
        assert_eq!(best, ["cheap", "better"]);
    }
}
