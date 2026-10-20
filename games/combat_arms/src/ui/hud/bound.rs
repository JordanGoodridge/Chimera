//! The rest of the HUD from the game's own layouts (specs/combat_arms/
//! ui_system.md L1, H, D; hud_crosshair.md D5-D7; game_modes.md M3,
//! M10's action gauge), bound by control name as `install` binds the
//! health and weapon panels: the places, sizes, fonts, colours and
//! textures are the layouts', the values and what shows are code's.
//!
//! - `FeedPanel` (the main HUD layout's `KillDeath`, D5): one clone of
//!   its line template (`KillLogLine1`) per kill line, 20 ref px apart;
//!   in it `Name1` the killer, `weapon` the weapon icon, `icon` the first
//!   badge (later badges in a copy of it, the spec's second slot), `Name2`
//!   the victim, each at the running x of `model::feed_line` (the
//!   layout's positions are samples); the texts' colours the spec's.
//! - `ScorePanel` (the mode indicator layout's `indicator`, D6, D7): the
//!   mode's child (`child_for`: TDM's `Eli`, the spec's; the others by
//!   their names) with `mode_text`, `goal_text`, `top_round` and
//!   `bottom_round` (the counters, in their highlight colours),
//!   `alpha_gauge` and `bravo_gauge` (the bars), `bg_up` or `bg_down` (your
//!   row's highlight), `top_title` and `bottom_title` (the labels, where
//!   the layout's are samples) and `round_timer` in `RoundTime` (D7).
//! - `ActionPanel` (the main HUD layout's `Action`): `ActionDesc`'s
//!   `Message` the pickup prompt (inventory.md 11), `ActionGauge` the
//!   plant, defuse or seize gauge (`Gauge`, `ActionTitle`, `ActionIcon`:
//!   the HUD layout attributes' bomb icons).
//!
//! Known shortcuts (the most literal reading): which indicator child a
//! mode other than TDM uses, and the pickup prompt's panel (`Action`, the
//! game's "press E" line), are our reading of their names; controls
//! nothing drives (alive counts, flag states, intel, the gauges' counts,
//! the feed's `SubInfo`) are hidden, a text whose layout value is a
//! sample (not a string id) and that nothing sets is hidden; the
//! seize gauge has no icon of its own (the plant one).

use bevy::math::{IVec2, Vec2};

use super::{
    FeedLineView,
    model::{Badge, FeedItem, KillIcon, Rgba},
    score::ScorePanelView,
};
use crate::{
    client::canvas::{Scale, WidgetId},
    ui::layout::{Kind, Layout},
};

/// A layout of the named control alone (cloned under a fresh root) and
/// its id there.
pub fn graft_named(src: &Layout, name: &str) -> Option<(Layout, WidgetId)> {
    let from = src.find(name)?;
    let mut l = Layout::new();
    let id = l.tree.graft(0, &src.tree, from);
    Some((l, id))
}

/// Set a text's string and colour (None: keep).
pub fn set_text(l: &mut Layout, id: Option<WidgetId>, s: Option<&str>, colour: Option<Rgba>) {
    let Some(id) = id else { return };
    if let Kind::Text(t) = &mut l.widget_mut(id).kind {
        if let Some(s) = s {
            t.set = Some(s.to_string());
        }
        if let Some(c) = colour {
            t.colour = c;
        }
    }
}

/// Set an image's texture.
pub fn set_texture(l: &mut Layout, id: Option<WidgetId>, texture: &str) {
    let Some(id) = id else { return };
    if let Kind::Image(look) | Kind::Window(look) = &mut l.widget_mut(id).kind {
        look.texture = texture.to_string();
    }
}

pub fn show(l: &mut Layout, id: Option<WidgetId>, show: bool) {
    if let Some(id) = id {
        l.tree.get_mut(id).show = show;
    }
}

/// Fill a gauge so it draws a bar `bar` ref px wide (W5's floor gives
/// the same width back).
pub fn set_gauge(l: &mut Layout, id: Option<WidgetId>, bar: f32) {
    let Some(id) = id else { return };
    if let Kind::Gauge(g) = &mut l.widget_mut(id).kind {
        let range = g.max_range.max(1) as f64;
        g.fill = if bar <= 0.0 { 0.0 } else { bar as f64 * 100.0 / range + 1e-6 };
    }
}

/// Fill a gauge to `fraction` (0 to 1) of its range.
pub fn set_fraction(l: &mut Layout, id: Option<WidgetId>, fraction: f32) {
    let Some(id) = id else { return };
    if let Kind::Gauge(g) = &mut l.widget_mut(id).kind {
        g.fill = (fraction.clamp(0.0, 1.0) * 100.0) as f64;
    }
}

