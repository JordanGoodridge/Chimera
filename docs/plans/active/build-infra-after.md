# Build-infra benchmark: after

Same machine, toolchain, slot and scenarios as
[build-infra-before.md](build-infra-before.md). This report is filled as
the changes land; rows measured under a different machine load say so.

What changed before these runs:

- `slots.ps1` discovers every `slotN` on disk (not just `1..-Count`) and
  has a `seed` action that copies a warm `target\` into a new slot.
- `scripts/affected-tests.ps1` selects the packages a change can affect
  (`repo_checks affected`) instead of building the whole workspace.
- `scripts/build-bench.ps1` is the shared harness for both reports.

| label | scenario | seconds | compiled units | jobs | exit |
|---|---|---|---|---|---|

## What the numbers mean

Filled from the runs: the new-session result is the seeded slot against
the before report's cold slot, and the daily-edit result compares the
full workspace build with the affected selection on the same warm
target. Absolute seconds move with the other agents' builds; the
before/after pairs were taken back to back, and the compiled-unit
column is load-independent.
