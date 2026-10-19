//! Drawing Combat Arms' laid-out UI (`layout::Draw`: quads and texts in
//! screen px) with the shell's UI nodes, for the HUD (`ui::hud`) and the
//! in-game menu (`ui::menu`).
//!
//! - Texts: each line at its pixel position in its font's face at the
//!   scaled height (`layout::text::Scaled`), the effect's copies (outline
//!   or shadow) under it in the effect's colour. The faces are the
//!   install's font files (`data/ui/fonts.toml`'s `file`, read from
//!   FONTS.rez at startup, `CaFaces`) when there is an install, else the
//!   shell's stand-ins (`UiFonts`: Tahoma or its fallback).
//! - Quads: textures by name are the game's art, which sits in keyed
//!   archives, so `StandIns` paints each named texture as a flat colour of
//!   the texture's size.
//!
//! Known shortcuts: glyphs are drawn at the font's height and their own
//! width, not stretched to the scaled average width (T2's ⌊a_f x_r⌋ /
//! a_f), and a face's glyph widths differ from the monospaced metrics the
//! layout measures with, so each line is aligned inside its measured box
//! as its text is (left, centred, right); nine-slice and UVs don't show
//! on flat stand-ins.

use std::collections::HashMap;

use bevy::{prelude::*, text::FontSmoothing};

use super::layout::{Draw, FontList, Rgba, TextDraw, Textures};
use crate::client::{canvas::HAlign, fonts::UiFonts};

/// The install's font faces by role (`bold`, `light`, `number`): their
/// handles and line height over the em.
#[derive(Resource, Default)]
pub struct CaFaces(pub HashMap<String, (Handle<Font>, f32)>);

/// Stand-in textures: a size and a flat colour per name.
#[derive(Resource, Clone, Debug, Default)]
pub struct StandIns(pub HashMap<String, (IVec2, Rgba)>);

impl Textures for StandIns {
    fn size(&self, name: &str) -> Option<IVec2> {
        self.0.get(name).map(|(s, _)| *s)
    }
}

impl StandIns {
    pub fn colour(&self, name: &str) -> Rgba {
        self.0.get(name).map_or([255; 4], |(_, c)| *c)
    }
}

/// Load the install's faces named in the font list (none without an
/// install, or headless).
pub fn load_faces(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    let mut faces = CaFaces::default();
    if let Some(mut assets) = fonts
        && let Some(mount) = crate::game::INFO.mount()
    {
        for face in &FontList::ours().faces {
            let Some(file) = &face.file else { continue };
            match mount.read(file) {
                Ok(bytes) => {
                    let line = crate::client::fonts::line_per_em(&bytes).unwrap_or(1.2);
                    let handle = assets.add(Font::from_bytes(bytes));
                    faces.0.insert(face.role.clone(), (handle, line));
                }
                Err(e) => warn!("combat_arms ui: font {file}: {e}; drawing with a stand-in"),
            }
        }
        info!("combat_arms ui: {} font faces from the install", faces.0.len());
    }
    commands.insert_resource(faces);
}

fn colour(c: Rgba) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

fn multiply(a: Rgba, b: Rgba) -> Rgba {
    let m = |x: u8, y: u8| ((x as u32 * y as u32) / 255) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3])]
}

/// The text font for a face and a cell height in px.
pub fn text_font(face: &str, height: i32, fonts: Option<&UiFonts>, faces: Option<&CaFaces>) -> TextFont {
    let h = height.max(1) as f32;
    if let Some((handle, line)) = faces.and_then(|f| f.0.get(face)) {
        return TextFont {
            font: handle.clone().into(),
            font_size: FontSize::Px((h / line).max(1.0)),
            font_smoothing: FontSmoothing::AntiAliased,
            ..default()
        };
    }
    let bold = face != "light";
    fonts.map_or_else(
        || TextFont {
            font_size: FontSize::Px(h * 0.85),
            ..default()
        },
        |f| f.line("Tahoma", bold, h, true),
    )
}

/// Spawn one text (its effect's copies first) under `p`.
pub fn spawn_text(p: &mut ChildSpawnerCommands, t: &TextDraw, fonts: Option<&UiFonts>, faces: Option<&CaFaces>) {
    let font = text_font(&t.face, t.font.height, fonts, faces);
    let justify = match t.h {
        HAlign::Centre => JustifyContent::Center,
        HAlign::Right => JustifyContent::End,
        _ => JustifyContent::Start,
    };
    for (at, line) in &t.lines {
        let width = line.chars().count() as i32 * t.font.advance;
        let mut one = |off: IVec2, c: Rgba| {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px((at.x + off.x) as f32),
                    top: px((at.y + off.y) as f32),
                    width: px(width as f32),
                    height: px(t.font.height as f32),
                    justify_content: justify,
                    align_items: AlignItems::Center,
                    overflow: Overflow::visible(),
                    ..default()
                },
                children![(Text::new(line.clone()), font.clone(), TextColor(colour(c)), TextLayout::no_wrap())],
            ));
        };
        for off in &t.copies {
            one(*off, t.effect_colour);
        }
        one(IVec2::ZERO, t.colour);
    }
}

/// Spawn a draw list under `p`, in order.
pub fn spawn_draws(
    p: &mut ChildSpawnerCommands,
    draws: &[Draw],
    stand_ins: &StandIns,
    fonts: Option<&UiFonts>,
    faces: Option<&CaFaces>,
) {
    for d in draws {
        match d {
            Draw::Quad(_, q) => {
                let r = q.cover;
                let s = r.size();
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(r.x0 as f32),
                        top: px(r.y0 as f32),
                        width: px(s.x as f32),
                        height: px(s.y as f32),
                        ..default()
                    },
                    BackgroundColor(colour(multiply(stand_ins.colour(&q.texture), q.colour))),
                ));
            }
            Draw::Text(t) => spawn_text(p, t, fonts, faces),
        }
    }
}
