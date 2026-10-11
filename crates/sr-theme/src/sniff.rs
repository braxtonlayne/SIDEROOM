//! Byte sniffing for package files (Q65). The declared metadata in `ThemePackage` is checked
//! against the real bytes: size, magic number, image dimensions, WOFF2 signature and the
//! licence file. Licence contents are never parsed. AVIF is recognised by its `ftyp` brand
//! only (no AVIF decoder is compiled in), so its dimensions are not read.

use crate::model::ThemePackage;
use crate::validate::{Issue, Severity, ValidationReport};
use std::collections::BTreeMap;
use std::io::Cursor;

/// Licence files: non-empty and at most 64 KiB (Q65).
pub const MAX_LICENSE_BYTES: usize = 64 * 1024;

fn fail(r: &mut ValidationReport, code: &'static str, path: String, message: String) {
    r.issues.push(Issue {
        severity: Severity::Error,
        code,
        path,
        message,
    });
}

/// Check every declared asset, font and licence against `files`, a map from package-relative
/// path to file bytes. Missing entries, size mismatches and unrecognised content are errors.
pub fn validate_files(pkg: &ThemePackage, files: &BTreeMap<String, Vec<u8>>) -> ValidationReport {
    let mut r = ValidationReport::default();
    for (i, a) in pkg.assets.iter().enumerate() {
        let base = format!("assets[{i}]");
        let Some(data) = files.get(&a.path) else {
            fail(
                &mut r,
                "missing-file",
                base,
                format!("no bytes for `{}`", a.path),
            );
            continue;
        };
        if data.len() as u64 != a.bytes {
            fail(
                &mut r,
                "bytes-mismatch",
                format!("{base}.bytes"),
                format!("declared {} bytes, found {}", a.bytes, data.len()),
            );
        }
        if let Err(m) = sniff_image(&a.mime, data) {
            fail(&mut r, "image-sniff-failed", format!("{base}.path"), m);
        }
    }
    for (i, f) in pkg.fonts.iter().enumerate() {
        let base = format!("fonts[{i}]");
        match files.get(&f.path) {
            None => fail(
                &mut r,
                "missing-file",
                base.clone(),
                format!("no bytes for `{}`", f.path),
            ),
            Some(data) => {
                if data.len() as u64 != f.bytes {
                    fail(
                        &mut r,
                        "bytes-mismatch",
                        format!("{base}.bytes"),
                        format!("declared {} bytes, found {}", f.bytes, data.len()),
                    );
                }
                if !data.starts_with(b"wOF2") {
                    fail(
                        &mut r,
                        "font-sniff-failed",
                        format!("{base}.path"),
                        "file does not start with the WOFF2 signature".to_string(),
                    );
                }
            }
        }
        match files.get(&f.license) {
            None => fail(
                &mut r,
                "missing-file",
                format!("{base}.license"),
                format!("no licence file `{}`", f.license),
            ),
            Some(lic) if lic.is_empty() => fail(
                &mut r,
                "empty-license",
                format!("{base}.license"),
                "licence file is empty".to_string(),
            ),
            Some(lic) if lic.len() > MAX_LICENSE_BYTES => fail(
                &mut r,
                "license-too-large",
                format!("{base}.license"),
                "licence file exceeds 64 KiB".to_string(),
            ),
            Some(_) => {}
        }
    }
    r
}

/// Checks that `data` is the image type `mime` declares and, for PNG and WebP, that its
/// header decodes to non-zero dimensions.
fn sniff_image(mime: &str, data: &[u8]) -> Result<(), String> {
    if mime == "image/avif" {
        let brand = data.get(4..12).unwrap_or_default();
        return if brand == b"ftypavif" || brand == b"ftypavis" {
            Ok(())
        } else {
            Err("file is not AVIF".to_string())
        };
    }
    let expected = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/webp" => image::ImageFormat::WebP,
        _ => return Err(format!("unsupported mime `{mime}`")),
    };
    let reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if reader.format() != Some(expected) {
        return Err(format!("file is not {mime}"));
    }
    let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if w == 0 || h == 0 {
        return Err("image has zero dimensions".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssetRef, FontRef, ThemeManifest, ThemeScheme};

    /// A 1x1 greyscale PNG.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn pkg(asset: Option<AssetRef>, font: Option<FontRef>) -> ThemePackage {
        ThemePackage {
            manifest: ThemeManifest {
                id: "test-theme".into(),
                name: "Test".into(),
                version: "1.0.0".into(),
                author: "Tester".into(),
                targets: vec!["lounge".into()],
                scheme: ThemeScheme::Dark,
                extends: None,
            },
            tokens: Default::default(),
            assets: asset.into_iter().collect(),
            fonts: font.into_iter().collect(),
        }
    }

    fn codes(r: &ValidationReport) -> Vec<&'static str> {
        r.errors().map(|i| i.code).collect()
    }

    #[test]
    fn real_png_passes_and_wrong_bytes_fail() {
        let asset = AssetRef {
            path: "assets/dot.png".into(),
            mime: "image/png".into(),
            bytes: PNG_1X1.len() as u64,
        };
        let p = pkg(Some(asset.clone()), None);
        let mut files = BTreeMap::new();
        files.insert(asset.path.clone(), PNG_1X1.to_vec());
        assert!(validate_files(&p, &files).is_valid());

        files.insert(asset.path.clone(), b"GIF89a not a png at all".to_vec());
        assert!(codes(&validate_files(&p, &files)).contains(&"image-sniff-failed"));
    }

    #[test]
    fn size_mismatch_and_missing_files_are_errors() {
        let asset = AssetRef {
            path: "assets/dot.png".into(),
            mime: "image/png".into(),
            bytes: 999,
        };
        let p = pkg(Some(asset.clone()), None);
        let mut files = BTreeMap::new();
        files.insert(asset.path.clone(), PNG_1X1.to_vec());
        assert!(codes(&validate_files(&p, &files)).contains(&"bytes-mismatch"));
        assert!(codes(&validate_files(&p, &BTreeMap::new())).contains(&"missing-file"));
    }

    #[test]
    fn font_needs_woff2_signature_and_a_licence() {
        let font = FontRef {
            path: "fonts/a.woff2".into(),
            family: "Alpha".into(),
            bytes: 8,
            license: "fonts/LICENSE.txt".into(),
        };
        let p = pkg(None, Some(font.clone()));
        let mut files = BTreeMap::new();
        files.insert(font.path.clone(), b"wOF2\0\0\0\0".to_vec());
        files.insert(font.license.clone(), b"OFL 1.1".to_vec());
        assert!(validate_files(&p, &files).is_valid());

        files.insert(font.path.clone(), b"\0\0\0\0\0\0\0\0".to_vec());
        assert!(codes(&validate_files(&p, &files)).contains(&"font-sniff-failed"));

        files.insert(font.path.clone(), b"wOF2\0\0\0\0".to_vec());
        files.insert(font.license.clone(), Vec::new());
        assert!(codes(&validate_files(&p, &files)).contains(&"empty-license"));

        files.insert(font.license.clone(), vec![b'x'; MAX_LICENSE_BYTES + 1]);
        assert!(codes(&validate_files(&p, &files)).contains(&"license-too-large"));
    }
}
