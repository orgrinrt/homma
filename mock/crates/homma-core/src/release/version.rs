//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The version a repo carries, read from and written back to its manifest in
//! place, so a bump touches one line and nothing else moves.

use std::fmt;
use std::path::Path;

use homma_api::{RepoKind, Version};

/// The manifest could not be read, parsed, or does not carry a version.
#[derive(Debug)]
pub enum VersionError {
    Io(std::io::Error),
    /// Which manifest, and what was wrong with it.
    Manifest(String, String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Io(e) => write!(f, "{e}"),
            VersionError::Manifest(file, why) => write!(f, "{file}: {why}"),
        }
    }
}

impl std::error::Error for VersionError {}

impl From<std::io::Error> for VersionError {
    fn from(e: std::io::Error) -> Self {
        VersionError::Io(e)
    }
}

/// One manifest that carries a version: its file at the root, and how its
/// version is read and written in place.
struct Manifest {
    file:  &'static str,
    read:  fn(&Path) -> Result<Version, VersionError>,
    write: fn(&Path, &Version) -> Result<(), VersionError>,
}

const CARGO: Manifest = Manifest {
    file:  "Cargo.toml",
    read:  read_cargo,
    write: write_cargo,
};
const DENO: Manifest = Manifest {
    file:  "deno.json",
    read:  read_deno,
    write: write_deno,
};
const NUT: Manifest = Manifest {
    file:  "nut.toml",
    read:  read_nut,
    write: write_nut,
};

/// The one list of which manifests carry the version of a repo of `kind`.
/// Content carries none.
fn of(kind: RepoKind) -> &'static [Manifest] {
    match kind {
        RepoKind::Crate => &[CARGO],
        RepoKind::Deno => &[DENO],
        RepoKind::Both => &[CARGO, DENO],
        RepoKind::Nut => &[NUT],
        RepoKind::Content => &[],
    }
}

/// The files that carry the version of a repo of `kind`, in the order a
/// release names them.
pub fn manifests(kind: RepoKind) -> impl Iterator<Item = &'static str> {
    of(kind).iter().map(|m| m.file)
}

/// The version one manifest of a repo of `kind` carries at `root`, by its
/// file name; a file the kind does not version is an error.
pub fn read_manifest(root: &Path, kind: RepoKind, file: &str) -> Result<Version, VersionError> {
    let m = of(kind).iter().find(|m| m.file == file).ok_or_else(|| {
        VersionError::Manifest(file.into(), "not a manifest this kind versions".into())
    })?;
    (m.read)(root)
}

/// Read the version at `root`. A repo with several manifests must carry the
/// same one in each, and disagreeing is an error rather than a pick.
pub fn read(root: &Path, kind: RepoKind) -> Result<Version, VersionError> {
    let mut found: Option<(&str, Version)> = None;
    for m in of(kind) {
        let v = (m.read)(root)?;
        match &found {
            Some((first, f)) if *f != v => {
                return Err(VersionError::Manifest(
                    format!("{first} and {}", m.file),
                    format!("disagree on the version: {f} against {v}"),
                ));
            },
            Some(_) => {},
            None => found = Some((m.file, v)),
        }
    }
    found
        .map(|(_, v)| v)
        .ok_or_else(|| VersionError::Manifest("manifest".into(), "no manifest".into()))
}

/// Write `version` into every manifest the kind has, preserving the rest of
/// the file as it was.
pub fn write(root: &Path, kind: RepoKind, version: &Version) -> Result<(), VersionError> {
    for m in of(kind) {
        (m.write)(root, version)?;
    }
    Ok(())
}

/// `[workspace.package] version` where the manifest is a workspace with one,
/// else `[package] version`.
fn cargo_version_item(doc: &mut toml_edit::DocumentMut) -> Option<&mut toml_edit::Item> {
    let has_ws = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .is_some();
    if has_ws {
        doc.get_mut("workspace")?
            .get_mut("package")?
            .get_mut("version")
    } else {
        doc.get_mut("package")?.get_mut("version")
    }
}

/// `[package] version` alone: `nut.toml` has no workspace table.
fn package_version_item(doc: &mut toml_edit::DocumentMut) -> Option<&mut toml_edit::Item> {
    doc.get_mut("package")?.get_mut("version")
}

type Locate = fn(&mut toml_edit::DocumentMut) -> Option<&mut toml_edit::Item>;

fn read_toml(root: &Path, file: &str, locate: Locate) -> Result<Version, VersionError> {
    let text = std::fs::read_to_string(root.join(file))?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| VersionError::Manifest(file.into(), e.to_string()))?;
    let item = locate(&mut doc)
        .and_then(|i| i.as_str())
        .ok_or_else(|| VersionError::Manifest(file.into(), "no version".into()))?;
    item.parse()
        .map_err(|e: homma_api::NotAVersion| VersionError::Manifest(file.into(), e.to_string()))
}

