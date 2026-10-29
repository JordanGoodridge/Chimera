//! What a receiving client makes of an incoming all, team or radio line
//! (specs/combat_arms/radio_chat.md P2) and the lines it keeps (`ChatLog`).

use std::collections::VecDeque;

use super::text::{BadWords, BlockList};

pub type Rgba = [u8; 4];

/// Message colours (the HUD layout attributes').
pub mod colours {
    use super::Rgba;
    pub const ALL_TEAMMATE: Rgba = [63, 191, 87, 255];
    pub const ALL_ENEMY: Rgba = [230, 87, 87, 255];
    pub const TEAM: Rgba = [196, 196, 196, 255];
    pub const OWN_TEAM: Rgba = [255, 255, 255, 255];
    pub const RADIO: Rgba = [107, 142, 242, 255];
    pub const CLAN: Rgba = [35, 206, 255, 255];
    pub const GM: Rgba = [255, 240, 0, 255];
    pub const WHISPER_TO: Rgba = [66, 158, 83, 255];
    pub const WHISPER_FROM: Rgba = [60, 230, 92, 255];
    /// Spam refusals, whisper refusals, errors, command replies.
    pub const SYSTEM: Rgba = [255, 0, 0, 255];
    /// The punishment notice and the lobby's.
    pub const NOTICE: Rgba = [255, 255, 0, 255];
    /// The same-sentence notice (the HUD layout's colour is unread: ours).
    pub const SAME_SENTENCE: Rgba = [255, 170, 0, 255];
}

/// Every player, team, radio, whisper, clan and most system lines live
/// this long, ms.
pub const LINE_LIFE: f64 = 10_000.0;
/// The same-sentence notice's life, ms.
pub const SAME_SENTENCE_LIFE: f64 = 5000.0;
/// Lines kept for the in-game user report.
pub const HISTORY: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    All,
    Team,
    Radio,
}

/// A line in the chat panel.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLine {
    /// Counted as lines come in.
    pub id: u64,
    /// Plain text (markup in a player's text is shown literally); a
    /// leading `$` is a string-table id the panel resolves.
    pub text: String,
    pub colour: Rgba,
    /// The sender's name in the text (byte range) and its colour.
    pub name: Option<(usize, usize, Rgba)>,
    /// Byte ranges of `text` that are string-table ids (`$...`) the panel
    /// resolves in the viewer's language: a radio call's class and
    /// message, which travel as the menu entry's indices, so each client
    /// shows them from its own table. A player's own text never has any.
    pub ids: Vec<(usize, usize)>,
    /// When it came in and how long it lives, ms.
    pub born: f64,
    pub life: f64,
}

impl PanelLine {
    pub fn new(text: impl Into<String>, colour: Rgba, now: f64, life: f64) -> Self {
        Self {
            id: 0,
            text: text.into(),
            colour,
            name: None,
            ids: Vec::new(),
            born: now,
            life,
        }
    }
}

/// A player as a receiver knows them.
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub name: String,
    /// 255: a spectator.
    pub team: u8,
    pub alive: bool,
}

/// The receiver's options (an account's, saved with the lobby).
pub struct ShowOpts<'a> {
    pub filter_on: bool,
    /// The mode is Hunted.
    pub hunted: bool,
    pub block: &'a BlockList,
    pub words: &'a BadWords,
}

/// One incoming line.
pub struct Incoming<'a> {
    pub channel: Channel,
    /// The sender as the client table has them; None: an unknown client
    /// id (dropped).
    pub sender: Option<&'a Peer>,
    /// The name that goes in the line, and its colour (0: none).
    pub name: &'a str,
    pub name_colour: u32,
    /// A radio line's class name.
    pub class: Option<&'a str>,
    pub text: &'a str,
}

/// P2: the line a receiver shows, or None when it is dropped.
pub fn show(me: &Peer, opts: &ShowOpts, inc: &Incoming, now: f64) -> Option<PanelLine> {
    // 1 The block list.
    if opts.block.blocks(inc.name, &me.name) {
        return None;
    }
    // 3 An unknown client id.
    let sender = inc.sender?;
    // 4 Hunted: the living don't read the dead.
    if opts.hunted && me.alive && !sender.alive {
        return None;
    }
    let mut line = match (inc.channel, inc.class) {
        (Channel::Radio, Some(class)) => format!("[{class}] {} : {}", inc.name, inc.text),
        _ => format!("{} : {}", inc.name, inc.text),
    };
    let unfiltered_len = line.len();
    // 5 The colour.
    let mut colour = match inc.channel {
        Channel::Radio => colours::RADIO,
        Channel::Team if inc.name == me.name => colours::OWN_TEAM,
        Channel::Team => colours::TEAM,
        Channel::All if sender.team == me.team => colours::ALL_TEAMMATE,
        Channel::All => colours::ALL_ENEMY,
    };
    if inc.name.starts_with("GM") {
        colour = colours::GM;
    }
    // 6 The filter (the name included).
    if opts.filter_on {
        line = opts.words.filter(&line);
    }
    // 7 The name colour, unless the filter changed the name.
    let name = (inc.name_colour != 0)
        .then(|| {
            let at = line.find(inc.name)?;
            let c = inc.name_colour;
            Some((at, at + inc.name.len(), [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]))
        })
        .flatten();
    // The ids in a radio line: the class and the message, when they are
    // string ids and the filter left the line's length alone.
    let mut ids = Vec::new();
    if inc.channel == Channel::Radio && line.len() == unfiltered_len {
        if let Some(class) = inc.class.filter(|c| c.starts_with('$')) {
            ids.push((1, 1 + class.len()));
        }
        if inc.text.starts_with('$') {
            ids.push((line.len() - inc.text.len(), line.len()));
        }
    }
    Some(PanelLine {
        name,
        ids,
        ..PanelLine::new(line, colour, now, LINE_LIFE)
    })
}

/// The lines the panel holds and the last twenty for the user report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatLog {
    /// Lines that came in, oldest first (the front end's queue shows and
    /// expires them; this keeps the last 64).
    pub lines: VecDeque<PanelLine>,
    /// The last `HISTORY` lines' texts, after filtering.
    pub history: VecDeque<String>,
    /// The server's own text, shown as a bad-news message instead of in
    /// the panel: (text, born ms).
    pub notices: Vec<(String, f64)>,
    next_id: u64,
}

impl ChatLog {
    pub fn push(&mut self, mut line: PanelLine) -> u64 {
        self.next_id += 1;
        line.id = self.next_id;
        self.history.push_back(line.text.clone());
        while self.history.len() > HISTORY {
            self.history.pop_front();
        }
        self.lines.push_back(line);
        while self.lines.len() > 64 {
            self.lines.pop_front();
        }
        self.next_id
    }

    /// The lines still alive at `now`, the way the panel expires them
    /// (for each line past its life the *oldest* goes, a quirk).
    pub fn alive(&self, now: f64) -> Vec<&PanelLine> {
        let expired = self.lines.iter().filter(|l| now - l.born > l.life).count();
        self.lines.iter().skip(expired).collect()
    }

    /// The history, concatenated without separators.
    pub fn report(&self) -> String {
        self.history.iter().map(String::as_str).collect()
    }
}
