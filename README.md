<h1 align="center">
<img src="https://raw.githubusercontent.com/Yrrrrrf/ud/main/assets/img/icon.png" alt="ud Icon" width="128" height="128">

ud
</h1>

<div align="center">

[![GitHub: Repo](https://img.shields.io/badge/ud-58A6FF?&logo=github)](https://github.com/Yrrrrrf/ud)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Crates.io](https://img.shields.io/crates/v/ud)](https://crates.io/crates/ud)
[![Crates.io Downloads](https://img.shields.io/crates/d/ud)](https://crates.io/crates/ud)
[![docs.rs](https://img.shields.io/badge/docs.rs-ud-66c2a5)](https://docs.rs/ud)

</div>

> **ud** (Up to Date) — The two-letter answer to "are my dependencies current?"

`ud` is a small, fast, and universal dependency updater. It is designed to be **headless-first** and **lossless by default**, performing surgical updates on manifests (like `Cargo.toml`) without disturbing your comments, formatting, or inline tables.

By default, `ud` is safe: it runs in **check-only mode** and will never modify your files unless explicitly requested.

---

## 🚦 Getting Started

### Installation

```sh
cargo install ud
```

### Quick Start

1. **Check for updates** in the current directory (dry-run):
   ```sh
   ud
   ```

2. **Apply compatible updates** losslessly:
   ```sh
   ud -u
   ```

3. **Apply breaking updates** as well:
   ```sh
   ud -u --allow-breaking
   ```

4. **List all dependencies** and their current version details:
   ```sh
   ud tree
   ```

---

## ⚙️ CLI Reference

### Usage
```sh
ud [OPTIONS] [PATH] [COMMAND]
```

* **`[PATH]`**: Path to the manifest file or directory containing one (defaults to `.`).

### Options
* `-u, --update`: Update the manifest file losslessly with compatible versions.
* `--allow-breaking`: Also apply breaking version bumps when updating (requires `-u` / `--update`).
* `--pre`: Include prerelease versions when resolving the latest available versions.
* `--json`: Output the report as JSON to `stdout`.
* `-v, --verbose`: Enable verbose debug logging (written to `stderr`).
* `-h, --help`: Print help information.
* `-V, --version`: Print version information.

### Subcommands
* `tree`: List all dependencies with their current status.

### Exit Codes
`ud` returns the following exit codes:
* **`0`**: Success. All dependencies are up to date (or were successfully updated).
* **`1`**: Outdated dependencies detected (in check/dry-run mode).
* **`2`**: Hard error (file not found, parse failure, network/index fetch error).

---

## 💡 Worked Example

Assume you have a `Cargo.toml` with the following dependencies:
```toml
[dependencies]
serde = "1.0.0"       # compatible bump: 1.0.219 (latest: 1.0.219)
tokio = "1.36.0"      # current
tower = "^0.4.13"     # compatible bump: 0.4.15 (latest breaking: 0.5.2)
```

Running different `ud` commands results in the following behaviors:

* **`ud`** (Check Mode)
  * Outputs the status of each dependency to console.
  * Does not edit the file.
  * Exits with code `1` because updates are available.

* **`ud -u`** (Compatible Updates)
  * Bumps `serde` to `"1.0.219"`.
  * Bumps `tower` to `"0.4.15"`.
  * Leaves `tokio` unchanged.
  * Exits with code `0`.

* **`ud -u --allow-breaking`** (All Updates)
  * Bumps `serde` to `"1.0.219"`.
  * Bumps `tower` to `"0.5.2"` (a breaking update under semver since it's a `0.x` crate).
  * Leaves `tokio` unchanged.
  * Exits with code `0`.

---

## 📊 JSON Schema

When run with the `--json` option, `ud` prints a JSON object to `stdout` containing the complete dependency analysis report.

### Schema Structure
* `verdicts`: An array of tuples, where each tuple is `[Dependency, Verdict]`.
  * **Dependency**:
    * `coordinate`: Crate/package name (string).
    * `constraint`: The version constraint declared in the manifest (string).
    * `span`: Start and end byte indices in the file, if known (object or null).
    * `source_hint`: Registry or repository hint, if known (string or null).
    * `section`: Manifest section where it was declared (string or null).
  * **Verdict**:
    * `type`: One of `"Current"`, `"Outdated"`, `"Yanked"`, `"Unsatisfiable"`, or `"Errored"`.
    * Fields depend on the `type` tag (see example below).

### Example JSON Payload
```json
{
  "verdicts": [
    [
      {
        "coordinate": "serde",
        "constraint": "1.0.0",
        "span": { "start": 30, "end": 45 },
        "source_hint": null,
        "section": "dependencies"
      },
      {
        "type": "Outdated",
        "compatible": "1.0.219",
        "latest": "1.0.219",
        "latest_pre": null
      }
    ],
    [
      {
        "coordinate": "tokio",
        "constraint": "1.36.0",
        "span": { "start": 46, "end": 62 },
        "source_hint": null,
        "section": "dependencies"
      },
      {
        "type": "Current",
        "latest": "1.36.0",
        "latest_pre": "1.37.0-rc.1"
      }
    ],
    [
      {
        "coordinate": "tower",
        "constraint": "^0.4.13",
        "span": { "start": 63, "end": 79 },
        "source_hint": null,
        "section": "dependencies"
      },
      {
        "type": "Outdated",
        "compatible": "0.4.15",
        "latest": "0.5.2",
        "latest_pre": null
      }
    ]
  ]
}
```

---

## 🔒 Stability & Public API (v0.1.0+)

`ud` commits to a stable public surface to support scripting and tool integration.

### Frozen Surface
The following items are frozen and subject to strict semver compatibility policies (additive changes are minor/patch bumps, breaking changes or removals require a major version bump):
1. **The Universal CLI JSON Output:** The `--json` payload structure and the names/meanings of all fields.
2. **The Universal Data Model (Rust types):** `Coordinate`, `Constraint`, `Version`, `Span`, `Dependency`, `Availability`, `VersionMetadata`, `Verdict`, and `Report`.
3. **Core traits:** The `Ecosystem` and `Scheme` traits.

### Out of Scope / Unstable
The following components are implementation details and may change without warning:
* The internals of the dependency resolver.
* Formatting and colors of the human-readable CLI outputs.
* Sparse-index network protocols and transport details.

---

## 📄 License

This project is licensed under the **MIT License**. See the [LICENSE](./LICENSE) file for details.
