//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The layers: the overlay files beside `homma.toml` that the hooks tables are
//! also read from, `layers` in `homma.toml`. A declared list rather than a glob
//! of `homma.*.toml`, because a hook is code run on every commit and the files
//! that may add one are a decision somebody made. No I/O; `homma-core` reads
//! the files.

use std::fmt;

/// The file the workspace's manifest is, which is never a layer of itself.
pub const MANIFEST: &str = "homma.toml";

/// The one layer a manifest that names none has: the clone's own file.
pub const DEFAULT_LAYER: &str = "homma.local.toml";

/// The overlay file names, in the order they are applied. Built through
/// [`Layers::new`] and nowhere else, so a name that is a path, the manifest
/// itself, or not a `homma.<name>.toml` does not exist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Layers(Vec<String>);

impl Layers {
    /// The list, or why a name in it is refused.
    pub fn new(names: Vec<String>) -> Result<Self, InvalidLayers> {
        for (i, name) in names.iter().enumerate() {
            if !is_layer_name(name) {
                return Err(InvalidLayers::Name(name.clone()));
            }
            if names[.. i].contains(name) {
                return Err(InvalidLayers::Twice(name.clone()));
            }
        }
        Ok(Self(names))
    }

    /// The names in the order applied.
    pub fn names(&self) -> &[String] {
        &self.0
    }

    /// Whether `file` is one of the layers.
    pub fn names_file(&self, file: &str) -> bool {
        self.0.iter().any(|n| n == file)
    }
}

impl Default for Layers {
    fn default() -> Self {
        Self(vec![DEFAULT_LAYER.to_string()])
    }
}

impl<'de> serde::Deserialize<'de> for Layers {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let names = Vec::<String>::deserialize(d)?;
        Self::new(names).map_err(serde::de::Error::custom)
    }
}

/// Whether `name` is `homma.<middle>.toml` with a plain middle, and not the
/// manifest.
pub fn is_layer_name(name: &str) -> bool {
    let Some(middle) = name
        .strip_prefix("homma.")
        .and_then(|rest| rest.strip_suffix(".toml"))
    else {
        return false;
    };
    !middle.is_empty()
        && middle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Why a `layers` list is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidLayers {
    /// Not a `homma.<name>.toml` in the workspace root.
    Name(String),
    /// Named twice, which would apply its entries twice.
    Twice(String),
}

impl fmt::Display for InvalidLayers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidLayers::Name(n) => {
                write!(
                    f,
                    "`layers` names `{n}`, which is not a `homma.<name>.toml` file in the \
                     workspace root; the name takes letters, digits, dash and underscore, and \
                     `{MANIFEST}` is the base and not a layer"
                )
            },
            InvalidLayers::Twice(n) => write!(f, "`layers` names `{n}` twice"),
        }
    }
}

impl std::error::Error for InvalidLayers {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Layers, String> {
        #[derive(serde::Deserialize)]
        struct T {
            layers: Layers,
        }
        toml::from_str::<T>(text)
            .map(|t| t.layers)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn a_manifest_naming_none_has_the_clones_own_file() {
        assert_eq!(Layers::default().names(), &["homma.local.toml".to_string()]);
    }

    #[test]
    fn the_list_keeps_the_order_written() {
        let l =
            parse("layers = [\"homma.b.toml\", \"homma.a.toml\", \"homma.local.toml\"]").unwrap();
        assert_eq!(l.names(), &[
            "homma.b.toml".to_string(),
            "homma.a.toml".to_string(),
            "homma.local.toml".to_string()
        ]);
        assert!(l.names_file("homma.a.toml"));
        assert!(!l.names_file("homma.c.toml"));
    }

    #[test]
    fn an_empty_list_is_a_workspace_with_no_overlay() {
        assert!(parse("layers = []").unwrap().names().is_empty());
    }

    #[test]
    fn what_is_not_a_homma_overlay_name_is_refused() {
        for bad in [
            "homma.toml",
            "homma..toml",
            "homma.local.toml.replaced",
            "other.local.toml",
            "../homma.local.toml",
            "sub/homma.local.toml",
            "homma.a.b.toml",
            "homma.a b.toml",
            "homma.local",
            "",
        ] {
            let err = parse(&format!("layers = [\"{bad}\"]")).unwrap_err();
            assert!(err.contains("homma.<name>.toml"), "{bad}: {err}");
        }
    }

    #[test]
    fn a_name_twice_is_refused() {
        let err = parse("layers = [\"homma.a.toml\", \"homma.a.toml\"]").unwrap_err();
        assert!(err.contains("twice"), "{err}");
    }
}
