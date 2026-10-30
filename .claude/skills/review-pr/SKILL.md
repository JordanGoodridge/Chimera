---
name: review-pr
description: Review a branch or PR against the project's rules before it is merged - identity, personal traces, the hard rules, tests, docs.
---

# Review a PR

Get the range: `git fetch origin && git log --format='%h %an <%ae> %ad' --date=format:%z origin/main..HEAD`.

Check each, and report what fails with the fix:

1. **Identity:** every commit's author and committer are
   `r0pi0x <339403489+r0pi0x@users.noreply.github.com>`, dates `+0000`.
2. **Personal traces:** no machine paths (`/home/<name>/`, `C:\Users\<name>`),
   real names or personal emails in messages or added lines:
   `git log -p origin/main..HEAD | grep -nE '^\+.*(/home/[^/ ]+/|/Users/|Users\\\\)'`.
3. **Hard rules (AGENTS.md):** no game assets, extracted files or keys
   (long hex runs); nothing from `chimera.local.toml`; behaviour of the
   original games traced to a spec in `specs/`, not invented or recalled;
   no third-party code; specs hold no code blocks.
4. **Rules as tests:** `cargo test -p repo_checks` passes (layering,
   assets, keys, specs, doc links, generated docs, ECS ratchets).
5. **Tests:** new behaviour has a scenario test in the right crate
   (new-scenario-test skill); the affected fast tier passes
   (affected-tests skill). Ratchets lowered, never raised without reason.
6. **Design:** parity by default, our additions behind an option; data
   over hard-coded Rust; ECS style (DESIGN.md "ECS style"): no new
   single-slot hook resources, no new whole-world systems per tick
   without a reason; Linux and Windows both (paths, line endings).
7. **Docs true:** changed facts updated in the same PR; `//!` docs
   edited and `cargo run -q -p repo_checks --bin docs_index` run;
   plans/backlog items marked done.
8. **Visible changes** come with before/after screenshots (not committed).
