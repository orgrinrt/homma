//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What each registry holds for a package, read without a token: crates.io
//! through its sparse index, jsr through its meta document, npm through its
//! registry document. A registry that answers "no such package" is an empty
//! list; one that could not be asked is an error, since the two mean
//! different things to a check.

use std::fmt;

use homma_api::Version;

/// The three registries a package here ships to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Registry {
    CratesIo,
    Jsr,
    Npm,
}

impl Registry {
    /// The name the credential tool takes for it.
    pub fn key(self) -> &'static str {
        match self {
            Registry::CratesIo => "crates-io",
            Registry::Jsr => "jsr",
            Registry::Npm => "npm",
        }
    }
}

impl fmt::Display for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// The registry could not be asked, which is not the same as it holding
/// nothing.
#[derive(Debug)]
pub struct Unreachable {
    pub registry: Registry,
    pub package:  String,
    pub why:      String,
}

impl fmt::Display for Unreachable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} could not answer for {}: {}", self.registry, self.package, self.why)
    }
}

impl std::error::Error for Unreachable {}

/// Where crates.io's sparse index files a crate, by the length of its name.
pub fn crates_index_path(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    match n.len() {
        1 => format!("1/{n}"),
        2 => format!("2/{n}"),
        3 => format!("3/{}/{n}", &n[.. 1]),
        _ => format!("{}/{}/{n}", &n[.. 2], &n[2 .. 4]),
    }
}

/// The url each registry is asked at.
pub fn url(registry: Registry, package: &str) -> String {
    match registry {
        Registry::CratesIo => format!("https://index.crates.io/{}", crates_index_path(package)),
        Registry::Jsr => {
            let bare = package.trim_start_matches('@');
            format!("https://jsr.io/@{bare}/meta.json")
        },
        Registry::Npm => format!("https://registry.npmjs.org/{package}"),
    }
}

