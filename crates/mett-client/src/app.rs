//! Zustand des Clients: folgt dem Spiel, zählt Eingaben und schickt die Anzeige ins HUD-Plugin.
use crate::apple_link::AppleLink;
use crate::chat_keys;
use crate::chat_link::ChatLink;
use crate::cross_link::CrossLink;
use crate::editor::{self, Action, Editor, View};
use crate::game::{self, Status, Watcher};
use crate::gear_link::{self, GearLink};
use crate::marker;
use crate::panel_link::{Goal, PanelLink, State as PanelState, Want};
use crate::ping_link::PingLink;
use crate::players_link::{self, PlayersLink};
use crate::plugins::Plugins;
use crate::screen::Screen;
use crate::style_link::StyleLink;
use crate::value_link::ValueLink;
use crate::window;
use crate::zoom_link::ZoomLink;
use hud::chat_input::{ChatInput, Key};
use hud::hints::Hints;
use hud::lang::t;
use hud::ui::Input;
use hud::{Draw, Keys, Kind, java_chat};
use hudlink::{Event, control};
use render::Metrics;
use sources::presence::Presence;
use sources::{CpsCounter, Hub, Reading, Stopwatch, Wanted};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct App {
    pub hwnd: HWND,
    pub hud_on: bool,
    pub timed_out: bool,
    pub save_failed: bool,
    exe: String,
    config: config::Config,
    /// `None`, wenn die Einstellungen nicht lesbar waren: dann nie überschreiben.
    path: Option<PathBuf>,
    watcher: Watcher,
    /// Minecrafts Schrift aus der Installation; wird nicht mitgeliefert.
    font: Option<Metrics>,
    /// `None` in der Entwicklung: dort wird nichts ins Spiel geladen.
    screen: Option<Screen>,
    shown: bool,
    /// Letzter Stand, solange Minecraft vorne war: ein Menü ist offen.
    in_menu: bool,
    fps: Option<u32>,
    status: String,
    game: HWND,
    editor: Option<Editor>,
    mouse: Input,
    keys: Keys,
    left: CpsCounter,
    right: CpsCounter,
    right_shift: bool,
    /// Wo auf dem nativen Knopf gedrückt wurde.
    armed: Option<POINT>,
    open_after_menu: Option<Instant>,
    hub: Hub,
    stopwatch: Stopwatch,
    /// Seit wann Minecraft läuft.
    started: Option<Instant>,
    presence: Option<Presence>,
    held: Option<VIRTUAL_KEY>,
    link: Option<ChatLink>,
    panel: Option<PanelLink>,
    style: Option<StyleLink>,
    players: Option<PlayersLink>,
    zoom: Option<ZoomLink>,
    apple: Option<AppleLink>,
    ping: Option<PingLink>,
    cross: Option<CrossLink>,
    values: Option<ValueLink>,
    gear: Option<GearLink>,
    /// Tab wird beim Spielen gehalten.
    tab: bool,
    /// Das Pack dieses Spielstarts blendet Minecrafts Anzeige aus; dann gibt es nichts zu verschieben.
    pack_hides: bool,
    keys_read: Instant,
    /// Für bewegte Farben.
    born: Instant,
    chat_input: ChatInput,
    hints: Hints,
    /// Offen seit.
    chat_open: Option<Instant>,
    /// Eigene Meldungen im Chat.
    notes: Vec<sources::Line>,
}

impl App {
    pub fn new(
        hwnd: HWND,
        exe: String,
        limited: bool,
        (config, path): (config::Config, Option<PathBuf>),
        plugins: Option<Plugins>,
    ) -> Self {
        let pack_hides = crate::pack::hides_chat();
        Self {
            hwnd,
            hud_on: true,
            timed_out: false,
            save_failed: false,
            watcher: Watcher::new(exe.clone(), limited),
            exe,
            config,
            path,
            font: None,
            screen: plugins.clone().map(|p| Screen::start(hwnd, p)),
            shown: false,
            in_menu: true,
            fps: None,
            status: String::new(),
            game: HWND::default(),
            editor: None,
            mouse: Input::default(),
            keys: Keys::default(),
            left: CpsCounter::default(),
            right: CpsCounter::default(),
            right_shift: false,
            armed: None,
            open_after_menu: None,
            hub: Hub::start(),
            stopwatch: Stopwatch::default(),
            started: None,
            presence: None,
            held: None,
            panel: plugins.clone().map(PanelLink::start),
            style: plugins.clone().map(StyleLink::start),
            players: plugins.is_some().then(PlayersLink::start),
            zoom: plugins.clone().map(ZoomLink::start),
            apple: plugins.is_some().then(AppleLink::start),
            ping: plugins.is_some().then(PingLink::start),
            cross: plugins.clone().map(CrossLink::start),
            values: plugins.clone().map(ValueLink::start),
            gear: plugins.is_some().then(GearLink::start),
            tab: false,
            link: plugins.map(ChatLink::start),
            pack_hides,
            keys_read: Instant::now(),
            born: Instant::now(),
            chat_input: ChatInput::default(),
            hints: Hints::default(),
            chat_open: None,
            notes: vec![],
        }
    }

