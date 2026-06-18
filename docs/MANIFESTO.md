# ud — Manifesto

> **ud** — _Up to Date._ The two-letter answer to "are my dependencies current?"

**Name:** `ud`. It's the command, the question, and the job. Type `ud` and
you're asking _"up to date?"_ — and because it also reads as **UpDate**, the
same two letters cover the fix. Two keystrokes, one idea, every ecosystem.

---

## The problem

[dependi](https://github.com/Yrrrrrf/dependi) is the best dependency-version
inspector we have, and it has one structural flaw that caps everything it can
become: **it only exists inside VS Code.** Its dependency logic isn't a library
an editor uses — the logic _is_ the editor integration. Its core type carries a
`vscode.Range`; its input is a `TextEditor`; its output is editor decorations.
There is no dependi that runs in a terminal, in CI, in a git hook, or in any
editor that isn't VS Code.

That one coupling produces every limit that matters: no CI gates, no scripting,
one editor, fragile string-based parsing, and the speed ceiling of a language
built for the browser. dependi answered "show me outdated dependencies" well —
then welded the answer to the one place you can't take it everywhere.

## The thesis

The problem is small and universally shaped: **read a manifest, ask a registry
what versions exist, compare under that ecosystem's rules, report.** That's a
library function, not an editor feature. Build it as a fast, headless tool and
every limit dissolves — CI and scripting come free, speed comes from the
runtime, and editors become _consumers_ of the tool instead of the only place it
can live.

`ud` is that tool.

## What `ud` is

A small, fast command that, pointed at any supported manifest, tells you what's
behind and can bring it current without disturbing your file:

```bash
$ ud                # Checks current directory, updates outdated versions automatically
$ ud --preview      # Or -y; dry-run mode, only shows what would change
$ ud tree           # Lists all dependencies, including up-to-date ones
$ ud path/to/dir    # Targets a specific directory or manifest
```

Example output:
```text
  serde 1.0.197 → 1.0.228
    Updated!
  tokio 1.36 → 1.52.4
    Updated!
  smallvec 1.13.2 → 2.0.0-alpha.12    # (in bold magenta)
    Updated!
```

First-class targets at launch: **Rust, npm/jsr, Python, Go.** But — and this is
the point — `ud` is built around an abstraction that treats those as the _first
four_ of _almost any_ package manager, not as four special cases.

## Principles

1. **Headless-first.** The core knows nothing about editors or terminals. The
   CLI is the first consumer; an editor integration or dashboard is a _later,
   separate_ consumer of the same core — never a dependency of it.
2. **The command is the question.** `ud` asks "up to date?" Everything the tool
   does is in service of answering that, fast and honestly.
3. **One canonical model, many providers.** Every package manager is translated
   into one shared vocabulary of "dependency." The core never learns ecosystem
   quirks; providers translate at the edge.
4. **A minimal plugin surface that scales to anything.** Adding a package
   manager is mostly two things — _read its manifest_ and _list a name's
   versions_. The rest has sane defaults you override only when an ecosystem is
   genuinely weird.
5. **Lossless by default.** Updating a version changes that version and nothing
   else — comments, ordering, whitespace survive byte-for-byte.
6. **Simple now, open-ended later.** Ship a tight CLI for four ecosystems today;
   design so a stranger can add a fifth — in their own language, without forking
   — in ten years.

## What `ud` is NOT

- **Not an editor extension.** Editor support is a downstream consumer enabled
  by the headless core — out of scope for v1.
- **Not a package manager or resolver.** It doesn't install, build, or solve a
  transitive graph. It reports and bumps _direct_ dependency versions.
- **Not a service.** No backend, no account, no telemetry. It's a binary.
- **Vulnerability scanning is deferred** — but reserved: an advisory feed is
  just another kind of "source" in the same model, addable later without
  redesign.

## Why it beats dependi

|                               | dependi                 | ud                                       |
| ----------------------------- | ----------------------- | ---------------------------------------- |
| Runs in CI / scripts          | ✗                       | ✓                                        |
| Editors supported             | 1 (VS Code)             | any, later, as consumers of the core     |
| Package managers              | a fixed set, hard-wired | a growing set behind one plugin contract |
| Third party adds an ecosystem | fork it                 | ship a provider                          |
| Parsing                       | hand-rolled strings     | format-aware, lossless edits             |
| Reusable as a library         | no (editor-coupled)     | yes (headless core)                      |

Same job dependi does well — done somewhere you can use it everywhere, and built
to outlive its own first four ecosystems.

---

_This manifesto is the **why**. The companion `ARCHITECTURE.md` is the **how** —
the core ideas of each part and the provider model that makes "almost any
package manager" a realistic claim._
