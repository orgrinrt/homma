# `homma`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/stargazers)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/issues)
![License](https://img.shields.io/github/license/orgrinrt/homma?color=%23009689)

> Workspace management for multi-repo Rust workspaces. Native git and forge operations, one CLI for orchestration across many repos.

</div>

## What it is

`homma` is a Rust workspace management tool for developers who work across many independently-versioned repositories that live side-by-side on disk and share refactors, dependency bumps, and migrations. It replaces ad-hoc shell scripts and provider-specific CLIs with one workspace-aware binary that speaks git and the forge HTTP APIs directly.

The intended workflow shape is a flat workspace directory of cloned repos plus a `homma.toml` at the workspace root that names them, their forge origins, and who works in them. `homma` reads the manifest, walks the repos, and runs operations across the set: reporting their state, standing participants up with their own clones and generated definitions, aggregating agent rules and hooks into the workspace, reading forge metadata, and migrating repositories between forges.

## Status

Working, and narrow. homma reads a workspace manifest, reports the state of every
member repository and its forge mapping, drives each member's own tooling,
migrates a repository between forges and archives the source, and carries a
registry of the participants who work in the workspace. A registry entry is
either staffed, somebody who works, or mapped, a domain recorded as owned before
anybody is put on it. Standing a staffed entry up is one command: its directories
are created, its definitions generated, its workspace cloned from the content
repository, and its author and committer identities set in that clone.

The participant's directories, definitions and memory link go through a path
proven against the filesystem to resolve inside the workspace root, rather than
checked lexically, because a symlink defeats anything lexical. The participant's
own clone is deliberately outside that root and is guarded differently: it is
refused when it would land inside a repository that is not its own, and homma
creates its immediate parent but never a chain of directories leading to it.

`org add` rewrites the registry at whatever path `--config` names, and that path
goes through the same check.

**The aggregation pass behind `agent regen` writes into `<workspace>/.claude/`**:
hook scripts, marked executable, plus a `settings.json` registering them, and it
removes the ones it wrote last time. Every one of those targets is proven against
the filesystem before it is written, through the same mechanism as everything
else, because a check on the directory says nothing about a path built below it
with an ordinary join.

What it may not aggregate into is a home directory's own `.claude`, which is
never a workspace, and any workspace belonging to somebody else. It may aggregate
into its own, which is the ordinary case and the only one.

What goes on `PATH` is a small launcher. It finds the workspace, reads the
version of the engine the workspace pins, builds that once into a shared cache,
and hands over. So the workspace decides which homma runs in it, and a checkout
you installed from months ago does not.

```
cargo install --path launcher
```

The engine is a separate package and is not installed by hand. The launcher
builds the pinned one on first use, and `--engine <path>` points it at a
checkout while you are working on the engine itself.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/main/LICENSE)
