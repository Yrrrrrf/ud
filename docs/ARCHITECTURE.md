# ud — Architecture

> Updated to match the v0.0.3 implementation. This is the living architecture reference: the universal model, the plugin contract, and — newly — the **low-level mechanics** of how each part actually works in the code. It also states, honestly, what is **implemented** versus still **planned**, so the document stops running ahead of the binary.
>
> Companion to `MANIFESTO.md` (the why) and the `vN` plan docs (the how-we-got-here). Stable as of the 0.1.0 freeze; see §10.

---

## 0. The one idea

Every "is my dependency current?" question — crates, npm, PyPI, Go, container images, system packages — has the same shape: a manifest declares dependencies, a source knows which versions exist, a scheme says which is newer, and you compare. `ud` models that shape once in the core and lets each package manager plug in by answering a few fixed questions. The core never learns any specific ecosystem; ecosystems never learn about each other. The contract between them is small and is the only thing meant to survive long-term.

Today that contract has exactly one implementation (Cargo). That's deliberate: prove the shape on one vertical, freeze it, then add more.

---

## 1. The universal model (as implemented)

The core speaks one vocabulary; plugins translate into it. These are the real types (`core/model.rs`), all `Serialize` — which matters, because **the serialized form is the `--json` schema and part of the frozen public surface (§10).**

```
Coordinate(String)        // package name, e.g. "serde"
Constraint(String)        // what the manifest asks for, e.g. "1.36", "^1.2"
Version(String)           // a concrete release, e.g. "1.45.0" — opaque; only compared via a Scheme
Span { start, end }       // byte offsets into the manifest (from toml_edit)

Dependency {
    coordinate, constraint,
    span:        Option<Span>,     // where it sits in the file
    source_hint: Option<String>,
    section:     Option<String>,   // which TOML section — used to target writes precisely (§4.3)
}

Availability { versions: Vec<VersionMetadata> }
VersionMetadata { version, yanked: bool, prerelease: bool }

Verdict =
  | Current      { latest, latest_pre: Option<Version> }
  | Outdated     { compatible: Option<Version>, latest: Version, latest_pre: Option<Version> }
  | Yanked
  | Unsatisfiable { constraint }
  | Errored(String)

Report { verdicts: Vec<(Dependency, Verdict)> }
```

The newtypes (`Coordinate`/`Constraint`/`Version`) use `derive_more` for `Display`/`From`/`Into` to stay boilerplate-free. `Version` is intentionally a string the core never inspects directly — it is only ever **compared** or **constraint-tested** through a `Scheme` (§4.5), which is what lets `ud` handle version flavors it doesn't hardcode.

**The key v0.0.3 evolution:** `Outdated` carries *both* `compatible` (latest version still satisfying the declared constraint — the safe bump) and `latest` (absolute newest — possibly a breaking jump). The CLI applies `compatible` by default and `latest` only under `--allow-breaking`. `latest_pre` surfaces a newer prerelease without acting on it.

---

## 2. The plugin contract (as implemented)

A plugin implements `Ecosystem` (`core/contract.rs`). Four required questions, two optional write capabilities expressed as default methods:

```
trait Ecosystem: Send + Sync {
    fn name(&self) -> &'static str;
    async fn detect(&self, path, content: Option<&str>) -> bool;          // 1. is this mine?
    async fn read(&self, content) -> Result<Vec<Dependency>>;             // 2. what's declared?
    async fn source(&self, coordinate) -> Result<Availability>;           // 3. what versions exist?
    fn scheme(&self) -> &dyn Scheme;                                      // 4. how do versions order?

    async fn write(&self, content, dep, version) -> Result<String>;       // optional — default: Err("unsupported")
    async fn write_batch(&self, content, edits) -> Result<String>;        // optional — default: write() in a loop
}

trait Scheme: Send + Sync {
    fn is_newer(&self, a, b) -> bool;
    fn satisfies(&self, version, constraint) -> bool;
}
```

**Capability handling is lightweight, not a formal capability enum.** "Optional" simply means a default method that errors (`write`) or falls back (`write_batch` defaults to calling `write` per edit). An ecosystem that can't safely rewrite just doesn't override `write`, and the core degrades gracefully — `--update` skips it. Async is provided by `async-trait` (boxed futures; fine for a CLI doing tens of fetches). Errors are `miette::Result` throughout.

This trait is the heart of the system and the stable seam. The whole 10-year extensibility story (§9) is "keep this small and don't break it."

---

## 3. The pipeline (as implemented)

`Pipeline` (`core/pipeline.rs`) holds the registered ecosystems and an `include_prerelease` flag, and orchestrates the run:

```
detect → read → (source ∥ resolve, bounded-concurrent) → Report → render / write
```

