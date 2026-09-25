//! Turns what the user names as a build into build XML.
//!
//! A source is read in two steps. `Source::read` handles everything local
//! (files, stdin) and must run before PoB starts, because PoB changes the
//! working directory. `Source::fetch` then downloads what is remote; links to
//! build sites are resolved with PoB's own list of sites.

use std::io::Read;
use std::path::Path;
use std::{fs, io};

use anyhow::{Context, Result, bail};

use crate::ninja::{self, Character, CharacterRef};
use crate::pob::{self, Pob};

#[derive(Debug, PartialEq)]
pub enum Source {
    /// A build code or build XML.
    Text(String),
    Profile(CharacterRef),
    AccountCharacter {
        account: String,
        character: String,
    },
    Link(String),
}

pub struct LoadedBuild {
    pub xml: String,
    /// poe.ninja's view of the character, when the build came from there.
    pub character: Option<Character>,
}

impl Source {
    /// Accepts a poe.ninja profile URL, `account/character`, a link to a build
    /// site, a file with a build code or XML, `-` for stdin, or a build code.
    pub fn read(arg: &str) -> Result<Self> {
        let arg = arg.trim();

        if arg == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            return Ok(Self::Text(text));
        }

        if arg.contains("poe.ninja/poe2/profile/") {
            return Ok(Self::Profile(ninja::parse_profile_url(arg)?));
        }

        if arg.starts_with("http://") || arg.starts_with("https://") {
            return Ok(Self::Link(arg.to_string()));
        }

        if Path::new(arg).is_file() {
            let text = fs::read_to_string(arg).with_context(|| format!("cannot read {arg}"))?;
            return Ok(Self::Text(text));
        }

        if let Some((account, character)) = arg.split_once('/')
            && !account.is_empty()
            && !character.is_empty()
            && !character.contains('/')
        {
            return Ok(Self::AccountCharacter {
                account: account.to_string(),
                character: character.to_string(),
            });
        }

        Ok(Self::Text(arg.to_string()))
    }

    pub fn fetch(self, pob: &Pob) -> Result<LoadedBuild> {
        match self {
            Self::Text(text) => Ok(LoadedBuild {
                xml: code_or_xml(&text)?,
                character: None,
            }),
            Self::Profile(reference) => from_ninja(&reference),
            Self::AccountCharacter { account, character } => {
                let summary = ninja::list_characters(&account)?
                    .into_iter()
                    .find(|c| c.name.eq_ignore_ascii_case(&character))
                    .with_context(|| format!("{account} has no public character {character}"))?;
                from_ninja(&ninja::parse_profile_url(&summary.profile_url(&account))?)
            }
            Self::Link(link) => {
                let Some(url) = pob.build_site_url(&link)? else {
                    bail!(
                        "unsupported link: {link}\n\
                         use a poe.ninja profile, a build site PoB can import from, or a build code"
                    );
                };
                let text = ureq::get(&url)
                    .call()
                    .with_context(|| format!("downloading {url}"))?
                    .body_mut()
                    .read_to_string()?;
                Ok(LoadedBuild {
                    xml: code_or_xml(&text)?,
                    character: None,
                })
            }
        }
    }
}

fn from_ninja(reference: &CharacterRef) -> Result<LoadedBuild> {
    let character = ninja::fetch_character(reference)?;
    Ok(LoadedBuild {
        xml: pob::decode_build_code(&character.path_of_building_export)?,
        character: Some(character),
    })
}

fn code_or_xml(text: &str) -> Result<String> {
    let text = text.trim();

    if text.starts_with('<') {
        return Ok(text.to_string());
    }

    pob::decode_build_code(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_sources() {
        let profile = "https://poe.ninja/poe2/profile/a-1/std/character/b";
        assert!(matches!(Source::read(profile).unwrap(), Source::Profile(_)));

        assert_eq!(
            Source::read("Name#1234/Character").unwrap(),
            Source::AccountCharacter {
                account: "Name#1234".into(),
                character: "Character".into(),
            }
        );
        assert_eq!(
            Source::read("https://pobb.in/abc").unwrap(),
            Source::Link("https://pobb.in/abc".into())
        );
        assert_eq!(
            Source::read(" eNrtPW1z2 ").unwrap(),
            Source::Text("eNrtPW1z2".into())
        );
    }

    #[test]
    fn reads_files() {
        let path = std::env::temp_dir().join(format!("poe2-source-{}.xml", std::process::id()));
        fs::write(&path, "<PathOfBuilding2/>").unwrap();

        let source = Source::read(path.to_str().unwrap());

        fs::remove_file(&path).unwrap();
        assert_eq!(source.unwrap(), Source::Text("<PathOfBuilding2/>".into()));
    }
}
