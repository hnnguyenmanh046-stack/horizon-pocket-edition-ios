// Litematica's menu (K): four tabs.
//   Schematics  the .litematic files in the schematics folder; click one to
//               load it where you stand
//   Placement   where it is (move it a block or ten at a time, or to you),
//               its turn and mirror, how far along the build is, unload
//   Materials   every item it needs: in total, still missing, in your
//               inventory
//   Settings    the overlay, the layers shown, how far around you, the
//               on-screen status
// Drawn on the Options screen's AI Agent page while it's open (Mod Menu has
// Mods, Complex Voice Chat has Credits). The game crate's runtime hands it a
// `View` each frame and takes its `Action`s.

use std::sync::Mutex;
use std::time::Instant;

use super::{OptionsUi, Page};
use crate::hud::{HudAtlas, HudBatch, rgba};
use crate::text::Text;

type Rgba = (u8, u8, u8, u8);
type Rect = [f32; 4];

const PANEL: Rgba = (14, 17, 24, 246);
const PANEL_LOW: Rgba = (9, 11, 16, 246);
const EDGE: Rgba = (255, 255, 255, 26);
const ROW_BG: Rgba = (255, 255, 255, 10);
const HOVER: Rgba = (255, 255, 255, 22);
const BUTTON: Rgba = (255, 255, 255, 16);
const BUTTON_HOVER: Rgba = (255, 255, 255, 40);
const ACCENT: Rgba = (90, 190, 255, 255);
const ACCENT_2: Rgba = (150, 120, 255, 255);
const WHITE: Rgba = (255, 255, 255, 255);
const DIM: Rgba = (165, 172, 184, 255);
const FAINT: Rgba = (100, 108, 120, 255);
const GOOD: Rgba = (92, 230, 138, 255);
const BAD: Rgba = (255, 90, 90, 255);
const WARN: Rgba = (255, 170, 50, 255);
const MISSING: Rgba = (80, 195, 255, 255);
const EXTRA: Rgba = (255, 96, 208, 255);

const W: f32 = 440.0;
const H: f32 = 280.0;
const HEADER: f32 = 44.0;
const FOOTER: f32 = 26.0;
const LINE: f32 = 11.0;

pub const TABS: [&str; 4] = ["Schematics", "Placement", "Materials", "Settings"];
pub const OVERLAYS: [&str; 3] = ["Off", "Missing + wrong", "Wrong only"];
pub const LAYER_MODES: [&str; 4] = ["All", "Only Y", "Below Y", "Above Y"];
pub const RADII: [u32; 5] = [16, 24, 32, 48, 64];
pub const ROTATIONS: [&str; 4] = ["0°", "90°", "180°", "270°"];
pub const MIRRORS: [&str; 3] = ["None", "East-West", "North-South"];

#[derive(Clone, Debug, Default)]
pub struct FileRow {
    pub name: String,
    pub kb: u64,
    pub loaded: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub file: String,
    pub name: String,
    pub author: String,
    pub size: [i32; 3],
    pub blocks: usize,
    pub origin: [i32; 3],
    pub rotation: u8,
    pub mirror: u8,
}

/// How far along the build is, from the last full pass over it.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub correct: usize,
    pub missing: usize,
    pub wrong_block: usize,
    pub wrong_state: usize,
    pub extra: usize,
    /// Blocks in chunks that aren't loaded (not counted either way).
    pub unloaded: usize,
    pub total: usize,
    /// A first pass is still running.
    pub scanning: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub item: String,
    pub total: u32,
    pub missing: u32,
    pub have: u32,
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub folder: String,
    pub files: Vec<FileRow>,
    pub info: Option<Info>,
    pub progress: Option<Progress>,
    pub materials: Vec<Material>,
    pub overlay: u8,
    pub layer_mode: u8,
    pub layer: i32,
    pub radius: u32,
    pub show_hud: bool,
    pub show_extra: bool,
    /// The last thing that happened (loaded, couldn't load...), and whether it went well.
    pub message: Option<(String, bool)>,
    pub keys: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Load(String),
    Unload,
    Here,
    /// Moves the placement along axis `.0` (0 x, 1 y, 2 z) by `.1`.
    Nudge(u8, i32),
    Rotation(u8),
    Mirror(u8),
    Overlay(u8),
    LayerMode(u8),
    Layer(i32),
    /// The layer to where you stand.
    LayerHere,
    Radius(u32),
    ShowHud(bool),
    ShowExtra(bool),
    OpenFolder,
    Refresh,
}

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Tab(usize),
    Do(Action),
    Done,
    /// Materials: sort by missing or by name.
    Sort,
}