1. **Detect** — read the file once, ask each ecosystem `detect()`, take the first match (deterministic). No match → error.
2. **Read** — the matched ecosystem parses the manifest into `Dependency[]`.
3. **Resolve, concurrently** — for each dependency, `source()` then `resolve()`, run through `futures::stream::buffer_unordered(MAX_CONCURRENT_REQUESTS)` (= 12). A `source()` failure becomes a per-dependency `Verdict::Errored` — **one bad fetch never crashes the run** (failure isolation).
4. **Report** — collect `(Dependency, Verdict)` pairs.
5. **Render or write** — the CLI renders the report, and in `--update` mode applies edits (§4.3, §5).

The core owns orchestration, concurrency, and error isolation. Rendering and version policy live in their own modules so the pipeline stays about flow, not presentation or judgment.

---

## 4. Component mechanics (the low-level layer)

### 4.1 Detection (`ecosystems/cargo.rs`)
Filename match: `path.file_name() == "Cargo.toml"`. The contract allows a content peek (`content: Option<&str>`) for ambiguous formats; Cargo doesn't need it yet. First-match wins at the pipeline level.

### 4.2 Parser — Read (`toml_edit`)
The manifest is parsed into a `toml_edit::DocumentMut` — an *editable* document that retains formatting, not a stripped data object. `parse_table` walks each dependency section and, per entry, extracts: the coordinate (key), the constraint (either a bare string `serde = "1"` or the `version` field of an inline/sub table), the byte `span`, and a **section label**. Sections read: `dependencies`, `dev-dependencies`, `build-dependencies`, `workspace.dependencies`, and every `target.<cfg>.<kind>`. Git/path deps with no `version` are skipped.

### 4.3 Writer — Write / Write_batch (the lossless core)
This is where "lossless" is earned. `update_table_item` handles all three Cargo forms — bare value, inline table, and table-like `[dependencies.x]` — and in each case **clones the existing `decor`** (the surrounding whitespace and comments) before swapping the version value and restoring the decor. Result: the version changes and *every other byte is preserved*.

`apply_edit` routes an edit to the right place using the stored `section`: root sections and `workspace.dependencies` are targeted precisely by section + coordinate. `write_batch` applies **all** edits to a single `DocumentMut` and serializes once — so an update is one read and one write, and a coordinate appearing in two sections updates each independently (the v0.0.3 duplicate-coordinate fix). Writes are idempotent.

> **Known residual (slated for v0.0.4):** the `target.*` branch of `apply_edit` still iterates all target blocks rather than honoring the exact `target.<key>.<kind>` it stored — the same fan-out bug already fixed for root sections, in a rarer corner.

### 4.4 Source — Fetch (crates.io sparse index)
`source()` queries the **sparse index** (`index.crates.io`), not the rate-limited JSON API. `sparse_index_path` implements the index's prefix bucketing: 1-char → `1/{name}`, 2-char → `2/{name}`, 3-char → `3/{c0}/{name}`, else `{c0c1}/{c2c3}/{name}`, lowercased. The response is newline-delimited JSON; each line deserializes to `{ vers, yanked }`, and `prerelease` is derived by parsing `vers` and checking for a pre-release component. `404` → empty `Availability`; other non-2xx → error. The HTTP client sets a real User-Agent, and the base URL is injectable (`with_base_url`) so the fetch path can be exercised against a mock without touching the network.

### 4.5 Version scheme — Compare (`SemverScheme`)
The only scheme today. `is_newer` parses both sides as `semver::Version` (unparseable → `false`). `satisfies` parses the constraint as a `semver::VersionReq` and calls `matches`. The crucial helper is `declared_version`: it parses the constraint as a `VersionReq`, takes the first non-`Less` comparator, and pads missing minor/patch with `0` — turning `"1.36"` into `1.36.0`, `"^1.2"` into `1.2.0`, `">=1, <2"` into `1.0.0`. This is what lets partial versions and ranges become a single comparable "declared" version. Because the core only ever calls `is_newer`/`satisfies`, no version logic is hardcoded outside a `Scheme` — a future ecosystem with different rules supplies its own.

### 4.6 Resolver (`core/resolve.rs`)
Given a dependency, its availability, the scheme, and the prerelease flag, `resolve` produces a `Verdict`. It filters yanked (always) and prerelease (unless opted in), sorts candidates with a **total order** (parseable by semver order, unparseable pushed to the bottom — no inconsistent comparator), then computes: `latest` (max), `compatible` (newest that `satisfies` the constraint), and `latest_pre` (newest prerelease above `latest`). If the declared version is older than `latest` → `Outdated { compatible, latest, latest_pre }`, else `Current`. **All version policy lives here, once, for every ecosystem.**

### 4.7 Reporter (`core/report.rs`)
Two reporters consume only a `Report`: `HumanReporter` (colorized via `owo-colors`, with the changed portion of the old constraint diff-highlighted, plus a `tree` mode listing every dep) and `JsonReporter` (`serde_json` pretty-print of the `Report`). Rendering is core and plugin-agnostic — `main` wires, it does not present.

