//! The pathofexile.com session (the POESESSID cookie) that the trade site
//! needs for weighted searches. It is as powerful as the account password, so
//! it lives in a file only the user can read and is never printed.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

fn path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("cannot determine the user config directory")?
        .join("poe2");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("session"))
}

/// Accepts the bare cookie value or `POESESSID=<value>`.
pub fn normalise(input: &str) -> Result<String> {
    let value = input.trim();
    let value = value.strip_prefix("POESESSID=").unwrap_or(value).trim();

    if value.len() != 32 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("a POESESSID is 32 hexadecimal characters");
    }

    Ok(value.to_string())
}

pub fn load() -> Result<Option<String>> {
    let path = path()?;

    if !path.exists() {
        return Ok(None);
    }

    Ok(Some(fs::read_to_string(path)?.trim().to_string()))
}

pub fn save(session: &str) -> Result<()> {
    let path = path()?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(session.as_bytes())?;
    }

    #[cfg(not(unix))]
    fs::write(&path, session)?;

    Ok(())
}

pub fn delete() -> Result<bool> {
    let path = path()?;

    if !path.exists() {
        return Ok(false);
    }

    fs::remove_file(path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_cookie_input() {
        let value = "0123456789abcdef0123456789abcdef";

        assert_eq!(normalise(value).unwrap(), value);
        assert_eq!(normalise(&format!(" POESESSID={value}\n")).unwrap(), value);
        assert!(normalise("too short").is_err());
    }
}