    pub fn tick(&mut self) {
        match self.watcher.poll(Instant::now()) {
            Status::Exited => {
                self.close_editor();
                return unsafe { PostQuitMessage(0) };
            }
            Status::TimedOut => {
                self.timed_out = true;
                return unsafe { PostQuitMessage(1) };
            }
            Status::Running if self.started.is_none() => {
                self.started = Some(Instant::now());
                self.presence = Presence::connect("Minecraft Bedrock");
            }
            _ => {}
        }
        self.want();
        self.follow();
        self.open_pending();
        self.chat_tick();
        self.paint();
    }

    fn module(&self, k: Kind) -> bool {
        self.config.profile().modules.get(&k).is_some_and(|m| m.enabled)
    }

    fn chat_module(&self) -> bool {
        self.module(Kind::Chat)
    }

    /// Im Spiel, kein Menü. Öffnen wir selbst etwas, ist der Zeiger auch frei.
    fn playing(&self) -> bool {
        self.in_front() && self.editor.is_none() && (self.chat_open.is_some() || !cursor_visible())
    }

    fn in_front(&self) -> bool {
        !self.game.is_invalid() && unsafe { GetForegroundWindow() } == self.game
    }

    fn chat_tick(&mut self) {
        let module = self.chat_module();
        let playing = self.playing();
        let on = self.config.chat_input && module;
        // Immer bereithalten, damit die Eingabe beim Einschalten sofort geht.
        let pid = self.watcher.pid();
        let now = Instant::now();
        if let Some(l) = &mut self.link {
            l.want(pid.map(|p| (p, playing && on)));
            self.notes.extend(
                l.notes().into_iter().map(|t| sources::Line { text: format!("§c{}", hud::lang::t(t)), at: Some(now) }),
            );
        }
        let extra = self.notes.len().saturating_sub(10);
        self.notes.drain(..extra);
        let state = match &self.link {
            None => "nur beim Start über den Mett Launcher",
            Some(_) if self.config.chat_input && !module => "Chat-Modul ist aus",
            Some(l) => l.state().text(),
        };
        let panel = match &self.panel {
            None => "nur beim Start über den Mett Launcher",
            Some(_) if !module => "Chat-Modul ist aus",
            Some(_) if self.pack_hides => "ab dem nächsten Spielstart",
            Some(p) => p.state().text(),
        };
        let goal = self.panel_goal();
        if let Some(p) = &mut self.panel {
            p.want(self.watcher.pid().zip(goal).map(|(pid, goal)| Want { pid, playing, goal }));
        }
        let java = self.watcher.pid().filter(|_| self.config.chat_java && module);
        if let Some(s) = &mut self.style {
            s.want(java);
        }
        let style = self.style.as_ref().map_or("nur beim Start über den Mett Launcher", StyleLink::state);
        let ping = self.watcher.pid().filter(|_| self.module(Kind::Ping)).map(|pid| (pid, playing));
        if let Some(p) = &mut self.ping {
            p.want(ping);
        }
        let apple = self.watcher.pid().filter(|_| self.module(Kind::AppleSkin)).map(|pid| (pid, playing));
        if let Some(a) = &mut self.apple {
            a.want(apple);
        }
        let apple = self.apple.as_ref().map_or("nur beim Start über den Mett Launcher", AppleLink::state);
        // Nur wenn unseres auch wirklich zu sehen ist.
        let drawn = self.hud_on && self.screen.as_ref().and_then(Screen::size).is_some();
        let on = drawn && self.module(Kind::Crosshair);
        let cross = self.watcher.pid().filter(|_| on).map(|pid| (pid, playing));
        if let Some(c) = &mut self.cross {
            c.want(cross);
        }
        let cross = self.cross.as_ref().map_or("nur beim Start über den Mett Launcher", CrossLink::state);
        let wish = [self.module(Kind::Fullbright).then_some(self.config.fullbright)];
        let values = self.watcher.pid().map(|pid| (pid, wish));
        if let Some(v) = &mut self.values {
            v.want(values);
        }
        let bright = self.values.as_ref().map_or("nur beim Start über den Mett Launcher", |v| v.state(0));
        let items = self.module(Kind::Armor) || self.module(Kind::Items);
        let effects = self.module(Kind::Potions);
        let gear = self.watcher.pid().filter(|_| items || effects);
        let bar = self.module(Kind::Armor) && self.config.profile().armor.sticky;
        if let Some(g) = &mut self.gear {
            g.want(gear.map(|pid| gear_link::Want { pid, playing, items, effects, bar }));
        }
        let gear = self.gear.as_ref().map_or(["nur beim Start über den Mett Launcher"; 2], GearLink::states);
        let held = self.tab;
        let zoom = self.watcher.pid().filter(|_| self.module(Kind::Zoom)).map(|pid| (pid, self.config.zoom.sane()));
        if let Some(z) = &mut self.zoom {
            z.want(zoom);
        }
        let zoom = self.zoom.as_ref().map_or("nur beim Start über den Mett Launcher", ZoomLink::state);
        let players = self.watcher.pid().filter(|_| self.module(Kind::Players));
        if let Some(p) = &mut self.players {
            p.want(players.map(|pid| players_link::Want { pid, playing, held }));
        }
        let tab = self.players.as_ref().map_or("nur beim Start über den Mett Launcher", PlayersLink::state);
        if let Some(e) = &mut self.editor {
            e.tab_state = tab;
            e.zoom_state = zoom;
            e.apple_state = apple;
            e.cross_state = cross;
            e.bright_state = bright;
            e.gear_states = gear;
            e.time = self.born.elapsed().as_secs_f32();
            e.style_state = style;
            e.chat_state = state;
            e.panel_state = panel;
        }
        self.sync();
    }

