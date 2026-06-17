# ud — Architecture

> The **ideas**, not the libraries. This document is deliberately
> language-agnostic: it describes the contracts, the component responsibilities,
> and the abstraction that lets `ud` extend to _any_ package manager. Concrete
> technology choices (which parser, which HTTP client) live in a separate build
> plan — they are implementation details below this line, chosen later,
> swappable forever.
>
> Companion to `MANIFESTO.md`. Simple for v1; designed for a ten-year horizon.

---

## 0. The one idea

Every "is my dependency current?" question — for crates, npm packages, Python
wheels, Go modules, Docker images, Debian packages, anything — is the _same
shape_: a manifest declares dependencies, a source knows which versions exist, a
scheme says which is newer, and you compare. `ud`'s entire architecture is the
decision to **model that shape once, in the core, and let each package manager
plug into it** by answering a few fixed questions. The core never learns about
any specific ecosystem. Ecosystems never learn about each other. The contract
between them is small, and it is the only thing that must survive ten years.

---

## 1. The universal model (the lingua franca)

The core speaks exactly one vocabulary. Every plugin translates its world into
these terms; the core manipulates nothing else. This translation layer is the
decoupling that makes everything else possible.

- **Coordinate** — what uniquely names a thing to be versioned (a package name,
  possibly with a namespace/registry qualifier). `serde`, `@scope/pkg`,
  `github.com/x/y`, `docker.io/library/redis`.
- **Constraint** — what the manifest _asks for_: a pin, a range, a caret,
  "latest," a tag. Opaque to the core except through the scheme.
- **Version** — a single concrete release. Opaque to the core: it is never
  inspected directly, only **compared** and **tested against constraints**
  through its scheme.
- **Manifest** — a parsed document plus the knowledge of where each dependency
  physically sits in it (its span), so edits can be surgical.
- **Dependency** — one declared requirement:
  `{ coordinate, constraint, span, source-hint }`.
- **Availability** — what a source returns for a coordinate: the set of known
  versions plus metadata (yanked, prerelease, published-at).
- **Verdict** — the core's judgment per dependency: _current_, _outdated (→
  target)_, _yanked_, _unsatisfiable_, or _errored_.
- **Report** — the collected verdicts, ready to render or to drive edits.

**Why this matters at year 10:** new package managers differ wildly in _format_
and _protocol_ but almost never in this _shape_. Freeze this vocabulary and the
strangest future ecosystem still fits — it just brings its own translator.

---

## 2. The pipeline (core-owned, plugin-agnostic)

```
   path ─▶ DETECT ─▶ READ(parse) ─▶ RESOLVE ─▶ REPORT / WRITE
                          │            │  │
                          │      ┌─────┘  └─────┐
                          │   FETCH(source)   COMPARE(scheme)
                          ▼
                  Dependency[]  ───────────────▶  Verdict[]

   ── everything in this row is the core; the boxes below are plugin-supplied ──
```

The core owns orchestration, concurrency, caching, error isolation,
configuration, and rendering. A plugin is invoked only to answer
ecosystem-specific questions. The core could run with zero plugins and do
nothing useful; a plugin in isolation is inert. Value lives at the seam — which
is exactly why the seam is the design.

---

## 3. The plugin contract (the heart of the system)

A plugin is an **Ecosystem provider**. It answers four questions and _declares
what it can do_. This is the entire public API — small on purpose, because it is
the one thing a third party depends on and the one thing we promise not to
break.

1. **Detect** — _"Is this manifest mine?"_ Given a path (and cheaply, a peek at
   content), claim it or pass.
2. **Read** — _"What does this manifest declare?"_ Produce `Dependency[]` with
   spans. (The parser.)
3. **Source** — _"What versions exist for this coordinate?"_ Produce
   `Availability`. (The fetcher.)
4. **Scheme** — _"How do versions order and satisfy constraints?"_ Usually
   delegated to a shared default; overridden when the ecosystem's version rules
   differ.

Plus optional, **declared capabilities**:

- **Write** — _"Set this dependency's version in the manifest, losslessly."_
  (Enables `--update`.)
- **Vulnerabilities** — _"Are any of these versions known-bad?"_ (Deferred
  feature; same shape.)
- **Lockfile-aware**, **workspace-aware**, etc. — future capabilities, added
  without breaking older plugins.

