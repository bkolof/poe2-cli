//! Downloads the pinned PoB-PoE2 release into the user's local data directory.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

/// The PoB-PoE2 release this binary is tested against. Bumping it means
/// running the stats regression test in `tests/pob.rs`.
pub const VERSION: &str = "v0.23.1";

/// Where the pinned release lives, if it is fully installed.
pub fn installed_dir() -> Option<PathBuf> {
    let dir = release_dir().ok()?;
    dir.join("src/HeadlessWrapper.lua").is_file().then_some(dir)
}

pub fn ensure_installed() -> Result<PathBuf> {
    if let Some(dir) = installed_dir() {
        return Ok(dir);
    }

    let dir = release_dir()?;
    let staging = dir.with_extension("partial");

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }

    eprintln!("Downloading Path of Building {VERSION} (about 390 MB, once per version)...");
    let url = format!(
        "https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/archive/refs/tags/{VERSION}.tar.gz"
    );
    let response = ureq::get(&url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    unpack(response.into_body().into_reader(), &staging)?;
    fs::rename(&staging, &dir)?;
    eprintln!("Installed to {}", dir.display());
    Ok(dir)
}

fn release_dir() -> Result<PathBuf> {
    let data =
        dirs::data_local_dir().context("cannot determine the user's local data directory")?;
    let dir = data.join("poe2").join("pob").join(VERSION);
    fs::create_dir_all(dir.parent().expect("release dir has a parent"))?;
    Ok(dir)
}

/// Unpack only what the calc engine needs: `src/` without the passive tree
/// images, which only the GUI draws, and the Lua libraries in `runtime/lua/`.
fn unpack(archive: impl std::io::Read, target: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(archive));

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // Drop the archive's top-level `PathOfBuilding-PoE2-<version>/` directory.
        let relative: PathBuf = path.components().skip(1).collect();

        if !is_needed(&relative) || !entry.header().entry_type().is_file() {
            continue;
        }

        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("unexpected path in PoB archive: {}", path.display());
        }

        let destination = target.join(&relative);
        fs::create_dir_all(destination.parent().expect("file has a parent"))?;
        entry.unpack(&destination)?;
    }

    Ok(())
}

fn is_needed(path: &Path) -> bool {
    if path.starts_with("src/TreeData") {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        return !matches!(extension, "png" | "jpg" | "zst");
    }

    path.starts_with("src") || path.starts_with("runtime/lua")
}
