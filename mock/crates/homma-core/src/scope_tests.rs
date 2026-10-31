use super::*;

fn write_local(dir: &Path, repos: &str) {
    std::fs::write(
        dir.join(crate::local::LOCAL_FILE),
        format!("[instance]\nwork = \"w\"\nrepos = {repos}\n"),
    )
    .unwrap();
}

#[test]
fn no_scope_admits_every_repository() {
    assert!(Scope::everything().admits("anything"));
    assert!(Scope::default().admits("anything"));
    assert!(Scope::everything().names().is_none());
}

#[test]
fn a_named_scope_admits_only_its_names() {
    let s = Scope::named(["kaski", "homma"]);
    assert!(s.admits("kaski"));
    assert!(s.admits("homma"));
    assert!(!s.admits("arvo"));
    assert!(!s.admits("kask"), "a prefix of a name is not the name");
}

#[test]
fn an_empty_list_narrows_nothing() {
    let none: [&str; 0] = [];
    assert_eq!(Scope::named(none), Scope::everything());
}

#[test]
fn a_named_repository_that_is_not_cloned_is_unmet_and_nothing_else_is() {
    let s = Scope::named(["kaski", "homma", "letkajenkka"]);
    assert_eq!(s.unmet(["kaski", "arvo"]), vec!["homma", "letkajenkka"]);
    assert!(Scope::everything().unmet(["arvo"]).is_empty());
}

#[test]
fn a_directory_with_no_local_file_gives_everything_and_no_error() {
    let d = tempfile::tempdir().unwrap();
    let (s, err) = Scope::load(d.path());
    assert_eq!(s, Scope::everything());
    assert!(err.is_none());
}

#[test]
fn the_local_files_repos_are_the_scope() {
    let d = tempfile::tempdir().unwrap();
    write_local(d.path(), "[\"kaski\", \"homma\"]");
    let (s, err) = Scope::load(d.path());
    assert!(err.is_none());
    assert_eq!(s, Scope::named(["kaski", "homma"]));
}

#[test]
fn a_local_file_that_does_not_parse_widens_the_scope_and_returns_why() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(crate::local::LOCAL_FILE), "[instance\n").unwrap();
    let (s, err) = Scope::load(d.path());
    assert_eq!(s, Scope::everything());
    assert!(
        err.is_some(),
        "silently ignoring a broken file is the failure"
    );
}

#[test]
fn the_repos_a_project_declares_are_the_scope_of_a_clone_that_names_it() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(crate::local::STORE_DIR)).unwrap();
    std::fs::write(
        d.path().join(crate::local::STORE_DIR).join("k.toml"),
        "[instance]\nwork = \"k\"\nrepos = [\"a\", \"b\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join(crate::local::LOCAL_FILE),
        "[instance]\nproject = \"k\"\n",
    )
    .unwrap();
    let (s, err) = Scope::load(d.path());
    assert!(err.is_none(), "{err:?}");
    assert!(s.admits("a") && s.admits("b") && !s.admits("c"));
    // The control: the file's own list overrides the project's, whole.
    std::fs::write(
        d.path().join(crate::local::LOCAL_FILE),
        "[instance]\nproject = \"k\"\nrepos = [\"c\"]\n",
    )
    .unwrap();
    let (s, _) = Scope::load(d.path());
    assert!(s.admits("c") && !s.admits("a"));
}

#[test]
fn a_named_project_the_store_lacks_widens_the_scope_and_returns_why() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join(crate::local::LOCAL_FILE),
        "[instance]\nproject = \"gone\"\n",
    )
    .unwrap();
    let (s, err) = Scope::load(d.path());
    assert_eq!(s, Scope::everything());
    assert!(err.is_some());
}
