<h1 align="center">
<img src="https://raw.githubusercontent.com/filllabs/ud/main/assets/img/icon.png" alt="ud Icon" width="128" height="128">

ud

</h1>
<div align="center">

<!-- CORE BADGES -->

[![GitHub: Repo](https://img.shields.io/badge/ud-58A6FF?&logo=github)](https://github.com/filllabs/ud)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

<!-- Rust based projects -->

[![Crates.io](https://img.shields.io/crates/v/ud)](https://crates.io/crates/ud)
[![Crates.io Downloads](https://img.shields.io/crates/d/ud)](https://crates.io/crates/ud)
[![docs.rs](https://img.shields.io/badge/docs.rs-ud-66c2a5)](https://docs.rs/ud)

</div>

> **ud** — Up to Date. The two-letter answer to "are my dependencies current?"

`ud` is a small, fast, and universal dependency updater. It is designed to be
**headless-first** and **lossless by default**, performing surgical updates on 
manifests (like `Cargo.toml`) without disturbing your comments, formatting, or 
inline tables.

> **Note:** `ud` doesn't just check for updates; it acts on them. Two keystrokes, 
> one idea, every ecosystem.

## 🚦 Getting Started

### Installation

```sh
cargo install ud
```

### Quick Start

Check for updates and automatically apply them in the current directory:

```sh
ud
```

Preview changes without modifying any files:

```sh
ud -y
```

List all dependencies and their current status:

```sh
ud tree
```

## 📄 License

This project is licensed under the **MIT License**. See the [LICENSE](./LICENSE)
file for details.
