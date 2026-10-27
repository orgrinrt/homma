//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::text::{Grade, edit_distance, grade, is_filler, pairs, stem, words};

#[test]
fn a_word_and_its_inflections_are_one_stem() {
    for group in [
        &["land", "lands", "landed", "landing"][..],
        &["free", "freed", "frees"],
        &["run", "runs", "running"],
        &["message", "messages"],
        &["branch", "branches"],
        &["entry", "entries"],
    ] {
        let first = stem(group[0]);
        for w in group {
            assert_eq!(stem(w), first, "{w} is not {}", group[0]);
        }
    }
}

#[test]
fn words_that_are_not_the_same_word_keep_their_own_stem() {
    assert_ne!(stem("lost"), stem("last"));
    assert_ne!(stem("bring"), stem("br"));
    assert_eq!(stem("class"), "class", "a double s is not a plural");
    assert_eq!(stem("status"), "status", "`us` is not a plural");
}

#[test]
fn filler_is_dropped_and_what_remains_is_the_subject() {
    let got: Vec<String> = pairs("homma what to use for landing a branch")
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    assert_eq!(got, ["homma", "land", "branch"]);
    assert!(is_filler("the"));
    assert!(!is_filler("disk"));
}

#[test]
fn a_stem_is_taken_once_however_often_it_is_said() {
    assert_eq!(pairs("land landed landing").len(), 1);
}

#[test]
fn a_query_with_nothing_but_filler_has_no_terms() {
    assert!(pairs("what is the way to do it").is_empty());
}

#[test]
fn words_split_at_everything_that_is_not_a_letter_or_a_digit() {
    assert_eq!(words("red-check, v2 (dev)!"), ["red", "check", "v2", "dev"]);
}

#[test]
fn no_word_is_in_two_synonym_groups_once_stemmed() {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, g) in super::text::SYNONYMS.iter().enumerate() {
        for w in *g {
            if let Some(j) = seen.insert(stem(w), i) {
                assert_eq!(
                    j, i,
                    "`{w}` stems to a word already in group {j}, and it is in group {i}"
                );
            }
        }
    }
}

#[test]
fn ending_is_not_finishing() {
    assert_eq!(grade(&stem("stop"), &stem("end")), Some(Grade::Synonym));
    assert_eq!(grade(&stem("end"), &stem("finished")), None);
}

#[test]
fn the_same_stem_is_the_same_word() {
    assert_eq!(grade(&stem("landed"), &stem("landing")), Some(Grade::Same));
}

#[test]
fn a_synonym_of_the_vocabulary_matches() {
    assert_eq!(grade(&stem("free"), &stem("trim")), Some(Grade::Synonym));
    assert_eq!(
        grade(&stem("done"), &stem("finished")),
        Some(Grade::Synonym)
    );
    assert_eq!(grade(&stem("lost"), &stem("stray")), Some(Grade::Synonym));
}

#[test]
fn a_word_outside_every_group_is_not_a_synonym() {
    assert_eq!(grade(&stem("free"), &stem("branch")), None);
}

#[test]
fn a_prefix_of_four_letters_matches_and_a_shorter_one_does_not() {
    assert_eq!(grade("coord", "coordinator"), Some(Grade::Prefix));
    assert_eq!(grade("coordinator", "coord"), Some(Grade::Prefix));
    assert_eq!(grade("pro", "promote"), None);
}

#[test]
fn a_misspelling_matches_by_the_length_of_the_word() {
    assert_eq!(grade("mesag", "messag"), Some(Grade::Fuzzy));
    assert_eq!(grade("cordinator", "coordinator"), Some(Grade::Fuzzy));
    assert_eq!(
        grade("cordinatr", "coordinator"),
        Some(Grade::Fuzzy),
        "two edits are allowed from eight letters"
    );
}

#[test]
fn a_short_word_is_only_forgiven_a_swap() {
    assert_eq!(grade("dsik", "disk"), Some(Grade::Fuzzy));
    assert_eq!(
        grade("bake", "make"),
        None,
        "one letter off is another word at this length"
    );
    assert_eq!(grade("cat", "cut"), None);
}

#[test]
fn two_edits_are_too_many_below_eight_letters() {
    assert_eq!(edit_distance("branch", "brunck"), 2);
    assert_eq!(grade("branch", "brunck"), None);
    assert_eq!(edit_distance("branch", "brunch"), 1);
    assert_eq!(grade("branch", "brunch"), Some(Grade::Fuzzy));
}

#[test]
fn edit_distance_counts_a_swap_as_one() {
    assert_eq!(edit_distance("disk", "dsik"), 1);
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("", "abc"), 3);
}

#[test]
fn a_root_that_ends_in_a_suffix_is_cut_the_same_bare_and_plural() {
    for (bare, plural) in [("speed", "speeds"), ("embed", "embeds"), ("string", "strings")] {
        assert_eq!(stem(bare), stem(plural), "{bare} is not {plural}");
    }
}

#[test]
fn the_filler_is_dropped_as_it_is_and_once_stemmed() {
    for w in ["script", "scripts", "command", "commands", "use", "uses"] {
        assert!(is_filler(w), "{w} carries a subject");
    }
    assert!(
        pairs("which scripts free disk")
            .iter()
            .all(|(w, _)| w != "scripts")
    );
}

#[test]
fn a_word_whose_stem_is_filler_keeps_its_subject() {
    for w in ["note", "notes", "noted", "noting"] {
        assert!(!is_filler(w), "{w} was dropped as the filler `not`");
    }
    let kept: Vec<String> = pairs("note taking").into_iter().map(|(w, _)| w).collect();
    assert_eq!(kept, ["note", "taking"]);
}
