use super::*;
use crate::release::sh;

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    assert!(
        sh::run(d.path(), "git", &["init", "--quiet", "-b", "main"])
            .unwrap()
            .ok()
    );
    d
}

#[test]
fn an_older_entrypoint_is_found_in_gits_own_directory_and_in_git_shooks_kept() {
    let d = repo();
    let common = common_dir(d.path()).unwrap();
    std::fs::create_dir_all(common.join("hooks")).unwrap();
    std::fs::create_dir_all(common.join("shook/kept")).unwrap();
    std::fs::write(common.join("hooks/pre-commit"), entrypoint("pre-commit")).unwrap();
    std::fs::write(common.join("shook/kept/pre-push"), entrypoint("pre-push")).unwrap();
    // one line of somebody's own makes the file theirs
    std::fs::write(
        common.join("hooks/pre-push"),
        format!("{}echo mine\n", entrypoint("pre-push")),
    )
    .unwrap();
    // and the text of another event is not this event's entrypoint
    std::fs::write(common.join("hooks/commit-msg"), entrypoint("pre-commit")).unwrap();

    assert_eq!(left_behind(d.path()).unwrap(), vec![
        common.join("hooks/pre-commit"),
        common.join("shook/kept/pre-push"),
    ]);
}

#[test]
fn clearing_takes_the_records_of_git_shook_with_each_file_and_leaves_the_rest() {
    let d = repo();
    let common = common_dir(d.path()).unwrap();
    for dir in ["hooks", "shook/kept", "shook/declined", "shook/entries/pre-push"] {
        std::fs::create_dir_all(common.join(dir)).unwrap();
    }
    std::fs::write(common.join("hooks/pre-commit"), entrypoint("pre-commit")).unwrap();
    std::fs::write(
        common.join("shook/declined/pre-commit"),
        entrypoint("pre-commit"),
    )
    .unwrap();
    std::fs::write(common.join("shook/kept/pre-push"), entrypoint("pre-push")).unwrap();
    std::fs::write(common.join("shook/entries/pre-push/00-existing"), "\nsh\n").unwrap();
    std::fs::write(common.join("shook/entries/pre-push/mockspace"), "\ntrue\n").unwrap();
    std::fs::write(
        common.join("hooks/post-merge"),
        "#!/bin/sh\necho somebody\n",
    )
    .unwrap();

    let removed = clear(d.path()).unwrap();

    assert_eq!(removed.len(), 4, "{removed:?}");
    for gone in [
        "hooks/pre-commit",
        "shook/declined/pre-commit",
        "shook/kept/pre-push",
        "shook/entries/pre-push/00-existing",
    ] {
        assert!(!common.join(gone).exists(), "{gone} is gone");
    }
    assert!(common.join("shook/entries/pre-push/mockspace").is_file());
    assert!(common.join("hooks/post-merge").is_file());
    assert!(left_behind(d.path()).unwrap().is_empty());
}
