//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! Whether one name is a misspelling of another: one edit apart.

/// Whether `a` is one edit from `b`: one character inserted, removed or
/// replaced, or two neighbours swapped. A name is not one edit from itself.
pub(super) fn one_edit(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len() == b.len() {
        let differ: Vec<usize> = (0 .. a.len()).filter(|&i| a[i] != b[i]).collect();
        return match differ.as_slice() {
            [_] => true,
            [i, j] => *j == i + 1 && a[*i] == b[*j] && a[*j] == b[*i],
            _ => false,
        };
    }
    let (long, short) = if a.len() > b.len() { (&a, &b) } else { (&b, &a) };
    if long.len() != short.len() + 1 {
        return false;
    }
    // the first place they differ is the one character the longer carries
    let at = (0 .. short.len())
        .find(|&i| long[i] != short[i])
        .unwrap_or(short.len());
    long[at + 1 ..] == short[at ..]
}

#[cfg(test)]
mod tests {
    use super::one_edit;

    #[test]
    fn one_insertion_removal_replacement_or_swap_is_one_edit() {
        for (a, b) in [
            ("defualts", "defaults"),
            ("homma_brnach", "homma_branch"),
            ("nudge", "nudges"),
            ("nudgess", "nudges"),
            ("nudgas", "nudges"),
            ("xnudges", "nudges"),
        ] {
            assert!(one_edit(a, b), "{a} and {b}");
            assert!(one_edit(b, a), "{b} and {a}");
        }
    }

    #[test]
    fn two_edits_or_none_is_not_one() {
        for (a, b) in [
            ("defaults", "defaults"),
            ("defualtss", "defaults"),
            ("later", "defaults"),
            ("abc", "cba"),
            ("ab", "ba x"),
        ] {
            assert!(!one_edit(a, b), "{a} and {b}");
        }
    }
}