#[derive(Default)]
struct State {
    open: bool,
    tab: usize,
    scroll: f32,
    target: f32,
    content: f32,
    view: View,
    actions: Vec<Action>,
    hits: Vec<(Rect, Hit)>,
    glow: Vec<(Hit, f32)>,
    opened: Option<Instant>,
    drawn: Option<Instant>,
    switched: Option<Instant>,
    tab_at: Option<f32>,
    done: bool,
    by_name: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// The menu opened (the game opened the Options screen on AI Agent for it).
pub fn open(placement_first: bool) {
    with_state(|s| {
        s.open = true;
        s.done = false;
        s.opened = Some(Instant::now());
        s.switched = s.opened;
        s.drawn = None;
        s.scroll = 0.0;
        s.target = 0.0;
        s.tab = if placement_first { 1 } else { 0 };
        s.tab_at = None;
        s.actions.push(Action::Refresh);
    });
}

pub fn closed() {
    with_state(|s| s.open = false);
}

pub fn is_open() -> bool {
    with_state(|s| s.open)
}

pub fn shows(options: &OptionsUi) -> bool {
    is_open() && options.page() == Page::Agent
}

pub fn set_view(view: View) {
    with_state(|s| s.view = view);
}

pub fn take_actions() -> Vec<Action> {
    with_state(|s| std::mem::take(&mut s.actions))
}

/// Done was clicked: the game closes the screen.
pub fn take_done() -> bool {
    with_state(|s| std::mem::take(&mut s.done))
}

// ---------- drawing helpers ----------

fn color(c: Rgba) -> u32 {
    rgba(c.0, c.1, c.2, c.3)
}

fn alpha(c: Rgba, a: f32) -> Rgba {
    (c.0, c.1, c.2, (f32::from(c.3) * a.clamp(0.0, 1.0)).round() as u8)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), m(a.3, b.3))
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn inside(r: Rect, (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

fn fill(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, c: Rgba) {
    if r[2] > 0.0 && r[3] > 0.0 && c.3 > 0 {
        batch.fill(atlas, r[0], r[1], r[2], r[3], color(c));
    }
}

fn rounded(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, c: Rgba) {
    let [x, y, w, h] = r;
    if w < 4.0 || h < 4.0 {
        fill(batch, atlas, r, c);
        return;
    }
    fill(batch, atlas, [x + 2.0, y, w - 4.0, 1.0], c);
    fill(batch, atlas, [x + 1.0, y + 1.0, w - 2.0, 1.0], c);
    fill(batch, atlas, [x, y + 2.0, w, h - 4.0], c);
    fill(batch, atlas, [x + 1.0, y + h - 2.0, w - 2.0, 1.0], c);
    fill(batch, atlas, [x + 2.0, y + h - 1.0, w - 4.0, 1.0], c);
}

fn framed(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, inner: Rgba, edge: Rgba) {
    rounded(batch, atlas, r, edge);
    rounded(batch, atlas, [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0], inner);
}

fn gradient_h(batch: &mut HudBatch, atlas: &HudAtlas, r: Rect, a: Rgba, b: Rgba) {
    let slices = (r[2] / 6.0).ceil().max(1.0) as usize;
    let step = r[2] / slices as f32;
    for k in 0..slices {
        let t = (k as f32 + 0.5) / slices as f32;
        fill(batch, atlas, [r[0] + k as f32 * step, r[1], step + 0.01, r[3]], mix(a, b, t));
    }
}

fn label(batch: &mut HudBatch, atlas: &HudAtlas, text: &str, x: f32, y: f32, c: Rgba) {
    batch.text_styled(atlas, &Text::literal(text), x, y, color(c), false);
}

fn label_big(batch: &mut HudBatch, atlas: &HudAtlas, text: &str, x: f32, y: f32, scale: f32, c: Rgba) {
    let start = batch.vertices.len();
    batch.text_styled(atlas, &Text::literal(text), x, y, color(c), true);
    batch.scale_since(start, x, y, scale, scale);
}

fn width(atlas: &HudAtlas, text: &str) -> f32 {
    atlas.font.width(text)
}

fn centred(batch: &mut HudBatch, atlas: &HudAtlas, text: &str, r: Rect, c: Rgba) {
    let x = (r[0] + (r[2] - width(atlas, text)) / 2.0).floor();
    label(batch, atlas, text, x, (r[1] + (r[3] - 8.0) / 2.0).floor(), c);
}

fn right(batch: &mut HudBatch, atlas: &HudAtlas, text: &str, x_right: f32, y: f32, c: Rgba) {
    label(batch, atlas, text, x_right - width(atlas, text), y, c);
}

fn fit(atlas: &HudAtlas, text: &str, w: f32) -> String {
    if width(atlas, text) <= w {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if width(atlas, &format!("{out}...")) > w {
            out.pop();
            break;
        }
    }
    format!("{}...", out.trim_end())
}

fn fade_since(batch: &mut HudBatch, start: usize, a: f32) {
    if a >= 1.0 {
        return;
    }
    for v in &mut batch.vertices[start..] {
        let old = (v.color >> 24) as f32;
        v.color = (v.color & 0x00FF_FFFF) | (((old * a.max(0.0)).round() as u32).min(255) << 24);
    }
}

fn shift_since(batch: &mut HudBatch, start: usize, dx: f32, dy: f32) {
    for v in &mut batch.vertices[start..] {
        v.pos[0] += dx;
        v.pos[1] += dy;
    }
}

/// One frame of drawing: what's clickable is recorded as it's drawn.
struct Ui<'a> {
    batch: &'a mut HudBatch,
    atlas: &'a HudAtlas,
    cursor: (f32, f32),
    hits: Vec<(Rect, Hit)>,
    glow: &'a mut Vec<(Hit, f32)>,
    dt: f32,
    /// Clicks only count inside this (the scrolled body is clipped).
    clip: Rect,
}

impl Ui<'_> {
    fn glow(&mut self, hit: &Hit, r: Rect) -> f32 {
        let over = inside(r, self.cursor) && inside(self.clip, self.cursor);
        let at = match self.glow.iter().position(|(h, _)| h == hit) {
            Some(k) => k,
            None => {
                self.glow.push((hit.clone(), 0.0));
                self.glow.len() - 1
            }
        };
        let g = &mut self.glow[at].1;
        let target = if over { 1.0 } else { 0.0 };
        *g += (target - *g) * (1.0 - (-14.0 * self.dt).exp());
        if (target - *g).abs() < 0.01 {
            *g = target;
        }
        *g
    }

    fn hit(&mut self, r: Rect, hit: Hit) {
        let c = self.clip;
        let x0 = r[0].max(c[0]);
        let y0 = r[1].max(c[1]);
        let x1 = (r[0] + r[2]).min(c[0] + c[2]);
        let y1 = (r[1] + r[3]).min(c[1] + c[3]);
        if x1 > x0 && y1 > y0 {
            self.hits.push(([x0, y0, x1 - x0, y1 - y0], hit));
        }
    }

    fn button(&mut self, r: Rect, text: &str, hit: Hit, primary: bool, enabled: bool) {
        let g = if enabled { self.glow(&hit, r) } else { 0.0 };
        let base = if primary { alpha(ACCENT, 0.24) } else { BUTTON };
        let lit = if primary { alpha(ACCENT, 0.45) } else { BUTTON_HOVER };
        let r2 = [r[0], r[1] - g, r[2], r[3]];
        framed(self.batch, self.atlas, r2, mix(base, lit, g), mix(EDGE, alpha(ACCENT, 0.8), g));
        let t = fit(self.atlas, text, r[2] - 4.0);
        centred(self.batch, self.atlas, &t, r2, if enabled { mix(DIM, WHITE, 0.6 + g * 0.4) } else { FAINT });
        if enabled {
            self.hit(r, hit);
        }
    }

    /// Choices side by side with the one picked lit.
    fn segments(&mut self, r: Rect, options: &[&str], picked: usize, hit: impl Fn(usize) -> Hit) {
        rounded(self.batch, self.atlas, r, alpha(WHITE, 0.05));
        let w = r[2] / options.len() as f32;
        for (k, o) in options.iter().enumerate() {
            let cell = [r[0] + w * k as f32, r[1], w, r[3]];
            let h = hit(k);
            let g = self.glow(&h, cell);
            if k == picked {
                rounded(self.batch, self.atlas, [cell[0] + 1.0, cell[1] + 1.0, cell[2] - 2.0, cell[3] - 2.0], alpha(ACCENT, 0.35));
                fill(self.batch, self.atlas, [cell[0] + 3.0, cell[1] + cell[3] - 2.0, cell[2] - 6.0, 1.0], ACCENT);
            } else if g > 0.0 {
                rounded(self.batch, self.atlas, [cell[0] + 1.0, cell[1] + 1.0, cell[2] - 2.0, cell[3] - 2.0], alpha(HOVER, g));
            }
            let c = if k == picked { WHITE } else { mix(FAINT, DIM, g) };
            let t = fit(self.atlas, o, w - 4.0);
            centred(self.batch, self.atlas, &t, cell, c);
            self.hit(cell, h);
        }
    }

    fn switch(&mut self, r: Rect, text: &str, on: bool, hit: Hit) {
        let g = self.glow(&hit, r);
        framed(self.batch, self.atlas, r, mix(BUTTON, BUTTON_HOVER, g), mix(EDGE, alpha(ACCENT, 0.6), g));
        label(self.batch, self.atlas, text, r[0] + 7.0, r[1] + (r[3] - 8.0) / 2.0, if on { WHITE } else { DIM });
        let track = [r[0] + r[2] - 30.0, r[1] + (r[3] - 10.0) / 2.0, 22.0, 10.0];
        rounded(self.batch, self.atlas, track, if on { ACCENT } else { (62, 66, 76, 255) });
        let kx = if on { track[0] + 13.0 } else { track[0] + 1.0 };
        rounded(self.batch, self.atlas, [kx, track[1] + 1.0, 8.0, 8.0], WHITE);
        self.hit(r, hit);
    }
}

