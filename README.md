# `homma`

[![Crates.io](https://img.shields.io/crates/v/homma)](https://crates.io/crates/homma) [![docs.rs](https://img.shields.io/docsrs/homma)](https://docs.rs/homma)

> One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli.

`homma` is for the case where a project is a handful of repositories sitting side by side in one directory, versioned apart but refactored together. It reads a manifest at the root, walks the set, reports the state of the whole of it, drives each member's own tooling and moves a repository from one forge to another.

The repositories are not named anywhere. A directory under the root that is its own clone is a member, and where it lives comes off its `origin` remote, so cloning one makes it a member and dropping it removes it. The manifest carries the rest: the forges it may talk to, who works in the workspace and the places it may not write.

It is working but narrow, and the api is nowhere near settled, so I'd not wire it into anything that has to run unattended yet.

## Usage

Run these from the workspace root, where `homma.toml` lives:

```bash
homma status
homma verify --forge
homma forge show github orgrinrt/notko
homma migrate notko --to codeberg
homma archive notko --from github
```

The manifest is a `homma.toml` at the root of the directory holding the clones, and tokens are read from the environment by the name the forge gives, so nothing secret goes in it:

```toml
[workspace]
name = "my-stack"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_env = "MY_GITHUB_TOKEN"
```

Where a credential lives somewhere other than an environment variable, a forge can name `token_cmd` instead, an argument list homma runs and reads the token off stdout. That is a program the manifest picks, so read that key before trusting a manifest you didn't write.

`--output json` applies to every command and gives one document each, which is handy for piping into `jq`. `--config` and `--dir` are global too, and say which manifest to read and which directory is the root.

## Commands

| Command | What it's for |
|---|---|
| `homma status` | the state of the workspace, every repo with its forge and owner |
| `homma local <op>` | this clone's own `homma.local.toml`, which is never committed |
| `homma verify` | checks the manifest parses and the forge tokens resolve |
| `homma repo <op>` | per-repo work against the local tree |
| `homma forge show` | reads a repo's metadata off its forge |
| `homma migrate` | mirror-clones a repo to another forge and pushes it |
| `homma archive` | archives the source, as a step of its own after a migration |
| `homma capture` | copies what a person said to an agent session into the capture store |
| `homma answers <op>` | takes answers and notes left on a page or channel into the capture store |
| `homma org <op>` | the registry of who works here |
| `homma agent <op>` | which member repos have adopted mockspace, and regenerating their surfaces |
| `homma agents <op>` | lists the agent personas and generates them |
| `homma identity <op>` | generates an identity's agent definition and starts a session as it |
| `homma docs status` | which documentation surfaces each repo has |
| `homma rules <op>` | which rules govern a subject, and generating the always-loaded cards |
| `homma skills <op>` | lists the skills and generates the tree |
| `homma tools` | lists and runs the content repository's own tools |
| `homma what` | which tool does a thing, asked in your own words |
| `homma release <op>` | the release gate, and the release that merges the trunk onto main |
| `homma milestone <op>` | where the current milestone began and what the range since holds |
| `homma hook <op>` | the git hooks, running what `homma.toml` names for each event |
| `homma workspace [op]` | the launcher's own: bare it lists the repos, `spawn <slug> [owner/name ...]` makes a workspace, `reap [<slug>]` removes one and `list` says what's there |
| `homma config <op>` | your own settings file, with `path`, `schema`, `get`, `set` and `edit` |

## Installation

What goes on `PATH` is a small launcher. It finds the workspace, reads the engine version the workspace pins, builds that once into a shared cache and hands over, so the workspace decides which homma runs in it.

```bash
cargo install homma
```

The engine is a separate package and isn't installed by hand. The pin is one key at the top of the manifest, and which key you use decides where the engine is fetched from:

```toml
homma_version = "0.0.1"   # a release
homma_tag     = "0.0.1"   # a tag in the repository
homma_rev     = "f2b0c4e" # one commit, which is what a workspace wants
homma_branch  = "dev"     # a moving target, so the engine moves under you
```

`homma_git` names a different repository to take it from, for a fork. With no key at all the launcher says so rather than guessing.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/main/LICENSE)
