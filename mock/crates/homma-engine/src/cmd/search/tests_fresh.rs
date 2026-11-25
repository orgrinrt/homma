//! A listing of what changed in the last days, over planted repositories with dated commits.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use clap::Parser;
use homma_api::SummaryConfig;
use muisti::repo::{DocClass, RepoIndex, Root, Words};
use muisti::{Header, Store};

use super::current::Currency;
use super::discover::Place;
use super::fresh::{self, NO_DAYS, age};
use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The moment `at` names, an RFC 3339 instant.
fn at(when: &str) -> SystemTime {
    let t: jiff::Timestamp = when.parse().unwrap();
    SystemTime::UNIX_EPOCH + Duration::from_secs(u64::try_from(t.as_second()).unwrap())
}

/// The day the listings below are made on.
const NOW: &str = "2026-11-22T12:00:00Z";

/// Commit everything under `dir` with `message`, authored and committed at `when`.
fn commit(dir: &Path, message: &str, when: &str) {
    for args in [&["add", "."][..], &["commit", "-q", "-m", message][..]] {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_AUTHOR_DATE", when)
            .env("GIT_COMMITTER_DATE", when)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }
}

/// A repository at `dir` with `files` committed one commit each, at the instant beside each.
fn repository(dir: &Path, files: &[(&str, &str, &str)]) {
    std::fs::create_dir_all(dir).unwrap();
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success();
    assert!(ok);
    for (path, text, when) in files {
        write(&dir.join(path), text);
        commit(dir, &format!("add {path}"), when);
    }
}

/// The repository index of `dir`, built with its history read, beside it.
fn built(dir: &Path) -> PathBuf {
    let file = dir.with_extension("index").join("search.sqlite");
    let mut index = RepoIndex::open(&file).unwrap();
    index
        .refresh(&[Root::walked(dir, "", DocClass::Research)], Some(dir))
        .unwrap();
    file
}

/// A repository with a file from three weeks back and one from yesterday, indexed.
fn planted(t: &Path, name: &str) -> Place {
    let dir = t.join(name);
    repository(&dir, &[
        (
            "old.md",
            "# Old note\n\nwritten long ago\n",
            "2026-11-01T10:00:00Z",
        ),
        (
            "new.md",
            "# New note\n\nwritten yesterday\n",
            "2026-11-21T10:00:00Z",
        ),
    ]);
    Place::at(name.to_string(), built(&dir))
}

fn paths(out: &SearchOut) -> Vec<&str> {
    out.hits.iter().map(|h| h.path.as_str()).collect()
}

#[test]
fn a_listing_holds_what_changed_in_the_window_and_the_commits_of_it() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    let out = fresh::list(&places, 3, None, at(NOW));
    let got = paths(&out);
    assert!(got.contains(&"new.md"), "{got:?}");
    let commits: Vec<&HitOut> = out
        .hits
        .iter()
        .filter(|h| h.path.starts_with(".git/"))
        .collect();
    assert_eq!(commits.len(), 1, "{got:?}");
    assert_eq!(commits[0].excerpt, "add new.md");
    assert_eq!(commits[0].date.as_deref(), Some("2026-11-21"));
    assert_eq!(out.window, Some(3));
    // THE CONTROL: what changed three weeks back is not in a window of three days.
    assert!(!got.contains(&"old.md"), "{got:?}");
    // and it is in one of thirty, with its commit.
    let wide = fresh::list(&places, 30, None, at(NOW));
    assert!(paths(&wide).contains(&"old.md"), "{:?}", paths(&wide));
    assert_eq!(
        wide.hits
            .iter()
            .filter(|h| h.path.starts_with(".git/"))
            .count(),
        2
    );
}

#[test]
fn a_window_counts_today_as_its_first_day() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    // Yesterday is the second day of a window of two.
    assert!(paths(&fresh::list(&places, 2, None, at(NOW))).contains(&"new.md"));
    // THE CONTROL: a window one day short of it holds only today, and nothing changed today.
    let one = fresh::list(&places, 1, None, at(NOW));
    assert!(one.hits.is_empty(), "{:?}", paths(&one));
    assert!(
        render(&one).contains("nothing in any index changed in the last 1 day\n"),
        "{}",
        render(&one)
    );
}