// ---------- the menu ----------

pub fn draw(batch: &mut HudBatch, atlas: &HudAtlas, cursor: (f32, f32)) {
    let now = Instant::now();
    let (sw, sh) = (batch.width, batch.height);
    with_state(|s| {
        let dt = s.drawn.map_or(0.0, |d| now.duration_since(d).as_secs_f32().min(0.1));
        s.drawn = Some(now);
        let open = ease_out(s.opened.map_or(1.0, |o| now.duration_since(o).as_secs_f32() / 0.25));
        let switched = ease_out(s.switched.map_or(1.0, |o| now.duration_since(o).as_secs_f32() / 0.2));
        let clock = s.opened.map_or(0.0, |o| now.duration_since(o).as_secs_f32());
        s.scroll += (s.target - s.scroll) * (1.0 - (-18.0 * dt).exp());

        fill(batch, atlas, [0.0, 0.0, sw, sh], alpha((4, 6, 12, 160), open));
        let w = (sw - 16.0).clamp(260.0, W);
        let h = (sh - 16.0).clamp(160.0, H);
        let x = ((sw - w) / 2.0).floor();
        let y = ((sh - h) / 2.0).floor();
        let panel_start = batch.vertices.len();
        for k in 1..=4 {
            let g = k as f32 * 2.0;
            rounded(batch, atlas, [x - g, y - g + 3.0, w + g * 2.0, h + g * 2.0], (0, 0, 0, 22));
        }
        framed(batch, atlas, [x, y, w, h], PANEL, EDGE);
        batch.fill_gradient(atlas, (x + 1.0, y + HEADER, w - 2.0, h - HEADER - 2.0), color((0, 0, 0, 0)), color(PANEL_LOW));
        gradient_h(batch, atlas, [x + 2.0, y + 1.0, w - 4.0, 2.0], ACCENT, ACCENT_2);
        // A sweep of light along the top.
        let sweep = (clock * 0.4).fract();
        let sx = x - 30.0 + (w + 60.0) * sweep;
        for k in 0..6 {
            let a = 1.0 - (k as f32 - 2.5).abs() / 3.0;
            let fx = (sx + k as f32 * 5.0).clamp(x + 2.0, x + w - 7.0);
            fill(batch, atlas, [fx, y + 1.0, 5.0, 2.0], alpha(WHITE, a * 0.5));
        }

        // The title, the schematic loaded, the tabs.
        label_big(batch, atlas, "Litematica", x + 10.0, y + 7.0, 1.4, WHITE);
        let sub = match &s.view.info {
            Some(i) => format!("{}  -  {} blocks", i.name, i.blocks),
            None => "No schematic loaded".to_owned(),
        };
        let tx = x + 10.0 + width(atlas, "Litematica") * 1.4 + 10.0;
        label(batch, atlas, &fit(atlas, &sub, x + w - tx - 8.0), tx, y + 11.0, FAINT);
        let tabs_y = y + 24.0;
        let tab_w = (w - 20.0) / TABS.len() as f32;
        let mut ui = Ui { batch: &mut *batch, atlas, cursor, hits: Vec::new(), glow: &mut s.glow, dt, clip: [x, y, w, h] };
        for (k, name) in TABS.iter().enumerate() {
            let r = [x + 10.0 + tab_w * k as f32, tabs_y, tab_w, 16.0];
            let hit = Hit::Tab(k);
            let g = ui.glow(&hit, r);
            let c = if k == s.tab { WHITE } else { mix(FAINT, DIM, g) };
            centred(ui.batch, ui.atlas, name, r, c);
            ui.hit(r, hit);
        }
        fill(ui.batch, ui.atlas, [x + 10.0, tabs_y + 16.0, w - 20.0, 1.0], alpha(WHITE, 0.07));
        let at = s.tab_at.get_or_insert(s.tab as f32);
        *at += (s.tab as f32 - *at) * (1.0 - (-18.0 * dt).exp());
        gradient_h(ui.batch, ui.atlas, [x + 10.0 + *at * tab_w + 8.0, tabs_y + 15.0, tab_w - 16.0, 2.0], ACCENT, ACCENT_2);

        // The page, scrolled and clipped, sliding in after a tab change.
        let body = [x + 8.0, y + HEADER, w - 16.0, h - HEADER - FOOTER];
        let start = ui.batch.vertices.len();
        ui.clip = body;
        let top = body[1] + 4.0 - s.scroll;
        let content = match s.tab {
            0 => schematics(&mut ui, &s.view, body, top),
            1 => placement(&mut ui, &s.view, body, top),
            2 => materials(&mut ui, &s.view, body, top, s.by_name),
            _ => settings(&mut ui, &s.view, body, top),
        };
        s.content = content;
        let most = (content - body[3]).max(0.0);
        s.target = s.target.clamp(0.0, most);
        s.scroll = s.scroll.clamp(0.0, most);
        shift_since(ui.batch, start, (1.0 - switched) * 12.0, 0.0);
        fade_since(ui.batch, start, switched);
        ui.batch.clip_since(start, body[0], body[1], body[0] + body[2], body[1] + body[3]);
        if content > body[3] {
            let bh = (body[3] * body[3] / content).max(12.0);
            let by = body[1] + (body[3] - bh) * s.scroll / (content - body[3]);
            rounded(ui.batch, ui.atlas, [body[0] + body[2] + 2.0, by, 2.0, bh], alpha(ACCENT, 0.8));
        }

        // The footer: what happened last, and Done.
        ui.clip = [x, y, w, h];
        let foot_y = y + h - FOOTER + 5.0;
        if let Some((text, good)) = &s.view.message {
            let c = if *good { GOOD } else { BAD };
            fill(ui.batch, ui.atlas, [x + 10.0, foot_y + 6.0, 3.0, 3.0], c);
            label(ui.batch, ui.atlas, &fit(ui.atlas, text, w - 120.0), x + 17.0, foot_y + 4.0, DIM);
        } else {
            label(ui.batch, ui.atlas, "Esc or K closes", x + 10.0, foot_y + 4.0, FAINT);
        }
        ui.button([x + w - 76.0, foot_y, 68.0, 16.0], "Done", Hit::Done, true, true);
        let hits = std::mem::take(&mut ui.hits);
        s.hits = hits;
        s.glow.retain(|(h, _)| s.hits.iter().any(|(_, k)| k == h));

        let grow = 0.94 + 0.06 * open;
        batch.scale_since(panel_start, x + w / 2.0, y + h / 2.0, grow, grow);
        fade_since(batch, panel_start, open);
    });
}

