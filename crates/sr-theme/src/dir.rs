//! A theme package as authors write it: a folder (docs/10 section 4)
//!
//! ```text
//! my-theme/
//!   theme.json    manifest
//!   tokens.json   token overrides: { "--ui-accent": "#5865f2", ... }
//!   assets/       PNG, WebP or AVIF images
//!   fonts/        WOFF2 files, each with a licence file
//! ```
//!
//! [`package_from_files`] turns the folder contents (a map from package-relative path to bytes) into the
//! [`ThemePackage`] that [`crate::validate`] and [`crate::validate_files`] check, so the local validator
//! (`sr-cli package validate`) and the upload pipeline judge the same thing. Pure and wasm-compatible: the
//! caller reads the files.

use crate::model::{AssetRef, FontRef, ThemeManifest, ThemePackage};
use std::collections::BTreeMap;

/// Licence file names tried for a font `x.woff2`, in order. The last is the default shown when none exists.
fn license_candidates(stem: &str) -> [String; 5] {
    [
        format!("fonts/{stem}.license.txt"),
        format!("fonts/{stem}.txt"),
        "fonts/LICENSE.txt".to_owned(),
        "fonts/LICENSE".to_owned(),
        "fonts/OFL.txt".to_owned(),
    ]
}

fn mime_for(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        // Unknown on purpose: the validator reports `invalid-asset-type`.
        _ => "application/octet-stream",
    }
}

/// Build the package from the folder contents, or the reasons it cannot be read at all (a missing or
/// unparsable `theme.json` or `tokens.json`). Content problems are left to the validators.
pub fn package_from_files(files: &BTreeMap<String, Vec<u8>>) -> Result<ThemePackage, Vec<String>> {
    let mut problems = Vec::new();
    let manifest = match files.get("theme.json") {
        None => {
            problems.push("theme.json is missing".to_owned());
            None
        }
        Some(b) => match serde_json::from_slice::<ThemeManifest>(b) {
            Ok(m) => Some(m),
            Err(e) => {
                problems.push(format!("theme.json: {e}"));
                None
            }
        },
    };
    let tokens = match files.get("tokens.json") {
        None => BTreeMap::new(),
        Some(b) => match serde_json::from_slice::<BTreeMap<String, String>>(b) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("tokens.json: {e}"));
                BTreeMap::new()
            }
        },
    };
    let Some(manifest) = manifest else {
        return Err(problems);
    };
    if !problems.is_empty() {
        return Err(problems);
    }
    let assets = files
        .iter()
        .filter(|(p, _)| p.starts_with("assets/") && !p.ends_with('/'))
        .map(|(p, b)| AssetRef {
            path: p.clone(),
            mime: mime_for(p).to_owned(),
            bytes: b.len() as u64,
        })
        .collect();
    let fonts = files
        .iter()
        .filter(|(p, _)| p.starts_with("fonts/") && p.to_ascii_lowercase().ends_with(".woff2"))
        .map(|(p, b)| {
            let file = p.rsplit('/').next().unwrap_or(p);
            let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
            let candidates = license_candidates(stem);
            let license = candidates
                .iter()
                .find(|c| files.contains_key(*c))
                .unwrap_or(&candidates[0])
                .clone();
            FontRef {
                path: p.clone(),
                family: stem.to_owned(),
                bytes: b.len() as u64,
                license,
            }
        })
        .collect();
    Ok(ThemePackage {
        manifest,
        tokens,
        assets,
        fonts,
    })
}

