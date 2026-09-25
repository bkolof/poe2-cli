//! What the user adds to a PoB weighted trade search: stat groups, item
//! filters and the sort order, written into the trade site's query JSON.

use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::pob::model::{TradeStat, TradeWeight};

/// A minimum, a maximum or both: `25`, `25..`, `..30` or `25..30`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl FromStr for Range {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let number = |part: &str| -> Result<Option<f64>> {
            let part = part.trim();

            if part.is_empty() {
                return Ok(None);
            }

            let value = part
                .parse()
                .with_context(|| format!("'{part}' is not a number"))?;
            Ok(Some(value))
        };
        let range = match text.split_once("..") {
            Some((min, max)) => Self {
                min: number(min)?,
                max: number(max)?,
            },
            None => Self {
                min: number(text)?,
                max: None,
            },
        };

        if range.min.is_none() && range.max.is_none() {
            bail!("'{text}' has neither a minimum nor a maximum");
        }

        Ok(range)
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(f, "{min} to {max}"),
            (Some(min), None) => write!(f, ">= {min}"),
            (None, Some(max)) => write!(f, "<= {max}"),
            (None, None) => Ok(()),
        }
    }
}

impl Range {
    fn to_json(self) -> Value {
        let mut value = Map::new();

        if let Some(min) = self.min {
            value.insert("min".into(), json!(min));
        }

        if let Some(max) = self.max {
            value.insert("max".into(), json!(max));
        }

        Value::Object(value)
    }
}

/// `--require "movement speed=25"`: a stat the item must have, within a range.
#[derive(Debug, Clone)]
pub struct Require {
    pub stat: String,
    pub range: Range,
}

impl FromStr for Require {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (stat, range) = text
            .rsplit_once('=')
            .with_context(|| format!("'{text}' is not `stat=minimum` or `stat=min..max`"))?;
        Ok(Self {
            stat: stat.trim().into(),
            range: range.parse()?,
        })
    }
}

/// `--count "2: fire resistance, cold resistance, lightning resistance"`: at
/// least that many of the stats.
#[derive(Debug, Clone)]
pub struct Count {
    pub min: u32,
    pub stats: Vec<String>,
}

impl FromStr for Count {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (min, stats) = text
            .split_once(':')
            .with_context(|| format!("'{text}' is not `count: stat, stat, ...`"))?;
        let min = min
            .trim()
            .parse()
            .with_context(|| format!("'{}' is not a whole number", min.trim()))?;
        Ok(Self {
            min,
            stats: list(stats)?,
        })
    }
}

/// `--sum "80: fire resistance=1, cold resistance=1"`: a weighted sum of
/// stats, with an optional minimum.
#[derive(Debug, Clone)]
pub struct Sum {
    pub min: Option<f64>,
    pub stats: Vec<(String, f64)>,
}

impl FromStr for Sum {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (min, stats) = match text.split_once(':') {
            Some((min, stats)) if min.trim().parse::<f64>().is_ok() => {
                (Some(min.trim().parse()?), stats)
            }
            _ => (None, text),
        };
        let stats = list(stats)?
            .into_iter()
            .map(|stat| {
                let (stat, weight) = stat
                    .rsplit_once('=')
                    .with_context(|| format!("'{stat}' is not `stat=weight`"))?;
                let weight = weight
                    .trim()
                    .parse()
                    .with_context(|| format!("'{}' is not a weight", weight.trim()))?;
                Ok((stat.trim().to_string(), weight))
            })
            .collect::<Result<_>>()?;
        Ok(Self { min, stats })
    }
}

fn list(text: &str) -> Result<Vec<String>> {
    let items: Vec<String> = text
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    if items.is_empty() {
        bail!("no stats given");
    }

    Ok(items)
}

enum FilterKind {
    Range,
    Options(&'static [&'static str]),
    Text,
}

const YES_NO: &[&str] = &["true", "false"];

