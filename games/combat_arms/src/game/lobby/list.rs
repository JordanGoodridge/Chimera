//! The room list (specs/combat_arms/lobby.md L): the client's copy of the
//! channel's rooms, the columns' texts, the filters and the sort, kept
//! live by the lobby's room notices.

use std::sync::Arc;

use super::{data::LobbyData, room::*};

/// A column header that sorts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Num,
    Title,
    Map,
    Mode,
    Mutation,
    Arms,
    Players,
    Status,
}

/// The one sort direction the whole list shares (quirk 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    None,
    Ascending,
    Descending,
}

/// The filters (checkboxes and the two combos).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub joinable: bool,
    pub no_password: bool,
    pub pure_combat: bool,
    /// A mode entry: the mode and whether it is the Pure Combat variant.
    pub mode: Option<(u16, bool)>,
    pub map: Option<u16>,
}

/// One row as the list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub num: String,
    pub title: String,
    pub locked: bool,
    pub map: String,
    pub mode: String,
    pub mutation: bool,
    pub arms: String,
    pub players: String,
    pub status: String,
    /// Greyed: the player cannot enter.
    pub disabled: bool,
}

/// The room list.
#[derive(Clone, Debug)]
pub struct RoomListView {
    data: Arc<LobbyData>,
    /// In display order.
    pub rooms: Vec<RoomSummary>,
    pub sort: Option<Column>,
    pub direction: Direction,
    pub filters: Filters,
    /// The rooms the last rebuild let through the filters: a room-changed
    /// notice only drops a room that now fails them; a room passing again
    /// stays hidden until the next rebuild (corrections 2026-10-22, 4).
    shown: Vec<RoomId>,
    /// The first row shown (scroll position, in rows).
    pub scroll: usize,
    /// The selected room's id.
    pub selected: Option<RoomId>,
}

impl RoomListView {
    pub fn new(data: Arc<LobbyData>) -> Self {
        Self {
            data,
            rooms: Vec::new(),
            sort: None,
            direction: Direction::None,
            filters: Filters::default(),
            shown: Vec::new(),
            scroll: 0,
            selected: None,
        }
    }

    /// The full list arrives (the room list reply).
    pub fn set_all(&mut self, rooms: Vec<RoomSummary>) {
        self.rooms = rooms;
        self.rebuild();
    }

    /// Rebuild the list: sorted again and the filters applied afresh (a
    /// header click, a filter change, a created room, the screen gaining
    /// focus, a map, mode or Pure Combat change).
    pub fn rebuild(&mut self) {
        self.resort();
        self.shown = self.rooms.iter().filter(|r| self.passes(r)).map(|r| r.id).collect();
    }

    /// A room was created: inserted and the list re-sorted, the scroll
    /// position kept.
    pub fn created(&mut self, room: RoomSummary) {
        self.rooms.retain(|r| r.id != room.id);
        self.rooms.push(room);
        self.rebuild();
    }

    pub fn destroyed(&mut self, id: RoomId) {
        self.rooms.retain(|r| r.id != id);
        self.shown.retain(|s| *s != id);
        if self.selected == Some(id) {
            self.selected = None;
        }
    }

    /// A room changed: its cells are updated in place; the list is rebuilt
    /// only for a map, mode or Pure Combat change, the moderator float is
    /// redone for a moderator level change, and a password, player count,
    /// maximum or state change drops a room that now fails a filter. Any
    /// other change rebuilds nothing.
    pub fn changed(&mut self, room: RoomSummary) {
        let Some(r) = self.rooms.iter_mut().find(|r| r.id == room.id) else {
            return self.created(room);
        };
        let rebuild = r.map != room.map || r.mode != room.mode || r.pure_combat != room.pure_combat;
        let float = r.moderator != room.moderator;
        let refilter =
            r.locked != room.locked || r.users != room.users || r.max != room.max || r.state != room.state;
        let id = room.id;
        *r = room;
        if rebuild {
            self.rebuild();
        } else if float {
            self.rooms.sort_by(|a, b| b.moderator.cmp(&a.moderator));
        }
        if refilter && !rebuild {
            let fails = self.find(id).is_some_and(|r| !self.passes(r));
            if fails {
                self.shown.retain(|s| *s != id);
            }
        }
    }

