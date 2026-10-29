# src changelist

Implements the doc changelist of round 202610292100-ydhm.

CHANGE mock/crates/homma-engine/src/cmd/rules.rs: a load row counts in `cards` and `bytes` the admitted rules with no `paths:` scope, and carries `scoped_cards` and `scoped_bytes` for the admitted ones that have one; the human line names both
CHANGE mock/crates/homma-engine/src/cli.rs: `--all` on `homma rules load` conflicts with `--role` and `--place`
CHANGE mock/crates/homma-engine/tests/rules_session_smoke.rs: the fixture gains a rule gated on a path and the rows are asserted to count it as scoped and not as loaded; `--all` with `--role` is refused; the named session's cards are asserted largest first; a misspelt role file is not read when `--role` is given; a role file that is a directory is refused
CHANGE mock/crates/homma-org/tests/rules_corpus.rs: the assertion that admitted and withheld add up to all rules is deleted, as it holds by construction, and the test is renamed for what it still checks
CHANGE mock/crates/homma-api/tests/session.rs: the four tests that are subsets of the literal table are deleted
CHANGE mock/crates/homma-api/tests/rule_meta.rs: the empty-list test is renamed for what it checks

## Open topics this round was filed

- `how-a-provisioned-workspace-defends-its-identity-against-the-environment`, `what-a-stranger-cannot-reach`, `work-only-on-the-repos-the-instance-names`, `homma-what-finds-the-tool-that-does-a-thing`: other work's; nothing changes here, and each stays flat.
