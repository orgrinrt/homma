//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const ORG: &str = r#"
content_repo = "git@example.invalid:someone/content.git"

[org.op]
role = "king"
handle = "op"
git_name = "Op"
git_email = "op@example.invalid"

[org.onni]
role = "hand"
handle = "onni"
aliases = ["vouti"]
git_name = "Onni Armas"
git_email = "onni.armas@example.invalid"

[org.kide]
role = "journeyman"
handle = "kide"
git_name = "Kide \"K\" Kivi"
git_email = "kide@example.invalid"

[org.nameless]
role = "hand"
handle = "nameless"
git_email = "nameless@example.invalid"

[org.nowhere]
role = "hand"
handle = "nowhere"
git_name = "Nowhere"
git_email = "nowhere.example.invalid"
"#;

fn ws() -> Workspace {
    Workspace::parse(ORG).unwrap()
}

#[test]
fn the_author_is_the_agent_and_the_committer_its_coordinator() {
    let id = git_identity(&ws(), "kide", Some("onni"), None).unwrap();
    assert_eq!(id.author_name(), "Kide \"K\" Kivi");
    assert_eq!(id.author_email(), "kide@example.invalid");
    assert_eq!(id.committer_name(), "Onni Armas");
    assert_eq!(id.committer_email(), "onni.armas@example.invalid");
}

#[test]
fn with_no_committer_the_author_commits_too() {
    let id = git_identity(&ws(), "onni", None, None).unwrap();
    assert_eq!(id.committer_name(), id.author_name());
    assert_eq!(id.committer_email(), id.author_email());
}

#[test]
fn an_alias_reaches_the_same_agent_on_either_side() {
    let by_alias = git_identity(&ws(), "vouti", Some("vouti"), None).unwrap();
    assert_eq!(by_alias, git_identity(&ws(), "onni", None, None).unwrap());
}

#[test]
fn a_tag_goes_on_the_authors_local_part_and_never_the_committers() {
    let id = git_identity(&ws(), "onni", Some("onni"), Some("review-pass-2")).unwrap();
    assert_eq!(
        id.author_email(),
        "onni.armas+review-pass-2@example.invalid"
    );
    assert_eq!(id.committer_email(), "onni.armas@example.invalid");
    // and the tagged address normalises back to its launcher
    assert_eq!(
        crate::catalogue::normalise(id.author_email()),
        "onni.armas@example.invalid"
    );
}

#[test]
fn a_tag_outside_lowercase_letters_digits_and_hyphens_or_its_length_is_refused() {
    let long = "a".repeat(TAG_MAX + 1);
    for bad in ["", "Review", "a+b", "a@b", "a b", "ä", long.as_str()] {
        assert_eq!(
            git_identity(&ws(), "onni", None, Some(bad)),
            Err(GitIdentityRefusal::BadTag(bad.to_string())),
            "{bad:?}"
        );
    }
    let longest = "a".repeat(TAG_MAX);
    assert!(git_identity(&ws(), "onni", None, Some(&longest)).is_ok());
}

#[test]
fn the_king_is_refused_on_either_side() {
    assert_eq!(
        git_identity(&ws(), "op", None, None),
        Err(GitIdentityRefusal::King("op".into()))
    );
    assert_eq!(
        git_identity(&ws(), "onni", Some("op"), None),
        Err(GitIdentityRefusal::King("op".into()))
    );
}

#[test]
fn an_unknown_name_is_refused_naming_it() {
    let refused = git_identity(&ws(), "kide", Some("nobody"), None).unwrap_err();
    assert_eq!(refused, GitIdentityRefusal::Unknown("nobody".into()));
    assert!(refused.to_string().contains("`nobody`"));
}

#[test]
fn an_entry_without_a_name_or_a_real_address_is_refused() {
    assert_eq!(
        git_identity(&ws(), "nameless", None, None),
        Err(GitIdentityRefusal::Unaddressed("nameless".into()))
    );
    assert_eq!(
        git_identity(&ws(), "nowhere", None, None),
        Err(GitIdentityRefusal::Unaddressed("nowhere".into()))
    );
}

#[test]
fn the_rendering_is_a_config_git_reads_back_as_written() {
    let id = git_identity(&ws(), "kide", Some("onni"), Some("x")).unwrap();
    let text = git_config(&id);
    assert_eq!(
        text,
        "[author]\n\tname = \"Kide \\\"K\\\" Kivi\"\n\temail = kide+x@example.invalid\n\
         [committer]\n\tname = \"Onni Armas\"\n\temail = onni.armas@example.invalid\n"
    );
    let dir = std::env::temp_dir().join(format!("homma-git-identity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("identity.gitconfig");
    std::fs::write(&file, &text).unwrap();
    let read = |key: &str| {
        let out = std::process::Command::new("git")
            .args(["config", "--file"])
            .arg(&file)
            .args(["--get", key])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{key}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end()
            .to_string()
    };
    assert_eq!(read("author.name"), "Kide \"K\" Kivi");
    assert_eq!(read("author.email"), "kide+x@example.invalid");
    assert_eq!(read("committer.name"), "Onni Armas");
    assert_eq!(read("committer.email"), "onni.armas@example.invalid");
    std::fs::remove_dir_all(&dir).unwrap();
}
