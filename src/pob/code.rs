use std::io::Read;

use anyhow::{Context, Result};
use base64::Engine;
use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use flate2::read::ZlibDecoder;

/// PoB build codes are zlib-compressed XML in URL-safe base64, padded or not.
const BUILD_CODE: GeneralPurpose = GeneralPurpose::new(
    &URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

pub fn decode_build_code(code: &str) -> Result<String> {
    let compressed = BUILD_CODE
        .decode(code.trim())
        .context("not a valid PoB build code")?;
    let mut xml = String::new();
    ZlibDecoder::new(compressed.as_slice())
        .read_to_string(&mut xml)
        .context("not a valid PoB build code")?;
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_build_code() {
        let code = include_str!("../../tests/fixtures/koftespies.pob");

        let xml = decode_build_code(code).unwrap();

        assert!(xml.contains(r#"className="Monk""#));
        assert!(xml.contains(r#"level="79""#));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_build_code("not a build code").is_err());
    }
}
