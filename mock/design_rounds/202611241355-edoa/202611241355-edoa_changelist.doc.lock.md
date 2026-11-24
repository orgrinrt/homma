# doc changelist

Descends from `202611241355-edoa_topic.a-large-output-summarised-by-one-isolated-call.md`.

## CHANGE: the command surface FROM no summarising pass TO `homma summarise` in the list and a section naming the function, the opt-in, the isolated call, the budgets, the kept locations, the rules' version and `homma search --summarise`
File: `mock/DESIGN.md.tmpl`
Verification: the command list has `homma summarise`; the section "Summarising a large output" names the function any verb calls, `[summary]` off by default with the output whole and one line when off or failed, the call's flags and its empty directory, the one-shot model as a trait, the 15k and 100k budgets at three bytes to a token with the 8k reserve and the parts cut at lines, the numbered lines and the count of summary lines with no place, the rules' digest as their version, and `--summarise` on `homma search` with its human and JSON shapes.

## CHANGE: the api's tables FROM no table for the summary TO `SummaryConfig`, the `[summary]` table, with `enabled`, `command` and `model`
File: `mock/crates/homma-api/DESIGN.md.tmpl`
Verification: "Where the files are" names `SummaryConfig` and its three keys with their defaults.

## CHANGE: the engine's summary FROM no `summarise` TO `summarise` named among the families
File: `mock/crates/homma-engine/README.md.tmpl`
Verification: the summary has the sentence on `summarise`.
