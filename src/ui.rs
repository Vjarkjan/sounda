//! Retained RGB555 interface shared by the ROM and host preview.
//!
//! Like Devy's `video::IndexedFrame`, presentation records dirty scanlines and
//! leaves unchanged pixels in the DS 2D background. Here the retained widget
//! state also avoids rerasterizing those pixels. RGB555 keeps the existing
//! two-screen renderer and does not require another video runtime to own VRAM.
use crate::{
    browser::{Browser, VISIBLE_ROWS},
    playlist::Mode,
};
use crate::{font, keyboard::Keyboard, playback::QueueTarget};
use alloc::vec::Vec;
use alloc::{format, string::String};
use core::ops::Range;

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 192;
pub const PIXELS: usize = WIDTH * HEIGHT;
pub const TOP_LIST_ROWS: usize = 8;
pub const LIST_Y: usize = 32;
pub const ROW_HEIGHT: usize = 18;
pub const fn bottom_rows(pane: Pane) -> usize {
    if matches!(pane, Pane::Search) {
        5
    } else {
        VISIBLE_ROWS
    }
}
pub fn view_rows(pane: Pane, listing: bool) -> usize {
    bottom_rows(pane) + if listing { TOP_LIST_ROWS } else { 0 }
}
pub fn bottom_first(b: &Browser, listing: bool) -> usize {
    b.first + if listing { TOP_LIST_ROWS } else { 0 }
}

const fn rgb(r: u16, g: u16, b: u16) -> u16 {
    0x8000 | ((r >> 3) & 31) | (((g >> 3) & 31) << 5) | (((b >> 3) & 31) << 10)
}
const BG: u16 = rgb(10, 21, 38);
const PANEL: u16 = rgb(22, 39, 56);
const EDGE: u16 = rgb(37, 58, 76);
const MINT: u16 = rgb(70, 222, 186);
const WHITE: u16 = rgb(232, 244, 248);
const MUTED: u16 = rgb(145, 168, 186);
const WARNING: u16 = rgb(255, 176, 129);

pub struct Canvas<'a> {
    pixels: &'a mut [u16],
}
impl<'a> Canvas<'a> {
    pub fn new(pixels: &'a mut [u16]) -> Self {
        assert_eq!(pixels.len(), PIXELS);
        Self { pixels }
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
            self.pixels[y as usize * WIDTH + x as usize] = color;
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        for row in y.max(0)..(y + h).min(HEIGHT as i32) {
            let a = x.clamp(0, WIDTH as i32) as usize;
            let b = (x + w).clamp(0, WIDTH as i32).max(a as i32) as usize;
            self.pixels[row as usize * WIDTH + a..row as usize * WIDTH + b].fill(color);
        }
    }
    fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        self.rect(x + 2, y, w - 4, h, c);
        self.rect(x, y + 2, w, h - 4, c);
        self.pixel(x + 1, y + 1, c);
        self.pixel(x + w - 2, y + 1, c);
        self.pixel(x + 1, y + h - 2, c);
        self.pixel(x + w - 2, y + h - 2, c);
    }
    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u16) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.pixel(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e = 2 * error;
            if e >= dy {
                error += dy;
                x += sx;
            }
            if e <= dx {
                error += dx;
                y += sy;
            }
        }
    }
    fn disc(&mut self, x: i32, y: i32, r: i32, color: u16) {
        for yy in -r..=r {
            for xx in -r..=r {
                if xx * xx + yy * yy <= r * r {
                    self.pixel(x + xx, y + yy, color);
                }
            }
        }
    }
    fn text(&mut self, x: i32, y: i32, text: &str, color: u16, scale: i32, max: usize) {
        for (i, ch) in text.chars().take(max).enumerate() {
            for (row, bits) in glyph(ch).iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(
                            x + i as i32 * 6 * scale + col * scale,
                            y + row as i32 * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
    }
    fn center(&mut self, y: i32, text: &str, color: u16, scale: i32, max: usize) {
        let len = text.chars().count().min(max) as i32;
        self.text(
            (256 - len * 6 * scale + scale) / 2,
            y,
            text,
            color,
            scale,
            max,
        );
    }
    fn label(&mut self, x: i32, y: i32, text: &str, color: u16, limit: usize) {
        let total = font::width(text, usize::MAX);
        let clipped = total > limit;
        let available = if clipped {
            limit.saturating_sub(8)
        } else {
            limit
        };
        let mut pen = 0usize;
        for c in crate::unicode::characters(text) {
            let glyph = font::glyph(c);
            if pen + glyph.width() > available {
                break;
            }
            self.character(x + pen as i32, y, glyph, color);
            pen += glyph.width();
        }
        if clipped {
            self.character(x + pen as i32, y, font::glyph('…'), color);
        }
    }
    fn character(&mut self, x: i32, y: i32, glyph: font::Glyph, color: u16) {
        for row in 0..16 {
            let bits = glyph.row(row);
            for col in 0..glyph.width() {
                if bits & (0x8000 >> col) != 0 {
                    self.pixel(x + col as i32, y + row as i32, color);
                }
            }
        }
    }
    fn center_label(&mut self, y: i32, text: &str, color: u16) {
        let width = font::width(text, usize::MAX).min(232);
        self.label((256 - width as i32) / 2, y, text, color, 232);
    }
    fn folder(&mut self, x: i32, y: i32, c: u16) {
        self.rect(x, y, 6, 2, c);
        self.round_rect(x, y + 2, 12, 8, c);
        self.rect(x + 2, y + 4, 8, 4, PANEL);
    }
    fn note(&mut self, x: i32, y: i32, c: u16) {
        self.rect(x + 7, y, 2, 10, c);
        self.rect(x + 7, y, 5, 2, c);
        self.disc(x + 5, y + 10, 3, c);
    }
    fn play(&mut self, x: i32, y: i32, c: u16) {
        for i in 0..10 {
            self.rect(x + i, y + i / 2, 1, 11 - i, c);
        }
    }
    fn previous(&mut self, x: i32, y: i32, c: u16) {
        self.rect(x, y, 2, 11, c);
        for i in 0..9 {
            self.rect(x + 3 + i, y + 5 - i / 2, 1, 1 + (i / 2) * 2, c);
        }
    }
    fn next(&mut self, x: i32, y: i32, c: u16) {
        self.play(x, y, c);
        self.rect(x + 12, y, 2, 11, c);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Library,
    Queue,
    Search,
}

pub struct NowPlaying<'a> {
    pub title: &'a str,
    pub folder: &'a str,
    pub status: &'a str,
    pub elapsed: u32,
    pub duration: u32,
    pub paused: bool,
    pub loaded: bool,
    pub mode: Mode,
    pub error: Option<&'a str>,
    pub scanning: bool,
    pub sequence: Option<(usize, usize)>,
    pub position: u16,
    pub clock: u32,
    pub pane: Pane,
    pub queue_len: usize,
    pub keyboard: Keyboard,
    pub keyboard_page: usize,
    pub keyboard_keys: &'a [char],
    pub listing: bool,
}
impl Default for NowPlaying<'_> {
    fn default() -> Self {
        Self {
            title: "",
            folder: "",
            status: "SELECT A SONG",
            elapsed: 0,
            duration: 0,
            paused: false,
            loaded: false,
            mode: Mode::Sequential,
            error: None,
            scanning: false,
            sequence: None,
            position: 0,
            clock: 0,
            pane: Pane::Library,
            queue_len: 0,
            keyboard: Keyboard::Latin,
            keyboard_page: 0,
            keyboard_keys: &[],
            listing: false,
        }
    }
}

