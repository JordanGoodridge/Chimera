//! The client shell's base, as `mashup::client` re-exports it: each
//! module's `//!` doc says what it does. `vgui` here draws the layouts;
//! the mouse grab while one is open and `keys_live` stay with the shell's
//! console and chat (`mashup::client::vgui`).

pub mod binds;
pub mod fonts;
pub mod front_end;
pub mod hud_text;
pub mod vgui;
pub mod widgets;
