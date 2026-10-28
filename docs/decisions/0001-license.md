# ADR 0001: Project license

Status: **Proposed**. Needs the owner's decision (placeholder `[LICENSE]` in the brief).

## Context

The protocol specs were written after reading Legendary and its Heroic fork (both GPL-3.0)
and an unlicensed community documentation repo. Facts about a protocol aren't copyrightable,
and our code is written from our own specs, not translated. The brief offers GPL-3.0 or MIT.

## Options

- **GPL-3.0-or-later** (recommended). Zero doubt if any part of our code ends up structurally
  close to Legendary's (the download planner is the most likely place). Matches the ecosystem
  (Legendary, Heroic, Rare are all GPL). Downside: others can't fold our crates into closed software.
- **MIT**. Maximum reuse. Requires keeping the clean re-implementation discipline strictly forever,
  and any future contributor who ports Legendary code would create a licensing problem.

## Decision

Pending. Until decided: no `LICENSE` file, the crates use `license = "UNLICENSED"` placeholders,
and nothing is published.