pub fn render(top: &mut [u16], bottom: &mut [u16], browser: &Browser, now: &NowPlaying<'_>) {
    draw_top(&mut Canvas::new(top), browser, now);
    draw_bottom(&mut Canvas::new(bottom), browser, now);
}

/// Scanlines changed by one render. Adjacent lines are returned as one upload
/// so a presenter can flush/copy only those parts of a retained 2D background.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowDamage {
    bits: [u32; HEIGHT / 32],
}

impl RowDamage {
    fn mark(&mut self, rows: Range<usize>) {
        for y in rows {
            self.bits[y / 32] |= 1 << (y % 32);
        }
    }
    fn contains(&self, row: usize) -> bool {
        self.bits[row / 32] & (1 << (row % 32)) != 0
    }
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|word| *word == 0)
    }
    pub fn row_count(&self) -> usize {
        self.bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
    pub fn runs(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        let mut next = 0;
        core::iter::from_fn(move || {
            while next < HEIGHT && !self.contains(next) {
                next += 1;
            }
            if next == HEIGHT {
                return None;
            }
            let first = next;
            while next < HEIGHT && self.contains(next) {
                next += 1;
            }
            Some(first..next)
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Damage {
    pub top: RowDamage,
    pub bottom: RowDamage,
}

impl Damage {
    pub fn is_empty(&self) -> bool {
        self.top.is_empty() && self.bottom.is_empty()
    }
}

#[derive(Default)]
struct VisibleRow {
    name: String,
    directory: bool,
    selected: bool,
    present: bool,
}
impl VisibleRow {
    fn changed(&self, browser: &Browser, index: usize) -> bool {
        match browser.entry(index) {
            Some(entry) => {
                !self.present
                    || self.name != entry.name
                    || self.directory != entry.directory
                    || self.selected != (index == browser.selected)
            }
            None => self.present,
        }
    }
    fn remember(&mut self, browser: &Browser, index: usize) {
        self.present = browser.entry(index).is_some();
        if let Some(entry) = browser.entry(index) {
            remember_string(&mut self.name, &entry.name);
            self.directory = entry.directory;
            self.selected = index == browser.selected;
        }
    }
}

/// Retained widget and visible-row state; unchanged ticks do no raster work.
pub struct Renderer {
    valid: bool,
    title: String,
    folder: String,
    status: String,
    error: Option<String>,
    elapsed: u32,
    duration: u32,
    sequence: Option<(usize, usize)>,
    loaded: bool,
    paused: bool,
    mode: Mode,
    path: String,
    scanning: bool,
    first: usize,
    selected: usize,
    count: usize,
    limited: bool,
    rows: [VisibleRow; VISIBLE_ROWS],
    top_rows: [VisibleRow; TOP_LIST_ROWS],
    clock: u32,
    position: u16,
    pane: Pane,
    query: String,
    searching: bool,
    queue_len: usize,
    keyboard: Keyboard,
    keyboard_page: usize,
    keyboard_keys: Vec<char>,
    listing: bool,
}
impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
impl Renderer {
    pub fn new() -> Self {
        Self {
            valid: false,
            title: String::new(),
            folder: String::new(),
            status: String::new(),
            error: None,
            elapsed: 0,
            duration: 0,
            sequence: None,
            loaded: false,
            paused: false,
            mode: Mode::Sequential,
            path: String::new(),
            scanning: false,
            first: 0,
            selected: 0,
            count: 0,
            limited: false,
            rows: core::array::from_fn(|_| VisibleRow::default()),
            top_rows: core::array::from_fn(|_| VisibleRow::default()),
            clock: 0,
            position: 0,
            pane: Pane::Library,
            query: String::new(),
            searching: false,
            queue_len: 0,
            keyboard: Keyboard::Latin,
            keyboard_page: 0,
            keyboard_keys: Vec::new(),
            listing: false,
        }
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    pub fn render(
        &mut self,
        top: &mut [u16],
        bottom: &mut [u16],
        browser: &Browser,
        now: &NowPlaying<'_>,
        mut service: impl FnMut(),
    ) -> Damage {
        let mut damage = Damage::default();
        let mut top = Canvas::new(top);
        let mut bottom = Canvas::new(bottom);
        let header_changed = self.path != browser.path
            || self.scanning != now.scanning
            || self.query != browser.query()
            || self.searching != browser.searching()
            || self.count != browser.len()
            || self.queue_len != now.queue_len
            || self.limited != browser.limited;
        if !self.valid || self.listing != now.listing {
            draw_top(&mut top, browser, now);
            damage.top.mark(0..HEIGHT);
            service();
        } else if now.listing {
            if header_changed
                || self.selected != browser.selected
                || self.pane != now.pane
                || self.error.as_deref() != now.error
            {
                top_background(&mut top, 0..44);
                top_list_header(&mut top, browser, now);
                damage.top.mark(0..44);
                service();
            } else if self.clock != now.clock {
                top_clock(&mut top, now.clock);
                damage.top.mark(5..25);
                service();
            }
            let empty_change = (self.count == 0) != browser.is_empty()
                || (browser.is_empty() && (header_changed || self.pane != now.pane));
            if empty_change {
                top_background(&mut top, 44..188);
            }
            for (row, previous) in self.top_rows.iter().enumerate() {
                if empty_change || previous.changed(browser, browser.first + row) {
                    let y = 44 + row * ROW_HEIGHT;
                    top_background(&mut top, y..y + ROW_HEIGHT);
                    list_row(&mut top, browser, browser.first + row, y as i32);
                    damage.top.mark(y..y + ROW_HEIGHT);
                    service();
                }
            }
            if empty_change {
                top_empty(&mut top, browser, now.scanning);
                damage.top.mark(44..188);
            }
        } else {
            if self.clock != now.clock {
                top_clock(&mut top, now.clock);
                damage.top.mark(5..25);
                service();
            }
            if self.status != now.status || self.sequence != now.sequence {
                top_background(&mut top, 30..40);
                top_status(&mut top, now);
                damage.top.mark(30..40);
                service();
            }
            if self.title != now.title || self.folder != now.folder {
                top_background(&mut top, 116..154);
                top_title(&mut top, now);
                damage.top.mark(116..154);
                service();
            }
            if self.elapsed != now.elapsed
                || self.duration != now.duration
                || self.position != now.position
            {
                top_background(&mut top, 158..178);
                top_progress(&mut top, now);
                damage.top.mark(158..178);
                service();
            }
            if self.error.as_deref() != now.error {
                top_background(&mut top, 45..112);
                top_art(&mut top);
                top_error(&mut top, now.error);
                damage.top.mark(45..112);
                service();
            }
        }
        if !self.valid || self.pane != now.pane {
            draw_bottom(&mut bottom, browser, now);
            damage.bottom.mark(0..HEIGHT);
            service();
        } else {
            if header_changed {
                bottom.rect(0, 0, WIDTH as i32, LIST_Y as i32, BG);
                bottom_header(&mut bottom, browser, now);
                damage.bottom.mark(0..LIST_Y);
                service();
            }
            let end = LIST_Y + bottom_rows(now.pane) * ROW_HEIGHT;
            let empty_change = (self.count == 0) != browser.is_empty()
                || (browser.is_empty() && (header_changed || self.pane != now.pane));
            if empty_change {
                bottom.rect(8, LIST_Y as i32, 232, (end - LIST_Y) as i32, BG);
            }
            for (row, previous) in self.rows.iter().enumerate().take(bottom_rows(now.pane)) {
                let index = bottom_first(browser, now.listing) + row;
                if empty_change || previous.changed(browser, index) {
                    let y = LIST_Y + row * ROW_HEIGHT;
                    bottom.rect(8, y as i32, 232, ROW_HEIGHT as i32, BG);
                    list_row(&mut bottom, browser, index, y as i32);
                    damage.bottom.mark(y..y + ROW_HEIGHT);
                    service();
                }
            }
            if empty_change {
                bottom_empty(&mut bottom, browser, now.scanning);
                damage.bottom.mark(LIST_Y..end);
            }
            if self.first != browser.first
                || self.count != browser.len()
                || self.listing != now.listing
            {
                bottom.rect(240, LIST_Y as i32, 16, (end - LIST_Y) as i32, BG);
                bottom_scrollbar(&mut bottom, browser, now.pane);
                damage.bottom.mark(LIST_Y..end);
                service();
            }
            if now.pane == Pane::Search {
                if self.keyboard != now.keyboard
                    || self.keyboard_page != now.keyboard_page
                    || self.keyboard_keys != now.keyboard_keys
                {
                    bottom.rect(0, 124, WIDTH as i32, 68, BG);
                    bottom_keyboard(&mut bottom, now);
                    damage.bottom.mark(124..192);
                    service();
                }
            } else {
                if self.loaded != now.loaded
                    || self.position != now.position
                    || self.elapsed != now.elapsed
                    || self.duration != now.duration
                {
                    bottom.rect(0, 142, WIDTH as i32, 14, BG);
                    bottom_seek(&mut bottom, now);
                    damage.bottom.mark(142..156);
                    service();
                }
                if self.loaded != now.loaded || self.paused != now.paused {
                    bottom.rect(0, 158, WIDTH as i32, 16, BG);
                    bottom_controls(&mut bottom, now.loaded, now.paused);
                    damage.bottom.mark(158..174);
                    service();
                }
                if self.mode != now.mode {
                    bottom.rect(0, 177, WIDTH as i32, 14, BG);
                    bottom_actions(&mut bottom, now);
                    damage.bottom.mark(177..191);
                    service();
                }
            }
        }
        self.remember(browser, now);
        damage
    }
    fn remember(&mut self, browser: &Browser, now: &NowPlaying<'_>) {
        remember_string(&mut self.title, now.title);
        remember_string(&mut self.folder, now.folder);
        remember_string(&mut self.status, now.status);
        if self.error.as_deref() != now.error {
            self.error = now.error.map(String::from);
        }
        self.elapsed = now.elapsed;
        self.duration = now.duration;
        self.sequence = now.sequence;
        self.loaded = now.loaded;
        self.paused = now.paused;
        self.mode = now.mode;
        remember_string(&mut self.path, &browser.path);
        self.scanning = now.scanning;
        self.first = browser.first;
        self.selected = browser.selected;
        self.count = browser.len();
        self.limited = browser.limited;
        for (row, previous) in self.rows.iter_mut().enumerate() {
            previous.remember(browser, bottom_first(browser, now.listing) + row);
        }
        for (row, previous) in self.top_rows.iter_mut().enumerate() {
            previous.remember(browser, browser.first + row);
        }
        self.clock = now.clock;
        self.position = now.position;
        self.pane = now.pane;
        remember_string(&mut self.query, browser.query());
        self.searching = browser.searching();
        self.queue_len = now.queue_len;
        self.keyboard = now.keyboard;
        self.keyboard_page = now.keyboard_page;
        self.keyboard_keys.clear();
        self.keyboard_keys.extend_from_slice(now.keyboard_keys);
        self.listing = now.listing;
        self.valid = true;
    }
}

fn remember_string(destination: &mut String, value: &str) {
    if destination != value {
        destination.clear();
        destination.push_str(value);
    }
}

fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn draw_top(c: &mut Canvas<'_>, b: &Browser, n: &NowPlaying<'_>) {
    if n.listing {
        draw_top_list(c, b, n);
        return;
    }
    top_background(c, 0..HEIGHT);
    c.disc(14, 13, 3, MINT);
    c.text(24, 7, "SOUNDA", WHITE, 2, 6);
    top_clock(c, n.clock);
    top_status(c, n);
    top_art(c);
    top_title(c, n);
    top_progress(c, n);
    c.center(181, "VOLUME MAX  /  STEREO", MINT, 1, 28);
    top_error(c, n.error);
}
fn top_clock(c: &mut Canvas<'_>, seconds: u32) {
    for y in 5..25 {
        c.rect(
            190,
            y,
            62,
            1,
            rgb(
                10 + (y / 24) as u16,
                21 + (y / 16) as u16,
                38 + (y / 12) as u16,
            ),
        );
    }
    c.round_rect(190, 6, 58, 17, PANEL);
    let time = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    );
    c.text(195, 11, &time, MINT, 1, 8);
}

fn top_background(c: &mut Canvas<'_>, rows: Range<usize>) {
    for y in rows {
        c.rect(
            0,
            y as i32,
            256,
            1,
            rgb(
                10 + (y / 24) as u16,
                21 + (y / 16) as u16,
                38 + (y / 12) as u16,
            ),
        );
    }
}

fn top_status(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    c.text(12, 32, n.status, MUTED, 1, 25);
    if let Some((current, count)) = n.sequence {
        let s = format!("{current}/{count}");
        c.text(244 - s.len() as i32 * 6, 32, &s, MUTED, 1, 10);
    }
}

fn top_art(c: &mut Canvas<'_>) {
    // Original geometric record graphic; no Nintendo UI assets are needed.
    c.round_rect(92, 47, 72, 65, EDGE);
    c.round_rect(94, 49, 68, 61, PANEL);
    c.disc(128, 80, 25, rgb(11, 24, 38));
    c.disc(128, 80, 21, EDGE);
    c.disc(128, 80, 18, rgb(12, 28, 42));
    c.disc(128, 80, 9, MINT);
    c.disc(128, 80, 3, PANEL);
    c.line(112, 64, 121, 59, MUTED);
    c.line(135, 101, 144, 96, MUTED);
    c.rect(169, 65, 3, 13, EDGE);
    c.rect(175, 60, 3, 23, EDGE);
    c.rect(181, 68, 3, 8, EDGE);
    c.rect(72, 71, 3, 8, EDGE);
    c.rect(78, 65, 3, 20, EDGE);
    c.rect(84, 69, 3, 13, EDGE);
}

fn top_title(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    let title = if n.title.is_empty() {
        "YOUR MUSIC, ON DSi"
    } else {
        n.title
    };
    c.center_label(117, title, WHITE);
    c.center_label(136, n.folder, MUTED);
}

fn top_progress(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    c.round_rect(12, 160, 232, 4, EDGE);
    let progress = if n.duration > 0 {
        (n.position as u32 * 232 / u16::MAX as u32) as i32
    } else {
        0
    };
    if progress > 0 {
        c.round_rect(12, 160, progress.max(4), 4, MINT);
    }
    c.text(12, 169, &clock(n.elapsed), MUTED, 1, 10);
    let duration = clock(n.duration);
    c.text(
        244 - duration.len() as i32 * 6,
        169,
        &duration,
        MUTED,
        1,
        10,
    );
}

fn top_error(c: &mut Canvas<'_>, error: Option<&str>) {
    if let Some(error) = error {
        c.round_rect(8, 45, 240, 67, rgb(58, 34, 39));
        c.text(16, 53, "UNABLE TO PLAY", WARNING, 1, 30);
        let mut line = String::new();
        let mut row = 0;
        for word in error.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > 36 {
                c.text(16, 67 + row * 10, &line, WHITE, 1, 36);
                line.clear();
                row += 1;
                if row == 4 {
                    break;
                }
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if row < 4 {
            c.text(16, 67 + row * 10, &line, WHITE, 1, 36);
        }
    }
}
fn draw_top_list(c: &mut Canvas<'_>, b: &Browser, n: &NowPlaying<'_>) {
    top_background(c, 0..HEIGHT);
    top_list_header(c, b, n);
    for row in 0..TOP_LIST_ROWS {
        list_row(c, b, b.first + row, (44 + row * ROW_HEIGHT) as i32);
    }
    top_empty(c, b, n.scanning);
}
fn top_list_header(c: &mut Canvas<'_>, b: &Browser, n: &NowPlaying<'_>) {
    let label = match n.pane {
        Pane::Library => "FILES",
        Pane::Queue => "UP NEXT",
        Pane::Search => "SEARCH",
    };
    let count = if b.is_empty() {
        "0 / 0".into()
    } else {
        format!("{} / {}", b.selected + 1, b.len())
    };
    c.text(8, 11, &format!("{label}  {count}"), MINT, 1, 29);
    top_clock(c, n.clock);
    let path = n.error.unwrap_or(if n.scanning {
        "READING SD..."
    } else if !b.query().is_empty() {
        b.query()
    } else {
        &b.path
    });
    c.label(
        8,
        26,
        path,
        if n.error.is_some() { WARNING } else { MUTED },
        236,
    );
}
fn top_empty(c: &mut Canvas<'_>, b: &Browser, scanning: bool) {
    if b.is_empty() {
        c.center(79, empty_message(b, scanning), MUTED, 1, 39);
        c.center(
            92,
            if b.path.starts_with("UP NEXT") {
                "ADD: LIBRARY  /  B BACK"
            } else {
                "Y ADD  /  A PLAY  /  B BACK"
            },
            MUTED,
            1,
            39,
        );
    }
}
fn draw_bottom(c: &mut Canvas<'_>, b: &Browser, n: &NowPlaying<'_>) {
    c.pixels.fill(BG);
    bottom_header(c, b, n);
    for row in 0..bottom_rows(n.pane) {
        list_row(
            c,
            b,
            bottom_first(b, n.listing) + row,
            (LIST_Y + row * ROW_HEIGHT) as i32,
        );
    }
    bottom_empty(c, b, n.scanning);
    bottom_scrollbar(c, b, n.pane);
    if n.pane == Pane::Search {
        bottom_keyboard(c, n);
    } else {
        bottom_seek(c, n);
        bottom_controls(c, n.loaded, n.paused);
        bottom_actions(c, n);
    }
}
fn bottom_header(c: &mut Canvas<'_>, b: &Browser, n: &NowPlaying<'_>) {
    c.round_rect(4, 1, 22, 14, PANEL);
    c.text(12, 4, "<", MINT, 1, 1);
    if n.pane == Pane::Search {
        c.text(
            32,
            4,
            &if b.searching() {
                "SEARCHING...".into()
            } else {
                format!("SEARCH / {}", b.len())
            },
            WHITE,
            1,
            28,
        );
        c.round_rect(210, 1, 38, 14, PANEL);
        c.text(220, 4, "ADD", MINT, 1, 3);
        c.label(8, 16, b.query(), WHITE, 230);
        return;
    }
    for (x, w, label, active) in [
        (30, 28, "SD", n.pane == Pane::Library),
        (62, 48, "QUEUE", n.pane == Pane::Queue),
        (114, 40, "FIND", false),
        (158, 36, "ADD", false),
        (198, 36, "^", false),
    ] {
        c.round_rect(x, 1, w, 14, PANEL);
        c.text(
            x + (w - label.len() as i32 * 6) / 2,
            4,
            label,
            if active { MINT } else { WHITE },
            1,
            8,
        );
    }
    let path = if n.scanning {
        "READING SD..."
    } else if b.limited {
        "FIRST 2048 ENTRIES / LIMIT"
    } else if !b.query().is_empty() {
        b.query()
    } else {
        &b.path
    };
    c.label(8, 16, path, if b.limited { WARNING } else { MUTED }, 230);
}
fn list_row(c: &mut Canvas<'_>, b: &Browser, index: usize, y: i32) {
    if let Some(entry) = b.entry(index) {
        if index == b.selected {
            c.round_rect(8, y, 231, 17, PANEL);
            c.rect(8, y + 3, 2, 11, MINT);
        }
        if entry.directory {
            c.folder(16, y + 3, MINT);
        } else {
            c.note(16, y + 2, MUTED);
        }
        c.label(
            35,
            y + 1,
            entry.display_name(),
            if index == b.selected { WHITE } else { MUTED },
            if entry.directory { 190 } else { 204 },
        );
        if entry.directory {
            c.text(229, y + 5, ">", MUTED, 1, 1);
        }
    }
}
fn empty_message(b: &Browser, scanning: bool) -> &'static str {
    if scanning {
        "READING FOLDERS..."
    } else if !b.query().is_empty() {
        "NO MATCHING FILENAMES"
    } else if b.path.starts_with("UP NEXT") {
        "QUEUE EMPTY - TAP ADD"
    } else {
        "NO FOLDERS OR M4A FILES"
    }
}
fn bottom_empty(c: &mut Canvas<'_>, b: &Browser, scanning: bool) {
    if b.is_empty() {
        c.center(66, empty_message(b, scanning), MUTED, 1, 39);
        c.center(
            80,
            if b.path.starts_with("UP NEXT") {
                "ADD: LIBRARY  /  B BACK"
            } else {
                "A PLAY  /  Y ADD  /  B BACK"
            },
            MUTED,
            1,
            39,
        );
    }
}
/// Thumb geometry is shared with input, so grabbing it does not jump the list.
pub fn scroll_thumb(b: &Browser, pane: Pane) -> (u32, u32) {
    let height = bottom_rows(pane) * ROW_HEIGHT;
    let thumb = (height * b.visible_rows() / b.len().max(1)).clamp(10, height);
    let travel = height - thumb;
    let y = LIST_Y + travel * b.first / b.len().saturating_sub(b.visible_rows()).max(1);
    (y as u32, thumb as u32)
}
pub fn scroll_fraction(b: &Browser, pane: Pane, y: u32, grab: u32) -> u16 {
    let (_, thumb) = scroll_thumb(b, pane);
    let travel = (bottom_rows(pane) * ROW_HEIGHT) as u32 - thumb;
    let offset = y.saturating_sub(LIST_Y as u32 + grab).min(travel);
    (offset * u16::MAX as u32 / travel.max(1)) as u16
}
fn bottom_scrollbar(c: &mut Canvas<'_>, b: &Browser, pane: Pane) {
    if b.len() > b.visible_rows() {
        let (y, height) = scroll_thumb(b, pane);
        c.round_rect(
            246,
            LIST_Y as i32,
            3,
            (bottom_rows(pane) * ROW_HEIGHT) as i32,
            EDGE,
        );
        c.round_rect(244, y as i32, 7, height as i32, MINT);
    }
}
fn bottom_controls(c: &mut Canvas<'_>, loaded: bool, paused: bool) {
    for (x, w) in [(8, 43), (57, 87), (150, 43), (199, 49)] {
        c.round_rect(x, 158, w, 16, if x == 57 { MINT } else { PANEL });
    }
    c.previous(23, 161, WHITE);
    if loaded && !paused {
        c.rect(70, 162, 2, 9, BG);
        c.rect(75, 162, 2, 9, BG);
        c.text(85, 163, "PAUSE", BG, 1, 5);
    } else {
        c.play(70, 160, BG);
        c.text(85, 163, "PLAY", BG, 1, 4);
    }
    c.next(164, 161, WHITE);
    c.rect(207, 162, 8, 8, WHITE);
    c.text(220, 163, "STOP", WHITE, 1, 4);
}
fn bottom_seek(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    c.text(12, 142, &clock(n.elapsed), MUTED, 1, 10);
    let duration = clock(n.duration);
    c.text(
        244 - duration.len() as i32 * 6,
        142,
        &duration,
        MUTED,
        1,
        10,
    );
    c.round_rect(15, 151, 225, 3, EDGE);
    let x = 15 + n.position as i32 * 225 / u16::MAX as i32;
    if n.loaded {
        c.rect(15, 152, x - 15, 1, MINT);
        c.disc(x, 152, 3, MINT);
    }
}
fn bottom_actions(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    if n.pane == Pane::Queue {
        for (x, w, label) in [
            (8, 43, "TOP"),
            (56, 43, "2ND"),
            (104, 55, "BOTTOM"),
            (164, 39, "DEL"),
            (208, 40, "ADD"),
        ] {
            c.round_rect(x, 177, w, 14, PANEL);
            c.text(x + (w - label.len() as i32 * 6) / 2, 181, label, MINT, 1, 8);
        }
    } else {
        for (i, (mode, label)) in [
            (Mode::Sequential, "ORDER"),
            (Mode::Shuffle, "SHUFFLE"),
            (Mode::RepeatOne, "LOOP 1"),
        ]
        .iter()
        .enumerate()
        {
            let x = 8 + i as i32 * 82;
            let active = n.mode == *mode;
            c.round_rect(x, 177, 76, 14, if active { EDGE } else { PANEL });
            c.text(x + 12, 181, label, if active { MINT } else { MUTED }, 1, 10);
        }
    }
}
fn bottom_keyboard(c: &mut Canvas<'_>, n: &NowPlaying<'_>) {
    for index in 0..30 {
        let x = 8 + (index % 10) as i32 * 24;
        let y = 124 + (index / 10) as i32 * 17;
        c.round_rect(x, y, 22, 17, PANEL);
        if let Some(&key) = n.keyboard_keys.get(index) {
            if key == ' ' {
                c.text(x + 2, y + 5, "SPC", MINT, 1, 3);
            } else {
                let glyph = font::glyph(key);
                c.character(x + (22 - glyph.width() as i32) / 2, y, glyph, WHITE);
            }
        }
    }
    for (x, w, label, active) in [
        (8, 43, "ABC", n.keyboard == Keyboard::Latin),
        (55, 43, "CYR", n.keyboard == Keyboard::Cyrillic),
        (102, 43, "KANA", n.keyboard == Keyboard::Kana),
        (149, 43, "FILES", n.keyboard == Keyboard::Filenames),
        (196, 27, "DEL", false),
        (227, 21, ">", false),
    ] {
        c.round_rect(x, 177, w, 14, if active { EDGE } else { PANEL });
        c.text(
            x + (w - label.len() as i32 * 6) / 2,
            181,
            label,
            if active { MINT } else { MUTED },
            1,
            5,
        );
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchAction {
    None,
    Parent,
    PageUp,
    Row(usize),
    Previous,
    PlayPause,
    Next,
    Stop,
    Mode(Mode),
    Pane(Pane),
    Add,
    MoveUp,
    MoveDown,
    QueueJump(QueueTarget),
    Remove,
    Keyboard(Keyboard),
    Key(usize),
    Backspace,
    KeyboardPage,
    Seek(u16),
    Scroll(u32),
}
pub fn hit_test(x: u32, y: u32) -> TouchAction {
    hit_test_view(x, y, Pane::Library)
}
pub fn hit_test_view(x: u32, y: u32, pane: Pane) -> TouchAction {
    if x >= 256 || y >= 192 {
        return TouchAction::None;
    }
    if y < 15 {
        if x < 26 {
            return TouchAction::Parent;
        }
        if pane == Pane::Search {
            return if x >= 210 {
                TouchAction::Add
            } else {
                TouchAction::None
            };
        }
        return match x {
            30..=57 => TouchAction::Pane(Pane::Library),
            62..=109 => TouchAction::Pane(Pane::Queue),
            114..=153 => TouchAction::Pane(Pane::Search),
            158..=193 if pane == Pane::Queue => TouchAction::Pane(Pane::Library),
            158..=193 => TouchAction::Add,
            198..=233 => TouchAction::PageUp,
            _ => TouchAction::None,
        };
    }
    if (LIST_Y as u32..(LIST_Y + bottom_rows(pane) * ROW_HEIGHT) as u32).contains(&y) {
        return if x >= 240 {
            TouchAction::Scroll(y)
        } else {
            TouchAction::Row((y as usize - LIST_Y) / ROW_HEIGHT)
        };
    }
    if pane == Pane::Search {
        if (124..175).contains(&y) && (8..248).contains(&x) {
            return TouchAction::Key(((y - 124) / 17 * 10 + (x - 8) / 24) as usize);
        }
        if (177..191).contains(&y) {
            return match x {
                8..=50 => TouchAction::Keyboard(Keyboard::Latin),
                55..=97 => TouchAction::Keyboard(Keyboard::Cyrillic),
                102..=144 => TouchAction::Keyboard(Keyboard::Kana),
                149..=191 => TouchAction::Keyboard(Keyboard::Filenames),
                196..=222 => TouchAction::Backspace,
                227..=247 => TouchAction::KeyboardPage,
                _ => TouchAction::None,
            };
        }
        return TouchAction::None;
    }
    if (149..156).contains(&y) && (8..249).contains(&x) {
        return TouchAction::Seek(((x.clamp(15, 240) - 15) * u16::MAX as u32 / 225) as u16);
    }
    if (158..174).contains(&y) {
        return match x {
            8..=50 => TouchAction::Previous,
            57..=143 => TouchAction::PlayPause,
            150..=192 => TouchAction::Next,
            199..=247 => TouchAction::Stop,
            _ => TouchAction::None,
        };
    }
    if (177..191).contains(&y) {
        if pane == Pane::Queue {
            return match x {
                8..=50 => TouchAction::QueueJump(QueueTarget::Top),
                56..=98 => TouchAction::QueueJump(QueueTarget::Second),
                104..=158 => TouchAction::QueueJump(QueueTarget::Bottom),
                164..=202 => TouchAction::Remove,
                208..=247 => TouchAction::Pane(Pane::Library),
                _ => TouchAction::None,
            };
        }
        return match x {
            8..=83 => TouchAction::Mode(Mode::Sequential),
            90..=165 => TouchAction::Mode(Mode::Shuffle),
            172..=247 => TouchAction::Mode(Mode::RepeatOne),
            _ => TouchAction::None,
        };
    }
    TouchAction::None
}

fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' | 'Á' | 'À' | 'Â' | 'Ä' | 'á' | 'à' | 'â' | 'ä' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' | 'É' | 'È' | 'Ê' | 'Ë' | 'é' | 'è' | 'ê' | 'ë' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' | 'Í' | 'Ï' | 'í' | 'ï' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' | 'Ñ' | 'ñ' => [17, 25, 25, 21, 19, 19, 17],
        'O' | 'Ó' | 'Ö' | 'ó' | 'ö' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' | 'Ú' | 'Ü' | 'ú' | 'ü' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ' ' => [0; 7],
        '.' => [0, 0, 0, 0, 0, 6, 6],
        ',' => [0, 0, 0, 0, 6, 6, 4],
        ':' => [0, 6, 6, 0, 6, 6, 0],
        ';' => [0, 6, 6, 0, 6, 6, 4],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '\\' => [16, 16, 8, 4, 2, 1, 1],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '"' => [10, 10, 0, 0, 0, 0, 0],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '<' => [1, 2, 4, 8, 4, 2, 1],
        '^' => [4, 10, 17, 0, 0, 0, 0],
        _ => [31, 17, 2, 4, 4, 0, 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::Entry;
    use alloc::{vec, vec::Vec};

    fn now() -> NowPlaying<'static> {
        NowPlaying {
            title: "A song",
            folder: "Music",
            status: "PLAYING",
            elapsed: 0,
            duration: 180,
            paused: false,
            loaded: true,
            mode: Mode::Sequential,
            error: None,
            scanning: false,
            sequence: Some((1, 200)),
            ..NowPlaying::default()
        }
    }

    fn library(count: usize) -> Browser {
        let mut browser = Browser::new();
        browser.replace(
            "sd:/Music".into(),
            (0..count)
                .map(|i| Entry {
                    name: format!("Canción {i:03}.m4a"),
                    directory: false,
                })
                .collect(),
            false,
        );
        browser
    }

    struct DisplayTest {
        renderer: Renderer,
        top: Vec<u16>,
        bottom: Vec<u16>,
        shown_top: Vec<u16>,
        shown_bottom: Vec<u16>,
    }

    impl DisplayTest {
        fn new() -> Self {
            Self {
                renderer: Renderer::new(),
                top: vec![0; PIXELS],
                bottom: vec![0; PIXELS],
                shown_top: vec![0; PIXELS],
                shown_bottom: vec![0; PIXELS],
            }
        }
        fn check(&mut self, browser: &Browser, now: &NowPlaying<'_>) -> Damage {
            let damage = self
                .renderer
                .render(&mut self.top, &mut self.bottom, browser, now, || {});
            // Simulate a real partial VRAM presenter, including retention of
            // rows omitted by the renderer. This catches lost damage as well
            // as stale text and selection backgrounds.
            for (rows, pixels, shown) in [
                (&damage.top, &self.top, &mut self.shown_top),
                (&damage.bottom, &self.bottom, &mut self.shown_bottom),
            ] {
                for run in rows.runs() {
                    shown[run.start * WIDTH..run.end * WIDTH]
                        .copy_from_slice(&pixels[run.start * WIDTH..run.end * WIDTH]);
                }
            }
            let mut full_top = vec![0; PIXELS];
            let mut full_bottom = vec![0; PIXELS];
            render(&mut full_top, &mut full_bottom, browser, now);
            assert_eq!(self.top, full_top, "retained top raster");
            assert_eq!(self.bottom, full_bottom, "retained bottom raster");
            assert_eq!(self.shown_top, full_top, "top dirty row coverage");
            assert_eq!(self.shown_bottom, full_bottom, "bottom dirty row coverage");
            damage
        }
    }

    #[test]
    fn selection_progress_and_clock_damage_only_their_widgets() {
        let mut browser = library(200);
        let mut now = now();
        let mut display = DisplayTest::new();
        let initial = display.check(&browser, &now);
        assert_eq!(initial.top.row_count(), HEIGHT);
        assert_eq!(initial.bottom.row_count(), HEIGHT);
        assert!(display.check(&browser, &now).is_empty());

        browser.move_by(1);
        let moved = display.check(&browser, &now);
        assert!(moved.top.is_empty());
        assert!(moved.bottom.runs().eq(core::iter::once(32..68)));
        assert_eq!(moved.bottom.row_count(), 36);

        now.elapsed = 1;
        let tick = display.check(&browser, &now);
        assert!(tick.top.runs().eq(core::iter::once(158..178)));
        assert!(tick.bottom.runs().eq(core::iter::once(142..156)));

        now.clock = 12 * 3600 + 1;
        let rtc_tick = display.check(&browser, &now);
        assert!(rtc_tick.top.runs().eq(core::iter::once(5..25)));
        assert!(rtc_tick.bottom.is_empty());

        let mut service_calls = 0;
        let unchanged = display.renderer.render(
            &mut display.top,
            &mut display.bottom,
            &browser,
            &now,
            || service_calls += 1,
        );
        assert!(unchanged.is_empty());
        assert_eq!(service_calls, 0);
    }

    #[test]
    fn retained_output_matches_full_render_across_navigation_and_playback_changes() {
        let mut browser = library(200);
        let mut now = now();
        let mut display = DisplayTest::new();
        display.check(&browser, &now);
        for _ in 0..210 {
            browser.move_by(1);
            display.check(&browser, &now);
        }
        for _ in 0..42 {
            browser.move_by(-5);
            display.check(&browser, &now);
        }
        now.paused = true;
        now.status = "PAUSED";
        display.check(&browser, &now);
        now.mode = Mode::Shuffle;
        now.title = "A considerably longer title that occupies both lines in the player";
        now.sequence = Some((24, 200));
        display.check(&browser, &now);
        now.title = "Short title";
        now.folder = "A different folder";
        now.duration = 321;
        now.elapsed = 140;
        display.check(&browser, &now);
        now.error = Some("DSP reply timed out while decoding an AAC frame");
        display.check(&browser, &now);
        now.error = Some("SD read failed");
        display.check(&browser, &now);
        now.error = None;
        now.loaded = false;
        now.paused = false;
        browser.limited = true;
        display.check(&browser, &now);
    }

    #[test]
    fn empty_and_replaced_folders_erase_old_rows_and_scrollbars() {
        let mut browser = library(200);
        let mut now = now();
        let mut display = DisplayTest::new();
        display.check(&browser, &now);
        now.scanning = true;
        display.check(&browser, &now);
        browser.replace("sd:/Empty".into(), Vec::new(), false);
        display.check(&browser, &now);
        now.scanning = false;
        display.check(&browser, &now);
        browser = library(2);
        display.check(&browser, &now);
        // Same count/path but replaced filenames still invalidate the rows.
        browser.entries[0] = Entry {
            name: "Album".into(),
            directory: true,
        };
        browser.entries[1].name = "Different.M4A".into();
        display.check(&browser, &now);
        display.renderer.invalidate();
        let damage = display.check(&browser, &now);
        assert_eq!(
            damage.top.row_count() + damage.bottom.row_count(),
            HEIGHT * 2
        );
    }

    #[test]
    fn search_queue_unicode_and_slider_updates_match_full_render() {
        let mut browser = library(200);
        let mut now = now();
        let mut display = DisplayTest::new();
        display.check(&browser, &now);
        now.pane = Pane::Search;
        now.keyboard_keys = &['é', 'Ł', 'Ж', 'あ', '中', ' '];
        now.keyboard = Keyboard::Filenames;
        display.check(&browser, &now);
        browser.set_query("cancion 19".into());
        display.check(&browser, &now);
        while browser.searching() {
            browser.search_step();
        }
        display.check(&browser, &now);
        now.keyboard_page = 1;
        now.keyboard_keys = &['日', '本', '語'];
        display.check(&browser, &now);
        now.pane = Pane::Queue;
        now.queue_len = 3;
        browser.replace_ordered(
            "QUEUE / 3 SONGS".into(),
            ["Żółć.m4a", "日本語.m4a", "Привет.m4a"]
                .iter()
                .map(|s| Entry {
                    name: (*s).into(),
                    directory: false,
                })
                .collect(),
            false,
        );
        display.check(&browser, &now);
        for position in [1, 30000, 65535, 0] {
            now.position = position;
            display.check(&browser, &now);
        }
        browser.entries.swap(0, 2);
        browser.selected = 2;
        display.check(&browser, &now);
        now.pane = Pane::Library;
        display.check(&browser, &now);
    }
    #[test]
    fn expanded_list_navigation_search_and_idle_restore_match_full_render() {
        let mut browser = library(200);
        let mut now = now();
        let mut display = DisplayTest::new();
        display.check(&browser, &now);
        now.listing = true;
        browser.set_visible_rows(view_rows(now.pane, true));
        display.check(&browser, &now);
        for step in [1, 6, 1, 5, 1, 30, 150, -2, -80, -199] {
            browser.move_by(step);
            display.check(&browser, &now);
        }
        for fraction in [0, 30000, 65535, 1] {
            browser.scroll_to(fraction);
            display.check(&browser, &now);
        }
        now.pane = Pane::Search;
        browser.set_visible_rows(view_rows(now.pane, true));
        display.check(&browser, &now);
        browser.set_query("cancion 19".into());
        display.check(&browser, &now);
        while browser.searching() {
            browser.search_step();
        }
        display.check(&browser, &now);
        browser.move_by(8);
        display.check(&browser, &now);
        now.listing = false;
        browser.set_visible_rows(view_rows(now.pane, false));
        let restored = display.check(&browser, &now);
        assert_eq!(restored.top.row_count(), HEIGHT);
        assert!(!restored.bottom.is_empty());
        now.elapsed = 50;
        now.clock += 10;
        display.check(&browser, &now);
        now.listing = true;
        browser.set_visible_rows(view_rows(now.pane, true));
        browser.set_query("no such filename".into());
        while browser.searching() {
            browser.search_step();
        }
        display.check(&browser, &now);
        now.pane = Pane::Queue;
        browser.replace_ordered("UP NEXT / 0 SONGS".into(), Vec::new(), false);
        browser.set_visible_rows(view_rows(now.pane, true));
        display.check(&browser, &now);
    }
    #[test]
    fn scrollbar_thumb_grab_and_endpoints_share_the_browser_viewport() {
        let mut browser = library(200);
        browser.set_visible_rows(view_rows(Pane::Library, true));
        browser.move_by(100);
        let (top, h) = scroll_thumb(&browser, Pane::Library);
        let fraction = scroll_fraction(&browser, Pane::Library, top + h / 2, h / 2);
        let before = browser.first;
        browser.scroll_to(fraction);
        assert!(browser.first.abs_diff(before) <= 2); // integer pixel quantization
        browser.scroll_to(scroll_fraction(&browser, Pane::Library, 0, h / 2));
        assert_eq!(browser.first, 0);
        browser.scroll_to(scroll_fraction(&browser, Pane::Library, 191, h / 2));
        assert_eq!(browser.first, 200 - 14);
        assert!(browser.selected >= browser.first && browser.selected < 200);
    }
    #[test]
    fn touch_targets_match_the_visible_controls() {
        assert_eq!(hit_test(80, 165), TouchAction::PlayPause);
        assert_eq!(hit_test(35, 181), TouchAction::Mode(Mode::Sequential));
        assert_eq!(hit_test(200, 182), TouchAction::Mode(Mode::RepeatOne));
        assert_eq!(hit_test(60, 55), TouchAction::Row(1));
        assert_eq!(hit_test(256, 60), TouchAction::None);
        assert_eq!(hit_test(15, 152), TouchAction::Seek(0));
        assert_eq!(hit_test(240, 152), TouchAction::Seek(u16::MAX));
        assert_eq!(hit_test_view(180, 181, Pane::Queue), TouchAction::Remove);
        assert_eq!(hit_test_view(45, 149, Pane::Search), TouchAction::Key(11));
        assert_eq!(
            hit_test_view(30, 181, Pane::Queue),
            TouchAction::QueueJump(QueueTarget::Top)
        );
        assert_eq!(
            hit_test_view(70, 181, Pane::Queue),
            TouchAction::QueueJump(QueueTarget::Second)
        );
        assert_eq!(
            hit_test_view(130, 181, Pane::Queue),
            TouchAction::QueueJump(QueueTarget::Bottom)
        );
        assert_eq!(hit_test(248, 70), TouchAction::Scroll(70));
        assert_eq!(
            hit_test_view(178, 7, Pane::Queue),
            TouchAction::Pane(Pane::Library)
        );
        assert_eq!(
            hit_test_view(170, 180, Pane::Search),
            TouchAction::Keyboard(Keyboard::Filenames)
        );
        assert_eq!(
            hit_test_view(210, 180, Pane::Search),
            TouchAction::Backspace
        );
    }
}