    /// Wohin mit Minecrafts Chat-Anzeige? Sie ist die einzige, wir zeigen selbst keine Nachrichten.
    fn panel_goal(&self) -> Option<Goal> {
        let c = &self.config;
        if !self.chat_module() || self.pack_hides {
            return None;
        }
        // Im Editor stehen dort Beispiele; echte Nachrichten lägen darunter.
        if self.editor.is_some() {
            return Some(Goal::Hide);
        }
        let size = self.screen.as_ref().and_then(Screen::size)?;
        let dpi = unsafe { GetDpiForWindow(self.game) }.max(96) as f32 / 96.0;
        let (_, at) =
            editor::items(c.profile(), size, dpi, self.native(size)).into_iter().find(|i| i.0 == Kind::Chat)?;
        Some(Goal::Place {
            // Ganze Pixel, sonst verwischen die Ränder.
            x: at.x.round(),
            bottom: (at.y + at.h).round(),
            height: size.1,
            rows: c.chat_entries(),
            sudden: !c.chat_fade,
            size: (at.w, at.h),
            java: self.java().is_some(),
        })
    }

    /// Der Java-Stil gilt gerade.
    fn java(&self) -> Option<java_chat::Native> {
        self.style.as_ref().filter(|s| s.on()).map(|_| java_chat::JAVA)
    }

    /// Größe von Minecrafts Anzeige, wenn sie den Platz unseres Chats einnimmt.
    fn native(&self, screen: (f32, f32)) -> Option<(f32, f32)> {
        let look = self.panel.as_ref().filter(|_| self.panel_shows())?.look(screen.1)?;
        // Mit bekannten Regeln ist sie so frei wie jedes andere Modul.
        if look.free {
            return None;
        }
        Some((
            look.width,
            java_chat::native_height(
                look.unit,
                self.config.chat_entries() as usize,
                self.java().unwrap_or(java_chat::BEDROCK),
            ),
        ))
    }

    /// Gehört uns Minecrafts Anzeige gerade?
    fn panel_shows(&self) -> bool {
        let ready = self.panel.as_ref().is_some_and(|p| p.state() == PanelState::Ready);
        self.chat_module() && !self.pack_hides && ready
    }

    /// Wünsche ans Plugin, mindestens einmal je Takt als Lebenszeichen.
    fn sync(&mut self) {
        let mut c = 0;
        let on = self.config.chat_input && self.chat_module();
        // T gehört uns, sobald Senden mit dieser Spielversion geht. Das Plugin prüft selbst, ob gerade gespielt wird.
        if on
            && let Some(l) = &self.link
            && l.state().usable()
        {
            c |= control::CHAT;
            if l.commands() {
                c |= control::COMMANDS;
            }
        }
        if self.module(Kind::Players) {
            c |= control::TAB;
        }
        let z = self.config.zoom;
        if self.module(Kind::Zoom) && self.zoom.as_ref().is_some_and(ZoomLink::on) {
            let bits = [
                (true, control::ZOOM),
                (z.scroll, control::ZOOM_WHEEL),
                (z.scroll_keys, control::ZOOM_KEYS),
                (z.second, control::ZOOM_SECOND),
            ];
            bits.iter().filter(|b| b.0).for_each(|b| c |= b.1);
        }
        if let Some(e) = &self.editor {
            c |= control::MENU | e.shape(&self.config, self.mouse.mouse) << control::SHAPE_SHIFT;
        }
        let (pid, ready) = (self.watcher.pid(), self.in_front());
        if let Some(s) = &mut self.screen {
            s.tick(pid, ready, c);
            s.set_zoom_keys([z.key, z.key_in, z.key_out, z.second_key]);
            // Die Tastenbelegung kann sich im Spiel ändern.
            if self.keys_read.elapsed() > Duration::from_secs(2) {
                self.keys_read = Instant::now();
                crate::set_language();
                let (chat, command) = chat_keys::read();
                s.set_keys(&chat, &command);
            }
        }
    }

    /// Text am Tray-Symbol.
    pub fn status(&self) -> String {
        let note = self.screen.as_ref().map_or("nur Entwicklung, keine Anzeige", |s| s.note());
        format!("Mett-Client: {}", t(note))
    }

    pub fn status_changed(&mut self) -> bool {
        let now = self.status();
        let changed = now != self.status;
        self.status = now;
        changed
    }

    fn close_chat(&mut self) {
        self.chat_open = None;
    }

    /// Eingaben aus dem Spiel.
    pub fn events(&mut self) {
        let events = self.screen.as_ref().map(Screen::events).unwrap_or_default();
        if events.is_empty() {
            return;
        }
        for e in events {
            if let Event::Tab { down } = e {
                self.tab = down;
                // Gleich lesen, nicht erst beim nächsten Takt.
                self.chat_tick();
                continue;
            }
            if self.editor.is_some() {
                self.menu_event(e);
            } else {
                self.chat_event(e);
            }
        }
        self.paint();
    }

