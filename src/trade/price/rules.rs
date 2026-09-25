//! Which mods set an item's price, and which are filler: the trading
//! judgement behind price checks, kept in one place to tune. The defaults
//! follow Exiled Exchange 2, the PoE2 fork of Awakened PoE Trade, and
//! community pricing guides.

use crate::pob::model::{PriceItem, PriceMod};

/// A weapon is searched by its physical or elemental DPS when that makes up
/// at least this share of its damage, else by its total DPS.
pub const MAIN_DAMAGE_SHARE: f64 = 0.67;

/// Elemental damage makes a weapon's total DPS count too from this share on.
pub const MINOR_DAMAGE_SHARE: f64 = 0.15;

/// Tiers from the best down to this one are searched; lower ones are filler.
/// Exiled Exchange 2 enables mods of tier 2 or better.
pub const BEST_TIERS: u32 = 2;

/// Stats that set an item's price at any tier. Rarity's tiers top out at low
/// item levels, so its tier says little about its value.
const ALWAYS: &[&str] = &[
    "increased Movement Speed",
    "to Level of all",
    "to Spirit",
    "increased Rarity of Items found",
];

/// Stats traders do not pay for.
const NEVER: &[&str] = &[
    "Light Radius",
    "reduced Attribute Requirements",
    "to Stun Threshold",
    "Thorns damage",
    "per enemy killed",
    "Stun Buildup",
    "Stun Duration",
    "Duration on you",
    "Duration of Bleeding on You",
    "to Accuracy Rating",
    "Life Regeneration per second",
];

/// Whether a mod is searched for, or why it is left out.
pub fn judge(m: &PriceMod, _item: &PriceItem) -> Result<(), String> {
    let has = |texts: &[&str]| texts.iter().any(|t| m.text.contains(t));

    if m.kind == "implicit" {
        return Err("comes with the base".into());
    }

    if has(NEVER) {
        return Err("rarely adds value".into());
    }

    if has(ALWAYS) {
        return Ok(());
    }

    match (m.tier, m.tiers) {
        (Some(tier), Some(tiers)) if tier > BEST_TIERS => {
            Err(format!("filler, tier {tier} of {tiers}"))
        }
        _ => Ok(()),
    }
}

/// The item level a normal or magic base is searched at: its own, capped at
/// the level where the best mods can roll, or none when it is too low to be
/// worth crafting on. Jewels, flasks and charms are not priced by item level.
pub fn base_item_level(item: &PriceItem) -> Option<u32> {
    let category = item.category.as_deref().unwrap_or_default();
    let level = item.item_level?;

    if !(item.rarity == "NORMAL" || item.rarity == "MAGIC")
        || category.starts_with("jewel")
        || category.starts_with("flask")
    {
        return None;
    }

    // Spell skill levels, the best mods on wands and staves, roll at 81.
    let cap = if category == "weapon.wand" || category == "weapon.staff" {
        81
    } else {
        82
    };
    (level + 15 >= cap).then(|| level.min(cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str, kind: &str, tier: Option<u32>) -> PriceMod {
        PriceMod {
            text: text.into(),
            kind: kind.into(),
            tier,
            tiers: tier.map(|_| 8),
            ..Default::default()
        }
    }

    fn item(rarity: &str, category: &str, level: u32) -> PriceItem {
        serde_json::from_value(serde_json::json!({
            "slot": "Boots", "name": "x", "base": "x", "rarity": rarity, "unique": false,
            "category": category, "corrupted": false, "itemLevel": level,
        }))
        .unwrap()
    }

    #[test]
    fn keeps_the_best_tiers_and_stats_that_set_prices() {
        let boots = item("RARE", "armour.boots", 80);

        assert!(judge(&m("+89 to Stun Threshold", "explicit", Some(1)), &boots).is_err());
        assert!(judge(&m("5% increased Movement Speed", "implicit", None), &boots).is_err());
        assert!(
            judge(
                &m("15% increased Movement Speed", "explicit", Some(6)),
                &boots
            )
            .is_ok()
        );
        assert!(
            judge(
                &m("30% increased Spell Damage", "explicit", Some(2)),
                &boots
            )
            .is_ok()
        );
        assert_eq!(
            judge(
                &m("12% increased Spell Damage", "explicit", Some(5)),
                &boots
            ),
            Err("filler, tier 5 of 8".into())
        );
        assert!(judge(&m("Some new mod", "explicit", None), &boots).is_ok());
    }

    #[test]
    fn prices_crafting_bases_by_item_level() {
        assert_eq!(
            base_item_level(&item("NORMAL", "armour.boots", 84)),
            Some(82)
        );
        assert_eq!(base_item_level(&item("MAGIC", "weapon.wand", 81)), Some(81));
        assert_eq!(base_item_level(&item("NORMAL", "armour.boots", 60)), None);
        assert_eq!(base_item_level(&item("RARE", "armour.boots", 84)), None);
        assert_eq!(base_item_level(&item("MAGIC", "jewel", 84)), None);
    }
}
