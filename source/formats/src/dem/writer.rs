//! Demos written in code, for tests: a header, then frames of the kinds
//! `DemoReader` reads (packets of net messages, user commands, console
//! commands, a stop), laid out as the reader expects them. Only messages
//! without game data are written (`Message`); entities need send tables
//! and are left to real recordings.

use super::{Header, ServerInfo, bits::BitWriter, usercmd::UserCmd};

/// A net message `DemoWriter::packet` can write.
#[derive(Clone, Debug)]
pub enum Message {
    /// net_Tick: the server's tick.
    Tick(u32),
    /// net_SetConVar.
    SetConVar(Vec<(String, String)>),
    /// svc_Print.
    Print(String),
    /// svc_ServerInfo (protocol 24's layout).
    ServerInfo(ServerInfo),
}

/// Builds a demo's bytes frame by frame; `finish` adds the stop frame.
pub struct DemoWriter {
    out: Vec<u8>,
    frames: i32,
    last_tick: i32,
}

impl DemoWriter {
    /// A demo with this header (`ticks` and `frames` are filled in by
    /// `finish`).
    pub fn new(header: &Header) -> Self {
        let mut out = b"HL2DEMO\0".to_vec();
        out.extend(header.demo_protocol.to_le_bytes());
        out.extend(header.network_protocol.to_le_bytes());
        for s in [&header.server_name, &header.client_name, &header.map, &header.game_dir] {
            let mut field = [0u8; 260];
            let b = s.as_bytes();
            field[..b.len().min(259)].copy_from_slice(&b[..b.len().min(259)]);
            out.extend(field);
        }
        out.extend(header.playback_time.to_le_bytes());
        out.extend(0i32.to_le_bytes()); // ticks
        out.extend(0i32.to_le_bytes()); // frames
        out.extend(header.signon_length.to_le_bytes());
        Self {
            out,
            frames: 0,
            last_tick: 0,
        }
    }

    fn frame(&mut self, kind: u8, tick: i32) {
        self.out.push(kind);
        self.out.extend(tick.to_le_bytes());
        self.frames += 1;
        self.last_tick = self.last_tick.max(tick);
    }

    fn sized(&mut self, data: &[u8]) {
        self.out.extend((data.len() as i32).to_le_bytes());
        self.out.extend(data);
    }

    /// A packet frame (`signon`: a signon frame, whose tick reads as 0) of
    /// net messages.
    pub fn packet(&mut self, tick: i32, signon: bool, messages: &[Message]) {
        self.frame(if signon { 1 } else { 2 }, tick);
        self.out.extend([0u8; 76 + 8]); // view info, sequence numbers
        let mut w = BitWriter::new();
        for m in messages {
            write_message(&mut w, m);
        }
        let data = w.finish();
        self.sized(&data);
    }

    /// A user command frame: the command delta-encoded against an empty
    /// one.
    pub fn usercmd(&mut self, tick: i32, sequence: i32, cmd: &UserCmd) {
        self.frame(5, tick);
        self.out.extend(sequence.to_le_bytes());
        let mut w = BitWriter::new();
        cmd.write(&mut w, &UserCmd::default());
        let data = w.finish();
        self.sized(&data);
    }

    /// A console command frame.
    pub fn console(&mut self, tick: i32, line: &str) {
        self.frame(4, tick);
        let mut data = line.as_bytes().to_vec();
        data.push(0);
        self.sized(&data);
    }

    /// The stop frame; the header's tick and frame counts filled in.
    pub fn finish(mut self) -> Vec<u8> {
        let tick = self.last_tick;
        self.out.push(7);
        self.out.extend(tick.to_le_bytes());
        let at = 8 + 4 + 4 + 260 * 4 + 4;
        self.out[at..at + 4].copy_from_slice(&tick.to_le_bytes());
        self.out[at + 4..at + 8].copy_from_slice(&self.frames.to_le_bytes());
        self.out
    }
}

