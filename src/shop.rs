//! Buying advice: PoB's verdict on an item joined with what it costs.

use anyhow::{Context, Result};
use poe2::market::{self, Price, Rates};
use poe2::ninja::Character;
use poe2::pob::model::UniqueCandidate;
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