### 4.8 Runtime (`core/runtime.rs`)
Holds `MAX_CONCURRENT_REQUESTS` (12) and `init_tracing` (a `tracing-subscriber` `fmt` layer, `env-filter`, stderr, off unless `-v`). The pipeline and ecosystems emit `tracing::debug!` at the useful points (detected ecosystem, dep count, fetch URL, applied edit).

---

## 5. CLI surface (`main.rs`)

Flags: `path` (file or dir; dir resolves to `Cargo.toml`), `-u/--update` (apply **compatible** bumps), `--allow-breaking` (also apply `latest`), `--pre` (include prereleases), `--json`, `-v/--verbose`; plus a `tree` subcommand. **Default is check-only — no flag mutates your file.** Update mode collects all chosen edits and calls `write_batch` once. Exit codes: `0` current / `1` outdated (check mode) / `2` hard error; errors print as a single human line, not a stack.

---

## 6. Cross-cutting concerns

- **Concurrency:** bounded fan-out at 12 via `buffer_unordered` — fast without thundering-herding the index.
- **Failure isolation:** per-dependency `Errored` verdicts; a single failure is contained.
- **Observability:** `tracing` behind `-v`.
- **Errors:** `miette` everywhere; friendly single-line messages at the binary boundary.
- **Caching:** none yet (runs are short) — an additive optimization later.

---

## 7. Logical layout

```
core/
  model        the universal vocabulary (§1) — the JSON schema lives here
  contract     the Ecosystem + Scheme traits (§2) — the stable seam
  pipeline     detect → read → resolve orchestration + concurrency (§3)
  resolve      SemverScheme + declared_version + verdict policy (§4.5–4.6)
  report       Human + Json reporters (§4.7)
  runtime      concurrency constant + tracing init (§4.8)
ecosystems/
  cargo        detect + read + source(sparse index) + write/write_batch + SemverScheme
main.rs        CLI wiring, flags, exit codes, single-pass update
```

Rule (to be enforced as a fitness test): `core` knows no concrete ecosystem; ecosystems don't know each other; dependencies point inward toward `model` + `contract`.

---

## 8. Implemented vs planned (the honest boundary)

**Implemented (v0.0.3):** the Cargo ecosystem end-to-end; sparse-index fetch with bucketing; lossless, section-targeted, single-pass writes; the compatible-vs-breaking resolver with prerelease policy; bounded-concurrent pipeline with per-dep failure isolation; human + JSON reporters; safe (check-by-default) CLI with `--update`/`--allow-breaking`.

**Planned (not yet built):**
- **Registration is manual today** — `pipeline.register(Box::new(CargoEcosystem::new()))` in `main`. Automatic/compile-time discovery is a future convenience, *not* the current state. (Earlier drafts of this doc described auto-registration as if it existed; it does not.)
- **One scheme only** — `SemverScheme`. The `Scheme` trait allows per-ecosystem overrides, but nothing exercises that path yet.
- **HTTP-only sources** — the "transport-agnostic, incl. subprocess" idea (below) is design intent; the only transport implemented is `reqwest` against an HTTP index.
- **A second ecosystem** (npm/PyPI/Go), **vulnerability scanning**, **caching**, **dynamic/sandboxed plugins**, and an **editor/LSP consumer** are all future.

---

## 9. Extensibility & the long horizon

The vision the contract is built for, with current status flagged:

- **Transport-agnostic sources.** A `source()` could sit on HTTP, a static index, a git repo, an image registry, *or a subprocess driving a package manager's own CLI* — the subprocess hatch is what would let `ud` support ecosystems with no machine API. *Status: HTTP-only today; the contract doesn't preclude the rest.*
- **Plugin hosting, three stages.** (1) **In-tree, manually registered** — where we are now. (2) **Dynamic discovery** — plugins enabled by config without a rebuild. (3) **Out-of-process / sandboxed** — third parties ship a plugin without forking `ud` or being trusted with full access; the contract is small enough to cross that boundary. Each stage is additive; none changes the trait.
- **Version schemes** beyond SemVer (PEP 440, Go pseudo-versions, OCI tags, Debian epochs) drop in behind `Scheme` without touching the core.

The point of freezing the contract now (§10) is precisely that all of the above can arrive later without reopening it.

---

## 10. The stable surface (frozen at 0.1.0)

Most things are free to churn — resolver internals, the sparse-index path logic, reporter formatting, transports. **Three things are the public API and are frozen:**

1. the **universal model** (§1),
2. the **`Ecosystem`** and **`Scheme`** traits (§2),
3. the **`--json` output schema** (the serialized `Report`/`Verdict`).

Compatibility policy: additive changes (new fields, new `Verdict` variants) are non-breaking; renames or removals are breaking and bump the version. These three are small on purpose — staying compatible should be cheap, so that package managers nobody has written yet can still fit without a rewrite. That is the entire reason the abstraction exists.

---

*The shape held across two releases of real code. v0.0.4 freezes and ships it; the road to 1.0 is the second ecosystem that proves the contract generalizes beyond Cargo.*