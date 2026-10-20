//! The loading screen (specs/combat_arms/ui_system.md H: the loading
//! screen's layout, shown from a map load's start to its end, stretched
//! by x_r, y_r): while the shell joins a server or follows its map change
//! (`client::front_end`'s `joining`, until `entered_game`,
//! `map_load_failed` or `left_game`), the install's loading layout
//! (`ui::install::LOADING_FILE`) bound by name: `MapImage` the map's
//! picture (`UI_SCR_Loading\MapLoading\MapImage\<map>.dtx`, read when
//! the screen opens), `mapName` and its shadow the map's name (the
//! joining label: the address until the map is known), `LoadingGauge`
//! the join's progress. `CaLoading` is the state, headless; without the
//! install's layout nothing is drawn (the shell's console says what
//! loads).
//!
//! Known shortcuts (the most literal reading): the gauge moves by the
//! join's stage (ours: connecting 10 %, the server's info 25 %, a level
//! change 35 %, verifying 45 %, downloading 55 %, loading the map 80 %),
//! not by bytes; the mode line, the operation's description and the
//! second banner's text (samples in the layout) and the tips whose
//! string ids the table lacks are hidden, the
//! Quarantine backdrop never shows; a local `map` loads in one frame and
//! shows no loading screen.

use bevy::prelude::*;

use super::{
    CaUi,
    draw::{self, CaArt, CaFaces},
    hud::{HudScreen, bound, layer, model::Screen},
    install::texture_key,
    layout::{Layout, Strings},
};
use crate::client::{canvas::Scale, fonts::UiFonts, front_end::FrontEnd};

/// The loading screen's state: what is loading (the joining label), and
/// how far (0 to 1).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaLoading {
    pub label: Option<String>,
    pub progress: f32,
}

/// The map picture's texture for `label` (a map id or name; `None` for
/// an address).
pub fn map_image(label: &str) -> Option<String> {
    let name = label.rsplit(':').next().unwrap_or(label).trim().to_lowercase();
    if name.is_empty() || name.contains('.') || name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(texture_key(&format!("UI_SCR_Loading\\MapLoading\\MapImage\\{name}")))
}

/// The join's progress by stage (ours: see the module doc).
pub fn progress(stage: crate::net::client::JoinStage) -> f32 {
    use crate::net::client::JoinStage as S;
    match stage {
        S::Idle => 0.0,
        S::Connecting => 0.1,
        S::ServerInfo => 0.25,
        S::ChangingLevel => 0.35,
        S::Verifying => 0.45,
        S::Downloading => 0.55,
        S::LoadingMap => 0.8,
        S::Joined => 1.0,
    }
}

/// The loading layout bound to `state` at `scale` (None: the layout lacks
/// its controls).
pub fn build(src: &Layout, state: &CaLoading, image: Option<&str>, strings: &Strings, scale: Scale) -> Option<Layout> {
    let label = state.label.as_deref()?;
    let mut l = Layout::new();
    l.scale = scale;
    for top in src.tree.get(0).children.clone() {
        l.tree.graft(0, &src.tree, top);
    }
    let find = |l: &Layout, name: &str| l.tree.find(name);
    find(&l, "mapName")?;
    let id = find(&l, "MapImage");
    match image {
        Some(t) => bound::set_texture(&mut l, id, t),
        None => bound::show(&mut l, id, false),
    }
    let name = label.rsplit(':').next().unwrap_or(label).to_uppercase();
    for n in ["mapName", "mapName_shadow"] {
        let id = find(&l, n);
        bound::set_text(&mut l, id, Some(&name), None);
    }
    let id = find(&l, "Quarantine");
    bound::show(&mut l, id, false);
    let id = find(&l, "LoadingGauge");
    bound::set_fraction(&mut l, id, state.progress);
    bound::hide_samples(&mut l, 0);
    // A tip whose string id the table lacks would show the raw id (R2):
    // hidden.
    for id in l.tree.order() {
        let missing = l
            .widget(id)
            .text()
            .is_some_and(|t| t.set.is_none() && t.element.starts_with('$') && strings.resolve(&t.element) == t.element);
        if missing {
            l.tree.get_mut(id).show = false;
        }
    }
    Some(l)
}

/// The loading screen and its hooks.
pub struct CaLoadingPlugin;

impl Plugin for CaLoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaLoading>()
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn((LoadingRoot, layer(60)));
            })
            .add_systems(Update, (track, draw).chain());
        FrontEnd::register(app, |hooks| {
            hooks.joining = |w, label| {
                if let Some(mut l) = w.get_resource_mut::<CaLoading>() {
                    l.label = Some(label.to_string());
                }
            };
            let done: fn(&mut World) = |w| {
                if let Some(mut l) = w.get_resource_mut::<CaLoading>() {
                    *l = CaLoading::default();
                }
            };
            hooks.entered_game = done;
            hooks.map_load_failed = done;
            hooks.left_game = done;
        });
    }
}

#[derive(Component)]
struct LoadingRoot;

/// The join's progress into `CaLoading`.
fn track(join: Option<Res<crate::net::client::JoinProgress>>, mut state: ResMut<CaLoading>) {
    if state.label.is_none() {
        return;
    }
    let p = join.map_or(0.0, |j| progress(j.stage));
    if state.progress != p {
        state.progress = p;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    state: Res<CaLoading>,
    ui: Res<CaUi>,
    screen: Option<Res<HudScreen>>,
    fonts: Option<Res<UiFonts>>,
    faces: Option<Res<CaFaces>>,
    mut art: Option<ResMut<CaArt>>,
    mut images: Option<ResMut<Assets<Image>>>,
    root: Single<Entity, With<LoadingRoot>>,
    mut last: Local<Option<(CaLoading, Screen)>>,
    mut commands: Commands,
) {
    let screen = screen.map_or(Screen::new(1024.0, 768.0), |s| s.0);
    if last.as_ref() == Some(&(state.clone(), screen)) {
        return;
    }
    *last = Some((state.clone(), screen));
    commands.entity(*root).despawn_related::<Children>();
    let (Some(src), Some(label)) = (ui.install.as_ref().and_then(|i| i.loading.as_ref()), state.label.as_deref()) else {
        return;
    };
    // The map's picture, read when first needed (they are large).
    let mut image = map_image(label);
    if let (Some(key), Some(art), Some(images)) = (image.clone(), art.as_mut(), images.as_mut())
        && art.get(&key).is_none()
    {
        match crate::game::INFO.mount().and_then(|m| super::install::decode(&m, &key)) {
            Some(a) => {
                let handle = images.add(draw::rgba_image(&a));
                art.0.insert(key, (handle, a.size));
            }
            None => image = None,
        }
    }
    let Some(layout) = build(src, &state, image.as_deref(), &ui.strings, crate::ui::hud::scale(screen)) else { return };
    let draws = layout.draw(&ui.ctx());
    commands.entity(*root).with_children(|c| {
        draw::spawn_draws(c, &draws, &ui.stand_ins, art.as_deref(), fonts.as_deref(), faces.as_deref());
    });
}