#[test]
fn the_newest_comes_first_across_places_and_a_day_keeps_the_place_order() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a");
    repository(&a, &[("a.md", "# A\n\nfrom a\n", "2026-11-20T10:00:00Z")]);
    let b = t.path().join("b");
    repository(&b, &[("b.md", "# B\n\nfrom b\n", "2026-11-22T09:00:00Z")]);
    let c = t.path().join("c");
    repository(&c, &[("c.md", "# C\n\nfrom c\n", "2026-11-20T11:00:00Z")]);
    let places = vec![
        Place::at("a".to_string(), built(&a)),
        Place::at("b".to_string(), built(&b)),
        Place::at("c".to_string(), built(&c)),
    ];
    let out = fresh::list(&places, 5, None, at(NOW));
    let files: Vec<(&str, &str)> = out
        .hits
        .iter()
        .filter(|h| !h.path.starts_with(".git/"))
        .map(|h| (h.index.as_str(), h.path.as_str()))
        .collect();
    // b changed today and comes first though it is the second place.
    assert_eq!(files, vec![("b", "b.md"), ("a", "a.md"), ("c", "c.md")]);
    // THE CONTROL: a and c changed the same day, and keep the order the places came in.
    let reversed = vec![places[2].clone(), places[1].clone(), places[0].clone()];
    let out = fresh::list(&reversed, 5, None, at(NOW));
    let files: Vec<&str> = out
        .hits
        .iter()
        .filter(|h| !h.path.starts_with(".git/"))
        .map(|h| h.index.as_str())
        .collect();
    assert_eq!(files, vec!["b", "c", "a"]);
}

#[test]
fn a_corpus_keeps_no_days_and_its_line_says_so() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("corpus.sqlite");
    let mut store = Store::open(&file, Header::new("words", std::num::NonZeroUsize::MIN)).unwrap();
    store
        .upsert(
            &muisti::Source::new("one.md", "# One\n\nsaturation\n".as_bytes()),
            &mut Words,
        )
        .unwrap();
    drop(store);
    let places = vec![Place::at("corpus".to_string(), file), planted(t.path(), "r")];
    let out = fresh::list(&places, 3, None, at(NOW));
    assert!(!out.indexes[0].answered);
    assert!(
        out.indexes[0].notes.iter().any(|n| n == NO_DAYS),
        "{:?}",
        out.indexes[0].notes
    );
    assert!(out.hits.iter().all(|h| h.index == "r"));
    // THE CONTROL: the repository beside it is listed and answered.
    assert!(out.indexes[1].answered);
    assert!(paths(&out).contains(&"new.md"));
}

