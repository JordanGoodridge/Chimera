//! The server's message of the day (CS:S's MOTD), shown to players as they
//! join (`games::cs_source::ui::join_menus`). Where it comes from, as a CS:S server
//! picks it: `motd.txt` in the host's config folder (`console::cfg_dir`,
//! where Chimera keeps its configs), else the install's (`cstrike/motd.txt`,
//! else `cfg/motd_default.txt`: the map loader's `GameMenus::motd_file`);
//! the plain-text one (`motd_text.txt`, else `cfg/motd_text_default.txt`)
//! the same way, else the first with its HTML taken out (`plain_text`).
//! A network server sends it to each client as it is let in
//! (`ServerMotd`); a game of one and a listen server's host read their own.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use super::ServerMotd;
use crate::{core::NetRole, map::hud::ActiveHud};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, (keep_current, receive).chain());
}

/// The message of the day from the host's config folder and the loaded
/// map's install files (as written, and plain text), with the server's
/// name (`hostname`).
pub fn current(hud: Option<&ActiveHud>, hostname: &str) -> ServerMotd {
    let own = |name: &str| {
        crate::console::cfg_dir()
            .and_then(|d| std::fs::read_to_string(d.join(name)).ok())
            .filter(|t| !t.trim().is_empty())
    };
    let menus = hud.and_then(|h| h.0.menus.as_ref());
    let html = own("motd.txt")
        .or_else(|| menus.and_then(|m| m.motd_file.clone()))
        .unwrap_or_default();
    let text = own("motd_text.txt")
        .or_else(|| menus.and_then(|m| m.motd_text_file.clone()))
        .unwrap_or_else(|| plain_text(&html));
    ServerMotd {
        server: hostname.to_string(),
        html,
        text,
    }
}

/// A server's or a game of one's own message of the day, kept with the
/// loaded map and its name (a network client's comes from its server:
/// `receive`).
fn keep_current(
    role: Option<Res<NetRole>>,
    hud: Option<Res<ActiveHud>>,
    hosting: Option<Res<super::query::Hosting>>,
    have: Option<Res<ServerMotd>>,
    mut commands: Commands,
) {
    if role.is_some_and(|r| *r == NetRole::Client) {
        return;
    }
    let changed = hud.as_ref().is_some_and(|h| h.is_changed()) || hosting.as_ref().is_some_and(|h| h.is_changed());
    if have.is_some() && !changed {
        return;
    }
    let name = hosting.map_or_else(|| super::query::GAME_NAME.to_string(), |h| h.hostname.clone());
    commands.insert_resource(current(hud.as_deref(), &name));
}

/// A network client keeps what its server sent.
fn receive(mut sent: MessageReader<ServerMotd>, role: Option<Res<NetRole>>, mut commands: Commands) {
    if role.is_none_or(|r| *r != NetRole::Client) {
        sent.clear();
        return;
    }
    if let Some(m) = sent.read().last() {
        commands.insert_resource(m.clone());
    }
}

/// The message of the day, to a client being let in.
pub(super) fn send(world: &mut World, client: Entity) {
    let motd = match world.get_resource::<ServerMotd>() {
        Some(m) => m.clone(),
        None => {
            let name = world
                .get_resource::<super::query::Hosting>()
                .map_or_else(|| super::query::GAME_NAME.to_string(), |h| h.hostname.clone());
            current(world.get_resource::<ActiveHud>(), &name)
        }
    };
    world.write_message(ToClients {
        targets: SendTargets::Single(ClientId::Client(client)),
        message: motd,
    });
}

/// An HTML message of the day as plain text: its tags and the `<head>`
/// (title, styles) out, line breaks kept (`<br>`, `<p>` and block ends
/// start lines), the common entities decoded. We don't draw HTML (CS:S
/// shows it in a web view); a message that is only a web address stays
/// that address.
pub fn plain_text(html: &str) -> String {
    if !html.contains('<') {
        return html.replace('\r', "").trim().to_string();
    }
    let mut out = String::new();
    let mut rest = html;
    let mut skip_until: Option<&str> = None;
    while !rest.is_empty() {
        if let Some(end) = skip_until {
            match rest.to_ascii_lowercase().find(end) {
                Some(i) => {
                    rest = &rest[i + end.len()..];
                    skip_until = None;
                }
                None => break,
            }
            continue;
        }
        match rest.find('<') {
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i + 1..];
                let close = after.find('>').map_or(after.len(), |j| j + 1);
                let tag = after[..close.saturating_sub(1).min(after.len())].trim().to_lowercase();
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                rest = &after[close.min(after.len())..];
                match name.as_str() {
                    "head" | "style" | "script" | "title" if !tag.starts_with('/') => {
                        skip_until = Some(match name.as_str() {
                            "head" => "</head>",
                            "style" => "</style>",
                            "script" => "</script>",
                            _ => "</title>",
                        });
                    }
                    "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "pre" | "table" => out.push('\n'),
                    _ => {}
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace('\r', "");
    // Lines trimmed of trailing space, runs of blank lines as one.
    let mut lines: Vec<&str> = Vec::new();
    for line in decoded.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_its_text() {
        let html = "<!DOCTYPE HTML>\n<html>\n<head>\n<title>Cstrike MOTD</title>\n<style type=\"text/css\">\npre { color:#FFB000; }\n</style>\n</head>\n<body scroll=\"no\">\n<pre>\nWelcome to the server\nRules &amp; such\n<a href=\"http://example.com\">Visit us</a>\n</pre>\n</body>\n</html>";
        assert_eq!(plain_text(html), "Welcome to the server\nRules & such\nVisit us");
        assert_eq!(plain_text("Just text\r\nlines\n"), "Just text\nlines");
        assert_eq!(plain_text("http://example.com/motd.html"), "http://example.com/motd.html");
        assert_eq!(plain_text("a<br>b<p>c</p>"), "a\nb\nc");
    }
}