/// Every version `registry` holds for `package`, in the order the registry
/// lists them, which is publish order on all three.
pub fn published_versions(
    registry: Registry,
    package: &str,
) -> Result<Vec<Version>, Unreachable> {
    let target = url(registry, package);
    let fail = |why: String| {
        Unreachable {
            registry,
            package: package.to_string(),
            why,
        }
    };
    let agent = ureq::AgentBuilder::new()
        .user_agent(concat!("homma/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let body = match agent.get(&target).call() {
        Ok(resp) => resp.into_string().map_err(|e| fail(e.to_string()))?,
        Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
        Err(e) => return Err(fail(e.to_string())),
    };
    parse(registry, &body).map_err(fail)
}

/// The versions in a registry's answer, kept separate from the fetch so the
/// three shapes are tested without a network.
pub fn parse(registry: Registry, body: &str) -> Result<Vec<Version>, String> {
    let mut out = Vec::new();
    match registry {
        Registry::CratesIo => {
            for line in body.lines().filter(|l| !l.trim().is_empty()) {
                let row: serde_json::Value =
                    serde_json::from_str(line).map_err(|e| e.to_string())?;
                if let Some(v) = row.get("vers").and_then(|v| v.as_str()) {
                    out.push(v.parse().map_err(|e: homma_api::NotAVersion| e.to_string())?);
                }
            }
        },
        Registry::Jsr => {
            let doc: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
            let Some(map) = doc.get("versions").and_then(|v| v.as_object()) else {
                return Ok(out);
            };
            for (v, meta) in map {
                if meta.get("yanked").and_then(|y| y.as_bool()).unwrap_or(false) {
                    continue;
                }
                out.push(v.parse().map_err(|e: homma_api::NotAVersion| e.to_string())?);
            }
            out.sort();
        },
        Registry::Npm => {
            let doc: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
            if let Some(times) = doc.get("time").and_then(|t| t.as_object()) {
                let mut dated: Vec<(&str, &str)> = times
                    .iter()
                    .filter(|(k, _)| *k != "created" && *k != "modified")
                    .filter_map(|(k, v)| v.as_str().map(|t| (k.as_str(), t)))
                    .collect();
                dated.sort_by(|a, b| a.1.cmp(b.1));
                for (v, _) in dated {
                    out.push(v.parse().map_err(|e: homma_api::NotAVersion| e.to_string())?);
                }
            } else if let Some(map) = doc.get("versions").and_then(|v| v.as_object()) {
                for v in map.keys() {
                    out.push(v.parse().map_err(|e: homma_api::NotAVersion| e.to_string())?);
                }
            }
        },
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sparse_index_path_follows_the_name_length() {
        assert_eq!(crates_index_path("a"), "1/a");
        assert_eq!(crates_index_path("ab"), "2/ab");
        assert_eq!(crates_index_path("abc"), "3/a/abc");
        assert_eq!(crates_index_path("notko"), "no/tk/notko");
        assert_eq!(crates_index_path("Include_Proc_Macro"), "in/cl/include_proc_macro");
    }

    #[test]
    fn each_registry_has_its_url() {
        assert_eq!(url(Registry::CratesIo, "renki"), "https://index.crates.io/re/nk/renki");
        assert_eq!(url(Registry::Jsr, "@hiisi/loitsu"), "https://jsr.io/@hiisi/loitsu/meta.json");
        assert_eq!(url(Registry::Jsr, "hiisi/loitsu"), "https://jsr.io/@hiisi/loitsu/meta.json");
        assert_eq!(url(Registry::Npm, "loitsu"), "https://registry.npmjs.org/loitsu");
    }

    #[test]
    fn the_index_is_one_object_per_line_in_publish_order() {
        let body = "{\"name\":\"x\",\"vers\":\"0.1.0\"}\n{\"name\":\"x\",\"vers\":\"0.1.1\"}\n\n{\"name\":\"x\",\"vers\":\"0.2.0\"}\n";
        let v = parse(Registry::CratesIo, body).unwrap();
        assert_eq!(v, vec![Version::new(0, 1, 0), Version::new(0, 1, 1), Version::new(0, 2, 0)]);
    }

    #[test]
    fn jsr_meta_lists_versions_as_a_map_and_a_yanked_one_is_dropped() {
        let body = r#"{"scope":"hiisi","name":"x","latest":"0.2.0","versions":{"0.2.0":{},"0.1.0":{},"0.1.5":{"yanked":true}}}"#;
        let v = parse(Registry::Jsr, body).unwrap();
        assert_eq!(v, vec![Version::new(0, 1, 0), Version::new(0, 2, 0)]);
    }

    #[test]
    fn npm_orders_by_publish_time_and_skips_the_two_bookkeeping_keys() {
        let body = r#"{"name":"x","time":{"created":"2026-01-01T00:00:00Z","modified":"2026-03-01T00:00:00Z","0.2.0":"2026-02-01T00:00:00Z","0.1.0":"2026-01-01T00:00:00Z"},"versions":{"0.1.0":{},"0.2.0":{}}}"#;
        let v = parse(Registry::Npm, body).unwrap();
        assert_eq!(v, vec![Version::new(0, 1, 0), Version::new(0, 2, 0)]);
    }

    #[test]
    fn a_body_that_is_not_the_shape_is_an_error_not_an_empty_list() {
        assert!(parse(Registry::CratesIo, "not json\n").is_err());
        assert!(parse(Registry::Jsr, "[]").is_ok_and(|v| v.is_empty()));
        assert!(parse(Registry::Npm, "{").is_err());
        assert!(parse(Registry::CratesIo, "{\"vers\":\"nope\"}\n").is_err());
    }

    #[test]
    fn the_credential_key_is_the_registry_name_the_tool_takes() {
        assert_eq!(Registry::CratesIo.key(), "crates-io");
        assert_eq!(Registry::Jsr.to_string(), "jsr");
        assert_eq!(Registry::Npm.key(), "npm");
    }
}
