use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir` that is not a test file.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the source directory is readable") {
        let path = entry.expect("a directory entry reads").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

/// The files of the crate's source that build an `ureq` agent from nothing, which would trust the
/// bundled roots and not the system's. Only `tls.rs` may, since it is where the roots are set.
fn builders_outside_tls(src: &Path) -> Vec<String> {
    let mut files = Vec::new();
    sources(src, &mut files);
    files
        .into_iter()
        .filter(|f| f.file_name().is_some_and(|n| n != "tls.rs"))
        .filter(|f| {
            fs::read_to_string(f)
                .expect("a source file reads")
                .contains("AgentBuilder::new")
        })
        .map(|f| {
            f.strip_prefix(src)
                .expect("under the source directory")
                .display()
                .to_string()
        })
        .collect()
}

#[test]
fn no_agent_is_built_outside_the_module_that_sets_the_roots() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(builders_outside_tls(&src), Vec::<String>::new());
}

#[test]
fn the_scan_names_a_file_that_builds_an_agent_from_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory is made");
    let dir = scratch.path();
    fs::create_dir_all(dir.join("forge")).expect("the scratch directory is made");
    fs::write(dir.join("tls.rs"), "ureq::AgentBuilder::new()").expect("a file writes");
    fs::write(
        dir.join("forge/bare.rs"),
        "ureq::AgentBuilder::new().build()",
    )
    .expect("a file writes");
    fs::write(
        dir.join("forge/clean.rs"),
        "crate::tls::agent_builder().build()",
    )
    .expect("a file writes");
    fs::write(dir.join("forge/bare_tests.rs"), "ureq::AgentBuilder::new()").expect("a file writes");
    assert_eq!(builders_outside_tls(dir), vec!["forge/bare.rs".to_owned()]);
}