    fn chat_event(&mut self, e: Event) {
        if let Event::Open { command } = e {
            self.chat_input.open(if command { "/" } else { "" });
            self.chat_open = Some(Instant::now());
            self.keys = Keys::default();
            self.hints.clear();
            return;
        }
        if self.chat_open.is_none() {
            return;
        }
        let input = &mut self.chat_input;
        match e {
            Event::Closed => self.close_chat(),
            Event::Char(c) => input.insert(c.encode_utf8(&mut [0; 4])),
            Event::Wheel { notches, .. } if self.hints.hit(self.mouse.mouse).is_some() => {
                self.hints.scroll(notches as i32);
            }
            Event::Wheel { .. } => {}
            Event::Key { vk, shift, ctrl } => self.chat_key(VIRTUAL_KEY(vk as u16), shift, ctrl),
            Event::Move { x, y } => {
                self.mouse.mouse = self.in_picture(x, y);
                if let Some(i) = self.hints.hit(self.mouse.mouse) {
                    self.hints.select(i);
                }
            }
            Event::Button { which: 0, down: true } => {
                if let Some(i) = self.hints.hit(self.mouse.mouse) {
                    self.hints.select(i);
                    self.hints.apply(&mut self.chat_input, false);
                }
            }
            Event::Open { .. } | Event::Button { .. } | Event::Tab { .. } => {}
        }
    }

    /// Das Bild kann anders groß sein als das Fenster.
    fn in_picture(&self, x: i16, y: i16) -> (f32, f32) {
        let scale = client_area(self.game).zip(self.screen.as_ref().and_then(Screen::size));
        let (w, h) =
            scale.map_or((1.0, 1.0), |(r, s)| (s.0 / (r.right - r.left) as f32, s.1 / (r.bottom - r.top) as f32));
        (x as f32 * w, y as f32 * h)
    }

    fn chat_key(&mut self, vk: VIRTUAL_KEY, shift: bool, ctrl: bool) {
        let input = &mut self.chat_input;
        let key = match vk {
            VK_ESCAPE => return self.close_chat(),
            VK_RETURN => {
                let text = input.submit();
                self.close_chat();
                if let (Some(t), Some(l)) = (text, &self.link) {
                    l.send(t);
                }
                return;
            }
            VK_A if ctrl => return input.select_all(),
            VK_C | VK_X if ctrl => {
                let t = input.selected();
                if !t.is_empty() && window::set_clipboard(self.hwnd, &t).is_ok() && vk == VK_X {
                    input.insert("");
                }
                return;
            }
            VK_V if ctrl => return input.insert(&window::clipboard().unwrap_or_default()),
            VK_TAB => return self.hints.apply(input, shift),
            // Ist die Liste offen, gehören ihr die Pfeiltasten, sonst dem Verlauf.
            VK_UP if self.hints.open() => return self.hints.step(-1),
            VK_DOWN if self.hints.open() => return self.hints.step(1),
            VK_LEFT => Key::Left,
            VK_RIGHT => Key::Right,
            VK_HOME => Key::Home,
            VK_END => Key::End,
            VK_UP => Key::Up,
            VK_DOWN => Key::Down,
            VK_BACK => Key::Backspace,
            VK_DELETE => Key::Delete,
            _ => return,
        };
        input.key(key, shift, ctrl);
    }

    fn menu_event(&mut self, e: Event) {
        match e {
            Event::Char(c) => self.char(c),
            Event::Key { vk, shift, ctrl } => self.key(VIRTUAL_KEY(vk as u16), shift, ctrl),
            Event::Wheel { notches, .. } => self.mouse(None, |i| i.wheel = notches as f32),
            Event::Move { x, y } => {
                let at = self.in_picture(x, y);
                self.mouse(Some(at), |_| {});
            }
            Event::Button { which: 0, down } => self.mouse(None, |i| {
                i.down = down;
                i.pressed = down;
            }),
            Event::Button { down: true, .. } => self.mouse(None, |i| i.right_pressed = true),
            Event::Open { .. } | Event::Closed | Event::Button { .. } | Event::Tab { .. } => {}
        }
    }

    /// Nur messen, was gerade zu sehen ist.
    fn want(&mut self) {
        // Aus den gezeichneten Bildern gezählt, ohne Windows-Recht.
        if let Some(f) = self.screen.as_mut().and_then(Screen::frames) {
            self.fps = Some(f);
        }
        let p = self.config.profile();
        let server = self.ping.as_ref().and_then(PingLink::server);
        let on = |k: Kind| self.shown && p.modules.get(&k).is_some_and(|m| m.enabled);
        self.hub.want(&Wanted {
            system: on(Kind::System),
            battery: on(Kind::Battery),
            music: on(Kind::Music),
            ping: on(Kind::Ping).then(|| server.map_or(String::new(), |ip| ip.to_string())),
            // Minecraft zeigt die Nachrichten selbst; wir lesen sie nicht mehr.
            chat_pid: None,
            fps_pid: None,
            commands_pid: self.watcher.pid().filter(|_| self.config.chat_input && self.chat_module()),
        });
    }

    /// F9 und F10 steuern die Stoppuhr, nur beim Spielen und nur, wenn sie angezeigt wird.
    fn stopwatch_key(&mut self, key: VIRTUAL_KEY, down: bool) {
        if !matches!(key, VK_F9 | VK_F10) {
            return;
        }
        // Tastenwiederholung zählt nicht.
        let first = down && self.held != Some(key);
        self.held = down.then_some(key);
        let on = self.config.profile().modules.get(&Kind::Stopwatch).is_some_and(|m| m.enabled);
        if first && on && !cursor_visible() {
            if key == VK_F9 { self.stopwatch.toggle(Instant::now()) } else { self.stopwatch.reset() }
        }
    }

