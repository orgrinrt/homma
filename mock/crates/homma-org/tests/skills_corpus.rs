//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Loading a skill corpus and generating the tree a session finds.
//!
//! A skill is a directory rather than a file, so most of what is worth testing
//! is about the walk: which files are rendered, which are copied untouched,
//! what happens to a mode bit and to a symlink, and what the pass refuses.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_org::skills::{Skills, SkillsError};

/// A corpus of two skills, one of them with a reference document and a script.
///
/// The directory is unique per call, not per process: the tests in one binary
/// are threads sharing a process id, so keying on that alone gives every test
/// the same directory and each one's setup deletes the others'.
fn fixture() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-skills-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("skills");

    write_skill(
        &src,
        "pr-review",
        "Use when reviewing or merging any pull request here.",
    );
    fs::create_dir_all(src.join("pr-review/reference")).unwrap();
    fs::write(
        src.join("pr-review/reference/the-flow.md.tmpl"),
        "# The flow\n\nRun {{ name }} first.\n\nSee `branch-flow.md`.\n",
    )
    .unwrap();
    fs::create_dir_all(src.join("pr-review/scripts")).unwrap();
    fs::write(
        src.join("pr-review/scripts/scan.sh"),
        "#!/usr/bin/env bash\nprintf '%s\\n' \"${x:-none}\"\n",
    )
    .unwrap();

    write_skill(&src, "write-tests", "Use when writing or auditing tests.");
    dir
}

/// One skill directory with just its manifest.
fn write_skill(root: &Path, name: &str, description: &str) {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md.tmpl"),
        format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n\
             # {name}\n\nThe body, which a session fetches rather than carries.\n"
        ),
    )
    .unwrap();
}

fn load(dir: &Path) -> Skills {
    Skills::load(&dir.join("skills")).expect("the fixture corpus should load")
}

/// Regular files under `dir`, walking without following any symlink.
///
/// `is_file` follows one, so a count built on it cannot tell a copied tree from
/// a link into one, which is the whole thing being measured here.
fn physical_files(dir: &Path) -> usize {
    let mut n = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            n += physical_files(&entry.path());
        } else {
            n += 1;
        }
    }
    n
}

#[test]
fn loads_every_skill_sorted_by_name() {
    let dir = fixture();
    let corpus = load(&dir);
    let names: Vec<&str> = corpus.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["pr-review", "write-tests"]);
}

#[test]
fn carries_the_description_since_that_is_what_the_listing_charges() {
    let dir = fixture();
    let corpus = load(&dir);
    assert_eq!(
        corpus.skills[1].meta.description,
        "Use when writing or auditing tests."
    );
}

#[test]
fn a_directory_with_no_manifest_is_refused_not_skipped() {
    let dir = fixture();
    fs::create_dir_all(dir.join("skills/half-written")).unwrap();
    let err = Skills::load(&dir.join("skills")).expect_err("a manifestless directory is refused");
    assert!(matches!(err, SkillsError::NoManifest { .. }), "{err}");
}

#[test]
fn a_declared_name_disagreeing_with_the_directory_is_refused() {
    let dir = fixture();
    let manifest = dir.join("skills/write-tests/SKILL.md.tmpl");
    let body = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        body.replace("name: write-tests", "name: write-test"),
    )
    .unwrap();
    let err = Skills::load(&dir.join("skills")).expect_err("a name mismatch is refused");
    match err {
        SkillsError::NameMismatch {
            declared,
            dir,
            ..
        } => {
            assert_eq!(declared, "write-test");
            assert_eq!(dir, "write-tests");
        },
        other => panic!("wrong refusal: {other}"),
    }
}

#[test]
fn a_missing_description_is_refused() {
    let dir = fixture();
    let manifest = dir.join("skills/write-tests/SKILL.md.tmpl");
    fs::write(&manifest, "---\nname: write-tests\n---\n\n# write-tests\n").unwrap();
    let err = Skills::load(&dir.join("skills")).expect_err("a description is required");
    assert!(matches!(err, SkillsError::Meta { .. }), "{err}");
}

#[test]
fn an_unknown_field_is_refused_rather_than_dropped() {
    let dir = fixture();
    let manifest = dir.join("skills/write-tests/SKILL.md.tmpl");
    fs::write(
        &manifest,
        "---\nname: write-tests\ndescription: x\ndescriptoin: y\n---\n\n# write-tests\n",
    )
    .unwrap();
    let err = Skills::load(&dir.join("skills")).expect_err("a typo'd key is refused");
    assert!(matches!(err, SkillsError::Meta { .. }), "{err}");
}

#[test]
fn a_host_field_is_taken_and_kept() {
    let dir = fixture();
    let manifest = dir.join("skills/write-tests/SKILL.md.tmpl");
    fs::write(
        &manifest,
        "---\nname: write-tests\ndescription: x\nallowed-tools: Read, Grep\n---\n\n# t\n",
    )
    .unwrap();
    let corpus = load(&dir);
    let extra = &corpus.skills[1].meta.extra;
    assert!(
        extra
            .iter()
            .any(|(k, v)| k == "allowed-tools" && v == "Read, Grep"),
        "the host's own fields round-trip: {extra:?}"
    );
}