fn read_cargo(root: &Path) -> Result<Version, VersionError> {
    read_toml(root, "Cargo.toml", cargo_version_item)
}

fn read_nut(root: &Path) -> Result<Version, VersionError> {
    read_toml(root, "nut.toml", package_version_item)
}

fn write_cargo(root: &Path, version: &Version) -> Result<(), VersionError> {
    write_toml(root, "Cargo.toml", cargo_version_item, version)
}

fn write_nut(root: &Path, version: &Version) -> Result<(), VersionError> {
    write_toml(root, "nut.toml", package_version_item, version)
}

fn write_toml(
    root: &Path,
    file: &str,
    locate: Locate,
    version: &Version,
) -> Result<(), VersionError> {
    let path = root.join(file);
    let text = std::fs::read_to_string(&path)?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| VersionError::Manifest(file.into(), e.to_string()))?;
    let item =
        locate(&mut doc).ok_or_else(|| VersionError::Manifest(file.into(), "no version".into()))?;
    let decor = item.as_value().map(|v| v.decor().clone());
    let mut value = toml_edit::Value::from(version.to_string());
    if let Some(d) = decor {
        *value.decor_mut() = d;
    }
    *item = toml_edit::Item::Value(value);
    std::fs::write(&path, doc.to_string())?;
    Ok(())
}

/// The byte range of the value of the top-level `"version"` key in a
/// `deno.json`, found textually so the file's own formatting survives a write.
fn deno_version_span(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b'"' => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let s = &text[start .. j.min(bytes.len())];
                i = j + 1;
                if depth != 1 || s != "version" {
                    continue;
                }
                // a key is followed by a colon; a value is not
                let after = &text[i ..];
                let trimmed = after.trim_start();
                if !trimmed.starts_with(':') {
                    continue;
                }
                let rest = trimmed[1 ..].trim_start();
                if !rest.starts_with('"') {
                    return None;
                }
                let vstart = text.len() - rest.len() + 1;
                let vend = vstart + text[vstart ..].find('"')?;
                return Some((vstart, vend));
            },
            _ => {},
        }
        i += 1;
    }
    None
}

fn read_deno(root: &Path) -> Result<Version, VersionError> {
    let text = std::fs::read_to_string(root.join("deno.json"))?;
    let (a, b) = deno_version_span(&text)
        .ok_or_else(|| VersionError::Manifest("deno.json".into(), "no version".into()))?;
    text[a .. b].parse().map_err(|e: homma_api::NotAVersion| {
        VersionError::Manifest("deno.json".into(), e.to_string())
    })
}