fn schematics(ui: &mut Ui<'_>, v: &View, body: Rect, top: f32) -> f32 {
    let x = body[0];
    let w = body[2];
    let mut y = top;
    label(ui.batch, ui.atlas, &fit(ui.atlas, &format!("Folder: {}", v.folder), w - 150.0), x + 2.0, y + 4.0, FAINT);
    ui.button([x + w - 140.0, y, 66.0, 15.0], "Open folder", Hit::Do(Action::OpenFolder), false, true);
    ui.button([x + w - 70.0, y, 66.0, 15.0], "Refresh", Hit::Do(Action::Refresh), false, true);
    y += 20.0;
    if v.files.is_empty() {
        let lines = [
            "No schematics yet.",
            "Put .litematic or .schem files in the folder above (Open",
            "folder), then press Refresh. Litematica schematics from Java",
            "Edition and WorldEdit .schem files work as they are.",
        ];
        for (k, l) in lines.iter().enumerate() {
            label(ui.batch, ui.atlas, l, x + 4.0, y + 6.0 + k as f32 * LINE, if k == 0 { WHITE } else { DIM });
        }
        return y + 6.0 + lines.len() as f32 * LINE - top + 8.0;
    }
    for f in &v.files {
        let r = [x, y, w - 4.0, 20.0];
        let hit = Hit::Do(Action::Load(f.name.clone()));
        let g = ui.glow(&hit, r);
        rounded(ui.batch, ui.atlas, r, if f.loaded { alpha(ACCENT, 0.16) } else { mix(ROW_BG, HOVER, g) });
        if f.loaded {
            fill(ui.batch, ui.atlas, [r[0], r[1] + 4.0, 2.0, 12.0], ACCENT);
        }
        // A little stack-of-blocks icon.
        let ix = r[0] + 6.0 + g;
        for (k, c) in [(0.0, ACCENT), (4.0, ACCENT_2), (2.0, mix(ACCENT, WHITE, 0.4))].iter().enumerate() {
            let oy = if k == 2 { 2.0 } else { 9.0 };
            fill(ui.batch, ui.atlas, [ix + c.0, r[1] + oy, 6.0, 6.0], alpha(c.1, 0.85));
        }
        let name = f.name.strip_suffix(".litematic").or(f.name.strip_suffix(".schem")).unwrap_or(&f.name);
        label(ui.batch, ui.atlas, &fit(ui.atlas, name, w - 120.0), r[0] + 22.0 + g, r[1] + 6.0, if f.loaded { WHITE } else { mix(DIM, WHITE, g) });
        let tag = if f.loaded { "Loaded".to_owned() } else if g > 0.5 { "Click to load".to_owned() } else { format!("{} KB", f.kb.max(1)) };
        right(ui.batch, ui.atlas, &tag, r[0] + r[2] - 8.0, r[1] + 6.0, if f.loaded { ACCENT } else { FAINT });
        ui.hit(r, hit);
        y += 22.0;
    }
    y - top + 4.0
}

