use super::*;

#[test]
fn last_meaningful_line_picks_last_nonempty() {
    let s = "first\nsecond\n\n   \nthird\n";
    assert_eq!(last_meaningful_line(s), Some("third"));
}

#[test]
fn last_meaningful_line_empty_input_is_none() {
    assert_eq!(last_meaningful_line(""), None);
    assert_eq!(last_meaningful_line("\n\n   \n"), None);
}

#[test]
fn truncate_short_is_unchanged() {
    assert_eq!(truncate("hello".into(), 16), "hello");
}

#[test]
fn truncate_long_clipped_with_ellipsis() {
    let s = "abcdefghij".to_string();
    let out = truncate(s, 5);
    assert_eq!(out, "abcde...");
}

#[test]
fn truncate_walks_back_to_char_boundary() {
    // Two-byte chars; cutting at 3 lands mid-char.
    let s = "ééé".to_string(); // 6 bytes
    let out = truncate(s, 3);
    // Walks back to byte 2 (between first and second char).
    assert!(out.ends_with("..."));
    assert!(out.is_char_boundary(out.len() - 3));
}