#[test]
fn an_index_is_brought_current_before_it_is_listed() {
    let t = tempfile::tempdir().unwrap();
    let place = planted(t.path(), "r");
    let dir = t.path().join("r");
    write(&dir.join("later.md"), "# Later\n\nwritten today\n");
    commit(&dir, "add later.md", "2026-11-22T08:00:00Z");
    let out = fresh::list(std::slice::from_ref(&place), 3, None, at(NOW));
    assert!(
        matches!(
            out.indexes[0].currency,
            Some(Currency::Refreshed {
                commits: 1 ..,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    assert_eq!(
        out.hits[0].age.as_deref(),
        Some("today"),
        "{:?}",
        paths(&out)
    );
    assert!(paths(&out).contains(&"later.md"), "{:?}", paths(&out));
    // THE CONTROL: listed again with nothing committed, no commit is read and the same is listed.
    let again = fresh::list(std::slice::from_ref(&place), 3, None, at(NOW));
    assert!(matches!(
        again.indexes[0].currency,
        Some(Currency::Refreshed {
            commits: 0,
            ..
        })
    ));
    assert_eq!(paths(&again), paths(&out));
}

#[test]
fn the_limit_bounds_a_listing_and_none_lists_everything() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    let all = fresh::list(&places, 30, None, at(NOW));
    // THE CONTROL: with no limit every file and commit of the window is there.
    assert_eq!(all.hits.len(), 4, "{:?}", paths(&all));
    let one = fresh::list(&places, 30, Some(1), at(NOW));
    assert_eq!(paths(&one), paths(&all)[.. 1].to_vec());
}

#[test]
fn an_age_is_said_in_days_before_today() {
    assert_eq!(age(0), "today");
    assert_eq!(age(1), "1 day ago");
    assert_eq!(age(5), "5 days ago");
    let t = tempfile::tempdir().unwrap();
    let out = fresh::list(&[planted(t.path(), "r")], 3, None, at(NOW));
    let new = out.hits.iter().find(|h| h.path == "new.md").unwrap();
    assert_eq!(new.age.as_deref(), Some("1 day ago"));
    assert!(render(&out).contains("1 day ago"), "{}", render(&out));
    // THE CONTROL: a question's hits carry no age.
    let asked = ask(&[planted(t.path(), "q")], "yesterday", 10, at(NOW));
    assert!(!asked.hits.is_empty());
    assert!(asked.hits.iter().all(|h| h.age.is_none()));
}

/// A model that counts its calls and answers each with one placed line, keeping the brief.
struct Counted(Cell<usize>, std::cell::RefCell<String>);

impl crate::cmd::summary::OneShot for Counted {
    fn ask(&self, system: &str, _: &str) -> Result<String, String> {
        self.0.set(self.0.get() + 1);
        *self.1.borrow_mut() = system.to_string();
        Ok("- new.md:1 a new note".to_string())
    }
}

#[test]
fn a_listing_goes_through_the_pass_with_its_own_brief() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    let on = SummaryConfig {
        enabled: true,
        ..SummaryConfig::default()
    };
    let model = Counted(Cell::new(0), std::cell::RefCell::default());
    let out = fresh::answer(&places, 3, None, Some((&on, &model)), at(NOW));
    assert_eq!(model.0.get(), 1);
    assert!(out.summary.is_some());
    assert!(
        model
            .1
            .borrow()
            .contains("everything that changed in the last 3 days"),
        "{}",
        model.1.borrow()
    );
    // THE CONTROL: with no pass given the listing is whole and says nothing of a summary.
    let whole = fresh::answer::<Counted>(&places, 3, None, None, at(NOW));
    assert_eq!(model.0.get(), 1);
    assert!(whole.summary.is_none() && whole.summary_note.is_none());
    assert!(render(&whole).contains("new.md"), "{}", render(&whole));
}

#[test]
fn fresh_parses_bare_with_a_number_and_refuses_words() {
    let parse = |args: &[&str]| {
        crate::cli::Cli::try_parse_from(
            ["homma-engine", "search"]
                .iter()
                .chain(args)
                .copied()
                .collect::<Vec<_>>(),
        )
    };
    assert!(matches!(
        parse(&["--fresh"]).unwrap().command,
        crate::cli::Command::Search {
            fresh: Some(3),
            limit: None,
            ..
        }
    ));
    assert!(matches!(
        parse(&["--fresh", "7"]).unwrap().command,
        crate::cli::Command::Search {
            fresh: Some(7),
            ..
        }
    ));
    assert!(parse(&["--fresh", "0"]).is_err());
    // THE CONTROL: words alone are a question, and with `--fresh` they are refused.
    assert!(matches!(
        parse(&["word"]).unwrap().command,
        crate::cli::Command::Search {
            fresh: None,
            ..
        }
    ));
    assert!(parse(&["word", "--fresh"]).is_err());
    assert!(parse(&["--fresh", "word"]).is_err());
    assert!(parse(&["--fresh", "3", "word"]).is_err());
    assert!(parse(&[]).is_err());
}

#[test]
fn a_listing_takes_the_pass_wherever_it_is_on_and_a_question_only_when_asked() {
    let table = |enabled: bool| {
        move || {
            Ok(SummaryConfig {
                enabled,
                ..SummaryConfig::default()
            })
        }
    };
    let listing = Asked::Fresh(3);
    let question = Asked::Words(vec!["word".to_string()]);
    // A listing goes through the pass where it is on, without `--summarise`.
    assert!(pass_for(&listing, false, table(true)).unwrap().is_some());
    // THE CONTROL: where it is off the listing is whole and nothing is said of the pass.
    assert!(pass_for(&listing, false, table(false)).unwrap().is_none());
    // Asked for, the pass is given even where off, so it says why.
    assert!(
        pass_for(&listing, true, table(false))
            .unwrap()
            .is_some_and(|s| !s.enabled)
    );
    // A question takes it only when asked, and the table is not read otherwise.
    assert!(
        pass_for(&question, false, || -> anyhow::Result<SummaryConfig> {
            panic!("read")
        })
        .unwrap()
        .is_none()
    );
    assert!(pass_for(&question, true, table(true)).unwrap().is_some());
}

#[test]
fn a_listing_says_its_days_in_their_number_and_is_listed_not_asked() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    let one = fresh::list(&places, 1, None, at(NOW));
    assert!(
        one.brief().contains("in the last 1 day across"),
        "{}",
        one.brief()
    );
    let text = render(&one);
    assert!(text.contains("listed by the days they changed"), "{text}");
    // THE CONTROL: a question's line says it was asked, and a listing's JSON carries no question.
    let asked = ask(&places, "note", 10, at(NOW));
    assert!(
        render(&asked).contains("asked by words"),
        "{}",
        render(&asked)
    );
    let json = serde_json::to_value(&one).unwrap();
    assert!(
        json.get("question").is_none() && json.get("window").is_some(),
        "{json}"
    );
    let json = serde_json::to_value(&asked).unwrap();
    assert!(
        json.get("question").is_some() && json.get("window").is_none(),
        "{json}"
    );
}

fn render(out: &SearchOut) -> String {
    let mut text = Vec::new();
    out.render_human(&mut text).unwrap();
    String::from_utf8(text).unwrap()
}
