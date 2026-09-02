//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The numbers a gate step reads off what a tool printed. Each parser takes
//! the whole log and answers with a count, or nothing where the tool never
//! printed the line, which is kept distinct from a count of zero.

/// Test counts summed over every `test result:` line cargo printed, so a
/// workspace of several crates and doctests adds up to one pair.
pub fn cargo_tests(log: &str) -> Option<(u64, u64)> {
    let mut total = 0u64;
    let mut passed = 0u64;
    let mut seen = false;
    for line in log.lines() {
        let Some(rest) = line.trim_start().strip_prefix("test result:") else {
            continue;
        };
        seen = true;
        // each part reads `<n> <word>`, after the `ok.` or `FAILED.` that
        // opens the first one
        for part in rest.split(';') {
            let words: Vec<&str> = part.split_whitespace().collect();
            let Some(i) = words.iter().position(|w| w.parse::<u64>().is_ok()) else {
                continue;
            };
            let n: u64 = words[i].parse().unwrap_or(0);
            match words.get(i + 1).copied().unwrap_or("") {
                w if w.starts_with("passed") => {
                    passed += n;
                    total += n;
                }
                w if w.starts_with("failed") || w.starts_with("ignored") => total += n,
                _ => {}
            }
        }
    }
    seen.then_some((total, passed))
}

/// Test counts off deno's summary line, `ok | 12 passed | 0 failed`, or the
/// same line opening with `FAILED`.
pub fn deno_tests(log: &str) -> Option<(u64, u64)> {
    for line in log.lines() {
        let line = line.trim();
        if !(line.starts_with("ok |") || line.starts_with("FAILED |")) {
            continue;
        }
        let mut passed = 0u64;
        let mut failed = 0u64;
        for part in line.split('|').skip(1) {
            let mut words = part.split_whitespace();
            let (Some(n), Some(word)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(n) = n.parse::<u64>() else { continue };
            match word {
                "passed" => passed = n,
                "failed" => failed = n,
                _ => {}
            }
        }
        return Some((passed + failed, passed));
    }
    None
}

/// The documented fraction off rustdoc's coverage table, as the percentage in
/// its `Total` row.
pub fn doc_coverage(log: &str) -> Option<String> {
    for line in log.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 4 || cells[1] != "Total" {
            continue;
        }
        return Some(cells[3].trim_end_matches('%').to_string());
    }
    None
}

/// How many findings `cargo deny check` printed, counting every diagnostic
/// header of the shape `error[advisory-id]` or `warning[...]`.
pub fn deny_findings(log: &str) -> u64 {
    log.lines()
        .filter(|l| {
            let l = l.trim_start();
            (l.starts_with("error[") || l.starts_with("warning[")) && l.contains(']')
        })
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_test_results_sum_over_every_crate_and_doctest_block() {
        let log = "running 3 tests\ntest a ... ok\ntest result: ok. 3 passed; 0 failed; 1 ignored; 0 measured\n\n   Doc-tests x\ntest result: ok. 2 passed; 0 failed; 0 ignored\n";
        assert_eq!(cargo_tests(log), Some((6, 5)));
    }

    #[test]
    fn a_failed_cargo_run_counts_the_failures_in_the_total() {
        let log = "test result: FAILED. 4 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out\n";
        assert_eq!(cargo_tests(log), Some((6, 4)));
    }

    #[test]
    fn no_result_line_is_none_rather_than_zero() {
        assert_eq!(cargo_tests("error: could not compile\n"), None);
        assert_eq!(deno_tests("error: Module not found\n"), None);
        assert_eq!(doc_coverage("nothing here"), None);
    }

    #[test]
    fn deno_counts_come_off_its_summary_line_in_both_shapes() {
        assert_eq!(deno_tests("ok | 12 passed | 0 failed (300ms)\n"), Some((12, 12)));
        assert_eq!(deno_tests("FAILED | 9 passed | 3 failed (1s)\n"), Some((12, 9)));
        assert_eq!(deno_tests("ok | 2 passed (1 step) | 0 failed (5ms)\n"), Some((2, 2)));
    }

    #[test]
    fn coverage_is_the_total_row_of_the_table() {
        let log = "+---------+------+------------+------------+\n| File | Documented | Percentage | Examples |\n+---+---+---+---+\n| src/lib.rs | 4 | 80.0% | 0 |\n| Total | 10 | 83.3% | 1 |\n+---+---+---+---+\n";
        assert_eq!(doc_coverage(log).as_deref(), Some("83.3"));
    }

    #[test]
    fn deny_findings_count_diagnostic_headers_and_nothing_else() {
        let log = "error[vulnerability]: something\n  ┌─ Cargo.lock:12\nwarning[unmaintained]: other\nerror: encountered 1 error\n";
        assert_eq!(deny_findings(log), 2);
        assert_eq!(deny_findings("advisories ok\n"), 0);
    }
}