/// Files in the folder that no part of the package refers to (a typo in a folder name, a stray source file).
pub fn unused_files(files: &BTreeMap<String, Vec<u8>>, pkg: &ThemePackage) -> Vec<String> {
    let mut used: Vec<&str> = vec!["theme.json", "tokens.json"];
    used.extend(pkg.assets.iter().map(|a| a.path.as_str()));
    for f in &pkg.fonts {
        used.push(&f.path);
        used.push(&f.license);
    }
    files
        .keys()
        .filter(|p| !used.contains(&p.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{validate, validate_files};

    fn manifest() -> Vec<u8> {
        br#"{"id":"night-owl","name":"Night Owl","version":"1.0.0","author":"Ada","targets":["lounge"],"scheme":"dark"}"#
            .to_vec()
    }

    fn png() -> Vec<u8> {
        // A 1x1 transparent PNG.
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xA7, 0x35, 0x81,
            0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    fn folder(entries: &[(&str, Vec<u8>)]) -> BTreeMap<String, Vec<u8>> {
        entries
            .iter()
            .map(|(p, b)| ((*p).to_owned(), b.clone()))
            .collect()
    }

    #[test]
    fn a_folder_becomes_a_package_the_validators_accept() {
        let files = folder(&[
            ("theme.json", manifest()),
            ("tokens.json", br##"{"--ui-accent":"#7c5cff"}"##.to_vec()),
            ("assets/dots.png", png()),
            ("fonts/Mono.woff2", b"wOF2....".to_vec()),
            ("fonts/Mono.license.txt", b"SIL OFL 1.1".to_vec()),
        ]);
        let pkg = package_from_files(&files).unwrap();
        assert_eq!(pkg.assets[0].mime, "image/png");
        assert_eq!(pkg.assets[0].bytes, png().len() as u64);
        assert_eq!(pkg.fonts[0].family, "Mono");
        assert_eq!(pkg.fonts[0].license, "fonts/Mono.license.txt");
        assert!(validate(&pkg).is_valid(), "{:?}", validate(&pkg).issues);
        assert!(
            validate_files(&pkg, &files).is_valid(),
            "{:?}",
            validate_files(&pkg, &files).issues
        );
        assert!(unused_files(&files, &pkg).is_empty());
    }

    #[test]
    fn unreadable_manifests_are_reported_not_guessed() {
        let e = package_from_files(&folder(&[])).unwrap_err();
        assert_eq!(e, ["theme.json is missing"]);
        let e = package_from_files(&folder(&[("theme.json", b"{".to_vec())])).unwrap_err();
        assert!(e[0].starts_with("theme.json:"), "{e:?}");
        let e = package_from_files(&folder(&[
            ("theme.json", manifest()),
            ("tokens.json", b"[1]".to_vec()),
        ]))
        .unwrap_err();
        assert!(e[0].starts_with("tokens.json:"), "{e:?}");
        // an unknown manifest field is a typo to surface
        let e = package_from_files(&folder(&[(
            "theme.json",
            br#"{"id":"night-owl","name":"N","version":"1.0.0","author":"A","targets":["lounge"],"scheme":"dark","colour":1}"#.to_vec(),
        )]))
        .unwrap_err();
        assert!(e[0].contains("colour"), "{e:?}");
    }

    #[test]
    fn content_problems_come_from_the_validators() {
        let files = folder(&[
            ("theme.json", manifest()),
            ("tokens.json", br##"{"--not-a-token":"#fff"}"##.to_vec()),
            ("assets/clip.gif", png()),
            ("fonts/Mono.woff2", b"wOF2....".to_vec()),
            ("notes.txt", b"todo".to_vec()),
        ]);
        let pkg = package_from_files(&files).unwrap();
        let codes: Vec<&str> = validate(&pkg).issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&"unknown-token"), "{codes:?}");
        assert!(codes.contains(&"invalid-asset-type"), "{codes:?}");
        let file_codes: Vec<&str> = validate_files(&pkg, &files)
            .issues
            .iter()
            .map(|i| i.code)
            .collect();
        assert!(file_codes.contains(&"missing-file"), "{file_codes:?}");
        assert_eq!(unused_files(&files, &pkg), ["notes.txt"]);
    }

    #[test]
    fn licence_files_are_found_by_name() {
        for lic in ["fonts/LICENSE.txt", "fonts/OFL.txt", "fonts/LICENSE"] {
            let files = folder(&[
                ("theme.json", manifest()),
                ("fonts/A.woff2", b"wOF2".to_vec()),
                (lic, b"x".to_vec()),
            ]);
            let pkg = package_from_files(&files).unwrap();
            assert_eq!(pkg.fonts[0].license, lic);
        }
    }
}