/// The trade site's item filters, by id, with the filter group each is in.
const FILTERS: &[(&str, &str, FilterKind)] = &[
    (
        "rarity",
        "type_filters",
        FilterKind::Options(&[
            "normal",
            "magic",
            "rare",
            "unique",
            "uniquefoil",
            "nonunique",
            "any",
        ]),
    ),
    ("ilvl", "type_filters", FilterKind::Range),
    ("quality", "type_filters", FilterKind::Range),
    ("damage", "equipment_filters", FilterKind::Range),
    ("aps", "equipment_filters", FilterKind::Range),
    ("crit", "equipment_filters", FilterKind::Range),
    ("dps", "equipment_filters", FilterKind::Range),
    ("pdps", "equipment_filters", FilterKind::Range),
    ("edps", "equipment_filters", FilterKind::Range),
    ("reload_time", "equipment_filters", FilterKind::Range),
    ("ar", "equipment_filters", FilterKind::Range),
    ("ev", "equipment_filters", FilterKind::Range),
    ("es", "equipment_filters", FilterKind::Range),
    ("ward", "equipment_filters", FilterKind::Range),
    ("block", "equipment_filters", FilterKind::Range),
    ("spirit", "equipment_filters", FilterKind::Range),
    ("rune_sockets", "equipment_filters", FilterKind::Range),
    (
        "total_augment_sockets",
        "equipment_filters",
        FilterKind::Range,
    ),
    ("lvl", "req_filters", FilterKind::Range),
    ("str", "req_filters", FilterKind::Range),
    ("dex", "req_filters", FilterKind::Range),
    ("int", "req_filters", FilterKind::Range),
    ("identified", "misc_filters", FilterKind::Options(YES_NO)),
    (
        "fractured_item",
        "misc_filters",
        FilterKind::Options(YES_NO),
    ),
    ("corrupted", "misc_filters", FilterKind::Options(YES_NO)),
    (
        "twice_corrupted",
        "misc_filters",
        FilterKind::Options(YES_NO),
    ),
    ("sanctified", "misc_filters", FilterKind::Options(YES_NO)),
    ("mutated", "misc_filters", FilterKind::Options(YES_NO)),
    ("veiled", "misc_filters", FilterKind::Options(YES_NO)),
    ("desecrated", "misc_filters", FilterKind::Options(YES_NO)),
    ("crafted", "misc_filters", FilterKind::Options(YES_NO)),
    ("foreseeing", "misc_filters", FilterKind::Options(YES_NO)),
    ("mirrored", "misc_filters", FilterKind::Options(YES_NO)),
    ("account", "trade_filters", FilterKind::Text),
    ("collapse", "trade_filters", FilterKind::Options(&["true"])),
    (
        "indexed",
        "trade_filters",
        FilterKind::Options(&[
            "1hour", "3hours", "12hours", "1day", "3days", "1week", "2weeks", "1month", "2months",
        ]),
    ),
    (
        "sale_type",
        "trade_filters",
        FilterKind::Options(&["any", "priced_with_info", "unpriced"]),
    ),
];

/// `--filter ilvl=80`: one of the trade site's item filters.
#[derive(Debug, Clone)]
pub struct Filter {
    /// As the user wrote it, for display.
    pub text: String,
    group: &'static str,
    id: &'static str,
    /// The filter's JSON, or None to remove it (`rarity=any`).
    value: Option<Value>,
}

impl FromStr for Filter {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (key, value) = text
            .split_once('=')
            .with_context(|| format!("'{text}' is not `filter=value`"))?;
        let (key, value) = (key.trim(), value.trim());
        let (id, group, kind) = FILTERS.iter().find(|(id, ..)| *id == key).ok_or_else(|| {
            let ids: Vec<&str> = FILTERS.iter().map(|(id, ..)| *id).collect();
            anyhow!("unknown filter '{key}'; filters: {}", ids.join(", "))
        })?;
        let value = match kind {
            FilterKind::Range => Some(value.parse::<Range>()?.to_json()),
            FilterKind::Options(options) => {
                if !options.contains(&value) {
                    bail!("{id} is one of {}", options.join(", "));
                }

                (value != "any").then(|| json!({ "option": value }))
            }
            FilterKind::Text => Some(json!({ "input": value })),
        };
        Ok(Self {
            text: text.trim().into(),
            group,
            id,
            value,
        })
    }
}

/// `--sort`: which listings the trade site returns first, and so which get
/// fetched and calculated.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Sort {
    /// PoB's weighted sum, highest first.
    #[default]
    Pob,
    /// Cheapest first.
    Price,
    /// A `--sum` group, highest first, counted from 1.
    Sum(usize),
    /// One stat, highest first.
    Stat(String),
}

impl FromStr for Sort {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let text = text.trim();

        if let Some(index) = text.strip_prefix("sum:") {
            let index = index
                .trim()
                .parse()
                .ok()
                .filter(|i| *i > 0)
                .with_context(|| format!("'{index}' is not a --sum number, counted from 1"))?;
            return Ok(Self::Sum(index));
        }

        if let Some(stat) = text.strip_prefix("stat:") {
            return Ok(Self::Stat(stat.trim().into()));
        }

