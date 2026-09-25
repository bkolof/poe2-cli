use std::io::{Read, Write};

use anyhow::{Context, Result};
use base64::Engine;
use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

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

pub fn encode_build_code(xml: &str) -> Result<String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(xml.as_bytes())?;
    Ok(BUILD_CODE.encode(encoder.finish()?))
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
    fn encodes_what_it_decodes() {
        let xml = decode_build_code(include_str!("../../tests/fixtures/koftespies.pob")).unwrap();

        let code = encode_build_code(&xml).unwrap();

        assert_eq!(decode_build_code(&code).unwrap(), xml);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_build_code("not a build code").is_err());
    }
}