/// Hide every text under `from` whose layout value is a sample (not a
/// `$` string id) and that code hasn't set.
pub fn hide_samples(l: &mut Layout, from: WidgetId) {
    for id in l.tree.order_from(from) {
        let sample = matches!(&l.widget(id).kind, Kind::Text(t) if t.set.is_none() && !t.element.starts_with('$'));
        if sample {
            l.tree.get_mut(id).show = false;
        }
    }
}

// ---------------------------------------------------------------------------
// The kill feed (D5)

/// The kill feed's panel and its line template.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedPanel {
    source: Layout,
    panel: WidgetId,
    line: WidgetId,
}

/// A kill line element's texture: by its icon and badge.
pub struct FeedArt<'a> {
    pub icon: &'a dyn Fn(&KillIcon) -> Option<String>,
    pub badge: &'a dyn Fn(Badge) -> Option<String>,
}

impl FeedPanel {
    /// `KillDeath` of the main HUD layout with its first line; None when
    /// the layout lacks them.
    pub fn new(hud: &Layout) -> Option<Self> {
        let (source, panel) = graft_named(hud, "KillDeath")?;
        let line = source.tree.find_from(panel, "KillLogLine1")?;
        Some(Self { source, panel, line })
    }

    /// The panel with `lines` (their tops in ref px, `FeedLineView::y`) at
    /// `scale`.
    pub fn build(&self, lines: &[FeedLineView], art: &FeedArt, scale: Scale) -> Layout {
        let mut l = Layout::new();
        l.scale = scale;
        let panel = l.tree.graft(0, &self.source.tree, self.panel);
        l.tree.get_mut(panel).show = true;
        let template = l.tree.find_from(panel, "KillLogLine1");
        show(&mut l, template, false);
        let panel_y = l.tree.abs_pos(panel).y;
        for line in lines {
            let id = l.tree.graft(panel, &self.source.tree, self.line);
            let n = l.tree.get_mut(id);
            n.show = true;
            n.pos = IVec2::new(n.pos.x, line.y as i32 - panel_y);
            let find = |l: &Layout, name: &str| l.tree.find_from(id, name);
            let (name1, name2, weapon, icon) = (find(&l, "Name1"), find(&l, "Name2"), find(&l, "weapon"), find(&l, "icon"));
            let id_ = find(&l, "SubInfo");
            show(&mut l, id_, false);
            show(&mut l, weapon, false);
            show(&mut l, icon, false);
            let mut texts = [name1, name2].into_iter();
            let mut badges = 0;
            for item in &line.items {
                let (target, x) = match item {
                    FeedItem::Text { x, text, colour } => {
                        let t = texts.next().flatten();
                        set_text(&mut l, t, Some(text), Some(*colour));
                        (t, *x)
                    }
                    FeedItem::Icon { x, icon: k } => {
                        let Some(tex) = (art.icon)(k) else { continue };
                        set_texture(&mut l, weapon, &tex);
                        show(&mut l, weapon, true);
                        (weapon, *x)
                    }
                    FeedItem::Badge { x, badge } => {
                        let Some(tex) = (art.badge)(*badge) else { continue };
                        // The first badge in `icon`, every later one in a
                        // copy of it (the second slot).
                        let slot = match (badges, icon) {
                            (0, _) => icon,
                            (_, Some(i)) => Some(l.tree.graft(id, &l.tree.clone(), i)),
                            _ => None,
                        };
                        badges += 1;
                        set_texture(&mut l, slot, &tex);
                        show(&mut l, slot, true);
                        (slot, *x)
                    }
                };
                if let Some(t) = target {
                    let n = l.tree.get_mut(t);
                    n.pos.x = x as i32;
                }
            }
        }
        l
    }
}

// ---------------------------------------------------------------------------
// The score panel (D6, D7)

/// The indicator layout's child for game mode `key` (TDM's `Eli`, the
/// spec's; the rest our reading of the children's names).
pub fn child_for(key: &str) -> &'static str {
    match key {
        "TDM" => "Eli",
        "TSV" => "Eli_pro",
        "TMM" => "s&d",
        "FFA" => "Indicator_OMA",
        "CTF" => "CTF",
        "BNT" => "Indicator_SPY",
        "LMS" => "LMS",
        "QRT" => "QRT",
        "HQM" => "HQT",
        "SQM2" => "Squad",
        "HG" => "HungerGame",
        "ESL" => "ESL",
        "VWRM" => "WeaponRace",
        "VTDM" => "VIP_Eli",
        _ => "Eli",
    }
}

/// The mode indicator layout.
#[derive(Clone, Debug, PartialEq)]
pub struct ScorePanel {
    source: Layout,
}

impl ScorePanel {
    pub fn new(indicator: &Layout) -> Option<Self> {
        indicator.find("indicator")?;
        Some(Self { source: indicator.clone() })
    }

