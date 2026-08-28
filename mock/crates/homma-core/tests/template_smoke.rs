//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke test that mockspace-template renders against a homma `Config`.

use homma_core::Config;
use homma_core::template::TemplateEnv;

#[test]
fn renders_workspace_name_into_template() {
    let config = Config::parse(
        r#"
[workspace]
name = "clause-dev"
"#,
    )
    .unwrap();

    let env = TemplateEnv::new();
    let out = env
        .render_str("Workspace: {{ workspace.name }}", &config)
        .unwrap();
    assert_eq!(out, "Workspace: clause-dev");
}

#[test]
fn renders_loop_over_repos() {
    // Detected rather than declared, so the fixture is a tree rather than a
    // manifest. Two clones and one plain directory, so the loop has something
    // to leave out as well as something to render.
    let root = tempfile::tempdir().expect("tempdir");
    for name in ["alpha", "beta"] {
        std::fs::create_dir_all(root.path().join(name).join(".git")).unwrap();
    }
    std::fs::create_dir_all(root.path().join("notes")).unwrap();

    let mut config = Config::parse(
        r#"
[workspace]
name = "demo"
"#,
    )
    .unwrap();
    config.detect_members(root.path(), &homma_core::repo::GixGit);

    let env = TemplateEnv::new();
    let out = env
        .render_str(
            "{% for name, _ in repos | items %}{{ name }};{% endfor %}",
            &config,
        )
        .unwrap();
    // BTreeMap iteration order is alphabetical.
    assert_eq!(out, "alpha;beta;");
}
