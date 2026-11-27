//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The entries of the manifest this engine does not read, on the workspace line.

use super::tests::{rendered, report_of};

#[test]
fn an_entry_this_engine_does_not_read_is_named_with_what_to_do() {
    let r = report_of(
        "homma_branch = \"dev\"\n[workspace]\nname = \"w\"\n[later]\nx = 1\n[[agent.later]]\na = 1\n",
    );
    assert_eq!(r.workspace.unread, ["agent.later", "later"]);
    let text = rendered(&r);
    assert!(
        text.contains("  not read by this engine: agent.later later\n"),
        "{text}"
    );
    assert!(text.contains("the branch `dev`"), "{text}");
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(
        doc["workspace"]["unread"],
        serde_json::json!(["agent.later", "later"])
    );
    // Failing control: a manifest this engine reads whole prints no line and still carries the
    // list, empty, since the field being there is what the order of landings checks.
    let read = report_of("homma_branch = \"dev\"\n[workspace]\nname = \"w\"\n");
    assert!(
        !rendered(&read).contains("not read by this engine"),
        "{}",
        rendered(&read)
    );
    let doc = serde_json::to_value(&read).unwrap();
    assert_eq!(doc["workspace"]["unread"], serde_json::json!([]));
    assert!(doc["workspace"].get("unread_advice").is_none());
}
