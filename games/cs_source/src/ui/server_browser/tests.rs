use super::*;
    use super::*;
    use crate::net::query::ServerInfo;

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([192, 168, 1, 5], port))
    }

    fn answered(port: u16, name: &str, map: &str, players: u8, bots: u8, ping: u32, password: bool) -> QueryResult {
        QueryResult {
            addr: addr(port),
            state: QueryState::Answered {
                info: ServerInfo {
                    name: name.into(),
                    map: map.into(),
                    folder: "chimera".into(),
                    game: "Lucker Party".into(),
                    players,
                    max_players: 8,
                    bots,
                    password,
                    version: NET_VERSION.into(),
                    ..default()
                },
                ping_ms: ping,
            },
            lan: true,
        }
    }

    fn lan_browser() -> (ServerBrowser, Vec<QueryResult>) {
        let mut b = ServerBrowser::default();
        let out = b.open(Some(Tab::Lan));
        assert_eq!(out.requests, [Request::ScanLan]);
        let results = vec![
            answered(27031, "Beta", "de_dust2", 3, 2, 40, false),
            answered(27032, "alpha", "cs_office", 8, 0, 12, true),
            answered(27033, "Gamma", "de_dust2", 0, 0, 90, false),
        ];
        b.set_results(&results, false);
        (b, results)
    }

    fn names(b: &ServerBrowser) -> Vec<&str> {
        b.rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn lan_rows_sort_by_latency_then_by_a_header() {
        let (mut b, _) = lan_browser();
        assert_eq!(names(&b), ["alpha", "Beta", "Gamma"], "by latency at first");
        b.handle(Input::Click(Target::Header(Column::Name)));
        assert_eq!(names(&b), ["alpha", "Beta", "Gamma"]);
        b.handle(Input::Click(Target::Header(Column::Name)));
        assert_eq!(names(&b), ["Gamma", "Beta", "alpha"], "again: reversed");
        b.handle(Input::Click(Target::Header(Column::Players)));
        assert_eq!(names(&b), ["Gamma", "Beta", "alpha"]);
        assert_eq!(b.columns()[2].1, "Servers (3)");
    }

    #[test]
    fn filters_as_the_original_offers() {
        let (mut b, results) = lan_browser();
        b.filters.no_password = true;
        b.set_results(&results, false);
        assert_eq!(names(&b), ["Beta", "Gamma"]);
        b.filters = Filters {
            not_empty: true,
            ..default()
        };
        b.set_results(&results, false);
        assert_eq!(
            names(&b),
            ["alpha", "Beta"],
            "bots alone aren't users; 3 players with 2 bots is"
        );
        b.filters = Filters {
            not_full: true,
            map: "DUST".into(),
            ..default()
        };
        b.set_results(&results, false);
        assert_eq!(names(&b), ["Beta", "Gamma"]);
        b.filters = Filters {
            latency: 1,
            ..default()
        };
        b.set_results(&results, false);
        assert_eq!(names(&b), ["alpha", "Beta"], "< 50 ms");
        assert_eq!(b.total, 3, "the header counts them all");
        assert!(b.filters.active());
        assert_eq!(b.filter_text(), "latency < 50");
    }

    #[test]
    fn connect_and_double_click_join() {
        let (mut b, _) = lan_browser();
        assert!(
            b.handle(Input::Click(Target::Button(ButtonId::Connect)))
                .lines
                .is_empty(),
            "nothing selected"
        );
        b.handle(Input::Click(Target::Row(1)));
        let out = b.handle(Input::Click(Target::Button(ButtonId::Connect)));
        assert_eq!(out.lines, ["connect 192.168.1.5:27031"]);
        assert!(!b.open, "the browser closes: the game connects");
        let (mut b, _) = lan_browser();
        let out = b.handle(Input::DoubleClick(2));
        assert_eq!(out.lines, ["connect 192.168.1.5:27033"]);
    }

    #[test]
    fn a_password_server_asks_for_it_first() {
        let (mut b, _) = lan_browser();
        let out = b.handle(Input::DoubleClick(0));
        assert!(out.lines.is_empty());
        assert!(matches!(&b.dialog, Dialog::Password { name, .. } if name == "alpha"));
        assert_eq!(b.typing, Some(Field::Password));
        b.handle(Input::Type("se".into()));
        b.handle(Input::Type("cret".into()));
        let out = b.handle(Input::Enter);
        assert_eq!(out.lines, ["password secret", "connect 192.168.1.5:27032"]);
        // Cancelled: nothing.
        let (mut b, _) = lan_browser();
        b.handle(Input::DoubleClick(0));
        assert!(b.handle(Input::Escape).lines.is_empty());
        assert_eq!(b.dialog, Dialog::None);
        assert!(b.open);
    }

    #[test]
    fn internet_is_greyed() {
        let mut b = ServerBrowser::default();
        b.open(Some(Tab::Lan));
        let out = b.handle(Input::Click(Target::Tab(Tab::Internet)));
        assert_eq!(b.tab, Tab::Lan);
        assert!(out.requests.is_empty());
    }

    #[test]
    fn add_a_server_to_favorites() {
        let mut b = ServerBrowser::default();
        let out = b.open(Some(Tab::Favorites));
        assert!(out.requests.is_empty(), "no favourites to ask");
        b.set_results(&[], false);
        assert!(b.empty_text().unwrap().contains("no favorite"));
        b.handle(Input::Click(Target::Button(ButtonId::AddServer)));
        assert_eq!(b.typing, Some(Field::AddAddress));
        b.handle(Input::Type("not an address!".into()));
        let out = b.handle(Input::Click(Target::Button(ButtonId::AddOk)));
        assert!(!out.save);
        assert!(matches!(&b.dialog, Dialog::AddServer { error: Some(_), .. }));
        for _ in 0..20 {
            b.handle(Input::Backspace);
        }
        b.handle(Input::Type("10.0.0.7:27040".into()));
        let out = b.handle(Input::Click(Target::Button(ButtonId::AddFind)));
        let a: SocketAddr = "10.0.0.7:27040".parse().unwrap();
        assert_eq!(out.requests, [Request::Query(vec![a])]);
        let out = b.handle(Input::Enter);
        assert!(out.save);
        assert_eq!(b.dialog, Dialog::None);
        assert_eq!(b.favorites.len(), 1);
        assert_eq!(out.requests, [Request::Query(vec![a])], "the tab refreshes");
        // Pending, then not responding: listed by address.
        b.set_results(&[], true);
        assert_eq!(b.rows.len(), 1);
        assert!(!b.rows[0].responded);
        b.set_results(
            &[QueryResult {
                addr: a,
                state: QueryState::NoResponse,
                lan: false,
            }],
            false,
        );
        assert!(b.rows[0].not_responding);
        // Delete takes it off.
        b.handle(Input::Down);
        let out = b.handle(Input::Delete);
        assert!(out.save && b.favorites.is_empty());
    }

    #[test]
    fn history_first_played_first() {
        let mut b = ServerBrowser::default();
        b.played(addr(1), "one", 100);
        b.played(addr(2), "two", 200);
        b.played(addr(1), "", 300);
        assert_eq!(b.history.len(), 2);
        assert_eq!(
            (b.history[0].addr, b.history[0].name.as_str(), b.history[0].last_played),
            (addr(1), "one", 300)
        );
        assert_eq!(last_played_text(300, 300 + 125), "2 min ago");
        assert_eq!(last_played_text(0, 5), "");
    }

    #[test]
    fn saved_lists_round_trip() {
        let mut b = ServerBrowser::default();
        b.add_favorite(addr(27031), "A \"quoted\" name \\ too");
        b.add_favorite("[::1]:27040".parse().unwrap(), "");
        b.played(addr(27032), "History one", 1_700_000_000);
        let text = b.saved_text();
        let mut c = ServerBrowser::default();
        c.load_text(&text);
        assert_eq!(c.favorites, b.favorites);
        assert_eq!(c.history, b.history);
        // Junk and missing parts: what can be read.
        let mut d = ServerBrowser::default();
        d.load_text("\"ServerBrowser\" { \"Favorites\" { \"1\" { \"address\" \"nope\" } \"2\" { \"address\" \"1.2.3.4:5\" } } // c\n");
        assert_eq!(d.favorites.len(), 1);
        assert_eq!(d.favorites[0].addr, "1.2.3.4:5".parse::<SocketAddr>().unwrap());
    }