    /// Gezeichnet wird im Spiel, deshalb bleibt alles stehen, auch wenn ein anderes Fenster vorne ist.
    fn follow(&mut self) {
        let front = unsafe { GetForegroundWindow() };
        let in_game = !front.is_invalid() && game::window_is(front, &self.exe);
        if in_game {
            self.game = front;
        }
        if !in_game {
            // Alt-Tab o. ä.: speichern und zumachen.
            self.close_editor();
            self.close_chat();
            self.tab = false;
        }
        let own = self.editor.is_some() || self.chat_open.is_some();
        // Der Zeiger sagt nur etwas, solange Minecraft vorne ist.
        if in_game {
            self.in_menu = cursor_visible();
        }
        let alive = in_game || (!self.game.is_invalid() && game::window_is(self.game, &self.exe));
        let menu = self.in_menu && !own;
        if !in_game {
            // Eingaben gehören jetzt einem anderen Fenster.
            self.keys = Keys::default();
            self.held = None;
        }
        let shown = alive && (own || (self.hud_on && !(self.config.hide_in_menus && menu)));
        if self.shown && !shown {
            self.keys = Keys::default();
            self.left.clear();
            self.right.clear();
        }
        self.shown = shown;
    }

    pub fn open_editor(&mut self, view: View) {
        if self.editor.is_some() || self.game.is_invalid() {
            return;
        }
        // Ohne Anzeige im Spiel wäre der Editor nicht zu sehen.
        if self.screen.as_ref().and_then(Screen::size).is_none() {
            return;
        }
        let folder = self.path.as_ref().and_then(|p| p.parent()).map(|p| p.join("Profile"));
        self.editor = Some(Editor::new(view, folder));
        self.close_chat();
        self.shown = true;
        self.keys = Keys::default();
        self.mouse = Input::default();
        self.sync();
        self.paint();
    }

    pub fn close_editor(&mut self) {
        let Some(mut e) = self.editor.take() else { return };
        e.end_typing(&mut self.config);
        if let Some(p) = &self.path {
            self.save_failed = config::save(p, &self.config).is_err();
        }
        self.sync();
    }

    /// Maus im Editor; `change` setzt, was passiert ist.
    fn mouse(&mut self, pos: Option<(f32, f32)>, change: impl FnOnce(&mut Input)) {
        if self.editor.is_none() {
            return;
        }
        if let Some(p) = pos {
            self.mouse.mouse = p;
        }
        change(&mut self.mouse);
        self.paint();
        // Das Zeigerbild soll der Maus ohne Verzug folgen.
        self.sync();
        self.mouse = Input { mouse: self.mouse.mouse, down: self.mouse.down, ..Default::default() };
    }

    fn key(&mut self, vk: VIRTUAL_KEY, shift: bool, ctrl: bool) {
        let Some(e) = &mut self.editor else { return };
        let n = if shift { 10.0 } else { 1.0 };
        let c = &mut self.config;
        match vk {
            VK_ESCAPE if e.is_typing() => e.end_typing(c),
            VK_ESCAPE if e.close_picker() => {}
            VK_ESCAPE => self.close_editor(),
            VK_BACK => e.char(c, '\u{8}'),
            VK_RETURN => e.char(c, '\r'),
            VK_Z if ctrl && !e.is_typing() => e.undo(c),
            VK_LEFT => e.nudge(c, -n, 0.0),
            VK_RIGHT => e.nudge(c, n, 0.0),
            VK_UP => e.nudge(c, 0.0, -n),
            VK_DOWN => e.nudge(c, 0.0, n),
            _ => {}
        }
    }

    fn char(&mut self, ch: char) {
        if let Some(e) = &mut self.editor {
            e.char(&mut self.config, ch);
        }
    }

    /// Rechte Umschalttaste: Menü öffnen und schließen; beim Anordnen des HUDs zurück ins Menü.
    fn hotkey(&mut self) {
        let front = self.in_front();
        match &mut self.editor {
            Some(e) if e.is_typing() => {}
            Some(e) if e.view == View::Compact => {
                e.view = View::Full;
                self.paint();
            }
            Some(_) => self.close_editor(),
            // Nur beim Spielen, damit Tippen im Chat nichts öffnet.
            None if self.shown && front && !cursor_visible() => {
                self.open_editor(View::Full);
            }
            None => {}
        }
    }

