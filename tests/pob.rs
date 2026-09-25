//! Runs against the real, pinned PoB. Booting PoB takes seconds, so each test
//! boots it once and checks several things. The tests skip themselves when the
//! pinned PoB is not installed yet.

use std::path::PathBuf;

use poe2::pob::model::{TradeQueryRequest, WhatIfRequest};
use poe2::pob::{self, Pob};

const BUILD_CODE: &str = include_str!("fixtures/koftespies.pob");

fn installed() -> Option<PathBuf> {
    let dir = pob::installed_dir();

    if dir.is_none() {
        eprintln!(
            "skipped: PoB {} not installed, run any poe2 build command once",
            pob::VERSION
        );
    }

    dir
}

/// poe.ninja stores PoB's own results as `PlayerStat` entries in its build
/// export. Headless PoB at the pinned version must reproduce them exactly.
#[test]
fn calculates_the_stats_poe_ninja_stored() {
    let Some(pob_dir) = installed() else { return };
    let build_xml = pob::decode_build_code(BUILD_CODE).unwrap();

    let pob = Pob::start(&pob_dir).unwrap();
    pob.load(&build_xml).unwrap();
    let output = pob.output().unwrap();

    let expected = player_stats(&build_xml);
    let compared: Vec<_> = expected
        .iter()
        .filter_map(|(key, value)| {
            output
                .get(key)?
                .as_f64()
                .map(|actual| (key, *value, actual))
        })
        .collect();
    assert!(
        compared.len() > 100,
        "only {} stats compared",
        compared.len()
    );

    for (key, expected, actual) in compared {
        let tolerance = expected.abs().max(1.0) * 1e-9;
        assert!(
            (expected - actual).abs() <= tolerance,
            "{key}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn analyses_a_build() {
    let Some(pob_dir) = installed() else { return };
    let pob = Pob::start(&pob_dir).unwrap();

    // A broken build is an error, not an empty default build, and PoB
    // recovers from it.
    let broken = pob.load("<notpob/>").unwrap_err().to_string();
    assert!(broken.contains("root element missing"), "{broken}");
    pob.load(&pob::decode_build_code(BUILD_CODE).unwrap())
        .unwrap();

    let info = pob.info().unwrap();
    assert_eq!(info.ascendancy.as_deref(), Some("Martial Artist"));
    assert_eq!(info.main_skill.as_deref(), Some("Falling Thunder"));

    let sidebar = pob.sidebar().unwrap();
    assert!(
        sidebar
            .iter()
            .any(|row| row.label.as_deref() == Some("Total Life"))
    );

    let skills = pob.skills().unwrap();
    let main: Vec<_> = skills
        .iter()
        .filter(|s| s.main)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(main, ["Falling Thunder"]);
    assert!(skills.iter().filter(|s| s.combined_dps > 0.0).count() > 5);

    // Trying every skill must leave the build as it was.
    assert_eq!(
        pob.info().unwrap().main_skill.as_deref(),
        Some("Falling Thunder")
    );

    let allocate = pob
        .what_if(&WhatIfRequest {
            allocate: vec!["Imbibed Power".into()],
            ..Default::default()
        })
        .unwrap();
    let total_dps = allocate.results[0]
        .changes
        .iter()
        .find(|c| c.stat == "TotalDPS")
        .expect("allocating Imbibed Power changes DPS");
    assert!(total_dps.better);
    assert!(allocate.points.added > 1, "the path is included");
    assert_eq!(allocate.passives[0].name, "Imbibed Power");
    assert_eq!(allocate.passives[0].points, allocate.points.added);

    let missing = pob.what_if(&WhatIfRequest {
        allocate: vec!["No Such Passive".into()],
        ..Default::default()
    });
    assert_eq!(
        missing.unwrap_err().to_string(),
        "no passive named 'No Such Passive'"
    );

    // 51690 is a Titan passive; this build is a Martial Artist.
    let other_ascendancy = pob.what_if(&WhatIfRequest {
        allocate: vec!["51690".into()],
        ..Default::default()
    });
    assert!(
        other_ascendancy
            .unwrap_err()
            .to_string()
            .contains("belongs to the Titan ascendancy")
    );

    let suggestions = pob.tree_suggestions(2).unwrap();
    assert!(suggestions.iter().any(|s| s.impact.dps_percent > 0.0));
    assert!(suggestions.iter().all(|s| s.impact.points <= 2));

    // The boots have movement speed, life and stun threshold, cold resistance
    // and freeze duration: two prefixes and three suffixes.
    let uniques = pob.uniques_for_slot("Boots").unwrap();
    let step = uniques
        .candidates
        .iter()
        .find(|c| c.name == "Atziri's Step")
        .expect("Atziri's Step fits the boots slot");
    assert_eq!(step.base, "Cinched Boots");
    assert!(uniques.candidates.iter().all(|c| !c.base.contains("Ring")));

    let query = pob
        .trade_query(&TradeQueryRequest {
            slot: "boots".into(),
            by: "balanced".into(),
            status: "securable".into(),
            max_exalted: 1000.0,
            jewel_type: "Base".into(),
        })
        .unwrap();
    assert_eq!(query.slot, "Boots");
    let json: serde_json::Value = serde_json::from_str(&query.query).unwrap();
    assert_eq!(
        json["query"]["stats"][0]["filters"]
            .as_array()
            .unwrap()
            .len(),
        query.weights.len()
    );

    let boots = pob.slot_upgrades("boots").unwrap();
    assert_eq!((boots.free_prefixes, boots.free_suffixes), (1, 0));
    assert!(
        !boots
            .upgrades
            .iter()
            .any(|u| u.text.contains("Movement Speed"))
    );
}

#[test]
fn searches_game_data_and_build_sites() {
    let Some(pob_dir) = installed() else { return };
    let pob = Pob::start(&pob_dir).unwrap();

    let mods = pob
        .search_mods("movement speed", Some("Silk Slippers"))
        .unwrap();
    assert!(mods.iter().any(|m| m.name.as_deref() == Some("Cheetah's")));

    let gems = pob.search_gems("Falling Thunder").unwrap();
    assert_eq!(gems[0].requirements.dex, 50);

    let uniques = pob.search_uniques("Atziri's Acuity").unwrap();
    assert!(!uniques[0].text.contains("{variant"));

    assert_eq!(
        pob.build_site_url("https://pobb.in/abc123")
            .unwrap()
            .as_deref(),
        Some("https://pobb.in/pob/abc123")
    );
    assert_eq!(pob.build_site_url("https://example.com/x").unwrap(), None);

    let stats = pob.trade_stats("movement speed").unwrap();
    assert_eq!(stats[0].id, "pseudo.pseudo_increased_movement_speed");
    let by_id = pob.trade_stats("explicit.stat_3372524247").unwrap();
    assert_eq!(by_id.len(), 1);
    assert!(pob.trade_stats("no such stat").unwrap().is_empty());
}

/// `<PlayerStat stat="Life" value="1754"/>` entries from the build XML.
fn player_stats(build_xml: &str) -> Vec<(String, f64)> {
    build_xml
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("<PlayerStat stat=\"")?;
            let (stat, rest) = line.split_once("\" value=\"")?;
            let (value, _) = rest.split_once('"')?;
            Some((stat.to_string(), value.parse().ok()?))
        })
        .collect()
}
