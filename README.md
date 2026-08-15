# `homma`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/stargazers)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/issues)
![License](https://img.shields.io/github/license/orgrinrt/homma?color=%23009689)

> Workspace management for multi-repo Rust workspaces. Native git and forge operations, one CLI for orchestration across many repos.

</div>

## What it is

`homma` is a Rust workspace management tool for developers who work across many independently-versioned repositories that live side-by-side on disk and share design rounds, refactors, and migrations. It replaces ad-hoc shell scripts and provider-specific CLIs with one workspace-aware binary that speaks git and the forge HTTP APIs directly.

The intended workflow shape is a flat workspace directory of cloned repos plus a `homma.toml` at the workspace root that names them, their forge origins, and any workspace-level conventions. `homma` reads the manifest, walks the repos, and runs operations across the set: opening PRs, applying branch protections, performing migrations between forges, enforcing workspace-wide conventions on commit/PR text.

## Status

Working, and narrow. homma reads a workspace manifest, reports the state of every
member repository and its forge mapping, drives each member's own tooling, and
carries a registry of the participants who work in the workspace.

It also stands one of them up: cloning a workspace for a participant, setting its
author and committer identities inside that clone, linking its memory where the
agent harness looks for it, and generating its definitions.

The participant's directories, definitions and memory link go through a path
proven against the filesystem to resolve inside the workspace root, rather than
checked lexically, because a symlink defeats anything lexical. The participant's
own clone is deliberately outside that root and is guarded differently: it is
refused when it would land inside a repository that is not its own, and homma
creates its immediate parent but never a chain of directories leading to it.

Other things homma writes, the aggregated hooks and settings the gen pass
produces, do not go through that check yet.

The Cargo workspace lives under `mock/`, not at the repository root, which is the
shape every repository in this ecosystem uses. Build from there:

```
cd mock && cargo build
```

Design documents are generated into `docs/` from templates under `mock/`; the
templates are the source and the rendered tree is not hand-edited.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/dev/LICENSE)
