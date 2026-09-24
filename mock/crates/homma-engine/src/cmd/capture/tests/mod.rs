//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The capture path over planted transcripts in the harness's shapes, each
//! shape as it was read off real sessions.

use serde_json::{Value, json};

mod make;
mod render;
mod store;
mod transcript;

/// One transcript out of lines.
fn lines(v: &[Value]) -> String {
    v.iter().map(|l| format!("{l}\n")).collect()
}

fn typed(uuid: &str, at: &str, words: &str) -> Value {
    json!({
        "type": "user", "uuid": uuid, "timestamp": at, "isSidechain": false,
        "origin": {"kind": "human"},
        "message": {"role": "user", "content": words},
    })
}

fn said_by_agent(uuid: &str, at: &str, text: &str) -> Value {
    json!({
        "type": "assistant", "uuid": uuid, "timestamp": at,
        "message": {"role": "assistant", "content": [
            {"type": "thinking", "thinking": "not text"},
            {"type": "text", "text": text},
            {"type": "tool_use", "id": "t", "name": "Bash", "input": {}},
        ]},
    })
}

fn queued(uuid: &str, at: &str, words: &str) -> Value {
    json!({
        "type": "attachment", "uuid": uuid, "timestamp": at,
        "attachment": {
            "type": "queued_command", "prompt": words, "commandMode": "prompt",
            "origin": {"kind": "human"}, "timestamp": at,
        },
    })
}

/// The agent's call putting round `uuid` to the person, which a round's result
/// has to answer to be read as one.
fn asking(uuid: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("{uuid}-call"), "timestamp": "2026-09-24T09:00:00Z",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": format!("ask-{uuid}"), "name": "AskUserQuestion", "input": {}},
        ]},
    })
}

/// An answered round of two questions, answering the call [`asking`] makes.
fn round(uuid: &str, at: &str, first: &str, second: &str, notes: Option<&str>) -> Value {
    let mut annotations = serde_json::Map::new();
    if let Some(n) = notes {
        annotations.insert("Which way?".into(), json!({"notes": n}));
    }
    json!({
        "type": "user", "uuid": uuid, "timestamp": at,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": format!("ask-{uuid}"), "content": "answered"},
        ]},
        "toolUseResult": {
            "questions": [
                {"question": "Which way?", "header": "Way", "multiSelect": false, "options": [
                    {"label": "Left", "description": "Go left."},
                    {"label": "Right", "description": "Go right.", "preview": "```\nR\n```"},
                ]},
                {"question": "Which colours?", "header": "Colours", "multiSelect": true, "options": [
                    {"label": "Red", "description": "Warm."},
                    {"label": "Blue", "description": "Cold."},
                ]},
            ],
            "answers": {"Which way?": first, "Which colours?": second},
            "annotations": annotations,
        },
    })
}