fn write_message(w: &mut BitWriter, m: &Message) {
    match m {
        Message::Tick(tick) => {
            w.bits(3, 6);
            w.u32(*tick);
            w.u16(0);
            w.u16(0);
        }
        Message::SetConVar(vars) => {
            w.bits(5, 6);
            w.u8(vars.len() as u8);
            for (k, v) in vars {
                w.string(k);
                w.string(v);
            }
        }
        Message::Print(text) => {
            w.bits(7, 6);
            w.string(text);
        }
        Message::ServerInfo(i) => {
            w.bits(8, 6);
            w.u16(i.protocol);
            w.u32(i.server_count);
            w.bit(i.hltv);
            w.bit(i.dedicated);
            w.u32(0); // client.dll CRC
            w.u16(i.max_classes);
            if i.protocol > 17 {
                for _ in 0..4 {
                    w.u32(0); // map MD5
                }
            } else {
                w.u32(0);
            }
            w.u8(i.player_slot);
            w.u8(i.max_clients);
            w.f32(i.tick_interval);
            w.u8(i.os as u8);
            for s in [&i.game_dir, &i.map, &i.skybox, &i.host_name] {
                w.string(s);
            }
            if i.protocol > 15 {
                w.bit(false); // replay
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DemoReader, Record, usercmd::buttons};
    use super::*;

    pub fn header(map: &str) -> Header {
        Header {
            demo_protocol: 3,
            network_protocol: 24,
            server_name: "localhost:27015".into(),
            client_name: "Player".into(),
            map: map.into(),
            game_dir: "cstrike".into(),
            playback_time: 0.0,
            ticks: 0,
            frames: 0,
            signon_length: 0,
        }
    }

    #[test]
    fn a_written_client_demo_reads_back_with_its_commands() {
        let mut w = DemoWriter::new(&header("de_dust2"));
        w.packet(
            0,
            true,
            &[
                Message::ServerInfo(ServerInfo {
                    protocol: 24,
                    player_slot: 2,
                    max_clients: 16,
                    tick_interval: 0.015,
                    os: 'L',
                    game_dir: "cstrike".into(),
                    map: "de_dust2".into(),
                    host_name: "test".into(),
                    ..Default::default()
                }),
                Message::SetConVar(vec![("sv_maxspeed".into(), "260".into())]),
            ],
        );
        for t in 1..=3 {
            w.packet(t, false, &[Message::Tick(1000 + t as u32)]);
            w.usercmd(
                t,
                100 + t,
                &UserCmd {
                    command_number: 500 + t as u32,
                    tick_count: 1000 + t as u32,
                    view_angles: [0.0, 90.0, 0.0],
                    forward_move: 400.0,
                    buttons: buttons::FORWARD,
                    ..Default::default()
                },
            );
        }
        w.console(3, "record test");
        let data = w.finish();
        let mut r = DemoReader::new(&data).unwrap();
        assert_eq!(r.header().map, "de_dust2");
        assert_eq!((r.header().ticks, r.header().frames), (3, 8));
        let mut cmds = Vec::new();
        let mut cvars = Vec::new();
        let mut console = Vec::new();
        while let Some(item) = r.next_item() {
            match item.unwrap().record {
                Record::UserCmd { sequence, cmd } => cmds.push((sequence, cmd.command_number, cmd.forward_move)),
                Record::SetConVar(v) => cvars.extend(v),
                Record::ConsoleCmd(c) => console.push(c),
                Record::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        let st = r.state();
        let info = st.server_info.as_ref().unwrap();
        assert_eq!((info.player_slot, st.tick_interval()), (2, 0.015));
        assert_eq!(st.server_tick, 1003);
        assert_eq!(cmds, vec![(101, 501, 400.0), (102, 502, 400.0), (103, 503, 400.0)]);
        assert_eq!(cvars, vec![("sv_maxspeed".to_string(), "260".to_string())]);
        assert_eq!(console, vec!["record test".to_string()]);
    }

    #[test]
    fn commands_sent_against_the_previous_one_read_too() {
        let mut w = DemoWriter::new(&header("x"));
        let first = UserCmd {
            command_number: 10,
            side_move: 250.0,
            ..Default::default()
        };
        w.usercmd(1, 1, &first);
        // A frame without a command number: the delta from the first.
        w.frame(5, 2);
        w.out.extend(2i32.to_le_bytes());
        let mut bits = BitWriter::new();
        UserCmd {
            buttons: buttons::DUCK,
            ..first
        }
        .write(&mut bits, &first);
        let data = bits.finish();
        w.sized(&data);
        let data = w.finish();
        let mut r = DemoReader::new(&data).unwrap();
        let mut cmds = Vec::new();
        while let Some(item) = r.next_item() {
            if let Record::UserCmd { cmd, .. } = item.unwrap().record {
                cmds.push(cmd);
            }
        }
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[1].command_number, 10);
        assert_eq!((cmds[1].side_move, cmds[1].buttons), (250.0, buttons::DUCK));
    }
}