fn placement(ui: &mut Ui<'_>, v: &View, body: Rect, top: f32) -> f32 {
    let x = body[0];
    let w = body[2] - 4.0;
    let mut y = top;
    let Some(i) = &v.info else {
        label(ui.batch, ui.atlas, "No schematic loaded.", x + 4.0, y + 6.0, WHITE);
        label(ui.batch, ui.atlas, "Load one on the Schematics tab: it's placed where you stand.", x + 4.0, y + 6.0 + LINE, DIM);
        return 30.0;
    };
    label(ui.batch, ui.atlas, &fit(ui.atlas, &i.name, w), x + 2.0, y, WHITE);
    y += LINE;
    let by = if i.author.is_empty() { String::new() } else { format!("by {}  -  ", i.author) };
    let meta = format!("{by}{} x {} x {}  -  {} blocks  -  {}", i.size[0], i.size[1], i.size[2], i.blocks, i.file);
    label(ui.batch, ui.atlas, &fit(ui.atlas, &meta, w), x + 2.0, y, FAINT);
    y += LINE + 6.0;

    // The progress bar.
    if let Some(p) = &v.progress {
        let known = p.total.saturating_sub(p.unloaded).max(1);
        let frac = p.correct as f32 / known as f32;
        let bar = [x, y, w, 6.0];
        rounded(ui.batch, ui.atlas, bar, alpha(WHITE, 0.07));
        if frac > 0.0 {
            gradient_h(ui.batch, ui.atlas, [bar[0], bar[1], (bar[2] * frac).max(2.0), bar[3]], GOOD, ACCENT);
        }
        y += 9.0;
        let pct = (frac * 1000.0).floor() / 10.0;
        let head = if p.scanning { format!("Checking... {pct}%") } else { format!("{pct}% built") };
        label(ui.batch, ui.atlas, &head, x + 2.0, y, if pct >= 100.0 { GOOD } else { WHITE });
        let mut cx = x + 70.0;
        for (n, text, c) in [
            (p.missing, "missing", MISSING),
            (p.wrong_block, "wrong", BAD),
            (p.wrong_state, "wrong state", WARN),
            (p.extra, "extra", EXTRA),
        ] {
            let t = format!("{n} {text}");
            fill(ui.batch, ui.atlas, [cx, y + 2.0, 4.0, 4.0], c);
            label(ui.batch, ui.atlas, &t, cx + 6.0, y, if n > 0 { DIM } else { FAINT });
            cx += width(ui.atlas, &t) + 14.0;
        }
        y += LINE;
        if p.unloaded > 0 {
            label(ui.batch, ui.atlas, &format!("{} blocks are in chunks that aren't loaded (not counted).", p.unloaded), x + 2.0, y, FAINT);
            y += LINE;
        }
        y += 4.0;
    }

    // Where it is.
    label(ui.batch, ui.atlas, "POSITION (its origin)", x + 2.0, y, FAINT);
    y += LINE;
    for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
        let r = [x, y, w, 16.0];
        rounded(ui.batch, ui.atlas, r, ROW_BG);
        label(ui.batch, ui.atlas, name, r[0] + 7.0, r[1] + 4.0, ACCENT);
        label(ui.batch, ui.atlas, &i.origin[axis].to_string(), r[0] + 20.0, r[1] + 4.0, WHITE);
        let bw = 30.0;
        for (k, (d, t)) in [(-10, "-10"), (-1, "-1"), (1, "+1"), (10, "+10")].iter().enumerate() {
            let br = [r[0] + r[2] - (4.0 - k as f32) * (bw + 3.0), r[1] + 1.0, bw, 14.0];
            ui.button(br, t, Hit::Do(Action::Nudge(axis as u8, *d)), false, true);
        }
        y += 18.0;
    }
    ui.button([x, y, w / 2.0 - 2.0, 16.0], "Move it to me", Hit::Do(Action::Here), true, true);
    ui.button([x + w / 2.0 + 2.0, y, w / 2.0 - 2.0, 16.0], "Unload", Hit::Do(Action::Unload), false, true);
    y += 22.0;
    label(ui.batch, ui.atlas, "ROTATION", x + 2.0, y, FAINT);
    y += LINE;
    ui.segments([x, y, w, 15.0], &ROTATIONS, i.rotation as usize, |k| Hit::Do(Action::Rotation(k as u8)));
    y += 19.0;
    label(ui.batch, ui.atlas, "MIRROR", x + 2.0, y, FAINT);
    y += LINE;
    ui.segments([x, y, w, 15.0], &MIRRORS, i.mirror as usize, |k| Hit::Do(Action::Mirror(k as u8)));
    y += 20.0;
    for hint in [
        "With a stick in hand: right-click moves it to the block you look at,",
        "Ctrl + scroll nudges it the way you look, Ctrl + right-click turns it.",
        "Left Alt + scroll nudges it with anything in hand.",
    ] {
        label(ui.batch, ui.atlas, &fit(ui.atlas, hint, w), x + 2.0, y, FAINT);
        y += LINE;
    }
    y - top + 4.0
}

