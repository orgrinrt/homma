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

The manifest names the repositories, where each of them lives on which forge,
and who works in the workspace. From there it reports the state of the whole
set, drives each member's own tooling, moves a repository from one forge to
another and archives the source behind it, and stands up the people and the
directories the workspace is organised around.

## Status

Working, but narrow, and the api is nowhere near settled so breaking changes
should be expected. What's here is the manifest, the per-repo reporting, the
registry, the forge reads and the migration path. I'd caution against wiring it
into anything that has to keep running unattended just yet.

## What it does

| Command | What it's for |
|---|---|
| `homma status` | The whole workspace at a glance: every repo, its forge, its resolved default branch |
| `homma verify` | Checks the manifest parses, its forges are declared, and their tokens resolve. `--remote` also asks each forge whether the repo is really there |
| `homma repo <op>` | Per-repo work against the local tree, without the `cd` |
| `homma forge show` | Reads a repo's metadata off whichever forge the manifest maps it to |
| `homma migrate` | Mirror-clones a repo to another forge and pushes it, replicating description, visibility and default branch |
| `homma archive` | Marks the source archived, deliberately as a second step rather than folded into the migration |
| `homma org` | The registry of who works here, and standing an entry up with its directories and its own clone |
| `homma docs` | Reports which documentation surfaces each member repo currently has |

`--output json` sits on the root and applies to all of them, one document per
command, which is there mostly so you can pipe it into `jq` and stop parsing our
terminal formatting. `--config` and `--dir` are global the same way, and say
which manifest to read and which directory to treat as the root.

## Usage

```bash
# from the workspace root, where homma.toml lives
homma status
homma verify --remote

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

[repos.notko]
forge = "github"
owner = "orgrinrt"
local_path = "notko"
```

`local_path` is relative and resolves against the manifest's own directory, so
nothing in here names a particular clone on a particular machine and the same
file works in every copy of the workspace. A repo the workspace hasn't cloned
yet is simply not on disk, which homma reports rather than trips over.

Tokens are read out of the environment by the name the forge names, so nothing
secret is ever written into the manifest. `homma verify` tells you which of them
did not resolve before any command goes and finds out the hard way.

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

## Responsible tooling

`homma agent` walks the workspace and drives each member repo's own template
regeneration, which in practice means assistant configuration files end up
written into the workspace. It's a convenience for a workflow that already has
those files, not a reason to adopt the tool, and everything else here works with
it untouched.

We do not recommend using coding agents with this codebase.

If you still choose to use one:

- Be aware of the environmental and social impact of large-scale model
  inference. Minimise agent use where it is not needed. Be responsible.
- Only use an agent if you yourself understand the architecture. Do not use an
  agent because you do not understand; you will waste time and energy, both
  yours and the planet's.

The recommendation stands: do this work yourself unless you know what you are
doing and why.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/main/LICENSE)