    fn scene(&mut self, screen: (f32, f32), dpi: f32) -> (Vec<Draw>, Action) {
        let mut out = vec![];
        let now = Instant::now();
        if self.editor.is_some() {
            out.push(crate::menu::backdrop(screen));
        }
        let data = self.hub.snapshot();
        if let Some(screen) = &mut self.screen {
            screen.show_symbols(&data.symbols);
        }
        let replaced = self.panel_shows();
        let look = self.panel.as_ref().filter(|_| replaced).and_then(|l| l.look(screen.1));
        let native = self.native(screen);
        let entries = self.config.chat_entries() as usize;
        let gear = self.gear.as_ref().map(GearLink::view).unwrap_or_default();
        let sample = self.editor.is_some();
        if let Some(e) = &mut self.editor {
            e.native = native;
        }
        let p = self.config.profile();
        for (kind, at) in editor::items(p, screen, dpi, native) {
            let style = &p.style(kind);
            let text = match kind {
                Kind::Keystrokes => {
                    hud::keystrokes(at, style, self.keys, &mut out);
                    continue;
                }
                Kind::Chat => {
                    let unit = kind.unit(screen, dpi) * p.modules[&kind].place.scale.clamp(0.25, 4.0);
                    let font = self.font.as_ref();
                    let symbols = &data.symbols;
                    let advance = |c: char| font.map_or(6.0, |f| f.advance(c, symbols));
                    if self.editor.is_some() {
                        let form = self.java().unwrap_or(java_chat::BEDROCK);
                        // Beispiele, damit Platz und Aussehen zu sehen sind.
                        let size = look.map_or(java_chat::unit(screen), |l| l.unit);
                        let count = entries.min(java_chat::native_rows(at.h, size, form));
                        java_chat::native_sample((at.x, at.y + at.h), (at.w, size), (count, form), &advance, &mut out);
                    } else {
                        // Eigene Meldungen kennt Minecraft nicht; sie stehen darüber.
                        let above = hud::Rect { y: at.y - at.h, ..at };
                        let width = (at.w / unit - java_chat::MARGINS).max(1.0);
                        let rows = note_rows(&self.notes, now, width, style.text, &advance);
                        java_chat::draw((above, unit), style, &rows, &advance, &mut out);
                    }
                    continue;
                }
                Kind::Armor => {
                    // Frei platziert; sonst weiter unten an der Schnellleiste.
                    let base = hud::armor_hud::size(&p.armor);
                    let unit = (at.w / base.0).min(at.h / base.1);
                    self.armor(None, (at.x, at.y), unit, &gear, &mut out);
                    continue;
                }
                Kind::Potions | Kind::Items => {
                    let rows = gear_rows(kind, &gear, &p.items, sample);
                    hud::gear::draw(at, kind.size(), style, &rows, &mut out);
                    continue;
                }
                Kind::Cps => format!("{} | {} CPS", self.left.value(now), self.right.value(now)),
                Kind::Clock => {
                    let t = unsafe { GetLocalTime() };
                    format!("{:02}:{:02}", t.wHour, t.wMinute)
                }
                Kind::Fps => self.fps.map_or("FPS …".into(), |f| format!("{f} FPS")),
                Kind::System => show(&data.system, "System …", |v| {
                    let gpu = v.gpu.map_or(String::new(), |g| format!("   GPU {g} %"));
                    format!("CPU {} %   RAM {} %{gpu}", v.cpu, v.ram)
                }),
                Kind::Battery => show(&data.battery, "Akku …", |b| format!("{} {b} %", t("Akku"))),
                Kind::Music => show(&data.music, "Musik …", |m| m.clone()),
                Kind::Ping => show(&data.ping, "Ping …", |ms| format!("{ms} ms")),
                Kind::Session => sources::clock_text(self.started.map_or(Duration::ZERO, |s| s.elapsed())),
                Kind::Stopwatch => sources::clock_text(self.stopwatch.value(now)),
                Kind::Note if p.note.is_empty() => t("Notiz").into(),
                Kind::Note => p.note.clone(),
                // Ohne Platz auf dem Bildschirm.
                Kind::Zoom | Kind::Players | Kind::AppleSkin | Kind::Crosshair | Kind::Fullbright => {
                    continue;
                }
            };
            hud::label(at, kind, style, &text, &mut out);
        }
        if p.modules[&Kind::Armor].enabled && p.armor.sticky && (sample || !self.in_menu || self.chat_open.is_some()) {
            let unit = java_chat::unit(screen);
            // Im Editor liegt die Leiste vielleicht unter einem Menü; dann dort, wo sie üblicherweise ist.
            let standard = || {
                let y = screen.1 - 24.0 * unit;
                hud::Rect { x: (screen.0 - 182.0 * unit) / 2.0, y, w: 182.0 * unit, h: 22.0 * unit }
            };
            let h = screen.1;
            let real = gear.bar.map(|(x, y, u)| hud::Rect { x: x * h, y: y * h, w: 182.0 * u * h, h: 22.0 * u * h });
            if let Some(bar) = real.or_else(|| sample.then(standard)) {
                self.armor(Some(bar), (0.0, 0.0), bar.w / 182.0, &gear, &mut out);
            }
        }
        if p.modules[&Kind::Crosshair].enabled {
            let time = self.born.elapsed().as_secs_f32();
            hud::crosshair::draw((screen.0 / 2.0, screen.1 / 2.0), &p.crosshair, dpi, time, &mut out);
        }
        // Nicht in Menüs: Dort verdeckt das Spiel seine Leisten.
        if self.editor.is_none()
            && (self.chat_open.is_some() || !self.in_menu)
            && let Some(v) = self.apple.as_ref().filter(|_| self.module(Kind::AppleSkin)).and_then(AppleLink::view)
        {
            let h = screen.1;
            let show = hud::appleskin::Show {
                saturation: self.config.apple_saturation.then_some(v.values.saturation),
                exhaustion: self.config.apple_exhaustion.then_some(v.values.exhaustion),
            };
            hud::appleskin::draw(v.right * h, v.top * h, v.unit * h, &show, &mut out);
        }
        if self.tab && self.chat_open.is_none() && self.editor.is_none() {
            let names = self.players.as_ref().map(PlayersLink::names).unwrap_or_default();
            let font = self.font.as_ref();
            let symbols = &data.symbols;
            let advance = |c: char| font.map_or(6.0, |f| f.advance(c, symbols));
            hud::tab_list::draw(screen, java_chat::unit(screen), &names, &advance, &mut out);
        }
        if let Some(since) = self.chat_open {
            // Wie in Java blinkt der Cursor alle 300 ms.
            let blink = (since.elapsed().as_millis() / 300).is_multiple_of(2);
            match &data.commands {
                Some(c) => self.hints.update(c, &self.chat_input),
                None => self.hints.clear(),
            }
            let font = self.font.as_ref();
            let symbols = &data.symbols;
            let advance = |c: char| font.map_or(6.0, |f| f.advance(c, symbols));
            let unit = java_chat::unit(screen);
            self.chat_input.draw(screen, unit, blink, &advance, &|i| self.hints.color(i), &mut out);
            self.hints.draw(&self.chat_input, screen, unit, &advance, &mut out);
        }
        let action = match &mut self.editor {
            Some(e) => e.frame(&mut self.config, screen, dpi, &self.mouse, &mut out),
            None => Action::None,
        };
        (out, action)
    }