fn materials(ui: &mut Ui<'_>, v: &View, body: Rect, top: f32, by_name: bool) -> f32 {
    let x = body[0];
    let w = body[2] - 4.0;
    let mut y = top;
    if v.info.is_none() {
        label(ui.batch, ui.atlas, "Load a schematic to see what it needs.", x + 4.0, y + 6.0, DIM);
        return 24.0;
    }
    let sort = if by_name { "Sorted by name" } else { "Most missing first" };
    ui.button([x + w - 100.0, y, 100.0, 15.0], sort, Hit::Sort, false, true);
    label(ui.batch, ui.atlas, &format!("{} different items", v.materials.len()), x + 2.0, y + 4.0, FAINT);
    y += 19.0;
    let (c1, c2, c3) = (x + w - 96.0, x + w - 50.0, x + w - 6.0);
    label(ui.batch, ui.atlas, "ITEM", x + 4.0, y, FAINT);
    right(ui.batch, ui.atlas, "TOTAL", c1, y, FAINT);
    right(ui.batch, ui.atlas, "MISSING", c2, y, FAINT);
    right(ui.batch, ui.atlas, "HAVE", c3, y, FAINT);
    y += LINE + 1.0;
    let mut rows: Vec<&Material> = v.materials.iter().collect();
    if by_name {
        rows.sort_by(|a, b| a.item.cmp(&b.item));
    } else {
        rows.sort_by(|a, b| b.missing.cmp(&a.missing).then(b.total.cmp(&a.total)).then(a.item.cmp(&b.item)));
    }
    for (k, m) in rows.iter().enumerate() {
        let r = [x, y, w, 12.0];
        if k % 2 == 0 {
            fill(ui.batch, ui.atlas, r, ROW_BG);
        }
        let done = m.missing == 0;
        let enough = m.have >= m.missing;
        let name = m.item.replace('_', " ");
        label(ui.batch, ui.atlas, &fit(ui.atlas, &name, c1 - x - 50.0), x + 4.0, y + 2.0, if done { FAINT } else { WHITE });
        right(ui.batch, ui.atlas, &m.total.to_string(), c1, y + 2.0, DIM);
        right(ui.batch, ui.atlas, &m.missing.to_string(), c2, y + 2.0, if done { GOOD } else { MISSING });
        right(ui.batch, ui.atlas, &m.have.to_string(), c3, y + 2.0, if done || enough { GOOD } else { BAD });
        y += 12.0;
    }
    y += 4.0;
    label(ui.batch, ui.atlas, "Missing counts only loaded chunks; Have is your inventory.", x + 2.0, y, FAINT);
    y + LINE - top
}