        match text {
            "pob" => Ok(Self::Pob),
            "price" => Ok(Self::Price),
            _ => bail!("sort by pob, price, sum:N or stat:TEXT"),
        }
    }
}

/// Everything the user adds to PoB's search.
#[derive(Debug, Clone, Default)]
pub struct Additions {
    pub require: Vec<Require>,
    pub exclude: Vec<String>,
    pub count: Vec<Count>,
    pub sum: Vec<Sum>,
    pub filter: Vec<Filter>,
    pub sort: Sort,
    /// Replaces PoB's minimum for its weighted sum.
    pub min_weight: Option<f64>,
    /// Raw query JSON merged in last.
    pub raw: Option<Value>,
}

/// A stat group added to the query, for display.
#[derive(Debug, Serialize)]
pub struct Group {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(flatten)]
    pub range: Range,
    pub stats: Vec<GroupStat>,
}

#[derive(Debug, Serialize)]
pub struct GroupStat {
    pub id: String,
    pub text: String,
    #[serde(flatten)]
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

/// The finished query and what went into it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub query: Value,
    /// PoB's weights that fit alongside the additions.
    pub weights: Vec<TradeWeight>,
    /// How many of PoB's least important weights were left out to fit.
    pub dropped_weights: usize,
    /// The minimum for PoB's weighted sum.
    pub min_weight: Option<f64>,
    pub groups: Vec<Group>,
    pub filters: Vec<String>,
    pub sort: String,
}

impl Additions {
    /// Write the additions into PoB's query. `resolve` finds a trade stat by
    /// text.
    pub fn apply(
        &self,
        pob_query: &str,
        weights: Vec<TradeWeight>,
        resolve: impl Fn(&str) -> Result<TradeStat>,
    ) -> Result<Search> {
        let mut query: Value =
            serde_json::from_str(pob_query).context("PoB generated an invalid query")?;
        let groups = self.groups(&weights, &resolve)?;
        let stats = query["query"]["stats"]
            .as_array_mut()
            .context("PoB's query has no stat groups")?;
        // PoB's own group of required stats is empty: the additions replace it.
        stats.retain(|g| g["filters"].as_array().is_some_and(|f| !f.is_empty()));

        if let Some(min) = self.min_weight {
            stats[0]["value"] = json!({ "min": min });
        }

        let min_weight = stats[0]["value"]["min"].as_f64();

        for group in &groups {
            stats.push(group_json(group));
        }

        for filter in &self.filter {
            let filters = &mut query["query"]["filters"][filter.group]["filters"];

            match &filter.value {
                Some(value) => filters[filter.id] = value.clone(),
                None => {
                    if let Some(filters) = filters.as_object_mut() {
                        filters.remove(filter.id);
                    }
                }
            }
        }

        query["sort"] = match &self.sort {
            Sort::Pob => json!({ "statgroup.0": "desc" }),
            Sort::Price => json!({ "price": "asc" }),
            // The site numbers only the weighted sums; PoB's is the first.
            Sort::Sum(n) => {
                if *n > self.sum.len() {
                    bail!("there is no --sum number {n}");
                }

                json!({ format!("statgroup.{n}"): "desc" })
            }
            Sort::Stat(text) => json!({ format!("stat.{}", resolve(text)?.id): "desc" }),
        };

        if let Some(raw) = &self.raw {
            merge(&mut query, raw);
        }

        Ok(Search {
            query,
            weights,
            dropped_weights: 0,
            min_weight,
            groups,
            filters: self.filter.iter().map(|f| f.text.clone()).collect(),
            sort: sort_name(&self.sort),
        })
    }