fn write_deno(root: &Path, version: &Version) -> Result<(), VersionError> {
    let path = root.join("deno.json");
    let text = std::fs::read_to_string(&path)?;
    let (a, b) = deno_version_span(&text)
        .ok_or_else(|| VersionError::Manifest("deno.json".into(), "no version".into()))?;
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(&text[.. a]);
    out.push_str(&version.to_string());
    out.push_str(&text[b ..]);
    std::fs::write(&path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_manifest_reads_and_writes_in_place_keeping_everything_else() {
        let d = tempfile::tempdir().unwrap();
        let text = "# top\n[package]\nname = \"x\"   # name\nversion = \"0.1.2\" # v\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\n";
        std::fs::write(d.path().join("Cargo.toml"), text).unwrap();
        assert_eq!(
            read(d.path(), RepoKind::Crate).unwrap(),
            Version::new(0, 1, 2)
        );
        write(d.path(), RepoKind::Crate, &Version::new(0, 1, 3)).unwrap();
        let after = std::fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
        assert_eq!(after, text.replace("0.1.2", "0.1.3"));
    }

    #[test]
    fn a_workspace_manifest_edits_the_workspace_package_version_not_a_member() {
        let d = tempfile::tempdir().unwrap();
        let text = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"1.0.0\"\n\n[package]\nname = \"root\"\nversion = \"9.9.9\"\n";
        std::fs::write(d.path().join("Cargo.toml"), text).unwrap();
        assert_eq!(
            read(d.path(), RepoKind::Crate).unwrap(),
            Version::new(1, 0, 0)
        );
        write(d.path(), RepoKind::Crate, &Version::new(1, 1, 0)).unwrap();
        let after = std::fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
        assert!(after.contains("version = \"1.1.0\""));
        assert!(
            after.contains("version = \"9.9.9\""),
            "the member's own version is untouched"
        );
    }

    #[test]
    fn deno_json_is_edited_textually_and_a_nested_version_key_is_not_the_one() {
        let d = tempfile::tempdir().unwrap();
        let text = "{\n  \"name\": \"@x/y\",\n  \"imports\": { \"z\": \"jsr:@a/b@1\", \"version\": \"nope\" },\n  \"version\":   \"2.3.4\",\n  \"exports\": \"./mod.ts\"\n}\n";
        std::fs::write(d.path().join("deno.json"), text).unwrap();
        assert_eq!(
            deno_version_span(text).map(|(a, b)| &text[a .. b]),
            Some("2.3.4")
        );
        assert_eq!(
            read(d.path(), RepoKind::Deno).unwrap(),
            Version::new(2, 3, 4)
        );
        let as_value = "{\"name\": \"version\", \"version\": \"1.0.0\"}";
        assert_eq!(
            deno_version_span(as_value).map(|(a, b)| &as_value[a .. b]),
            Some("1.0.0")
        );
        let escaped = "{\"desc\": \"a \\\"version\\\" of it\", \"version\": \"1.0.0\"}";
        assert_eq!(
            deno_version_span(escaped).map(|(a, b)| &escaped[a .. b]),
            Some("1.0.0")
        );
    }

    #[test]
    fn deno_json_top_level_version_round_trips_with_the_formatting_kept() {
        let d = tempfile::tempdir().unwrap();
        let text = "{\n  \"name\": \"@x/y\",\n  \"version\":   \"2.3.4\",\n  \"exports\": \"./mod.ts\"\n}\n";
        std::fs::write(d.path().join("deno.json"), text).unwrap();
        assert_eq!(
            read(d.path(), RepoKind::Deno).unwrap(),
            Version::new(2, 3, 4)
        );
        write(d.path(), RepoKind::Deno, &Version::new(3, 0, 0)).unwrap();
        let after = std::fs::read_to_string(d.path().join("deno.json")).unwrap();
        assert_eq!(after, text.replace("2.3.4", "3.0.0"));
    }

    #[test]
    fn both_manifests_must_agree_and_both_get_written() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(d.path().join("deno.json"), "{\"version\": \"0.2.0\"}").unwrap();
        assert!(matches!(
            read(d.path(), RepoKind::Both),
            Err(VersionError::Manifest(..))
        ));
        write(d.path(), RepoKind::Both, &Version::new(0, 3, 0)).unwrap();
        assert_eq!(
            read(d.path(), RepoKind::Both).unwrap(),
            Version::new(0, 3, 0)
        );
    }

    #[test]
    fn nut_toml_reads_and_writes_the_package_version_in_place_and_not_the_schema_s() {
        let d = tempfile::tempdir().unwrap();
        let text = "nutshell_branch = \"dev\"\n\n[package]\nname = \"x\"\nversion = \"0.2.0\" # v\n\n[meta]\n# the schema's\nversion = \"1.0.0\"\n";
        std::fs::write(d.path().join("nut.toml"), text).unwrap();
        assert_eq!(
            read(d.path(), RepoKind::Nut).unwrap(),
            Version::new(0, 2, 0)
        );
        write(d.path(), RepoKind::Nut, &Version::new(0, 3, 0)).unwrap();
        let after = std::fs::read_to_string(d.path().join("nut.toml")).unwrap();
        assert_eq!(after, text.replace("0.2.0", "0.3.0"));
        assert_eq!(
            read_manifest(d.path(), RepoKind::Nut, "nut.toml").unwrap(),
            Version::new(0, 3, 0)
        );
        assert!(read_manifest(d.path(), RepoKind::Nut, "Cargo.toml").is_err());
        // a nut.toml with only the schema's version carries none
        std::fs::write(d.path().join("nut.toml"), "[meta]\nversion = \"1.0.0\"\n").unwrap();
        assert!(matches!(
            read(d.path(), RepoKind::Nut),
            Err(VersionError::Manifest(..))
        ));
    }

    #[test]
    fn each_kind_names_its_manifests_and_content_names_none() {
        let of = |k| manifests(k).collect::<Vec<_>>();
        assert_eq!(of(RepoKind::Crate), ["Cargo.toml"]);
        assert_eq!(of(RepoKind::Deno), ["deno.json"]);
        assert_eq!(of(RepoKind::Both), ["Cargo.toml", "deno.json"]);
        assert_eq!(of(RepoKind::Nut), ["nut.toml"]);
        assert!(of(RepoKind::Content).is_empty());
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("nut.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(read(d.path(), RepoKind::Content).is_err());
    }

    #[test]
    fn a_manifest_without_a_version_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        assert!(matches!(
            read(d.path(), RepoKind::Crate),
            Err(VersionError::Manifest(..))
        ));
        std::fs::write(d.path().join("deno.json"), "{}").unwrap();
        assert!(matches!(
            read(d.path(), RepoKind::Deno),
            Err(VersionError::Manifest(..))
        ));
    }
}