fn settings(ui: &mut Ui<'_>, v: &View, body: Rect, top: f32) -> f32 {
    let x = body[0];
    let w = body[2] - 4.0;
    let mut y = top;
    label(ui.batch, ui.atlas, "OVERLAY: block outlines in the world", x + 2.0, y, FAINT);
    y += LINE;
    ui.segments([x, y, w, 15.0], &OVERLAYS, v.overlay as usize, |k| Hit::Do(Action::Overlay(k as u8)));
    y += 18.0;
    for (text, c) in [("missing", MISSING), ("wrong block", BAD), ("wrong state (turned...)", WARN), ("extra", EXTRA)] {
        fill(ui.batch, ui.atlas, [x + 4.0, y + 2.0, 5.0, 5.0], c);
        label(ui.batch, ui.atlas, text, x + 12.0, y, DIM);
        y += LINE - 1.0;
    }
    y += 3.0;
    ui.switch([x, y, w, 16.0], "Show extra blocks (where it should be air)", v.show_extra, Hit::Do(Action::ShowExtra(!v.show_extra)));
    y += 22.0;
    label(ui.batch, ui.atlas, "LAYERS", x + 2.0, y, FAINT);
    y += LINE;
    ui.segments([x, y, w, 15.0], &LAYER_MODES, v.layer_mode as usize, |k| Hit::Do(Action::LayerMode(k as u8)));
    y += 18.0;
    if v.layer_mode != 0 {
        let r = [x, y, w, 16.0];
        rounded(ui.batch, ui.atlas, r, ROW_BG);
        label(ui.batch, ui.atlas, &format!("Y = {}", v.layer), r[0] + 7.0, r[1] + 4.0, WHITE);
        let bw = 34.0;
        ui.button([r[0] + r[2] - 3.0 * (bw + 3.0) - 30.0, r[1] + 1.0, bw + 30.0, 14.0], "Where I am", Hit::Do(Action::LayerHere), false, true);
        ui.button([r[0] + r[2] - 2.0 * (bw + 3.0), r[1] + 1.0, bw, 14.0], "Down", Hit::Do(Action::Layer(v.layer - 1)), false, true);
        ui.button([r[0] + r[2] - (bw + 3.0), r[1] + 1.0, bw, 14.0], "Up", Hit::Do(Action::Layer(v.layer + 1)), false, true);
        y += 18.0;
        label(ui.batch, ui.atlas, "Page Up / Page Down change the layer while you play.", x + 2.0, y, FAINT);
        y += LINE;
    }
    y += 4.0;
    label(ui.batch, ui.atlas, "SHOW AROUND YOU (blocks)", x + 2.0, y, FAINT);
    y += LINE;
    let radii: Vec<String> = RADII.iter().map(|r| r.to_string()).collect();
    let refs: Vec<&str> = radii.iter().map(String::as_str).collect();
    let picked = RADII.iter().position(|&r| r == v.radius).unwrap_or(1);
    ui.segments([x, y, w, 15.0], &refs, picked, |k| Hit::Do(Action::Radius(RADII[k])));
    y += 20.0;
    ui.switch([x, y, w, 16.0], "On-screen status (top right)", v.show_hud, Hit::Do(Action::ShowHud(!v.show_hud)));
    y += 22.0;
    label(ui.batch, ui.atlas, "KEYS (change them in Options > Controls > Key Binds)", x + 2.0, y, FAINT);
    y += LINE;
    for (what, key) in &v.keys {
        label(ui.batch, ui.atlas, what, x + 4.0, y, DIM);
        right(ui.batch, ui.atlas, key, x + w - 4.0, y, WHITE);
        y += LINE;
    }
    label(ui.batch, ui.atlas, "Chat: /litematica (or /lm) list, load, here, move, rotate, mirror, unload", x + 2.0, y + 3.0, FAINT);
    y += LINE + 4.0;
    y - top + 4.0
}

