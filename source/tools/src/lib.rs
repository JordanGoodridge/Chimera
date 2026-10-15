//! Source's tools: what tools for any Source game use, knowing no game.
//!
//! - `rcon`: Valve's RCON client, for driving a real Source server.
//! - `demodump` (src/bin/demodump.rs): print a Source demo's events,
//!   user messages, sounds and entity props by tick.
//! - `testmap` (src/bin/testmap.rs): write the entity test map's `.vmf`
//!   (scripts/compile_testmap.ps1 compiles it).

pub mod rcon;
