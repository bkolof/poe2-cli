//! poe.ninja stores PoB's own results as `PlayerStat` entries in its build
//! export. Headless PoB at the pinned version must reproduce them exactly.

use poe2::pob::{self, Pob};

const BUILD_CODE: &str = include_str!("fixtures/koftespies.pob");

#[test]
fn calculates_the_stats_poe_ninja_stored() {
    let Some(pob_dir) = pob::installed_dir() else {
        eprintln!(
            "skipped: PoB {} not installed, run `poe2 char stats` once",
            pob::VERSION
        );
        return;
    };
    let build_xml = pob::decode_build_code(BUILD_CODE).unwrap();

    let result = Pob::start(&pob_dir).unwrap().calculate(&build_xml).unwrap();

    let expected = player_stats(&build_xml);
    let compared: Vec<_> = expected
        .iter()
        .filter_map(|(key, value)| result.number(key).map(|actual| (key, *value, actual)))
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