#[test]
fn a_template_loses_its_suffix_and_is_rendered() {
    let dir = fixture();
    let corpus = load(&dir);
    let out = dir.join("out");
    corpus.render(&out).unwrap();

    let flow = out.join("pr-review/reference/the-flow.md");
    assert!(flow.is_file(), "the suffix comes off");
    let body = fs::read_to_string(&flow).unwrap();
    assert!(body.contains("Run pr-review first."), "rendered: {body}");
    assert!(
        !out.join("pr-review/reference/the-flow.md.tmpl").exists(),
        "the template itself is not written into the generated tree"
    );
}

#[test]
fn a_script_is_copied_untouched_braces_and_all() {
    let dir = fixture();
    let corpus = load(&dir);
    let out = dir.join("out");
    corpus.render(&out).unwrap();

    let src = fs::read_to_string(dir.join("skills/pr-review/scripts/scan.sh")).unwrap();
    let dst = fs::read_to_string(out.join("pr-review/scripts/scan.sh")).unwrap();
    assert_eq!(
        src, dst,
        "a shell script is not prose: `${{x:-none}}` survives byte for byte"
    );
}

#[cfg(unix)]
#[test]
fn the_executable_bit_survives_the_copy() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fixture();
    let script = dir.join("skills/pr-review/scripts/scan.sh");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let corpus = load(&dir);
    let out = dir.join("out");
    corpus.render(&out).unwrap();

    let mode = fs::metadata(out.join("pr-review/scripts/scan.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "a skill whose tool arrives unexecutable fails when it is reached for"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_is_recreated_and_never_walked_through() {
    let dir = fixture();
    // A link standing in for the vendored-dependency links a real skill holds.
    // Following one copies the whole target tree into the generated output:
    // measured at 944 files from a single link, against the 9 a skill owned.
    let deep = dir.join("elsewhere/a/b");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("buried.txt"), "not the skill's\n").unwrap();
    fs::create_dir_all(dir.join("skills/pr-review/lib")).unwrap();
    std::os::unix::fs::symlink(
        dir.join("elsewhere"),
        dir.join("skills/pr-review/lib/vendored"),
    )
    .unwrap();

    let corpus = load(&dir);
    let out = dir.join("out");
    corpus.render(&out).unwrap();

    let link = out.join("pr-review/lib/vendored");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link is recreated as a link"
    );
    assert_eq!(
        physical_files(&out),
        4,
        "nothing behind the link is copied: the tree holds the two manifests, the reference \
         document and the script, and the link itself is not a file"
    );
    // Read through the link to prove it still resolves, which is the half a
    // skip would have lost.
    assert_eq!(
        fs::read_to_string(link.join("a/b/buried.txt")).unwrap(),
        "not the skill's\n"
    );
}

#[test]
fn a_file_dropped_from_the_authored_side_does_not_survive_regeneration() {
    let dir = fixture();
    let out = dir.join("out");
    load(&dir).render(&out).unwrap();
    assert!(out.join("pr-review/reference/the-flow.md").is_file());

    fs::remove_file(dir.join("skills/pr-review/reference/the-flow.md.tmpl")).unwrap();
    load(&dir).render(&out).unwrap();
    assert!(
        !out.join("pr-review/reference/the-flow.md").exists(),
        "the generated directory is rewritten whole, so a dropped file goes"
    );
}

#[test]
fn a_generated_directory_nothing_authors_is_reported_and_left_alone() {
    let dir = fixture();
    let corpus = load(&dir);
    let out = dir.join("out");
    corpus.render(&out).unwrap();
    fs::create_dir_all(out.join("came-from-somewhere-else")).unwrap();

    let stray = corpus.unclaimed(&out).unwrap();
    assert_eq!(stray, vec!["came-from-somewhere-else".to_string()]);
    assert!(
        out.join("came-from-somewhere-else").is_dir(),
        "reported, not deleted: this pass cannot tell what put it there"
    );
}

#[test]
fn a_template_naming_something_the_meta_does_not_have_is_refused() {
    let dir = fixture();
    fs::write(
        dir.join("skills/write-tests/SKILL.md.tmpl"),
        "---\nname: write-tests\ndescription: x\n---\n\n# t\n\n{{ nonexistent }}\n",
    )
    .unwrap();
    let corpus = load(&dir);
    let err = corpus
        .render(&dir.join("out"))
        .expect_err("strict undefined handling is the point");
    assert!(matches!(err, SkillsError::Render { .. }), "{err}");
}

#[test]
fn bearing_on_takes_a_backticked_filename_and_a_link_never_a_bare_word() {
    let dir = fixture();
    // The reference document cites `branch-flow.md`; nothing cites `test-gate`
    // by either form, though the words appear in a skill's own name.
    fs::write(
        dir.join("skills/write-tests/SKILL.md.tmpl"),
        "---\nname: write-tests\ndescription: x\n---\n\n# t\n\nSee [[test-gate]].\n\
         \n\nAnd the suite gate, written out as words.\n",
    )
    .unwrap();
    let corpus = load(&dir);

    assert_eq!(corpus.bearing_on("branch-flow").unwrap(), vec![
        "pr-review".to_string()
    ]);
    assert_eq!(corpus.bearing_on("test-gate").unwrap(), vec![
        "write-tests".to_string()
    ]);
    assert!(
        corpus.bearing_on("suite").unwrap().is_empty(),
        "a bare word is not a citation, or every rule matches every skill"
    );
}

#[test]
fn a_missing_corpus_is_refused_rather_than_read_as_empty() {
    let dir = fixture();
    let err = Skills::load(&dir.join("nothing-here")).expect_err("a missing directory is refused");
    assert!(matches!(err, SkillsError::Unreadable { .. }), "{err}");
}
