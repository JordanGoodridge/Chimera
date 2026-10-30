---
name: affected-tests
description: Run the tests a change can affect (the crates it touches and everything depending on them), on Linux or Windows, before committing or opening a PR.
---

# Run the affected tests

Which crates a change affects, and why:
```
cargo run -q -p repo_checks --bin affected -- --base origin/main   # committed changes on the branch
cargo run -q -p repo_checks --bin affected -- --staged             # what's staged
cargo run -q -p repo_checks --bin affected -- --help
```

Linux, their fast tier (what the pre-commit hook runs for staged changes):
```
cargo test $(cargo run -q -p repo_checks --bin affected -- --base origin/main --format cargo) \
    --features dev --no-fail-fast -- --skip heavy::
cargo test -p repo_checks
```
Everything including heavy tests: `cargo nextest run <same -p list> --features dev`.

Windows (PowerShell):
```
.\scripts\affected-tests.ps1 -Base origin/main       # committed changes
.\scripts\affected-tests.ps1 -Staged                 # staged changes
.\scripts\affected-tests.ps1 -Files path\to\file.rs  # specific files
.\scripts\affected-tests.ps1 -Base origin/main -Nextest
```

- One crate: `cargo test -p <crate> --features dev --test it <module>::`.
- Tests needing a game install skip without one; say so in the PR.
- Several agents on one Linux box: `source tools/agent-env.sh` and prefix
  cargo with `agent_run` (optional).
- After a push, `tools/post-push-tests.sh` runs the affected tests, heavy
  ones too (optional).
