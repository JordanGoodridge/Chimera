# Build-infra benchmark: before

Measured before the changes in `docs/plans/active/build-infra.md` on the
Windows PC, in a fresh slot with an empty `target\`.

- Machine: Ryzen 7 7800X3D (8 cores / 16 threads), 32 GB, NVMe.
- Toolchain: `cargo 1.99.1`, `rustc 1.99.1`, `rust-lld`
  (`.cargo/config.toml`).
- Slot: 5, branch `build-infra`, empty `target\`.
- Command: `scripts\build-bench.ps1 <scenario> -Label before -Jobs 8`.
- Load: another agent's build was running (16 `rustc` processes, CPU at
  100% for the first scenarios). Read the ratios, and the alternating
  leaf-full/leaf-affected rows in the after report, more than the
  absolute seconds.
- Edits are one appended comment line in the named file, restored when
  the scenario ends. `affected` selections use `--files`, so the edit is
  visible to `git diff` without staging.

| label | scenario | seconds | compiled units | jobs | exit |
|---|---|---|---|---|---|
| before | cold | 1,225.4 | 584 | 8 | 0 |
| before | noop | 0.8 | 0 | 8 | 0 |
| before | fast-tier | 120.0 | 0 | 8 | 101 |
| before | leaf-full | 41.6 | 20 | 8 | 0 |
| before | leaf-direct | 10.0 | 16 | 8 | 0 |
| before | bottom-full | 43.4 | 21 | 8 | 0 |
