//! poe.ninja profile API: public characters, no auth.
//!
//! A character is fetched in two steps. An SSE endpoint announces the current
//! model version, and the model endpoint returns the character for that version.

use std::io::{BufRead, BufReader};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const BASE_URL: &str = "https://poe.ninja/poe2/api";

#[derive(Debug, PartialEq, Eq)]
pub struct CharacterRef {
    /// poe.ninja's form of the account name, with `-` in place of `#`.
    pub account: String,
    pub league: String,
    pub character: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub name: String,
    pub level: u32,
    pub class: String,
    pub league: String,
    pub path_of_building_export: String,
}

/// Parse `https://poe.ninja/poe2/profile/<account>/<league>/character/<name>`.
pub fn parse_profile_url(url: &str) -> Result<CharacterRef> {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

    match parts.as_slice() {
        [
            "poe.ninja",
            "poe2",
            "profile",
            account,
            league,
            "character",
            character,
        ] => Ok(CharacterRef {
            account: account.to_string(),
            league: league.to_string(),
            character: character.to_string(),
        }),
        _ => bail!(
            "not a poe.ninja character URL: {url}\n\
             expected https://poe.ninja/poe2/profile/<account>/<league>/character/<name>"
        ),
    }
}

pub fn fetch_character(character: &CharacterRef) -> Result<Character> {
    #[derive(Deserialize)]
    struct Model {
        #[serde(rename = "charModel")]
        char_model: Character,
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let path = format!(
        "{}/{}/{}",
        character.account, character.league, character.character
    );
    let version = read_sse_version(&agent, &format!("{BASE_URL}/events/character/{path}"))?
        .with_context(|| {
            format!(
                "poe.ninja has no character {path}; check the URL and that the profile is public"
            )
        })?;
    let model: Model = agent
        .get(format!(
            "{BASE_URL}/profile/characters/{path}/model/{version}"
        ))
        .call()?
        .body_mut()
        .read_json()?;

    Ok(model.char_model)
}

/// The first event's version, or `None` when poe.ninja answers `event: notfound`.
fn read_sse_version(agent: &ureq::Agent, url: &str) -> Result<Option<u64>> {
    #[derive(Deserialize)]
    struct Event {
        version: u64,
    }

    let response = agent.get(url).call()?;
    let reader = BufReader::new(response.into_body().into_reader());

    for line in reader.lines() {
        let line = line?;

        if line == "event: notfound" {
            return Ok(None);
        }

        if let Some(data) = line.strip_prefix("data:") {
            return Ok(Some(serde_json::from_str::<Event>(data.trim())?.version));
        }
    }

    bail!("poe.ninja sent no version for {url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_url() {
        let url =
            "https://poe.ninja/poe2/profile/Exile-1234/forbiddenrites/character/Koftespies";

        assert_eq!(
            parse_profile_url(url).unwrap(),
            CharacterRef {
                account: "Exile-1234".into(),
                league: "forbiddenrites".into(),
                character: "Koftespies".into(),
            }
        );
    }

    #[test]
    fn ignores_trailing_slash_and_query() {
        let parsed =
            parse_profile_url("https://poe.ninja/poe2/profile/a-1/std/character/b/?tab=items");

        assert_eq!(parsed.unwrap().character, "b");
    }

    #[test]
    fn rejects_other_urls() {
        for url in [
            "https://poe.ninja/poe2/builds/runesofaldur",
            "https://poe.ninja/poe2/profile/a-1/std",
            "https://poe.ninja/poe1/profile/a-1/std/character/b",
            "https://example.com/poe2/profile/a-1/std/character/b",
        ] {
            assert!(parse_profile_url(url).is_err(), "{url}");
        }
    }
}
