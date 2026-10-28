# doc changelist

Answers `202610281012-giex_topic.one-call-takes-the-consoles-answers.md`. The five foreign flat topics (`capture-answers-left-on-a-page` is this round's neighbour, see below) need nothing else.

## Current state

The capture paragraphs of `mock/DESIGN.md.tmpl` describe one source, a session's transcript, and one command, `homma capture`. Nothing reads the answer and note documents the console and the dashboard's page hold, so the drain script in the workspace writes their capture by hand, edits the registry rows with its own awk, and nothing runs it.

## Changes proposed

- The command list in `mock/DESIGN.md.tmpl` gains `homma answers`: answers and notes left on a page or a channel, into the capture store and onto their registry rows.
- A paragraph after the capture ones describes `homma answers take`: its two sources (a directory of `answers/` and `notes/` documents, or a channel read through the workspace's `msg` tool), the checks a document meets before anything is derived from it, the store key that makes a run idempotent, the capture it writes in the shape of the dashboard's, the attach to a row as `<at>: <capture path>` through `registry-append --local`, the output (nothing when nothing was new, one line when something was), and that committing is the caller's.
- The same paragraph describes `homma answers delegate`, the entry a workspace's `.mockspace/hooks/registry-append` runs: it takes a row's `answers` entry made of words and turns it into a capture plus a pointer, and declines, by status 3, anything that is not that. `homma answers hook` places and removes that file.
- The topic `202610270717_topic.capture-answers-left-on-a-page.md` is answered by the paragraph, since its waiting condition, a real answer read back, is met by the console's document, and it is archived with this round.

## What the topic raised that needs no document change

- The dashboard script in the workspace repository calling this command in place of its own capture text is a change there, handed back as a patch.