    fn groups(
        &self,
        weights: &[TradeWeight],
        resolve: &impl Fn(&str) -> Result<TradeStat>,
    ) -> Result<Vec<Group>> {
        let stat = |text: &str, range: Range, weight: Option<f64>| -> Result<GroupStat> {
            let found = resolve(text)?;
            Ok(GroupStat {
                id: found.id,
                text: found.text,
                range,
                weight,
            })
        };
        let mut groups = Vec::new();

        if !self.require.is_empty() {
            groups.push(Group {
                kind: "and",
                range: Range::default(),
                stats: self
                    .require
                    .iter()
                    .map(|r| stat(&r.stat, r.range, None))
                    .collect::<Result<_>>()?,
            });
        }

        if !self.exclude.is_empty() {
            groups.push(Group {
                kind: "not",
                range: Range::default(),
                stats: self
                    .exclude
                    .iter()
                    .map(|text| stat(text, Range::default(), None))
                    .collect::<Result<_>>()?,
            });
        }

        for count in &self.count {
            groups.push(Group {
                kind: "count",
                range: Range {
                    min: Some(count.min.into()),
                    max: None,
                },
                stats: count
                    .stats
                    .iter()
                    .map(|text| stat(text, Range::default(), None))
                    .collect::<Result<_>>()?,
            });
        }

        for sum in &self.sum {
            groups.push(Group {
                kind: "weight",
                range: Range {
                    min: sum.min,
                    max: None,
                },
                stats: sum
                    .stats
                    .iter()
                    .map(|(text, weight)| stat(text, Range::default(), Some(*weight)))
                    .collect::<Result<_>>()?,
            });
        }

        // The site sorts only by stats the query has; an `if` group adds one
        // without filtering on it.
        if let Sort::Stat(text) = &self.sort {
            let id = resolve(text)?.id;
            let present = weights.iter().any(|w| w.id == id)
                || groups.iter().flat_map(|g| &g.stats).any(|s| s.id == id);

            if !present {
                groups.push(Group {
                    kind: "if",
                    range: Range::default(),
                    stats: vec![stat(text, Range::default(), None)?],
                });
            }
        }

        Ok(groups)
    }
}

impl Search {
    /// Keep only PoB's `keep` most important weights.
    pub fn keep_weights(&mut self, keep: usize) {
        self.dropped_weights += self.weights.len().saturating_sub(keep);
        self.weights.truncate(keep);

        if let Some(filters) = self.query["query"]["stats"][0]["filters"].as_array_mut() {
            filters.truncate(keep);
        }
    }
}

fn group_json(group: &Group) -> Value {
    let filters: Vec<Value> = group
        .stats
        .iter()
        .map(|s| {
            let mut value = s.range.to_json();

            if let Some(weight) = s.weight {
                value["weight"] = json!(weight);
            }

            json!({ "id": s.id, "value": value })
        })
        .collect();
    json!({ "type": group.kind, "value": group.range.to_json(), "filters": filters })
}

fn sort_name(sort: &Sort) -> String {
    match sort {
        Sort::Pob => "PoB's weighted sum".into(),
        Sort::Price => "price".into(),
        Sort::Sum(n) => format!("weighted sum {n}"),
        Sort::Stat(text) => text.clone(),
    }
}

