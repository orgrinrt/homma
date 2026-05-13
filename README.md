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

Pre-implementation. The repo carries the bootstrap workspace shape; substantive code lands across a sequence of upcoming tasks (config schema, git ops via `gix`, Forge trait, Forgejo and GitHub clients, CLI parser, migrate command, built-in lints).

This README will fill in the standard sections (Installation, Usage, Contents, Cargo features) once the surface stabilises.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/dev/LICENSE)