    /// Rüstung an der Schnellleiste oder frei.
    fn armor(&self, bar: Option<hud::Rect>, at: (f32, f32), unit: f32, gear: &gear_link::View, out: &mut Vec<Draw>) {
        use hud::armor_hud::{self, Frame, Slot};
        let Some(icons) = hud::game_icons::get() else { return };
        let mut slots: [Slot; 4] = [None; 4];
        match &gear.own {
            Some(o) if !o.worn.is_empty() => {
                for s in &o.worn {
                    if let Some(slot) = slots.get_mut(s.slot as usize) {
                        *slot = Some((s.name.as_str(), s.damage));
                    }
                }
            }
            _ if self.editor.is_some() => {
                slots = [
                    Some(("diamond_helmet", 40)),
                    Some(("netherite_chestplate", 120)),
                    Some(("iron_leggings", 215)),
                    Some(("golden_boots", 40)),
                ];
            }
            _ => {}
        }
        let font = self.font.as_ref();
        let none = hud::symbols::Symbols::default();
        let width = |t: &str| t.chars().map(|c| font.map_or(6.0, |f| f.advance(c, &none))).sum();
        // Zwanzig Takte je Sekunde.
        let tick = (self.born.elapsed().as_millis() / 50) as u32;
        let f = Frame { icons, unit, width: &width, tick };
        let s = &self.config.profile().armor;
        match bar {
            Some(bar) => armor_hud::beside(bar, s, &slots, &f, out),
            None => armor_hud::free(at, s, &slots, &f, out),
        }
    }

    fn paint(&mut self) {
        let Some(size) = self.screen.as_ref().and_then(Screen::size) else { return };
        if self.font.is_none()
            && let Some(dir) = game::directory(self.game)
        {
            hud::game_icons::init(&dir);
            self.font = Some(Metrics::load(&dir.join("data").join("fonts").join("Mojangles.ttf")));
        }
        let mut scene = vec![];
        if self.shown {
            let dpi = unsafe { GetDpiForWindow(self.game) }.max(96) as f32 / 96.0;
            let action;
            (scene, action) = self.scene(size, dpi);
            match action {
                Action::Close => return self.close_editor(),
                Action::OpenFolder => {
                    self.close_editor();
                    return window::open_folder(self.path.as_deref());
                }
                Action::None => {}
            }
        }
        if let Some(s) = &mut self.screen {
            s.show(&scene);
        }
    }

