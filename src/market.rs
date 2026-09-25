//! Market data from poe.ninja: the current league, currency exchange rates
//! and unique item prices. Responses are cached on disk for an hour, about
//! as often as poe.ninja refreshes them.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::ninja;

const BASE_URL: &str = "https://poe.ninja/poe2/api";
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// The poe.ninja economy categories that hold unique items.
const UNIQUE_TYPES: &[&str] = &[
    "UniqueWeapons",
    "UniqueArmours",
    "UniqueAccessories",
    "UniqueJewels",
    "UniqueFlasks",
    "UniqueCharms",
];

/// The current challenge league, the first economy league poe.ninja lists.
pub fn current_league() -> Result<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct IndexState {
        economy_leagues: Vec<League>,
    }

    #[derive(Deserialize)]
    struct League {
        name: String,
    }

    let body = cached_get("index-state", &format!("{BASE_URL}/data/index-state"))?;
    let state: IndexState = serde_json::from_str(&body)?;
    state
        .economy_leagues
        .into_iter()
        .next()
        .map(|league| league.name)
        .context("poe.ninja lists no economy leagues")
}

/// Currency values in divine orbs, keyed by the currency ids the trade site
/// also uses (`divine`, `exalted`, `chaos`, ...).
#[derive(Debug, Serialize)]
pub struct Rates {
    pub league: String,
    pub currencies: Vec<Currency>,
}

#[derive(Debug, Serialize)]
pub struct Currency {
    pub id: String,
    pub name: String,
    pub divines: f64,
}

impl Rates {
    pub fn divines(&self, currency: &str) -> Option<f64> {
        self.currencies
            .iter()
            .find(|c| c.id == currency)
            .map(|c| c.divines)
    }

    pub fn to_divines(&self, amount: f64, currency: &str) -> Option<f64> {
        self.divines(currency).map(|value| amount * value)
    }

    /// A price in divines, written in exalted orbs below one divine.
    pub fn format(&self, divines: f64) -> String {
        match self.divines("exalted") {
            Some(exalted) if divines < 1.0 => format!("{:.0} ex", divines / exalted),
            _ => format!("{divines:.1} div"),
        }
    }
}

pub fn currency_rates(league: &str) -> Result<Rates> {
    #[derive(Deserialize)]
    struct Overview {
        lines: Vec<Line>,
        items: Vec<Item>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Line {
        id: String,
        primary_value: f64,
    }

    #[derive(Deserialize)]
    struct Item {
        id: String,
        name: String,
    }

    let url = format!(
        "{BASE_URL}/economy/exchange/current/overview?league={}&type=Currency",
        encode(league)
    );
    let overview: Overview =
        serde_json::from_str(&cached_get(&format!("{league}-currency"), &url)?)?;

    if overview.lines.is_empty() {
        bail!("poe.ninja has no currency prices for league '{league}'");
    }

    let names: HashMap<String, String> = overview
        .items
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect();
    let mut currencies: Vec<Currency> = overview
        .lines
        .into_iter()
        .map(|line| Currency {
            name: names
                .get(&line.id)
                .cloned()
                .unwrap_or_else(|| line.id.clone()),
            id: line.id,
            divines: line.primary_value,
        })
        .collect();

    // Divine is the unit, so poe.ninja does not list it as a line.
    if !currencies.iter().any(|c| c.id == "divine") {
        currencies.push(Currency {
            id: "divine".into(),
            name: "Divine Orb".into(),
            divines: 1.0,
        });
    }

    currencies.sort_by(|a, b| b.divines.total_cmp(&a.divines));
    Ok(Rates {
        league: league.to_string(),
        currencies,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UniquePrice {
    pub name: String,
    pub base_type: String,
    /// Price in divine orbs.
    #[serde(rename = "primaryValue")]
    pub divines: f64,
    pub listing_count: u32,
}

/// Every unique poe.ninja prices in the league.
pub fn unique_prices(league: &str) -> Result<Vec<UniquePrice>> {
    #[derive(Deserialize)]
    struct Overview {
        lines: Vec<UniquePrice>,
    }

    let mut prices = Vec::new();

    for kind in UNIQUE_TYPES {
        let url = format!(
            "{BASE_URL}/economy/stash/current/item/overview?league={}&type={kind}",
            encode(league)
        );
        let overview: Overview =
            serde_json::from_str(&cached_get(&format!("{league}-{kind}"), &url)?)?;
        prices.extend(overview.lines);
    }

    Ok(prices)
}

/// The price of a unique: the listing on the same base if poe.ninja has one
/// (a unique can come on several bases), else the cheapest one.
pub fn unique_price<'a>(
    prices: &'a [UniquePrice],
    name: &str,
    base: &str,
) -> Option<&'a UniquePrice> {
    let same_name = || prices.iter().filter(|p| p.name.eq_ignore_ascii_case(name));

    same_name()
        .find(|p| p.base_type.eq_ignore_ascii_case(base))
        .or_else(|| same_name().min_by(|a, b| a.divines.total_cmp(&b.divines)))
}

/// An amount of currency as the user writes it: `5` (divines), `5div`,
/// `300ex`, `20c`, `2 divine`.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    pub amount: f64,
    /// A trade site currency id.
    pub currency: String,
}

impl std::str::FromStr for Price {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim().to_lowercase();
        let split = text
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(text.len());
        let (number, unit) = text.split_at(split);
        let amount: f64 = number
            .parse()
            .map_err(|_| format!("'{text}' does not start with an amount"))?;
        let currency = match unit.trim() {
            "" | "d" | "div" | "divs" | "divine" | "divines" => "divine",
            "e" | "ex" | "exa" | "exalt" | "exalts" | "exalted" => "exalted",
            "c" | "chaos" => "chaos",
            other => return Err(format!("unknown currency '{other}': use div, ex or c")),
        };

        Ok(Self {
            amount,
            currency: currency.into(),
        })
    }
}

