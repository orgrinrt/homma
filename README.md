# `homma`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/stargazers)
[![Crates.io](https://img.shields.io/crates/v/homma)](https://crates.io/crates/homma)
[![docs.rs](https://img.shields.io/docsrs/homma)](https://docs.rs/homma)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/issues)
![License](https://img.shields.io/github/license/orgrinrt/homma?color=%23009689)

> One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli.

</div>

Some projects end up as a handful of repositories sitting side by side in one
directory, versioned apart but refactored together. The shell scripts that keep
that arrangement moving tend to grow one flag at a time until nobody remembers
which of them is current, and the provider clis only know about one forge each.
`homma` is the attempt at replacing that pile with a single binary that reads a
manifest at the root and walks the set.

The repositories are not named anywhere. A directory under the root that is its
own clone is a member, and where it lives comes off its `origin` remote. What
the manifest carries is the rest: the forges it may talk to, who works in the
workspace, and the places it may not write. From there it reports the state of
the whole set, drives each member's own tooling, moves a repository from one
forge to another and archives the source behind it, and stands up the people and
the directories the workspace is organised around.

## Status

Working, but narrow, and the api is nowhere near settled so breaking changes
should be expected. What's here is the manifest, the per-repo reporting, the
registry, the forge reads and the migration path. I'd caution against wiring it
into anything that has to keep running unattended just yet.

## What it does

| Command | What it's for |
|---|---|
| `homma status` | The whole workspace at a glance: every repo it found, and the forge and owner off its remote |
| `homma verify` | Checks the manifest parses, its forges are declared, and their tokens resolve. `--forge` also asks each forge whether the repo is really there |
| `homma repo <op>` | Per-repo work against the local tree, without the `cd` |
| `homma forge show` | Reads a repo's metadata off whichever forge the manifest maps it to |
| `homma migrate` | Mirror-clones a repo to another forge and pushes it, replicating description, visibility and default branch |
| `homma archive` | Marks the source archived, deliberately as a second step rather than folded into the migration |
| `homma org <op>` | The registry of who works here, and standing an entry up with its directories and its own clone |
| `homma agent <op>` | Reports which member repos carry their own template scaffolding, and drives each one's regeneration |
| `homma docs status` | Reports which documentation surfaces each member repo currently has |
| `homma release <op>` | The gate that runs on the pushing machine and posts its status, and the release that merges the trunk onto `main`, tags it, writes the changelog, publishes to the registries and rewrites the badges |

`--output json` sits on the root and applies to all of them, one document per
command, which is there mostly so you can pipe it into `jq` and stop parsing our
terminal formatting. `--config` and `--dir` are global the same way, and say
which manifest to read and which directory to treat as the root.

## Usage

```bash
# from the workspace root, where homma.toml lives
homma status
homma verify --forge

# read one repo's metadata off its forge
homma forge show github orgrinrt/notko

# move a repo across, then archive what it came from
homma migrate notko --to codeberg
homma archive notko --from github
```

The manifest is a `homma.toml` at the root of the directory holding the clones:

```toml
[workspace]
name = "my-stack"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_env = "MY_GITHUB_TOKEN"
```

The repos are not in there, and there is nothing to add when one arrives. A
directory one level under the workspace root that is its own repository is a
member, and where it lives and who owns it come from its `origin` remote, which
is the thing that decides where a push lands anyway. Clone one and it is a
member; rename it and the name follows; drop it and it is gone. Nothing is left
behind saying otherwise, which is what a list is for and what a list keeps being
wrong about.

The root is `workspace.path`, so a manifest that sits somewhere else still
looks in the right place. A worktree is not a second member either: it carries a
`.git` file rather than a directory, and only the directory counts.

A clone whose remote is a local path, or is on a host you have no `[forges]`
profile for, is still a member. It has no forge and no owner, homma says so,
and the commands that need one take it as a flag rather than guessing.

There is also `deny`, which is the places homma may not write. Two are denied
without being named, because they are the same wherever it runs: your own
`~/.claude`, where an assistant keeps its settings and its credentials, and
every other participant's workspace as the registry gives them. Anything else
is your own arrangement rather than homma's guess, so it goes in the manifest:

```toml
deny = [
    "~/work/not-mine",
    { path = "scratch", why = "regenerated, and not worth a merge conflict" },
]
```

A bare path is enough. The table form adds the reason you'll see in the
refusal, which is worth the extra words on anything you might forget you wrote
down. It goes above the first `[table]`, since it belongs to the manifest and
not to any section of it. The `~/` works here and only here: `workspace.path`
takes it literally, and you get a directory named `~`.

One thing the list does not cover, and it is deliberate. `homma agent regen`
writes into the workspace root even when an entry names it, because the
alternative is a command that quietly does nothing after you asked for it. Deny
the root and you have denied homma the place it works in, so the aggregation
permits it back. Everywhere else the entry holds.

Tokens are read out of the environment by the name the forge names, so nothing
secret is ever written into the manifest. `homma verify` tells you which of them
did not resolve before any command goes and finds out the hard way.

Where a credential lives in something other than an environment variable, a
forge may instead name `token_cmd`, an argument list homma runs and reads the
token off stdout. That is a program the manifest chooses, so a manifest you got
from somewhere else runs whatever it names when a forge is asked for anything.
Read that key before you trust a manifest you did not write.

A workspace grows tools that homma has no business owning. What fills a session's
context window, what the rule corpus costs to load, what is still open on the
tracker: real things, but somebody else's, and not something a workspace
orchestrator should have an opinion about. So `homma status` can carry what they
say instead, through `[[status.inject]]`:

```toml
[[status.inject]]
tool = "tools/context"

[[status.inject]]
title = "the rule corpus"
tool = ["tools/rules/rules", "size"]
format = "grep -v '^gated'"
```

Each one runs in the workspace root, in the order it is declared, and its stdout
becomes a block under the repos. `title` is optional and falls back to the
program's own file name, which is usually what you would have typed anyway.
`format` is optional too, a shell line the output goes through on its way out, so
`head -3` and friends work the way you would expect.

The command itself is an argument list rather than a shell line, same as
`token_cmd`, and a bare string counts as a one word one. A relative path with a
separator in it resolves against the workspace root, and a bare name is left for
`PATH`.

Tools break, and `homma status` is the cheap look you take at a workspace before
anything else, so a tool that is missing or exits non-zero does not take the rest
of it down. The block says what happened and carries the first line the tool put
on stderr, and everything else still prints. Same warning as `token_cmd` here
though: these are programs the manifest picks.

## Installation

What goes on `PATH` is a small launcher. It finds the workspace, reads the
engine version the workspace pins, builds that once into a shared cache, and
hands over. So the workspace decides which homma runs in it, and a checkout
installed months ago does not.

```bash
cargo install homma
```

```bash
cargo install --path launcher
```

The second one works from a checkout and is what to use while changing the
launcher itself. Do note that it installs from a path rather than a remote, so
there is nothing for the update check to compare against and it stays exactly as
current as the checkout it came from. `HOMMA_NO_SELF_UPDATE` turns the check off
where it isn't wanted, on a build machine say.

The engine is a separate package and isn't installed by hand. The launcher
builds the pinned one on first use, and `--engine <path>` points it at a
checkout while you're working on the engine instead.

The pin is one key at the top of the manifest, and which key decides where the
engine is fetched from:

```toml
homma_version = "0.0.1"   # a release
homma_tag     = "0.0.1"   # a tag in the repository
homma_rev     = "f2b0c4e" # one commit, which is what a workspace wants
homma_branch  = "dev"     # a moving target, so the engine moves under you
```

`homma_git` names a different repository to take the engine from, for a fork.
With no key at all the launcher says so rather than guessing, because a
workspace that has not decided which engine it runs has not decided.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/main/LICENSE)
