#![no_std]
#![no_main]

extern crate alloc;
mod platform;
mod player;

use alloc::{format, string::String, vec, vec::Vec};
use platform::{DirectoryScan, sounda_audio_init};
use player::Player;
use sounda::{
    Result,
    activity::Activity,
    browser::{self, Browser, Entry},
    keyboard::{self, Keyboard, PAGE_KEYS},
    playback::{Playback, QueueTarget},
    power::{Lid, State},
    ui::{self, NowPlaying, PIXELS, Pane, TouchAction},
};

#[global_allocator]
static ALLOCATOR: platform::Allocator = platform::Allocator;

struct App {
    browser: Browser,
    queue: Browser,
    scan: Option<DirectoryScan>,
    playback: Playback,
    activity: Activity,
    listing: bool,
    scroll_drag: Option<u32>,
    pane: Pane,
    keyboard: Keyboard,
    keyboard_page: usize,
    filename_chars: Vec<char>,
    keyboard_keys: Vec<char>,
    player: Option<Player>,
    error: Option<&'static str>,
    ready: bool,
    dirty: bool,
    lid: Lid,
    suspended_at: u32,
    seek_drag: Option<u16>,
    seeks: u32,
    clock: u32,
    clock_at: u32,
}

impl App {
    fn new() -> Self {
        let mut queue = Browser::new();
        queue.replace_ordered("UP NEXT / 0 SONGS".into(), Vec::new(), false);
        Self {
            browser: Browser::new(),
            queue,
            scan: None,
            playback: Playback::new(platform::milliseconds()),
            activity: Activity::default(),
            listing: false,
            scroll_drag: None,
            pane: Pane::Library,
            keyboard: Keyboard::Latin,
            keyboard_page: 0,
            filename_chars: Vec::new(),
            keyboard_keys: Keyboard::Latin.keys(&[]),
            player: None,
            error: None,
            ready: false,
            dirty: true,
            lid: Lid::default(),
            suspended_at: 0,
            seek_drag: None,
            seeks: 0,
            clock: unsafe { platform::sounda_rtc() },
            clock_at: platform::milliseconds(),
        }
    }
    fn view(&self) -> &Browser {
        if self.pane == Pane::Queue {
            &self.queue
        } else {
            &self.browser
        }
    }
    fn view_mut(&mut self) -> &mut Browser {
        if self.pane == Pane::Queue {
            &mut self.queue
        } else {
            &mut self.browser
        }
    }
    fn layout(&mut self) {
        let rows = ui::view_rows(self.pane, self.listing);
        self.view_mut().set_visible_rows(rows);
    }
    fn browsing(&mut self) {
        self.activity.touch(platform::milliseconds());
        self.listing = true;
        self.layout();
        self.dirty = true;
    }
    fn idle_tick(&mut self, now: u32) {
        let listing = self.activity.active(now);
        if listing != self.listing {
            self.listing = listing;
            self.layout();
            self.dirty = true;
        }
    }
    fn show_pane(&mut self, pane: Pane) {
        self.pane = pane;
        self.seek_drag = None;
        self.scroll_drag = None;
        self.browsing();
    }
    fn move_selection(&mut self, by: isize) {
        self.browsing();
        self.view_mut().move_by(by);
    }
    fn folder(&mut self, path: String) {
        match DirectoryScan::open(path.clone()) {
            Ok(scan) => {
                self.browser.replace(path, Vec::new(), false);
                self.scan = Some(scan);
                self.error = None;
                self.pane = Pane::Library;
                self.filename_chars.clear();
            }
            Err(e) => self.error = Some(e),
        }
        self.browsing();
    }
    fn scan_step(&mut self) {
        if self.browser.search_step() {
            self.layout();
            self.dirty = true;
        }
        let Some(scan) = &mut self.scan else {
            return;
        };
        match scan.step() {
            Ok(false) => {}
            Ok(true) => {
                let mut scan = self.scan.take().unwrap();
                self.browser.replace(
                    core::mem::take(&mut scan.path),
                    core::mem::take(&mut scan.entries),
                    scan.limited,
                );
                self.filename_chars =
                    keyboard::filename_characters(self.browser.entries.iter().map(|e| &e.name));
                self.refresh_keyboard();
                self.layout();
                self.dirty = true;
            }
            Err(error) => {
                self.scan = None;
                self.error = Some(error);
                self.dirty = true;
            }
        }
    }
    fn refresh_keyboard(&mut self) {
        self.keyboard_keys = self.keyboard.keys(&self.filename_chars);
        self.keyboard_page %= self.keyboard_keys.len().div_ceil(PAGE_KEYS).max(1);
    }
    fn sync_queue(&mut self, selected: usize) {
        let songs = self.playback.queue();
        self.queue.replace_ordered(
            format!("UP NEXT / {} SONGS", songs.len()),
            songs
                .iter()
                .enumerate()
                .map(|(i, path)| Entry {
                    name: format!("{:03} {}", i + 1, path.rsplit('/').next().unwrap_or(path)),
                    directory: false,
                })
                .collect(),
            false,
        );
        self.queue
            .set_visible_rows(ui::view_rows(Pane::Queue, self.listing));
        self.queue.move_by(selected as isize);
        self.dirty = true;
    }
    fn load_current(&mut self) {
        self.seek_drag = None;
        self.player = None;
        self.error = None;
        if let Some(path) = self.playback.current() {
            match Player::open(path) {
                Ok(player) => self.player = Some(player),
                Err(error) => {
                    unsafe { platform::sounda_dsp_off() };
                    self.error = Some(error);
                }
            }
        }
        self.dirty = true;
    }
    fn selected_path(&self) -> Option<String> {
        if self.scan.is_some() || self.browser.searching() {
            return None;
        }
        let entry = self.browser.selected_entry()?;
        if entry.directory {
            return None;
        }
        browser::join(&self.browser.path, &entry.name).ok()
    }
    fn activate(&mut self) {
        if !self.ready {
            return;
        }
        self.browsing();
        if self.pane == Pane::Queue {
            let selected = self.queue.selected;
            if self.playback.play_queued(selected) {
                self.sync_queue(selected);
                self.load_current();
            }
            return;
        }
        if self.scan.is_some() || self.browser.searching() {
            return;
        }
        let Some(entry) = self.browser.selected_entry().cloned() else {
            return;
        };
        let path = match browser::join(&self.browser.path, &entry.name) {
            Ok(path) => path,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        if entry.directory {
            self.folder(path);
            return;
        }
        let paths: Vec<String> = self
            .browser
            .music()
            .iter()
            .filter_map(|name| browser::join(&self.browser.path, name).ok())
            .collect();
        let current = paths.iter().position(|song| *song == path).unwrap_or(0);
        if self.playback.play_folder(paths, current) {
            self.load_current();
        }
    }
    fn add_selected(&mut self) {
        self.browsing();
        if let Some(path) = self.selected_path() {
            if let Err(error) = self.playback.add(path) {
                self.error = Some(error);
            } else {
                self.sync_queue(self.playback.queue().len() - 1);
            }
        }
    }
    fn move_queue_to(&mut self, to: usize) {
        let from = self.queue.selected;
        self.playback.move_item(from, to);
        self.sync_queue(to);
        self.browsing();
    }
    fn move_queue(&mut self, by: isize) {
        let to = self
            .queue
            .selected
            .saturating_add_signed(by)
            .min(self.playback.queue().len().saturating_sub(1));
        self.move_queue_to(to);
    }
    fn jump_queue(&mut self, target: QueueTarget) {
        if let Some(to) = self.playback.move_to(self.queue.selected, target) {
            self.sync_queue(to);
            self.browsing();
        }
    }
    fn remove_selected(&mut self) {
        let selected = self.queue.selected;
        self.playback.remove(selected);
        self.sync_queue(selected);
        self.browsing();
    }
    fn toggle_play(&mut self) {
        if let Some(player) = &mut self.player {
            player.pause(!player.paused);
        } else {
            let before = self.playback.queue().len();
            if self.playback.start() {
                if self.playback.queue().len() != before {
                    self.sync_queue(self.queue.selected.saturating_sub(1));
                }
                self.load_current();
            } else {
                self.activate();
            }
        }
        self.dirty = true;
    }
    fn advance(&mut self, automatic: bool) {
        let before = self.playback.queue().len();
        if self.playback.advance(automatic) {
            if self.playback.queue().len() != before {
                self.sync_queue(self.queue.selected.saturating_sub(1));
            }
            self.load_current();
        } else if automatic {
            self.player = None;
            self.dirty = true;
        } else if self.playback.current().is_none() {
            self.activate();
        }
    }
    fn edit_query(&mut self, key: Option<char>) {
        self.browsing();
        let mut query: String = self.browser.query().into();
        if let Some(key) = key {
            if query.chars().count() < 48 {
                query.push(key);
            }
        } else {
            query.pop();
        }
        self.browser.set_query(query);
        self.dirty = true;
    }
    fn back(&mut self) {
        if self.pane != Pane::Library {
            self.show_pane(Pane::Library);
        } else if !self.browser.query().is_empty() {
            self.browser.set_query(String::new());
            self.browsing();
        } else if self.ready {
            self.folder(self.browser.parent_path());
        }
    }
    fn scroll_touch(&mut self, y: u32, grab: u32) {
        self.browsing();
        let fraction = ui::scroll_fraction(self.view(), self.pane, y, grab);
        self.view_mut().scroll_to(fraction);
        self.dirty = true;
    }
    fn action(&mut self, action: TouchAction) {
        match action {
            TouchAction::None => {}
            TouchAction::Parent => self.back(),
            TouchAction::Pane(pane) => self.show_pane(pane),
            TouchAction::PageUp => self.move_selection(-(self.view().visible_rows() as isize)),
            TouchAction::Row(row) => {
                let index = ui::bottom_first(self.view(), self.listing) + row;
                if index < self.view().len() {
                    let repeated = index == self.view().selected;
                    self.view_mut().selected = index;
                    self.browsing();
                    if self.pane != Pane::Queue || repeated {
                        self.activate();
                    }
                }
            }
            TouchAction::Previous => {
                if self.playback.previous() {
                    self.load_current();
                }
            }
            TouchAction::PlayPause => self.toggle_play(),
            TouchAction::Next => self.advance(false),
            TouchAction::Stop => {
                self.player = None;
                self.seek_drag = None;
                self.error = None;
            }
            TouchAction::Mode(mode) => self.playback.set_mode(mode),
            TouchAction::Add => self.add_selected(),
            TouchAction::MoveUp => self.move_queue(-1),
            TouchAction::MoveDown => self.move_queue(1),
            TouchAction::QueueJump(target) => self.jump_queue(target),
            TouchAction::Remove => self.remove_selected(),
            TouchAction::Keyboard(keyboard) => {
                self.keyboard = keyboard;
                self.keyboard_page = 0;
                self.refresh_keyboard();
                self.browsing();
            }
            TouchAction::Key(index) => {
                if let Some(&key) = self
                    .keyboard_keys
                    .get(self.keyboard_page * PAGE_KEYS + index)
                {
                    self.edit_query(Some(key));
                }
            }
            TouchAction::Backspace => self.edit_query(None),
            TouchAction::KeyboardPage => {
                self.keyboard_page += 1;
                self.refresh_keyboard();
                self.browsing();
            }
            TouchAction::Seek(fraction) => {
                if self.player.is_some() {
                    self.seek_drag = Some(fraction);
                }
            }
            TouchAction::Scroll(y) => {
                self.browsing();
                let (top, height) = ui::scroll_thumb(self.view(), self.pane);
                let grab = if (top..top + height).contains(&y) {
                    y - top
                } else {
                    height / 2
                };
                self.scroll_drag = Some(grab);
                self.scroll_touch(y, grab);
            }
        }
        self.dirty = true;
    }
    fn power(&mut self, held: u32) {
        let closed = held & 8192 != 0;
        let change = self.lid.update(
            closed,
            unsafe { platform::sounda_headphones() } != 0,
            self.player.as_ref().is_some_and(|p| !p.paused),
        );
        if change.pause
            && let Some(player) = &mut self.player
        {
            player.pause(true);
        }
        if change.suspend {
            self.suspended_at = platform::milliseconds();
            unsafe {
                platform::sounda_dsp_clock(0);
            }
        }
        if change.wake {
            unsafe {
                platform::sounda_dsp_clock(1);
            }
            if let Some(player) = &mut self.player {
                player.wake(platform::milliseconds().wrapping_sub(self.suspended_at));
                if change.resume {
                    player.pause(false);
                }
            }
        }
    }
    fn input(&mut self, keys: u32, held: u32) -> bool {
        if self.lid.state != State::Open {
            self.seek_drag = None;
            self.scroll_drag = None;
            return true;
        }
        if held & 12 == 12 && keys & 12 != 0 {
            return false;
        }
        if (keys != 0 || held & 4096 != 0) && self.listing {
            self.activity.touch(platform::milliseconds());
        }
        if keys & 4 != 0 {
            self.show_pane(if self.pane == Pane::Queue {
                Pane::Library
            } else {
                Pane::Queue
            });
        }
        if keys & 64 != 0 {
            self.move_selection(-1);
        }
        if keys & 128 != 0 {
            self.move_selection(1);
        }
        if self.pane == Pane::Queue {
            if keys & 32 != 0 {
                self.move_queue(-1);
            }
            if keys & 16 != 0 {
                self.move_queue(1);
            }
        } else {
            if keys & 32 != 0 {
                self.move_selection(-(self.view().visible_rows() as isize));
            }
            if keys & 16 != 0 {
                self.move_selection(self.view().visible_rows() as isize);
            }
        }
        if keys & 1 != 0 {
            self.activate();
        }
        if keys & 2 != 0 {
            self.action(TouchAction::Parent);
        }
        if keys & 8 != 0 {
            self.toggle_play();
        }
        if keys & 512 != 0 {
            self.action(TouchAction::Previous);
        }
        if keys & 256 != 0 {
            self.advance(false);
        }
        if keys & 1024 != 0 {
            self.playback.set_mode(self.playback.mode.next());
        }
        if keys & 2048 != 0 {
            if self.pane == Pane::Queue {
                self.remove_selected();
            } else {
                self.add_selected();
            }
        }
        if keys & 4096 != 0 {
            let xy = unsafe { platform::sounda_touch() };
            self.action(ui::hit_test_view(xy & 65535, xy >> 16, self.pane));
        }
        if let Some(grab) = self.scroll_drag {
            if held & 4096 != 0 {
                let xy = unsafe { platform::sounda_touch() };
                self.scroll_touch(xy >> 16, grab);
            } else {
                self.scroll_drag = None;
            }
        }
        if self.seek_drag.is_some() {
            if held & 4096 != 0 {
                let xy = unsafe { platform::sounda_touch() };
                let x = (xy & 65535).clamp(15, 240);
                self.seek_drag = Some(((x - 15) * u16::MAX as u32 / 225) as u16);
                self.dirty = true;
            } else if let Some(fraction) = self.seek_drag.take()
                && let Some(player) = &mut self.player
            {
                if let Err(error) = player.seek(fraction) {
                    self.player = None;
                    self.error = Some(error);
                } else {
                    self.seeks += 1;
                }
                self.dirty = true;
            }
        }
        if keys != 0 {
            self.dirty = true;
        }
        true
    }
    fn telemetry(&self, now: u32) {
        unsafe {
            platform::sounda_diagnostic(20, self.pane as u32);
            platform::sounda_diagnostic(21, self.playback.queue().len() as u32);
            platform::sounda_diagnostic(
                22,
                self.playback
                    .sequence()
                    .map_or(u32::MAX, |(i, _)| (i - 1) as u32),
            );
            platform::sounda_diagnostic(23, self.player.as_ref().map_or(0, |p| p.elapsed()));
            platform::sounda_diagnostic(24, self.player.as_ref().map_or(0, |p| p.duration()));
            platform::sounda_diagnostic(
                25,
                self.player.as_ref().map_or(0, |p| p.position() as u32),
            );
            platform::sounda_diagnostic(26, self.lid.state as u32);
            platform::sounda_diagnostic(27, platform::sounda_headphones() as u32);
            platform::sounda_diagnostic(28, u32::from(self.browser.searching()));
            platform::sounda_diagnostic(29, self.browser.len() as u32);
            platform::sounda_diagnostic(30, self.seeks);
            platform::sounda_diagnostic(
                31,
                u32::from(self.player.as_ref().is_some_and(|p| p.paused)),
            );
            platform::sounda_diagnostic(32, u32::from(self.listing));
            platform::sounda_diagnostic(33, self.view().selected as u32);
            platform::sounda_diagnostic(34, self.view().first as u32);
            platform::sounda_diagnostic(35, self.view().visible_rows() as u32);
            platform::sounda_diagnostic(36, self.queue.selected as u32);
            platform::sounda_diagnostic(37, path_hash(self.playback.current().unwrap_or("")));
            platform::sounda_diagnostic(38, u32::from(self.player.is_some()));
            platform::sounda_diagnostic(
                39,
                path_hash(self.playback.queue().first().map_or("", |s| s.as_str())),
            );
            platform::sounda_diagnostic(17, now);
        }
    }
    fn draw(
        &mut self,
        renderer: &mut ui::Renderer,
        top: &mut [u16],
        bottom: &mut [u16],
    ) -> Result<ui::Damage> {
        let path = self.playback.current().unwrap_or("");
        let name = path.rsplit('/').next().unwrap_or(path);
        let title = name
            .strip_suffix(".m4a")
            .or_else(|| name.strip_suffix(".M4A"))
            .unwrap_or(name);
        let folder = path
            .rsplit_once('/')
            .and_then(|(p, _)| p.rsplit('/').next())
            .unwrap_or("SELECT A FOLDER TO BEGIN");
        let (mut elapsed, duration, paused, status, mut position) = self
            .player
            .as_ref()
            .map(|p| {
                (
                    p.elapsed(),
                    p.duration(),
                    p.paused,
                    p.status(),
                    p.position(),
                )
            })
            .unwrap_or((0, 0, false, "SELECT A SONG", 0));
        if let Some(fraction) = self.seek_drag {
            position = fraction;
            elapsed = duration * fraction as u32 / u16::MAX as u32;
        }
        let browser = if self.pane == Pane::Queue {
            &self.queue
        } else {
            &self.browser
        };
        let player = &mut self.player;
        let mut fault = None;
        let start = self.keyboard_page * PAGE_KEYS;
        let damage = renderer.render(
            top,
            bottom,
            browser,
            &NowPlaying {
                title,
                folder,
                status,
                elapsed,
                duration,
                paused,
                loaded: player.is_some(),
                mode: self.playback.mode,
                error: self.error,
                scanning: self.pane != Pane::Queue && self.scan.is_some(),
                sequence: self.playback.sequence(),
                position,
                clock: self.clock,
                pane: self.pane,
                queue_len: self.playback.queue().len(),
                listing: self.listing,
                keyboard: self.keyboard,
                keyboard_page: self.keyboard_page,
                keyboard_keys: &self.keyboard_keys
                    [start..(start + PAGE_KEYS).min(self.keyboard_keys.len())],
            },
            || {
                if fault.is_none()
                    && let Some(player) = player
                    && let Err(error) = player.pump()
                {
                    fault = Some(error);
                }
            },
        );
        fault.map_or(Ok(damage), Err)
    }
}

fn path_hash(path: &str) -> u32 {
    path.bytes().fold(2166136261u32, |hash, b| {
        (hash ^ b as u32).wrapping_mul(16777619)
    })
}

fn initialize(app: &mut App) -> Result<()> {
    if unsafe { platform::sounda_is_dsi() } == 0 {
        return Err("Requires Nintendo DSi mode and its SD card slot.");
    }
    if unsafe { platform::sounda_mount_sd() } != 0 {
        return Err("Cannot mount SD. Check the card and DSi-mode loader.");
    }
    if unsafe { sounda_audio_init() } != 0 {
        return Err("Unable to initialize audio output");
    }
    app.ready = true;
    app.folder(browser::ROOT.into());
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    unsafe { platform::sounda_init() };
    let mut app = App::new();
    if let Err(error) = initialize(&mut app) {
        app.error = Some(error);
    }
    let mut top = vec![0u16; PIXELS];
    let mut bottom = vec![0u16; PIXELS];
    let mut renderer = ui::Renderer::new();
    let mut redraws = 0u32;
    let mut max_draw_ms = 0;
    let mut input_at = platform::milliseconds().wrapping_sub(16);
    let mut draw_at = input_at.wrapping_sub(100);
    let mut lid_was_closed = false;
    loop {
        let now = platform::milliseconds();
        if now.wrapping_sub(input_at) >= 16 {
            input_at = now;
            let keys = unsafe { platform::sounda_keys() };
            let held = unsafe { platform::sounda_held() };
            app.power(held);
            let closed = app.lid.state != State::Open;
            if closed != lid_was_closed {
                unsafe {
                    platform::sounda_backlights(i32::from(!closed));
                }
                lid_was_closed = closed;
                app.dirty = true;
            }
            if !app.input(keys, held) {
                break;
            }
            if app.lid.state == State::Open {
                app.idle_tick(now);
            }
            app.telemetry(now);
        }
        if app.lid.state == State::Suspended {
            // No decoding, SD scanning, search, clock reads, rasterization or
            // audio refill. Halt until an interrupt; ARM7 still senses wake.
            unsafe {
                platform::sounda_vblank();
            }
            continue;
        }
        if let Some(player) = &mut app.player
            && let Err(error) = player.pump()
        {
            app.player = None;
            app.error = Some(error);
            app.dirty = true;
        }
        if app.player.as_ref().is_some_and(|p| p.finished()) {
            app.advance(true);
        }
        if app.lid.state == State::Open {
            app.scan_step();
            if now.wrapping_sub(app.clock_at) >= 1000 {
                app.clock_at = now;
                app.clock = unsafe { platform::sounda_rtc() };
            }
            if app.dirty || now.wrapping_sub(draw_at) >= 200 {
                let started = platform::milliseconds();
                match app.draw(&mut renderer, &mut top, &mut bottom) {
                    Ok(damage) => {
                        for (screen, rows, frame) in
                            [(0, &damage.top, &top), (1, &damage.bottom, &bottom)]
                        {
                            for run in rows.runs() {
                                unsafe {
                                    platform::sounda_present_rows(
                                        screen,
                                        run.start as u32,
                                        run.len() as u32,
                                        frame.as_ptr(),
                                    );
                                }
                            }
                        }
                        if !damage.is_empty() {
                            redraws = redraws.wrapping_add(1);
                            max_draw_ms =
                                max_draw_ms.max(platform::milliseconds().wrapping_sub(started));
                            unsafe {
                                platform::sounda_diagnostic(10, redraws);
                                platform::sounda_diagnostic(11, max_draw_ms);
                            }
                        }
                        app.dirty = false;
                    }
                    Err(error) => {
                        renderer.invalidate();
                        app.player = None;
                        app.error = Some(error);
                        app.dirty = true;
                    }
                }
                draw_at = now;
            }
        }
        let background_work =
            app.lid.state == State::Open && (app.scan.is_some() || app.browser.searching());
        if !background_work && !app.player.as_ref().is_some_and(|p| p.needs_service()) {
            unsafe {
                platform::sounda_vblank();
            }
        }
    }
    if let Some(p) = &app.player {
        let _ = (p.sample_rate(), p.underruns());
    }
    app.player = None;
    unsafe {
        platform::sounda_dsp_off();
        platform::sounda_backlights(1);
    }
    0
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { platform::sounda_fatal() };
    loop {
        unsafe { platform::sounda_vblank() };
    }
}