**Capability declaration is the trick that lets _any_ package manager fit.** A
read-only ecosystem (one you can inspect but shouldn't rewrite) simply doesn't
claim _Write_; the core degrades gracefully and `--update` skips it instead of
failing. A package manager with no public API still satisfies _Source_ — see
§4.3. The contract never assumes more than the weakest reasonable ecosystem can
provide.

---

## 4. Component ideas (what each part _is_)

### 4.1 Detection

Cheap and layered: filename match first, then a light content sniff for
ambiguous cases (`*.toml` could be several things), then an explicit user
override that always wins. When two plugins claim a file, resolution is
deterministic and configurable. Detection is pure and side-effect-free so it can
be run speculatively across a whole tree.

### 4.2 Parser architecture (Read + Write)

The parser is **bidirectional and span-preserving**, and that single decision
drives its whole design:

- **It separates structure from values.** It parses to an _editable
  representation_ of the document — not a stripped data object — so it knows
  both _what_ is declared and _exactly where_.
- **Read** projects that representation into the universal `Dependency[]`.
- **Write** (a capability) mutates one node and re-serializes, leaving every
  other byte — comments, ordering, whitespace, trailing commas — untouched.
  Lossless editing is therefore not a separate feature bolted on; it is the
  parser refusing to throw information away in the first place.
- **It prefers a real grammar over pattern-matching.** Guessing with regex is
  how the prior art accumulated a graveyard of edge-case bugs. A format with a
  real grammar gets a real parser; only formats with no grammar get a
  hand-written one, and even then it tracks spans.
- **It is tolerant.** Comments, unusual-but-valid formatting, and partial files
  degrade to per-line errors, never a crashed run.

The core never sees the format. It sees `Dependency[]` in and a rewritten
document out. Two ecosystems sharing a format (e.g. two tools both using the
same manifest language) can share a parser core and differ only in _which
sections_ hold dependencies.

### 4.3 Source architecture (Fetch)

A source answers `coordinate → Availability`, and the deliberate generality here
is what unlocks "any package manager":

- **Transport-agnostic.** Behind a source might be an HTTP registry, a static
  index file, a git repository, an image registry, _or a subprocess that drives
  the package manager's own CLI_. The core asks for versions; it does not care
  how they arrive.
- **The subprocess escape hatch is load-bearing.** Many ecosystems (system
  packages, some private registries) have no clean machine API — but they all
  have a CLI that can answer "what versions of X exist." Allowing a source to
  shell out to that tool means `ud` can support ecosystems that were never
  designed to be queried by a third party. This is the difference between "four
  languages" and "anything."
- **It reports metadata, not judgments.** Yanked, prerelease, published-at come
  back as facts; deciding what to _do_ with them is the resolver's job, kept out
  of every individual source.
- **It is the natural cache boundary and the natural concurrency unit** (see
  §5).
- **It is the natural wasm/embedding seam.** The day `ud`'s core runs somewhere
  without native networking, only the source's transport is swapped — the
  contract above it is unchanged.

### 4.4 Version scheme (Compare)

Versions are **opaque** to the core. A scheme provides just two pure functions
conceptually: _order two versions_, and _does this version satisfy this
constraint_. The default is the most common scheme (semantic versioning); an
ecosystem whose rules differ — date-based versions, epochs, image tags,
language-specific pre-release rules — supplies its own. Because the core only
ever calls "newer?" and "satisfies?", it can compare apples it has never seen,
as long as the apple brought its own comparator. **No version logic is ever
hardcoded in the core.**

### 4.5 Resolver (core)

Given a `Dependency`, its `Availability`, and a `Scheme`, the resolver produces
a `Verdict`. **All policy lives here, once, for every ecosystem:** whether to
consider prereleases, whether "latest" means "latest within your constraint" or
"latest that exists," how to treat yanked targets, how to rank a same-major bump
versus a major jump. Centralizing policy means a behavior change ships
everywhere at once and no plugin can quietly disagree.

### 4.6 Reporter & Writer (core)

Rendering (human table, colorized, or machine `--json`) is core and
plugin-agnostic — it consumes only `Report`. Writing orchestrates the parser's
_Write_ capability across the manifest, applying every approved bump in one
pass, and verifies the result still parses. Output modes and exit codes are
designed for both a human at a terminal and a CI job gating a merge.

---

## 5. Cross-cutting concerns (solved once, for all plugins)

- **Concurrency.** Sources are fetched in parallel under a bounded cap; the core
  schedules, plugins stay simple.
- **Caching.** Keyed by coordinate→availability, aware of the transport's own
  freshness signals; persistence is optional and additive.
- **Failure isolation.** _Critical for a tool that will host many plugins._ One
  dependency's network error, one malformed entry, one flaky source becomes a
  per-dependency _errored_ verdict — never a crashed run. A bad plugin can fail
  its own ecosystem and nothing else.
- **Configuration & logging.** A single config surface and a single
  verbose/trace mechanism, shared by all plugins, so the tool behaves
  consistently no matter which ecosystem is in play.

---

## 6. Registration & discovery — the ten-year axis

This is where "simple now" and "anything later" are reconciled. The _contract_
(§3) never changes; only _how plugins are hosted_ evolves:

- **Stage 1 — in-tree, compile-time (v1).** Plugins ship with the binary and
  register themselves automatically; there is no central list to maintain.
  Adding one is a local, self-contained change. Simplest possible thing that
  works.
- **Stage 2 — dynamic discovery.** Plugins declared by configuration and loaded
  at startup, so an ecosystem can be enabled/disabled without rebuilding. The
  plugins themselves are unchanged — same contract.
- **Stage 3 — out-of-process / sandboxed plugins (the endgame).** Because the
  contract is only four small questions, it fits across a process or sandbox
  boundary. Third parties can then ship a `ud` plugin for their package manager
  **without forking `ud` and without `ud` having to trust their code with full
  access.** This is the moment "any package manager" becomes a community reality
  rather than a maintainer's to-do list.

Capability negotiation (§3) is what lets these heterogeneous plugins coexist: a
sandboxed third-party plugin that can only _Read_ and _Source_ slots in beside a
built-in that can also _Write_, and the core simply offers each user the actions
their installed plugins actually support.

---

## 7. Design Decisions (the decisions that shape the abstraction)

```
DECISION: CLI Default Behavior
  A. Report-only by default (safe)
  B. Update by default (active)
CHOSEN: B.
REASON: The name 'ud' (UpDate) implies action. Users want to get to 'current' in the fewest keystrokes.
        Lossless editing (§4.2) makes active-by-default safe enough to be the primary UX.
```

```
DECISION: Visibility of non-outdated dependencies
  A. Always show everything (noisy)
  B. Only show changes (high-signal)
CHOSEN: B for default; A via a dedicated 'tree' command.
REASON: Most runs are "what do I need to fix?". A tree view is useful for inspection but shouldn't 
        clutter the primary 'ud' check.
```

```
DECISION: Pre-release Visibility
  A. Treat same as stable
  B. Distinct styling (Purple/Magenta)
CHOSEN: B.
REASON: Moving to a pre-release (alpha/beta/rc) is a higher-risk decision than a stable bump. 
        Visual distinction warns the user without needing extra text labels.
```

```
DECISION: How thick is the plugin contract?
  A. One bundled provider answering all four questions   — fewer moving parts; cohesive per ecosystem.
  B. Four independently-implemented ports                — maximal reuse; more wiring.
CHOSEN: A, with the four questions as named responsibilities inside it.
REASON: A new ecosystem author thinks in one unit ("here is my package manager"), but the four
        responsibilities stay nameable so two ecosystems can still share, say, a parser.
REVISIT IF: real-world reuse pressure (shared parsers/sources) makes B clearly cheaper.
```

```
DECISION: Is the version scheme always pluggable, or a shared default?
  A. Shared default (SemVer), override per ecosystem   — most plugins write nothing.
  B. Always explicit                                   — uniform, but boilerplate everywhere.
CHOSEN: A.
REASON: Most ecosystems are SemVer-shaped; charging every plugin for the exceptions is a tax.
        Opaque comparison keeps the core honest regardless.
REVISIT IF: the "default" starts accreting special-cases — that means it was never really default.
```

```
DECISION: What transports may a Source use?
  A. HTTP only                          — simple; excludes whole classes of package manager.
  B. Any transport, incl. subprocess    — supports ecosystems with no API; more surface.
CHOSEN: B.
REASON: The subprocess hatch is the single thing that turns "four languages" into "anything."
        Worth the extra surface; it is opt-in per plugin.
REVISIT IF: subprocess sources prove too slow/unsafe in practice → constrain, don't remove.
```

```
DECISION: Plugin hosting model.
  A. In-tree compile-time only          — simplest; third parties must fork.
  B. Dynamic discovery                  — flexible; still trusts plugin code fully.
  C. Sandboxed out-of-process           — third-party-safe; most complex.
CHOSEN: A for v1, with the contract shaped so B then C are additive, never rewrites.
REASON: YAGNI now; but the contract is intentionally small enough to cross an IPC/sandbox
        boundary, so the ten-year path is unlocked without paying for it today.
REVISIT IF: third-party ecosystems are demanded before C exists → that is the trigger to build it.
```

```
DECISION: Manifest editing — lossless or regenerate?
  A. Regenerate from a data model       — easy; destroys formatting & comments.
  B. Lossless surgical edit             — preserves the file; needs span-tracking parsers.
CHOSEN: B.
REASON: People do not accept a tool that reformats their manifest. Non-negotiable.
REVISIT IF: never, for editing. (A is acceptable only for read-only reporting.)
```

```
DECISION: Failure model.
  A. Fail-fast on first error           — simple; brittle as plugins multiply.
  B. Isolate per dependency / per plugin — resilient; needs disciplined error typing.
CHOSEN: B.
REASON: A tool meant to host many ecosystems must treat partial failure as normal, not fatal.
REVISIT IF: never; this only gets more important with scale.
```

---

## 8. Logical layout (responsibilities, not files)

A conceptual map — names are roles, not directories or modules in any language:

```
core/
  model        the universal vocabulary (§1) — depended on by everything, depends on nothing
  contract     the plugin contract (§3) — the stable public API
  pipeline     detect → read → resolve → report/write orchestration
  resolve      verdict policy (§4.5)
  report       rendering + write orchestration (§4.6)
  runtime      concurrency, cache, failure isolation, config, logging (§5)
ecosystems/
  <each plugin>  detect + read(+write) + source + scheme, self-contained
io/
  transports   http / index / git / oci / subprocess — chosen by sources, hidden from core
```

The one hard rule, enforced as a test (§9): **`core` knows no ecosystem, and no
ecosystem knows another.** Dependencies point inward toward `model` and
`contract` only.

---

## 9. Validation strategy

- **Contract conformance suite.** A single suite every plugin must pass: given
  fixture manifests, does it detect, read the expected dependencies with correct
  spans, write losslessly, and order versions correctly? Passing it _is_ the
  definition of "a valid `ud` plugin" — including future third-party ones.
- **Fitness functions** (run as ordinary tests): the core imports no ecosystem;
  no ecosystem imports another; every registered plugin round-trips its own
  fixtures.
- **Per-component:** parsers get snapshot tests seeded with known historical
  edge cases; sources get tests against _recorded_ responses (never live
  networks in CI); the resolver and schemes get golden ordering/verdict tables.
- **End-to-end:** run the tool against fixture project trees and assert output,
  `--json` shape, and exit codes.

---

## 10. The ten-year invariants

Almost everything is replaceable — parsers, transports, the cache, the CLI
itself. **Two things must not break:** the **universal model** (§1) and the
**plugin contract** (§3). They are the public API of the whole system. Treat
every change to them as a breaking change with a real cost. Keep them small
precisely so that staying compatible is cheap. If those two hold, `ud` can
absorb package managers nobody has invented yet without a rewrite — which is the
entire point.

---

## 11. Phased plan (simple now → general later)

**P0 — Skeleton.** [DONE] Model, contract, pipeline, one in-tree ecosystem registering
itself; detection works.

**P1 — First vertical.** [DONE] One full ecosystem end-to-end: read → source (real Crates.io) → compare
→ table report. Lossless surgical updates for strings, inline tables, and full tables. CLI with 'tree' and 'preview' modes.

**P2 — Prove the contract.** A _second_, deliberately different ecosystem.
**Freeze the contract here** — the second plugin is where wrong assumptions
surface, and the cheapest place to fix them. _Done when:_ identical UX for a
different package manager with no core changes.

**P3 — Breadth + a stress test.** Remaining launch ecosystems, plus one that
exercises the _edges_ of the contract — a read-only or subprocess-sourced one
(e.g. container images or a system package manager) to prove capability
negotiation and the subprocess transport are real, not theoretical. _Done when:_
a heterogeneous mix of ecosystems coexists.

**P4 — Write-back & CI.** The _Write_ capability (lossless `--update`), machine
output, CI-friendly exit codes. _Done when:_ updates rewrite manifests
byte-faithfully and a CI job can gate on staleness.

**P5 — Extensibility & beyond (the ten-year work).** Dynamic discovery, then
sandboxed third-party plugins; the vulnerability capability; an editor/LSP
consumer of the same core. _Done when:_ someone outside the project ships a
plugin without forking it.

---

## 12. Assumptions & risks

- `[ASSUMPTION]` v1 ships four in-tree ecosystems (Rust, npm, PyPI, Go);
  everything past that is additive via the unchanged contract.
- `[ASSUMPTION]` Most ecosystems are SemVer-shaped; the shared default scheme
  earns its keep.
- `[ASSUMPTION]` "Outdated" means newest non-yanked, constraint-aware version;
  prereleases are opt-in.
- `[ASSUMPTION]` No persistent cache in v1 (runs are short); it's an additive
  optimization later.
- `[REVISIT]` The contract's shape is the highest-stakes decision — validate it
  against a genuinely odd ecosystem (container images / system packages)
  _before_ declaring it stable in P2/P3.
- `[REVISIT]` Subprocess sources are powerful but introduce latency and trust
  questions — prove one early.
- `[HIGH RISK]` Breaking the model or contract after third-party plugins exist
  is the one expensive mistake. Keep both minimal so compatibility stays cheap.

---

## 13. Out of scope (for now)

Editor/LSP integration · wasm build · vulnerability scanning · private-registry
auth UX · lockfile resolution · transitive dependency graphs · installing or
building packages.

_Each is shaped to fit the existing contract later — none requires redesigning
it._
