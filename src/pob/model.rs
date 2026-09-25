//! What the Lua API in `api.lua` returns, field for field.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub class: String,
    pub ascendancy: Option<String>,
    pub level: u32,
    pub main_skill: Option<String>,
}

/// A row of PoB's sidebar. A row without a label is a section break.
#[derive(Debug, Deserialize, Serialize)]
pub struct SidebarRow {
    pub label: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDps {
    pub name: String,
    pub group: Option<String>,
    pub slot: Option<String>,
    /// "passive tree" or "item" when the skill is not from a gem.
    pub granted_by: Option<String>,
    pub main: bool,
    /// PoB rates this skill by damage per use, so `combined_dps` is that
    /// damage rather than a rate.
    pub per_use: bool,
    pub combined_dps: f64,
    pub hit_dps: f64,
    pub dot_dps: f64,
    pub minion_dps: f64,
    pub average_damage: f64,
    pub speed: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct WhatIfRequest {
    /// Item text as copied in game with Ctrl+C.
    pub item: Option<String>,
    /// Only compare the item in this slot, instead of every slot it fits.
    pub slot: Option<String>,
    pub allocate: Vec<String>,
    pub unallocate: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatIf {
    pub item: Option<String>,
    /// Passive points allocated (paths included) and unallocated (dependents included).
    pub points: PassivePoints,
    /// The passives the requested names or ids resolved to.
    #[serde(default)]
    pub passives: Vec<ResolvedPassive>,
    pub results: Vec<WhatIfResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResolvedPassive {
    pub id: u32,
    pub name: String,
    /// "allocate" or "unallocate".
    pub action: String,
    pub ascendancy: Option<String>,
    /// Points this passive adds (its path) or removes (its dependents).
    pub points: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PassivePoints {
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatIfResult {
    pub slot: Option<String>,
    pub replacing: Option<String>,
    #[serde(default)]
    pub changes: Vec<StatChange>,
}

/// A stat that changed, formatted the way PoB shows it.
#[derive(Debug, Deserialize, Serialize)]
pub struct StatChange {
    pub stat: String,
    pub label: String,
    pub before: String,
    pub after: String,
    pub diff: String,
    pub percent: Option<f64>,
    pub better: bool,
}

/// How a change moves the headline numbers.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Impact {
    /// Passive points the change costs.
    pub points: u32,
    pub dps_percent: f64,
    pub ehp_percent: f64,
    pub life: f64,
    pub energy_shield: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TreeSuggestion {
    /// The node id, which `whatif --allocate` accepts when names are shared.
    pub id: u32,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub stats: Vec<String>,
    #[serde(flatten)]
    pub impact: Impact,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotUpgrades {
    pub slot: String,
    pub item: String,
    pub item_level: u32,
    /// Corrupted items cannot be modified further.
    pub corrupted: bool,
    /// Open affixes on a rare, recognised from the item's mod text.
    pub free_prefixes: u32,
    pub free_suffixes: u32,
    #[serde(default)]
    pub upgrades: Vec<ModUpgrade>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModUpgrade {
    #[serde(rename = "mod")]
    pub text: String,
    pub affix: String,
    pub level: u32,
    /// Whether the item has a free affix of this kind.
    pub fits: bool,
    #[serde(flatten)]
    pub impact: Impact,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModInfo {
    pub id: String,
    pub kind: String,
    pub affix: Option<String>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub level: Option<u32>,
    pub lines: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GemInfo {
    pub name: String,
    pub kind: Option<String>,
    pub tags: Option<String>,
    pub support: bool,
    pub description: Option<String>,
    pub requirements: Requirements,
    pub max_level: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Requirements {
    pub str: u32,
    pub dex: u32,
    pub int: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UniqueInfo {
    pub name: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlotUniques {
    pub slot: String,
    #[serde(default)]
    pub candidates: Vec<UniqueCandidate>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UniqueCandidate {
    pub name: String,
    pub base: String,
    pub level_required: u32,
    #[serde(flatten)]
    pub impact: Impact,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeQueryRequest {
    pub slot: String,
    /// "balanced", "dps" or "ehp".
    pub by: String,
    /// A trade site listing status: "securable" (instant buyout), "available", "online" or "any".
    pub status: String,
    /// The price cap in exalted orb equivalents.
    pub max_exalted: f64,
    /// For a jewel socket: "Base" or "Radius" jewels.
    pub jewel_type: String,
}

/// A unique and the base it is made on.
#[derive(Debug, Deserialize, Serialize)]
pub struct UniqueBase {
    pub name: String,
    pub base: String,
}

/// A weighted search generated by PoB for a slot.
#[derive(Debug, Deserialize, Serialize)]
pub struct TradeQuery {
    pub slot: String,
    /// The trade site query, as JSON.
    pub query: String,
    /// The weighted stats, most important first, as in the query.
    #[serde(default)]
    pub weights: Vec<TradeWeight>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TradeWeight {
    pub id: String,
    pub text: Option<String>,
    pub weight: f64,
}

/// An item as a price check searches for it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriceItem {
    /// The slot it is equipped in, or whose trade category it belongs to.
    pub slot: String,
    pub name: String,
    pub base: String,
    pub rarity: String,
    pub unique: bool,
    /// The trade site category, e.g. "armour.boots".
    pub category: Option<String>,
    pub corrupted: bool,
    /// Mods matched to trade stats; none for uniques, which are searched by name.
    #[serde(default)]
    pub mods: Vec<PriceMod>,
    /// Mods with no trade stat.
    #[serde(default)]
    pub unsearchable: Vec<String>,
    /// Armour ("ar"), evasion ("ev") and energy shield ("es").
    #[serde(default)]
    pub defences: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriceMod {
    pub text: String,
    /// More than one when the text matches several trade stats.
    pub ids: Vec<String>,
    pub value: Option<f64>,
    /// The trade site counts the stat the other way round (reduced for increased).
    pub invert: bool,
    /// An option stat, matched exactly.
    pub option: bool,
}

/// A trade site stat, e.g. `pseudo.pseudo_increased_movement_speed`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TradeStat {
    pub id: String,
    pub text: String,
    /// The stat's category: pseudo, explicit, implicit, rune, ...
    pub kind: String,
}

/// A trade listing as PoB calculates it in the searched slot.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub id: String,
    pub name: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub seller: Option<String>,
    pub whisper: Option<String>,
    pub item_text: String,
    /// Whether the character has the attributes to wear it after the swap.
    pub meets_requirements: bool,
    #[serde(flatten)]
    pub impact: Impact,
}
