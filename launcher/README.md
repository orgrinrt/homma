# `homma`

[![Crates.io](https://img.shields.io/crates/v/homma)](https://crates.io/crates/homma) [![docs.rs](https://img.shields.io/docsrs/homma)](https://docs.rs/homma)

> One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli.

This package is the launcher, the small binary that goes on `PATH`. It finds the workspace, reads the engine version the workspace pins in its manifest, builds that engine once into a shared cache and hands over to it. The commands themselves are the engine's, and they are listed in the [repository readme](https://github.com/orgrinrt/homma).

## Usage

```bash
cargo install homma
```

Pin the engine at the top of the workspace's `homma.toml` with one of `homma_version`, `homma_tag`, `homma_rev` or `homma_branch`. `--engine <path>` points the launcher at a checkout instead, which is for working on the engine itself.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/homma/blob/main/LICENSE)