pub fn click(x: f32, y: f32) {
    with_state(|s| {
        let Some((_, hit)) = s.hits.iter().rev().find(|(r, _)| inside(*r, (x, y))).cloned() else { return };
        match hit {
            Hit::Tab(k) => {
                if k != s.tab {
                    s.tab = k;
                    s.scroll = 0.0;
                    s.target = 0.0;
                    s.switched = Some(Instant::now());
                }
            }
            Hit::Done => s.done = true,
            Hit::Sort => s.by_name = !s.by_name,
            Hit::Do(a) => {
                if matches!(a, Action::Load(_)) {
                    s.tab = 1;
                    s.scroll = 0.0;
                    s.target = 0.0;
                    s.switched = Some(Instant::now());
                }
                s.actions.push(a);
            }
        }
    });
}

pub fn wheel(amount: f32) {
    with_state(|s| s.target = (s.target - amount * 22.0).clamp(0.0, (s.content - 100.0).max(0.0)));
}

/// A key on the menu; whether it used it (Escape is left to close the screen).
pub fn key(key: winit::keyboard::KeyCode) -> bool {
    use winit::keyboard::KeyCode as K;
    with_state(|s| match key {
        K::ArrowLeft | K::ArrowRight => {
            s.tab = if key == K::ArrowLeft { (s.tab + TABS.len() - 1) % TABS.len() } else { (s.tab + 1) % TABS.len() };
            s.scroll = 0.0;
            s.target = 0.0;
            s.switched = Some(Instant::now());
            true
        }
        K::ArrowUp => {
            s.target = (s.target - 22.0).max(0.0);
            true
        }
        K::ArrowDown => {
            s.target += 22.0;
            true
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_become_actions() {
        open(false);
        with_state(|s| {
            s.hits = vec![
                ([0.0, 0.0, 10.0, 10.0], Hit::Do(Action::Nudge(1, 10))),
                ([20.0, 0.0, 10.0, 10.0], Hit::Tab(2)),
                ([40.0, 0.0, 10.0, 10.0], Hit::Do(Action::Load("house.litematic".into()))),
                ([60.0, 0.0, 10.0, 10.0], Hit::Done),
            ];
            s.actions.clear();
        });
        click(5.0, 5.0);
        click(25.0, 5.0);
        assert_eq!(with_state(|s| s.tab), 2);
        click(45.0, 5.0);
        assert_eq!(with_state(|s| s.tab), 1);
        click(100.0, 100.0);
        assert_eq!(take_actions(), vec![Action::Nudge(1, 10), Action::Load("house.litematic".into())]);
        assert!(!take_done());
        click(65.0, 5.0);
        assert!(take_done());
        assert!(key(winit::keyboard::KeyCode::ArrowRight));
        assert_eq!(with_state(|s| s.tab), 2);
        assert!(!key(winit::keyboard::KeyCode::Escape));
        closed();
        assert!(!is_open());
    }
}