    /// A header click: the shared direction goes none, ascending, then
    /// flips at every click, whatever the column.
    pub fn click(&mut self, column: Column) {
        self.direction = match self.direction {
            Direction::None | Direction::Descending => Direction::Ascending,
            Direction::Ascending => Direction::Descending,
        };
        self.sort = Some(column);
        self.rebuild();
    }

    fn key_cmp(&self, column: Column, a: &RoomSummary, b: &RoomSummary) -> std::cmp::Ordering {
        match column {
            Column::Num => a.id.cmp(&b.id),
            Column::Title => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Column::Map => self.data.map_name(a.map).cmp(&self.data.map_name(b.map)),
            // By mode id, not by name.
            Column::Mode => a.mode.cmp(&b.mode),
            Column::Mutation => a.super_soldiers.cmp(&b.super_soldiers),
            Column::Arms => self.arms_order(a).cmp(&self.arms_order(b)),
            Column::Players => (a.max, a.users).cmp(&(b.max, b.users)),
            Column::Status => a.state.cmp(&b.state),
        }
    }

    fn arms_order(&self, r: &RoomSummary) -> u32 {
        self.data.arms_option(r.arms[0]).map_or(0, |o| o.order)
    }

    fn resort(&mut self) {
        if let Some(c) = self.sort {
            let mut rooms = std::mem::take(&mut self.rooms);
            rooms.sort_by(|a, b| self.key_cmp(c, a, b));
            if self.direction == Direction::Descending {
                rooms.reverse();
            }
            self.rooms = rooms;
        } else {
            self.rooms.sort_by_key(|r| r.id);
        }
        // Rooms of an Elite Moderator host go to the top, higher level
        // first, the rest in order (stable).
        self.rooms.sort_by(|a, b| b.moderator.cmp(&a.moderator));
    }

    /// Choosing a mode resets the map filter.
    pub fn filter_mode(&mut self, mode: Option<(u16, bool)>) {
        self.filters.mode = mode;
        self.filters.map = None;
        self.rebuild();
    }

    fn passes(&self, r: &RoomSummary) -> bool {
        let f = &self.filters;
        (!f.joinable || r.can_enter())
            && (!f.no_password || !r.locked)
            && (!f.pure_combat || r.pure_combat)
            && f.mode.is_none_or(|(m, p)| r.mode == m && r.pure_combat == p)
            && f.map.is_none_or(|m| r.map == m)
    }

    /// The rooms the last rebuild let through the filters, in order.
    pub fn visible(&self) -> Vec<&RoomSummary> {
        self.rooms.iter().filter(|r| self.shown.contains(&r.id)).collect()
    }

    /// A row's cells (L, "Columns").
    pub fn row(&self, r: &RoomSummary) -> Row {
        let mode = self.data.mode(r.mode).map_or("?".to_string(), |m| m.name.clone());
        let arms = if r.arms[1] != 0 {
            "Multi".to_string()
        } else {
            self.data.arms_option(r.arms[0]).map_or(String::new(), |o| if o.id == 0 { String::new() } else { o.name.clone() })
        };
        Row {
            num: (r.id + 1).to_string(),
            title: r.name.clone(),
            locked: r.locked,
            map: self.data.map_name(r.map),
            mode: if r.pure_combat { format!("[PCM] {mode}") } else { mode },
            mutation: r.super_soldiers,
            arms,
            players: format!("{}/{}", r.users, r.max),
            status: r.state.text().to_string(),
            disabled: !r.can_enter(),
        }
    }

    /// The rows as shown.
    pub fn rows(&self) -> Vec<Row> {
        self.visible().into_iter().map(|r| self.row(r)).collect()
    }

    pub fn find(&self, id: RoomId) -> Option<&RoomSummary> {
        self.rooms.iter().find(|r| r.id == id)
    }

    /// JOIN or a double-click on a row (L, "Selecting and joining"): the
    /// request to send, or what the client says instead. A locked room
    /// needs its password first (`Joining::Password`).
    pub fn join(&self, id: Option<RoomId>) -> Result<Joining, String> {
        let Some(id) = id else { return Err("Select a game room that you wish to enter.".into()) };
        let Some(r) = self.find(id) else { return Err("Select a game room that you wish to enter.".into()) };
        if !r.can_enter() {
            return Err(Refusal::CannotEnter.text());
        }
        Ok(if r.locked { Joining::Password(id) } else { Joining::Send(id) })
    }
}

/// The result of choosing a room to join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Joining {
    /// Send the enter-room request.
    Send(RoomId),
    /// Ask for the password first.
    Password(RoomId),
}
