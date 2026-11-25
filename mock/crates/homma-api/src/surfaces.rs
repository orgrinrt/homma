//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `[surfaces]` table: where the modes and the principles are authored, how
//! their generated forms are named, and the gate on the effort keys.

use serde::{Deserialize, Serialize};

use crate::path::RelPath;

fn rel(s: &str) -> RelPath {
    RelPath::new(s).expect("a default path is relative and contained")
}

/// `[surfaces]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SurfacesConfig {
    pub modes:      ModesConfig,
    pub principles: PrinciplesConfig,
    pub effort:     EffortGate,
}

/// `[surfaces.modes]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModesConfig {
    /// Where the modes are authored, relative to the workspace root.
    pub dir:            RelPath,
    /// What a mode's skill is named: this, then the mode's type.
    pub skill_prefix:   String,
    /// The most characters a mode's body may hold.
    pub body_max_chars: usize,
}

impl Default for ModesConfig {
    fn default() -> Self {
        Self {
            dir:            rel(".shared/modes"),
            skill_prefix:   "mode-".to_string(),
            body_max_chars: 500,
        }
    }
}

/// `[surfaces.principles]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrinciplesConfig {
    /// Where the principles are authored, relative to the workspace root.
    pub dir:          RelPath,
    /// What a principle's card is named: this, then the principle's name.
    pub card_prefix:  String,
    /// What a principle's first heading opens with, before its name.
    pub title_prefix: String,
}

impl Default for PrinciplesConfig {
    fn default() -> Self {
        Self {
            dir:          rel(".shared/principles"),
            card_prefix:  "principle-".to_string(),
            title_prefix: "Principle: ".to_string(),
        }
    }
}

/// `[surfaces.effort]`: where a change of effort keeps the host's prompt cache,
/// which is the only place a skill's effort key and a mode skill are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EffortGate {
    /// Globs over the model id, any of which keeps the cache on a change.
    pub models:       Vec<String>,
    /// Variables whose being set turns the gate off whatever the model.
    pub off_env:      Vec<String>,
    /// The API hosts the cache survives an effort change on.
    pub cache_hosts:  Vec<String>,
    /// The variable naming the API's base URL; unset is the first host.
    pub base_url_env: String,
}

impl Default for EffortGate {
    fn default() -> Self {
        let own = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect();
        Self {
            models:       own(&[
                "claude-fable-5-[1-9]*",
                "claude-fable-[6-9]*",
                "claude-opus-5-5*",
                "claude-sonnet-5-5*",
                "claude-haiku-5-5*",
            ]),
            off_env:      own(&[
                "CLAUDE_CODE_USE_BEDROCK",
                "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS",
            ]),
            cache_hosts:  own(&["api.anthropic.com"]),
            base_url_env: "ANTHROPIC_BASE_URL".to_string(),
        }
    }
}

impl EffortGate {
    /// Whether the effort keys and the mode skills are kept for a session on
    /// `model`, with `env` answering for the process environment.
    ///
    /// No model is no: a render that was not told the model cannot know the
    /// cache survives, and an effort change that empties it costs more than the
    /// effort saves. A glob that does not parse matches nothing.
    pub fn keeps(&self, model: Option<&str>, env: impl Fn(&str) -> Option<String>) -> bool {
        let set = |v: &str| env(v).is_some_and(|x| !x.is_empty());
        if self.off_env.iter().any(|v| set(v)) {
            return false;
        }
        if let Some(url) = env(&self.base_url_env).filter(|u| !u.is_empty()) {
            let host = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
            let host = host.split(['/', ':']).next().unwrap_or("");
            if !self.cache_hosts.iter().any(|h| h == host) {
                return false;
            }
        }
        let Some(model) = model.filter(|m| !m.is_empty()) else {
            return false;
        };
        self.models
            .iter()
            .filter_map(|g| glob::Pattern::new(g).ok())
            .any(|p| p.matches(model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_model_the_globs_match_keeps_and_one_they_do_not_strips() {
        let g = EffortGate::default();
        assert!(g.keeps(Some("claude-opus-5-5"), none));
        assert!(g.keeps(Some("claude-fable-5-2"), none));
        assert!(!g.keeps(Some("claude-fable-5-0"), none), "only from 5.1 on");
        assert!(!g.keeps(Some("claude-opus-5"), none));
        assert!(
            !g.keeps(None, none),
            "a render not told the model keeps nothing"
        );
    }

    #[test]
    fn an_off_variable_set_strips_whatever_the_model() {
        let g = EffortGate::default();
        let env = |v: &str| (v == "CLAUDE_CODE_USE_BEDROCK").then(|| "1".to_string());
        assert!(!g.keeps(Some("claude-opus-5-5"), env));
        let empty = |v: &str| (v == "CLAUDE_CODE_USE_BEDROCK").then(String::new);
        assert!(
            g.keeps(Some("claude-opus-5-5"), empty),
            "set to nothing is unset"
        );
    }

    #[test]
    fn a_base_url_on_another_host_strips_and_one_on_a_cache_host_keeps() {
        let g = EffortGate::default();
        let gateway = |v: &str| {
            (v == "ANTHROPIC_BASE_URL")
                .then(|| "https://gateway.example.invalid:8443/v1".to_string())
        };
        assert!(!g.keeps(Some("claude-opus-5-5"), gateway));
        let direct =
            |v: &str| (v == "ANTHROPIC_BASE_URL").then(|| "https://api.anthropic.com".to_string());
        assert!(g.keeps(Some("claude-opus-5-5"), direct));
    }

    #[test]
    fn the_tables_refuse_a_key_they_do_not_know_and_default_what_is_absent() {
        let s: SurfacesConfig =
            toml::from_str("[modes]\nskill_prefix = \"m-\"\n[principles]\n[effort]\nmodels = []\n")
                .unwrap();
        assert_eq!(s.modes.skill_prefix, "m-");
        assert_eq!(s.modes.body_max_chars, 500);
        assert_eq!(s.principles, PrinciplesConfig::default());
        assert!(s.effort.models.is_empty());
        assert!(toml::from_str::<SurfacesConfig>("[modes]\nprefix = \"m-\"\n").is_err());
        assert!(toml::from_str::<SurfacesConfig>("[colours]\n").is_err());
    }
}