    /// Mode `key`'s panel showing `view` and `timer` at `scale`; None
    /// when the layout has no child for it.
    pub fn build(&self, key: &str, view: &ScorePanelView, timer: Option<&str>, scale: Scale) -> Option<Layout> {
        let (mut l, root) = graft_named(&self.source, "indicator")?;
        l.scale = scale;
        let want = child_for(key);
        let children = l.tree.get(root).children.clone();
        let child = children
            .iter()
            .copied()
            .find(|&c| l.tree.get(c).name.eq_ignore_ascii_case(want))?;
        for c in children {
            l.tree.get_mut(c).show = c == child;
        }
        let find = |l: &Layout, name: &str| l.tree.find_from(child, name);
        let id_ = find(&l, "mode_text");
        set_text(&mut l, id_, Some(&view.mode_name), None);
        let id_ = find(&l, "goal_text");
        set_text(&mut l, id_, Some(&view.goal), None);
        for (i, (round, gauge, title)) in [
            ("top_round", "alpha_gauge", "top_title"),
            ("bottom_round", "bravo_gauge", "bottom_title"),
        ]
        .into_iter()
        .enumerate()
        {
            let row = &view.rows[i];
            let id_ = find(&l, round);
            set_text(&mut l, id_, Some(&row.score), Some(row.colour));
            let id_ = find(&l, gauge);
            set_gauge(&mut l, id_, row.bar);
            // The label: the layout's string id, else ours, unless a
            // sibling carries the label (then this one is a count).
            if let Some(t) = find(&l, title) {
                let is_id = l.widget(t).text().is_some_and(|x| x.element.starts_with('$'));
                let parent = l.tree.get(t).parent.unwrap_or(child);
                let sibling = l.tree.get(parent).children.iter().any(|&s| {
                    s != t && l.widget(s).text().is_some_and(|x| x.element.starts_with('$'))
                });
                if is_id {
                    set_text(&mut l, Some(t), None, Some(row.colour));
                } else if !sibling {
                    set_text(&mut l, Some(t), Some(&view.labels[i]), Some(row.colour));
                }
            }
        }
        let (up, down) = (find(&l, "bg_up"), find(&l, "bg_down"));
        let mine = view.highlight.map(|h| if h.y < 22.0 { 0 } else { 1 });
        show(&mut l, up, mine == Some(0));
        show(&mut l, down, mine == Some(1));
        let round_time = find(&l, "RoundTime");
        show(&mut l, round_time, timer.is_some());
        if let Some(t) = timer {
            let id_ = find(&l, "round_timer");
            set_text(&mut l, id_, Some(t), None);
        }
        hide_samples(&mut l, child);
        Some(l)
    }
}

// ---------------------------------------------------------------------------
// The action panel: the pickup prompt and the action gauge

/// The main HUD layout's `Action`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionPanel {
    source: Layout,
    root: WidgetId,
}

impl ActionPanel {
    pub fn new(hud: &Layout) -> Option<Self> {
        let (source, root) = graft_named(hud, "Action")?;
        Some(Self { source, root })
    }

    /// The panel with the pickup prompt and the action gauge (title,
    /// fullness 0 to 1, icon texture); None when it shows neither.
    pub fn build(&self, prompt: Option<&str>, gauge: Option<(&str, f32, Option<&str>)>, scale: Scale) -> Option<Layout> {
        if prompt.is_none() && gauge.is_none() {
            return None;
        }
        let mut l = self.source.clone();
        l.scale = scale;
        let root = self.root;
        l.tree.get_mut(root).show = true;
        let find = |l: &Layout, name: &str| l.tree.find_from(root, name);
        let desc = find(&l, "ActionDesc");
        show(&mut l, desc, prompt.is_some());
        let id_ = find(&l, "Message");
        set_text(&mut l, id_, prompt, None);
        let id_ = find(&l, "bullion_icon");
        show(&mut l, id_, false);
        let bar = find(&l, "ActionGauge");
        show(&mut l, bar, gauge.is_some());
        if let Some((title, fraction, icon)) = gauge {
            let id_ = find(&l, "ActionTitle");
            set_text(&mut l, id_, Some(title), None);
            let id_ = find(&l, "Gauge");
            set_fraction(&mut l, id_, fraction);
            let icon_id = find(&l, "ActionIcon");
            match icon {
                Some(t) => set_texture(&mut l, icon_id, t),
                None => show(&mut l, icon_id, false),
            }
        }
        hide_samples(&mut l, root);
        Some(l)
    }
}

/// A control's absolute reference position (tests).
pub fn at(l: &Layout, id: WidgetId) -> Vec2 {
    l.tree.abs_pos(id).as_vec2()
}
