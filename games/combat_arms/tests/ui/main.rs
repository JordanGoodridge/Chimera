//! `combat_arms`' integration tests of its front end (feature `ui`), as
//! one test binary (each file a module): the HUD's behaviour and the
//! scenario tests that read its state (`ui::hud::CaHud`, headless, on
//! `combat_arms::harness::CaSim` and the greybox, as tests/it/'s); the UI
//! system's layout, text, input and in-game menu (`ui_system`); the
//! first-person camera's rise and jitter (`camera`).
//! `cargo test -p combat_arms --features dev,ui --test ui`; where tests
//! go: docs/OBSERVABILITY.md, section 1.

mod attachments;
mod camera;
mod hud;
mod net_hud;
mod score;
mod ui_system;
