//! Smoke test that mockspace-template renders against a homma `Config`.

use homma_core::{template::TemplateEnv, Config};

#[test]
fn renders_workspace_name_into_template() {
    let config = Config::from_str(
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
    let config = Config::from_str(
        r#"
[workspace]
name = "demo"

[repos.alpha]
forge = "github"
owner = "x"
local_path = "alpha"

[repos.beta]
forge = "github"
owner = "x"
local_path = "beta"
"#,
    )
    .unwrap();

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
