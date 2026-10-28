# source changelist

Implements `202610281012-giex_changelist.doc.lock.md`.

## CHANGE: enum `Command` FROM no answers command TO `Answers` with the operations `take`, `delegate` and `hook`
File: `mock/crates/homma-engine/src/cli.rs`
Verification: `homma answers --help` lists the three; `take` requires exactly one of `--dir` and `--channel`, and a test parses each form and refuses neither and both.

## CHANGE: module `cmd::answers` added, with `docs`, `render`, `attach`, `delegate` and `hook`
File: `mock/crates/homma-engine/src/cmd/answers/mod.rs`
Verification: the module's tests below; `wc -l` of every file under the module is under 500.

## CHANGE: fn `docs::read_dir` and `docs::read_channel` added, and `docs::fault` the one check a document meets
File: `mock/crates/homma-engine/src/cmd/answers/docs.rs`
Verification: a test per refusal the design lists (a bad `at`, an `id` with a path in it, a `kind` that is neither agenda nor registry, a control character in a choice, an option that is not text), each skipped and named while a good document beside it is taken; the channel read is tested against a stand-in `msg` script that prints a fixed listing, including a body that is not JSON and a message of another kind.

## CHANGE: fn `render::text` added, writing the capture in the shape the dashboard's drain writes
File: `mock/crates/homma-engine/src/cmd/answers/render.rs`
Verification: a golden file holds the text the drain's `_capture_text` produced for a fixed set of documents (one answer with options and text, one free-text answer, one note, a backtick run in a question), generated once by running that function, and the test compares byte for byte.

## CHANGE: fn `store::recorded` and fn `store::name` added, the keys a store already lists and the file a capture gets
File: `mock/crates/homma-engine/src/cmd/answers/render.rs`
Verification: a test that a document whose key a capture lists under `store:` is not taken, that a line in a capture's body that reads like a key is not a listing, and that a name already taken gets the hash suffix.

## CHANGE: fn `attach::row` added, `cargo mock registry-append <ns>::<id> answers "<at>: <path>" --local` in the repository the document names
File: `mock/crates/homma-engine/src/cmd/answers/attach.rs`
Verification: a test against a stand-in launcher named by `HOMMA_MOCK` that records its arguments and working directory: a ticket document and a bare question id reach the right target, `--local` is always passed, a `where` that is no clone falls to the work's repository, and a target nobody can name is reported and not run.

## CHANGE: fn `delegate::run` added, taking an `answers` entry of words whole and declining everything else with status 3
File: `mock/crates/homma-engine/src/cmd/answers/delegate.rs`
Verification: a test that words become a capture and a pointer, that another field and a value without a leading timestamp exit 3 and write nothing, and that the pointer attach carries `--local`.

## CHANGE: fn `hook::place` and fn `hook::remove` added, the workspace's `.mockspace/hooks/registry-append`
File: `mock/crates/homma-engine/src/cmd/answers/hook.rs`
Verification: a test that the file is executable, names the engine that wrote it, is replaced by a second placing, and is gone after removal with nothing printed.

## CHANGE: the end-to-end path tested without a network
File: `mock/crates/homma-engine/tests/answers_smoke.rs`
Verification: a workspace fixture, a directory of documents and a stand-in launcher: the first run writes one capture and attaches, a second run prints nothing and writes nothing (the case that must fail if the store keys were not read), `--output json` names the capture, and the delegate run through a hook placed by `homma answers hook` writes a capture and a pointer.

## CHANGE: enum variant `Outcome::Exit` added, a command that ends on a status of its own
File: `mock/crates/homma-engine/src/cmd/mod.rs`
Verification: the smoke test reads status 3 from the placed hook when the delegate declines, which is how mockspace tells a declined step from a failed one.

## Finding: the console's ticket document

The console sends a ticket question as `kind: ticket` with `key: {"ticket": "<id>"}`, where the drain accepts only a plain identifier for both. `docs::fault` takes either shape, and the drain's own validation still refuses the console's documents until it is rewired onto this command.

## Open topics

The four other foreign flat topics need nothing from this round; `202610270717_topic.capture-answers-left-on-a-page.md` is answered and archived with it.