    pub fn input(&mut self, raw: &RAWINPUT) {
        // Was in den Chat geht, zählt weder als Klick noch als Kürzel.
        if self.chat_open.is_some() {
            return;
        }
        if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
            self.shift(unsafe { raw.data.keyboard });
        }
        if raw.header.dwType == RIM_TYPEMOUSE.0 && self.editor.is_none() {
            self.menu_click(unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags } as u32);
        }
        // Tasten für andere Fenster gehen uns nichts an.
        if !self.shown || self.editor.is_some() || !self.in_front() {
            return;
        }
        let before = self.keys;
        let now = Instant::now();
        if raw.header.dwType == RIM_TYPEMOUSE.0 {
            let f = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags } as u32;
            if f & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                self.keys.lmb = true;
                self.left.click(now);
            }
            if f & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                self.keys.lmb = false;
            }
            if f & RI_MOUSE_RIGHT_BUTTON_DOWN != 0 {
                self.keys.rmb = true;
                self.right.click(now);
            }
            if f & RI_MOUSE_RIGHT_BUTTON_UP != 0 {
                self.keys.rmb = false;
            }
        } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
            let k = unsafe { raw.data.keyboard };
            let down = k.Flags as u32 & RI_KEY_BREAK == 0;
            match VIRTUAL_KEY(k.VKey) {
                VK_W => self.keys.w = down,
                VK_A => self.keys.a = down,
                VK_S => self.keys.s = down,
                VK_D => self.keys.d = down,
                VK_SPACE => self.keys.space = down,
                key => self.stopwatch_key(key, down),
            }
        }
        if self.keys != before {
            self.paint();
        }
    }

    /// Klick auf den nativen Knopf: auf der Knopffarbe drücken, in der Nähe loslassen.
    fn menu_click(&mut self, flags: u32) {
        if flags & (RI_MOUSE_LEFT_BUTTON_DOWN | RI_MOUSE_LEFT_BUTTON_UP) == 0 {
            return;
        }
        if flags & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
            self.armed = self.on_native_button();
            return;
        }
        // Beim Loslassen kann der Knopf schon anders aussehen; es reicht, dass der Zeiger in der Nähe blieb.
        let near = |a: POINT, b: POINT| (a.x - b.x).abs() <= 40 && (a.y - b.y).abs() <= 40;
        let mut p = POINT::default();
        let start = self.armed.take();
        if unsafe { GetCursorPos(&mut p) }.is_ok() && start.is_some_and(|s| near(s, p)) {
            self.open_after_menu = Some(Instant::now());
        }
    }

    /// Minecraft schließt sein Pausenmenü selbst. Erst danach öffnen, sonst nähme der Editor ihm den Klick weg.
    fn open_pending(&mut self) {
        let Some(since) = self.open_after_menu else { return };
        let closed = !cursor_visible();
        // Bleibt das Menü nach einem Update offen, kommt der Editor trotzdem.
        if closed || since.elapsed() > Duration::from_millis(500) {
            self.open_after_menu = None;
            if unsafe { GetForegroundWindow() } == self.game {
                self.open_editor(View::Full);
            }
        }
    }

    fn on_native_button(&self) -> Option<POINT> {
        unsafe {
            let front = GetForegroundWindow();
            let mut p = POINT::default();
            let hit = !front.is_invalid()
                && front == self.game
                && cursor_visible()
                && GetCursorPos(&mut p).is_ok()
                && WindowFromPoint(p) == front
                && marker::at(p);
            hit.then_some(p)
        }
    }

    fn shift(&mut self, k: RAWKEYBOARD) {
        if VIRTUAL_KEY(k.VKey) != VK_SHIFT {
            return;
        }
        let down = k.Flags as u32 & RI_KEY_BREAK == 0;
        if k.MakeCode != 0x36 {
            return;
        }
        // Tastenwiederholung zählt nicht.
        let first = down && !self.right_shift;
        self.right_shift = down;
        if first {
            self.hotkey();
        }
    }
}

/// Zeilen der drei Module; im Editor Beispiele, solange es nichts gibt.
fn gear_rows(kind: Kind, v: &gear_link::View, items: &str, sample: bool) -> Vec<hud::gear::Row> {
    use hud::gear;
    match (kind, &v.own, &v.effects) {
        (Kind::Potions, _, Some(e)) if !e.is_empty() => {
            gear::potions(&e.iter().map(|e| (e.id, e.left)).collect::<Vec<_>>())
        }
        (Kind::Potions, ..) if sample => gear::potions(&[(1, 1800), (5, 900), (12, 4200)]),
        (Kind::Items, Some(o), _) => {
            gear::counts(items, &o.main.iter().map(|s| (s.name.as_str(), s.count as u32)).collect::<Vec<_>>())
        }
        (Kind::Items, ..) if sample => gear::counts(items, &[]),
        _ => vec![],
    }
}

/// Frische eigene Meldungen, von alt nach neu.
fn note_rows(
    notes: &[sources::Line],
    now: Instant,
    width: f32,
    color: u32,
    advance: &dyn Fn(char) -> f32,
) -> Vec<(java_chat::Row, f32)> {
    let mut rows = vec![];
    for l in notes {
        let age = l.at.map(|t| now.saturating_duration_since(t).as_secs_f32());
        let alpha = age.map_or(0.0, java_chat::fade);
        if alpha > 0.0 {
            let wrapped = java_chat::wrap(&java_chat::parse(&l.text, color), width, advance);
            rows.extend(wrapped.into_iter().map(|w| (w, alpha)));
        }
    }
    rows
}

/// Wert, Platzhalter oder Grund, warum es hier nicht geht.
fn show<T>(r: &Reading<T>, pending: &str, f: impl FnOnce(&T) -> String) -> String {
    match r {
        Reading::Ready(v) => f(v),
        Reading::Pending => t(pending).into(),
        Reading::Off(why) => t(why).into(),
    }
}

/// Menü, Inventar und Chat zeigen den Mauszeiger, beim Spielen ist er eingefangen. Braucht keine Spieldaten.
/// Grenze: Mit Controller ist der Zeiger auch in Menüs aus.
pub fn cursor_visible() -> bool {
    let mut c = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
    // Im Zweifel anzeigen, damit das HUD nie wegen eines Fehlers ganz verschwindet.
    unsafe { GetCursorInfo(&mut c) }.is_ok() && c.flags.0 & CURSOR_SHOWING.0 != 0
}

fn client_area(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r).ok()?;
        let mut p = POINT::default();
        ClientToScreen(hwnd, &mut p).ok().ok()?;
        let area = RECT { left: p.x, top: p.y, right: p.x + r.right, bottom: p.y + r.bottom };
        (r.right > 0 && r.bottom > 0).then_some(area)
    }
}