/// Merge `extra` into `base`: objects key by key, arrays appended, anything
/// else replaced.
fn merge(base: &mut Value, extra: &Value) {
    match (base, extra) {
        (Value::Object(base), Value::Object(extra)) => {
            for (key, value) in extra {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (Value::Array(base), Value::Array(extra)) => base.extend(extra.iter().cloned()),
        (base, extra) => *base = extra.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POB_QUERY: &str = r#"{
        "query": {
            "filters": {"type_filters": {"filters": {"category": {"option": "armour.boots"}, "rarity": {"option": "nonunique"}}}},
            "status": {"option": "securable"},
            "stats": [
                {"type": "weight", "value": {"min": 10}, "filters": [
                    {"id": "pseudo.life", "value": {"weight": 2}},
                    {"id": "pseudo.fire", "value": {"weight": 1}}
                ]},
                {"type": "and", "filters": []}
            ]
        },
        "sort": {"statgroup.0": "desc"},
        "engine": "new"
    }"#;

    fn weights() -> Vec<TradeWeight> {
        ["pseudo.life", "pseudo.fire"]
            .iter()
            .map(|id| TradeWeight {
                id: (*id).into(),
                text: None,
                weight: 1.0,
            })
            .collect()
    }

    fn resolve(text: &str) -> Result<TradeStat> {
        Ok(TradeStat {
            id: format!("pseudo.{}", text.replace(' ', "_")),
            text: text.into(),
            kind: "pseudo".into(),
        })
    }

    #[test]
    fn parses_ranges() {
        let range = |text: &str| text.parse::<Range>().unwrap();
        assert_eq!(
            range("25"),
            Range {
                min: Some(25.0),
                max: None
            }
        );
        assert_eq!(
            range("25.."),
            Range {
                min: Some(25.0),
                max: None
            }
        );
        assert_eq!(
            range("..30"),
            Range {
                min: None,
                max: Some(30.0)
            }
        );
        assert_eq!(
            range("2.5..30"),
            Range {
                min: Some(2.5),
                max: Some(30.0)
            }
        );
        assert!("..".parse::<Range>().is_err());
        assert!("lots".parse::<Range>().is_err());
    }

    #[test]
    fn parses_stat_options() {
        let require: Require = "movement speed=25..35".parse().unwrap();
        assert_eq!(require.stat, "movement speed");
        assert_eq!(require.range.max, Some(35.0));

        let count: Count = "2: fire resistance, cold resistance,".parse().unwrap();
        assert_eq!((count.min, count.stats.len()), (2, 2));

        let sum: Sum = "80: fire resistance=1, life=0.5".parse().unwrap();
        assert_eq!(sum.min, Some(80.0));
        assert_eq!(sum.stats[1], ("life".into(), 0.5));
        let sum: Sum = "fire resistance=1".parse().unwrap();
        assert_eq!(sum.min, None);

        assert!("ilvl=80..".parse::<Filter>().is_ok());
        assert!("corrupted=maybe".parse::<Filter>().is_err());
        assert!("colour=red".parse::<Filter>().is_err());

        assert_eq!("sum:2".parse::<Sort>().unwrap(), Sort::Sum(2));
        assert!("sum:0".parse::<Sort>().is_err());
        assert_eq!(
            "stat: life".parse::<Sort>().unwrap(),
            Sort::Stat("life".into())
        );
    }

    #[test]
    fn writes_the_additions_into_the_query() {
        let additions = Additions {
            require: vec!["movement speed=25".parse().unwrap()],
            exclude: vec!["chaos resistance".into()],
            sum: vec!["60: fire=1, cold=1".parse().unwrap()],
            filter: vec![
                "ilvl=80..".parse().unwrap(),
                "rarity=any".parse().unwrap(),
                "corrupted=false".parse().unwrap(),
            ],
            sort: Sort::Sum(1),
            min_weight: Some(0.0),
            raw: Some(
                json!({ "query": { "filters": { "trade_filters": { "filters": { "collapse": { "option": "true" } } } } } }),
            ),
            ..Default::default()
        };
        let search = additions.apply(POB_QUERY, weights(), resolve).unwrap();
        let q = &search.query;

        let types: Vec<&str> = q["query"]["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["weight", "and", "not", "weight"]);
        assert_eq!(q["query"]["stats"][0]["value"]["min"], 0.0);
        assert_eq!(
            q["query"]["stats"][1]["filters"][0],
            json!({ "id": "pseudo.movement_speed", "value": { "min": 25.0 } })
        );
        assert_eq!(q["query"]["stats"][3]["value"]["min"], 60.0);
        assert_eq!(q["sort"], json!({ "statgroup.1": "desc" }));

        let type_filters = &q["query"]["filters"]["type_filters"]["filters"];
        assert_eq!(type_filters["ilvl"], json!({ "min": 80.0 }));
        assert!(type_filters.get("rarity").is_none());
        assert_eq!(type_filters["category"]["option"], "armour.boots");
        assert_eq!(
            q["query"]["filters"]["misc_filters"]["filters"]["corrupted"]["option"],
            "false"
        );
        assert_eq!(
            q["query"]["filters"]["trade_filters"]["filters"]["collapse"]["option"],
            "true"
        );
    }

    #[test]
    fn sorting_by_a_stat_adds_it_when_the_query_lacks_it() {
        let by = |text: &str| Additions {
            sort: Sort::Stat(text.into()),
            ..Default::default()
        };

        let search = by("life").apply(POB_QUERY, weights(), resolve).unwrap();
        assert_eq!(search.query["query"]["stats"].as_array().unwrap().len(), 1);
        assert_eq!(search.query["sort"], json!({ "stat.pseudo.life": "desc" }));

        let search = by("spirit").apply(POB_QUERY, weights(), resolve).unwrap();
        assert_eq!(search.query["query"]["stats"][1]["type"], "if");
    }

    #[test]
    fn drops_the_least_important_weights() {
        let mut search = Additions::default()
            .apply(POB_QUERY, weights(), resolve)
            .unwrap();
        search.keep_weights(1);

        assert_eq!(search.dropped_weights, 1);
        assert_eq!(search.weights[0].id, "pseudo.life");
        assert_eq!(
            search.query["query"]["stats"][0]["filters"],
            json!([{ "id": "pseudo.life", "value": { "weight": 2 } }])
        );
    }

    #[test]
    fn sorting_by_a_missing_sum_is_an_error() {
        let additions = Additions {
            sort: Sort::Sum(1),
            ..Default::default()
        };
        assert!(additions.apply(POB_QUERY, weights(), resolve).is_err());
    }
}