fn encode(text: &str) -> String {
    text.replace(' ', "%20")
}

/// A response from the disk cache when it is younger than the TTL, else from
/// the network. When the network fails, a stale cached copy is still better
/// than nothing.
fn cached_get(key: &str, url: &str) -> Result<String> {
    let path = cache_dir()?.join(format!("{}.json", key.replace([' ', '/'], "_")));
    let age = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok());

    if age.is_some_and(|age| age < CACHE_TTL) {
        return Ok(fs::read_to_string(&path)?);
    }

    let fetched = ninja::agent()
        .get(url)
        .call()
        .with_context(|| format!("requesting {url}"))
        .and_then(|mut response| Ok(response.body_mut().read_to_string()?));

    match fetched {
        Ok(body) => {
            fs::write(&path, &body)?;
            Ok(body)
        }
        Err(error) if path.exists() => {
            eprintln!("Using cached poe.ninja data; refreshing failed: {error:#}");
            Ok(fs::read_to_string(&path)?)
        }
        Err(error) => Err(error),
    }
}

fn cache_dir() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .context("cannot determine the user cache directory")?
        .join("poe2")
        .join("market");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prices() {
        let price = |text: &str| text.parse::<Price>().unwrap();

        assert_eq!(price("5").currency, "divine");
        assert_eq!(price("2.5div").amount, 2.5);
        assert_eq!(price("300ex").currency, "exalted");
        assert_eq!(price("20 c").currency, "chaos");
        assert!("ex".parse::<Price>().is_err());
        assert!("5 mirrors".parse::<Price>().is_err());
    }

    #[test]
    fn prefers_the_same_base() {
        let price = |base: &str, divines: f64| UniquePrice {
            name: "Atziri's Step".into(),
            base_type: base.into(),
            divines,
            listing_count: 10,
        };
        let prices = [
            price("Runemastered Cinched Boots", 1.8),
            price("Cinched Boots", 0.01),
        ];

        assert_eq!(
            unique_price(&prices, "Atziri's Step", "Cinched Boots")
                .unwrap()
                .divines,
            0.01
        );
        assert_eq!(
            unique_price(&prices, "atziri's step", "Other Boots")
                .unwrap()
                .divines,
            0.01
        );
        assert!(unique_price(&prices, "Nope", "Cinched Boots").is_none());
    }
}
