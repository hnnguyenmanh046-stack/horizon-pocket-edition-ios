// Litematica for Horizon: schematics in the world. Load a .litematic file
// from the schematics folder (Litematica's own format, so schematics from
// Java Edition work), place it where you stand, move, turn and mirror it,
// and build it: an overlay outlines every block that's missing (blue),
// wrong (red), the right block in the wrong state (orange) or in the way
// (pink, optional), layer by layer if you like. A material list says what
// it needs, what's still missing and what you have; the on-screen status
// shows how far along you are.
//
// Inspired by Litematica by masa (LGPLv3); written for Horizon from scratch,
// no Litematica code. Blocks show as outlines, not see-through models (Horizon
// has no way yet for a mod to draw extra block models).
//
// K opens the menu; hold Left Alt and scroll to move the placement the way
// you look; Page Up / Page Down move the layer. Chat: /litematica or /lm.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use horizon_client::options_ui::litematica_menu as menu;
use litematica_schematic::{self as sch, Block, Schematic, materials, transform::Placement};

pub const ID: &str = "litematica";
pub const NAME: &str = "Litematica";
pub const VERSION: &str = "1.1.0";
pub const DESCRIPTION: &str = "Schematics in the world, after Litematica by masa: load .litematic files (Java Edition ones work) and WorldEdit .schem files, place, move, turn and mirror them, and build with an overlay that outlines every missing (blue), wrong (red), wrongly turned (orange) and extra (pink) block, layer by layer. A material list shows what it needs, what's missing and what you have. K opens the menu; with a stick in hand right-click moves it to the block you look at, Ctrl + scroll nudges it and Ctrl + right-click turns it; Left Alt + scroll moves it with anything in hand; Page Up/Down change the layer, /litematica in chat.";

pub const MENU: [&str; 3] = ["menu", "Litematica menu", "key.keyboard.k"];
pub const NUDGE: [&str; 3] = ["nudge", "Hold + scroll: move the schematic", "key.keyboard.left.alt"];
pub const LAYER_UP: [&str; 3] = ["layer_up", "Layer up", "key.keyboard.page.up"];
pub const LAYER_DOWN: [&str; 3] = ["layer_down", "Layer down", "key.keyboard.page.down"];
pub const TOOL_USE: [&str; 3] = ["tool_use", "With a stick: move it to the block you look at", "key.mouse.right"];
pub const TOOL_MOD: [&str; 3] = ["tool_mod", "With a stick: hold + scroll moves it, + use turns it", "key.keyboard.left.control"];
pub const KEYS: [[&str; 3]; 6] = [MENU, NUDGE, LAYER_UP, LAYER_DOWN, TOOL_USE, TOOL_MOD];

/// The tool item, as in Litematica: a stick in your hand.
const TOOL: &str = "stick";
/// How far the stick reaches to pick where the schematic goes.
const TOOL_REACH: f64 = 96.0;

/// Most block outlines drawn at once, and blocks checked per tick by the full pass.
const MAX_OUTLINES: usize = 2500;
const PASS_BUDGET: usize = 60_000;

/// Overlay colours (ARGB).
const C_MISSING: u32 = 0xFF50_C3FF;
const C_WRONG: u32 = 0xFFFF_4848;
const C_STATE: u32 = 0xFFFF_A830;
const C_EXTRA: u32 = 0xFFFF_60D0;
const C_BOX: u32 = 0xFFFF_FFFF;
const C_ORIGIN: u32 = 0xFFFF_E040;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Correct,
    Missing,
    WrongBlock,
    WrongState,
    Extra,
    /// Air that should be air, or a block whose chunk isn't loaded.
    Nothing,
    Unloaded,
}

/// A palette entry as it stands once placed, matched to Horizon's blocks.
struct Want {
    block: Block,
    air: bool,
    /// Horizon's id for exactly this state, when it knows it.
    id: Option<horizon_world::registry::BlockStateId>,
    item: Option<(String, u32)>,
}

struct Loaded {
    file: String,
    schem: Schematic,
    place: Placement,
    /// Per region, per palette index.
    wants: Vec<Vec<Want>>,
    /// The materials it needs in total, by item, and its blocks that aren't air.
    totals: Vec<(String, u32)>,
    count: usize,
    /// The full pass: where it got to (region, cell), and its counts so far.
    pass_at: (usize, usize),
    pass: Counts,
    /// The last finished pass.
    done: Option<Counts>,
    /// The blocks outlined, near you, found by the last near pass.
    outlines: Vec<([i32; 3], u32)>,
    near_tick: u32,
}

#[derive(Clone, Default)]
struct Counts {
    correct: usize,
    missing: usize,
    wrong_block: usize,
    wrong_state: usize,
    extra: usize,
    unloaded: usize,
    /// Items still needed (missing or wrong), by item.
    needed: HashMap<String, u32>,
}

#[derive(Clone)]
struct Prefs {
    overlay: u8,
    layer_mode: u8,
    layer: i32,
    radius: u32,
    show_hud: bool,
    show_extra: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { overlay: 1, layer_mode: 0, layer: 64, radius: 24, show_hud: true, show_extra: false }
    }
}

#[derive(Default)]
struct State {
    loaded: Option<Loaded>,
    prefs: Prefs,
    prefs_read: bool,
    /// The last placement was put back after the game started.
    restored: bool,
    nudge_held: bool,
    tool_mod_held: bool,
    message: Option<(String, bool)>,
    files: Vec<(String, u64)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

pub fn key_text(key: &str) -> String {
    match key.strip_prefix("key.keyboard.") {
        Some(k) => k.replace('.', " ").to_uppercase(),
        None => key.to_owned(),
    }
}

/// `%APPDATA%\horizon\schematics` (next to options.txt), made if missing.
fn folder(game: &crate::game::Game<'_>) -> PathBuf {
    let data = game.native_data_dir(ID);
    let home = data.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(PathBuf::from).unwrap_or_default();
    let dir = home.join("schematics");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn list_files(dir: &std::path::Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let meta = e.metadata().ok()?;
            (meta.is_file() && is_schematic(&name)).then(|| (name, meta.len() / 1024))
        })
        .collect();
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out
}

/// The file kinds it reads: Litematica's, and Sponge/WorldEdit's.
const EXTENSIONS: [&str; 2] = [".litematic", ".schem"];

fn is_schematic(name: &str) -> bool {
    let lower = name.to_lowercase();
    EXTENSIONS.iter().any(|e| lower.ends_with(e))
}

/// A file's name without its extension.
fn stem(name: &str) -> &str {
    EXTENSIONS.iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name)
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// The palette's entries placed and matched to Horizon's blocks.
fn wants(game: &crate::game::Game<'_>, schem: &Schematic, place: &Placement) -> Vec<Vec<Want>> {
    let registry = game.pack.assets.registry;
    schem
        .regions
        .iter()
        .map(|r| {
            r.palette
                .iter()
                .map(|b| {
                    let block = place.block(b);
                    let properties = block.props.iter().cloned().collect();
                    let mut state = horizon_world::chunk::BlockState { name: block.name.clone(), properties };
                    let mut id = registry.id(&state);
                    if id.is_none() {
                        state.name = short(&block.name).to_owned();
                        id = registry.id(&state);
                    }
                    Want { air: block.is_air(), item: materials::item_for(&block), id, block }
                })
                .collect()
        })
        .collect()
}

fn totals(schem: &Schematic, wants: &[Vec<Want>]) -> Vec<(String, u32)> {
    let mut map: HashMap<String, u32> = HashMap::new();
    for (r, region) in schem.regions.iter().enumerate() {
        let mut per = vec![0u32; region.palette.len()];
        for &b in &region.blocks {
            per[b as usize] += 1;
        }
        for (k, n) in per.into_iter().enumerate() {
            if let Some((item, each)) = &wants[r][k].item {
                *map.entry(item.clone()).or_default() += n * each;
            }
        }
    }
    let mut out: Vec<(String, u32)> = map.into_iter().collect();
    out.sort();
    out
}

/// How the world's block at `pos` compares with `want`.
fn status(game: &crate::game::Game<'_>, pos: [i32; 3], want: &Want) -> Status {
    if !game.world.chunks.contains_key(&(pos[0] >> 4, pos[2] >> 4)) {
        return Status::Unloaded;
    }
    let id = game.world.block(pos[0], pos[1], pos[2]);
    let air = id == game.world.air;
    let have = game.pack.assets.registry.state(id);
    let have_name = have.map_or("air", |s| short(&s.name));
    let have_air = air || matches!(have_name, "air" | "cave_air" | "void_air");
    if want.air {
        // Water and lava where air should be aren't "in the way".
        return if have_air || matches!(have_name, "water" | "lava" | "bubble_column") { Status::Nothing } else { Status::Extra };
    }
    if want.id == Some(id) {
        return Status::Correct;
    }
    if have_air {
        return Status::Missing;
    }
    if have_name == short(&want.block.name) {
        // Horizon may not know every state the file names: same block,
        // same properties it has, counts as right.
        let same = want.id.is_none()
            && have.is_some_and(|s| want.block.props.iter().all(|(k, v)| s.properties.get(k).is_none_or(|h| h == v)));
        return if same { Status::Correct } else { Status::WrongState };
    }
    Status::WrongBlock
}

fn in_layer(prefs: &Prefs, y: i32) -> bool {
    match prefs.layer_mode {
        1 => y == prefs.layer,
        2 => y <= prefs.layer,
        3 => y >= prefs.layer,
        _ => true,
    }
}

impl Loaded {
    fn new(game: &crate::game::Game<'_>, file: String, schem: Schematic, place: Placement) -> Self {
        let wants = wants(game, &schem, &place);
        let totals = totals(&schem, &wants);
        let count = schem.block_count();
        Self { file, schem, place, wants, totals, count, pass_at: (0, 0), pass: Counts::default(), done: None, outlines: Vec::new(), near_tick: 0 }
    }

    /// The placement changed: the states turn again and the checks start over.
    fn replace(&mut self, game: &crate::game::Game<'_>, place: Placement) {
        if place.rotation != self.place.rotation || place.mirror != self.place.mirror {
            self.wants = wants(game, &self.schem, &place);
        }
        self.place = place;
        self.pass_at = (0, 0);
        self.pass = Counts::default();
        self.done = None;
        self.outlines.clear();
        self.near_tick = 0;
    }

    fn world_box(&self) -> ([i32; 3], [i32; 3]) {
        let (lo, hi) = self.schem.bounds();
        self.place.world_box(lo, hi)
    }

    /// The want for world position `w`, if it's inside a region.
    fn want_at(&self, w: [i32; 3]) -> Option<&Want> {
        let l = self.place.local(w);
        for (k, r) in self.schem.regions.iter().enumerate() {
            let p = [l[0] - r.min[0], l[1] - r.min[1], l[2] - r.min[2]];
            if (0..3).all(|i| p[i] >= 0 && p[i] < r.size[i]) {
                return self.wants[k].get(r.index_at(p[0], p[1], p[2]) as usize);
            }
        }
        None
    }

    /// Part of the full pass over every block: counts for the progress and
    /// the materials still needed.
    fn pass_step(&mut self, game: &crate::game::Game<'_>) {
        let mut budget = PASS_BUDGET;
        while budget > 0 {
            let (ri, ci) = self.pass_at;
            let Some(r) = self.schem.regions.get(ri) else {
                self.done = Some(std::mem::take(&mut self.pass));
                self.pass_at = (0, 0);
                return;
            };
            let end = (ci + budget).min(r.volume());
            let [sx, _, sz] = r.size;
            for i in ci..end {
                let ii = i as i32;
                let local = [r.min[0] + ii % sx, r.min[1] + ii / (sx * sz), r.min[2] + (ii / sx) % sz];
                let want = &self.wants[ri][r.blocks[i] as usize];
                let pos = self.place.world(local);
                let c = &mut self.pass;
                let st = status(game, pos, want);
                match st {
                    Status::Correct => c.correct += 1,
                    Status::Missing => c.missing += 1,
                    Status::WrongBlock => c.wrong_block += 1,
                    Status::WrongState => c.wrong_state += 1,
                    Status::Extra => c.extra += 1,
                    Status::Unloaded if !want.air => c.unloaded += 1,
                    _ => {}
                }
                if let Some((item, n)) = &want.item
                    && matches!(st, Status::Missing | Status::WrongBlock)
                {
                    *c.needed.entry(item.clone()).or_default() += n;
                }
            }
            budget -= end - ci;
            self.pass_at = if end >= r.volume() { (ri + 1, 0) } else { (ri, end) };
        }
    }

    /// The blocks to outline near `at`.
    fn near(&mut self, game: &crate::game::Game<'_>, prefs: &Prefs, at: [i32; 3]) {
        self.outlines.clear();
        if prefs.overlay == 0 {
            return;
        }
        let (lo, hi) = self.world_box();
        let r = prefs.radius as i32;
        let lo = [lo[0].max(at[0] - r), lo[1].max(at[1] - r), lo[2].max(at[2] - r)];
        let hi = [hi[0].min(at[0] + r), hi[1].min(at[1] + r), hi[2].min(at[2] + r)];
        // Nearest layers first, so a cap drops the far ones.
        let mut ys: Vec<i32> = (lo[1]..=hi[1]).filter(|&y| in_layer(prefs, y)).collect();
        ys.sort_by_key(|y| (y - at[1]).abs());
        for y in ys {
            for z in lo[2]..=hi[2] {
                for x in lo[0]..=hi[0] {
                    let w = [x, y, z];
                    let Some(want) = self.want_at(w) else { continue };
                    let c = match status(game, w, want) {
                        Status::Missing if prefs.overlay == 1 => C_MISSING,
                        Status::WrongBlock => C_WRONG,
                        Status::WrongState => C_STATE,
                        Status::Extra if prefs.show_extra => C_EXTRA,
                        _ => continue,
                    };
                    self.outlines.push((w, c));
                    if self.outlines.len() >= MAX_OUTLINES {
                        return;
                    }
                }
            }
        }
    }
}

fn read_prefs(game: &crate::game::Game<'_>) -> Prefs {
    let get = |k: &str| game.native_setting(ID, k);
    let d = Prefs::default();
    let num = |k: &str, d: i64| get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(d);
    Prefs {
        overlay: num("overlay", d.overlay.into()).clamp(0, 2) as u8,
        layer_mode: num("layer_mode", d.layer_mode.into()).clamp(0, 3) as u8,
        layer: num("layer", d.layer.into()).clamp(-2048, 2048) as i32,
        radius: num("radius", d.radius.into()).clamp(8, 128) as u32,
        show_hud: get("show_hud").map_or(d.show_hud, |v| v == "true"),
        show_extra: get("show_extra").map_or(d.show_extra, |v| v == "true"),
    }
}

fn save_prefs(game: &crate::game::Game<'_>, p: &Prefs) {
    for (k, v) in [
        ("overlay", p.overlay.to_string()),
        ("layer_mode", p.layer_mode.to_string()),
        ("layer", p.layer.to_string()),
        ("radius", p.radius.to_string()),
        ("show_hud", p.show_hud.to_string()),
        ("show_extra", p.show_extra.to_string()),
    ] {
        game.native_set_setting(ID, k, &v);
    }
}

/// Keeps the placement for next time.
fn save_placement(game: &crate::game::Game<'_>, l: Option<&Loaded>) {
    let v = l.map_or(String::new(), |l| {
        let p = l.place;
        format!("{}|{}|{}|{}|{}|{}", l.file, p.origin[0], p.origin[1], p.origin[2], p.rotation, p.mirror)
    });
    game.native_set_setting(ID, "placement", &v);
}

fn player_block(game: &crate::game::Game<'_>) -> Option<[i32; 3]> {
    let p = game.native_player()?;
    Some([p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32])
}

/// Loads `file` from the schematics folder at `place` (or where you stand).
fn load(game: &crate::game::Game<'_>, s: &mut State, file: &str, place: Option<Placement>) -> Result<String, String> {
    let dir = folder(game);
    // A name without its extension finds either kind.
    let name = if is_schematic(file) {
        file.to_owned()
    } else {
        EXTENSIONS.iter().map(|e| format!("{file}{e}")).find(|n| dir.join(n).is_file()).unwrap_or_else(|| format!("{file}.litematic"))
    };
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("give just the file's name".into());
    }
    let path = dir.join(&name);
    let bytes = std::fs::read(&path).map_err(|_| format!("{name} isn't in the schematics folder"))?;
    let schem = sch::load(&bytes).map_err(|e| format!("couldn't read {name}: {e}"))?;
    let place = match place {
        Some(p) => p,
        None => Placement { origin: player_block(game).ok_or("join a world first")?, ..Default::default() },
    };
    let label = if schem.name.is_empty() { stem(&name).to_owned() } else { schem.name.clone() };
    let count = schem.block_count();
    s.loaded = Some(Loaded::new(game, name, schem, place));
    save_placement(game, s.loaded.as_ref());
    Ok(format!("Loaded {label} ({count} blocks) at {} {} {}", place.origin[0], place.origin[1], place.origin[2]))
}

/// The way you look along one axis: 0 x, 1 y, 2 z, and its sign.
fn facing_axis(game: &crate::game::Game<'_>) -> (usize, i32) {
    let Some(p) = game.native_player() else { return (1, 1) };
    if p.pitch < -55.0 {
        return (1, 1);
    }
    if p.pitch > 55.0 {
        return (1, -1);
    }
    // Yaw 0 looks south (+z), 90 west (-x), 180 north, 270 east.
    let yaw = p.yaw.rem_euclid(360.0);
    match ((yaw + 45.0) / 90.0) as u32 % 4 {
        0 => (2, 1),
        1 => (0, -1),
        2 => (2, -1),
        _ => (0, 1),
    }
}

fn say(game: &mut crate::game::Game<'_>, text: &str) {
    game.native_chat(&format!("Litematica: {text}"));
}

/// Applies one change; returns what to tell the player.
fn apply(game: &mut crate::game::Game<'_>, a: menu::Action) -> Option<(String, bool)> {
    use menu::Action as A;
    let g = &*game;
    with_state(|s| {
        let mut place = s.loaded.as_ref().map(|l| l.place);
        let msg = match a {
            A::Load(f) => Some(match load(g, s, &f, None) {
                Ok(m) => (m, true),
                Err(e) => (e, false),
            }),
            A::Unload => {
                let m = s.loaded.take().map(|l| (format!("Unloaded {}", l.file), true));
                save_placement(g, None);
                place = None;
                m
            }
            A::Here => {
                if let (Some(p), Some(at)) = (place.as_mut(), player_block(g)) {
                    p.origin = at;
                }
                Some(("Moved to you".to_owned(), true))
            }
            A::Nudge(axis, d) => {
                if let Some(p) = place.as_mut() {
                    p.origin[axis.min(2) as usize] += d;
                }
                None
            }
            A::Rotation(r) => {
                if let Some(p) = place.as_mut() {
                    p.rotation = r % 4;
                }
                None
            }
            A::Mirror(m) => {
                if let Some(p) = place.as_mut() {
                    p.mirror = m.min(2);
                }
                None
            }
            A::Overlay(o) => {
                s.prefs.overlay = o.min(2);
                None
            }
            A::LayerMode(m) => {
                s.prefs.layer_mode = m.min(3);
                if let Some(at) = player_block(g)
                    && m != 0
                {
                    s.prefs.layer = at[1];
                }
                None
            }
            A::Layer(y) => {
                s.prefs.layer = y.clamp(-2048, 2048);
                None
            }
            A::LayerHere => {
                if let Some(at) = player_block(g) {
                    s.prefs.layer = at[1];
                }
                None
            }
            A::Radius(r) => {
                s.prefs.radius = r.clamp(8, 128);
                None
            }
            A::ShowHud(v) => {
                s.prefs.show_hud = v;
                None
            }
            A::ShowExtra(v) => {
                s.prefs.show_extra = v;
                None
            }
            A::OpenFolder => {
                let dir = folder(g);
                #[cfg(windows)]
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&dir).spawn();
                #[cfg(all(unix, not(target_os = "macos")))]
                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                None
            }
            A::Refresh => {
                s.files = list_files(&folder(g));
                None
            }
        };
        if let (Some(l), Some(p)) = (s.loaded.as_mut(), place)
            && p != l.place
        {
            l.replace(g, p);
            save_placement(g, Some(l));
        }
        save_prefs(g, &s.prefs);
        if let Some(m) = &msg {
            s.message = Some(m.clone());
        }
        msg
    })
}

// ---------- hooks ----------

/// Each tick: settings read once, the last placement put back, the checks.
pub(super) fn tick(game: &mut crate::game::Game<'_>) {
    if !game.native_on(ID) {
        return;
    }
    let g = &*game;
    with_state(|s| {
        if !s.prefs_read {
            s.prefs = read_prefs(g);
            s.prefs_read = true;
        }
        let Some(at) = player_block(g) else {
            return;
        };
        if !s.restored {
            s.restored = true;
            if let Some(v) = g.native_setting(ID, "placement").filter(|v| !v.is_empty()) {
                let parts: Vec<&str> = v.split('|').collect();
                if parts.len() == 6 {
                    let n = |i: usize| parts[i].parse::<i32>().unwrap_or(0);
                    let place = Placement { origin: [n(1), n(2), n(3)], rotation: n(4) as u8 % 4, mirror: n(5).clamp(0, 2) as u8 };
                    if let Err(e) = load(g, s, parts[0], Some(place)) {
                        s.message = Some((e, false));
                    }
                }
            }
        }
        let prefs = s.prefs.clone();
        if let Some(l) = s.loaded.as_mut() {
            l.pass_step(g);
            if l.near_tick == 0 {
                l.near(g, &prefs, at);
            }
            l.near_tick = (l.near_tick + 1) % 10;
        }
    });
}

/// The overlay: the placement's box, its origin, and the blocks to fix.
pub(super) fn lines(game: &crate::game::Game<'_>, lines: &mut Vec<horizon_client::gizmos::Line>) {
    if !game.native_on(ID) {
        return;
    }
    with_state(|s| {
        let Some(l) = &s.loaded else { return };
        let mut cube = |lo: [f64; 3], hi: [f64; 3], color: u32, width: f32| {
            let c = |x: bool, y: bool, z: bool| [if x { hi[0] } else { lo[0] }, if y { hi[1] } else { lo[1] }, if z { hi[2] } else { lo[2] }];
            let edges = [
                (c(false, false, false), c(true, false, false)),
                (c(false, false, true), c(true, false, true)),
                (c(false, true, false), c(true, true, false)),
                (c(false, true, true), c(true, true, true)),
                (c(false, false, false), c(false, true, false)),
                (c(true, false, false), c(true, true, false)),
                (c(false, false, true), c(false, true, true)),
                (c(true, false, true), c(true, true, true)),
                (c(false, false, false), c(false, false, true)),
                (c(true, false, false), c(true, false, true)),
                (c(false, true, false), c(false, true, true)),
                (c(true, true, false), c(true, true, true)),
            ];
            for (start, end) in edges {
                lines.push(horizon_client::gizmos::Line { start, end, color, width });
            }
        };
        let (lo, hi) = l.world_box();
        let f = |p: [i32; 3], d: f64| p.map(|v| f64::from(v) + d);
        cube(f(lo, -0.01), f(hi, 1.01), C_BOX, 2.5);
        let o = l.place.origin;
        cube(f(o, 0.3), f(o, 0.7), C_ORIGIN, 2.0);
        for &(p, color) in &l.outlines {
            let width = if color == C_MISSING { 1.5 } else { 2.5 };
            cube(f(p, -0.004), f(p, 1.004), color, width);
        }
    });
}

/// The menu's key, the layer keys, and the Alt key held for the wheel.
pub(super) fn key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    if !game.native_on(ID) {
        return;
    }
    if key == game.native_mod_key(ID, NUDGE[0], NUDGE[2]) {
        with_state(|s| s.nudge_held = pressed);
        return;
    }
    if key == game.native_mod_key(ID, TOOL_MOD[0], TOOL_MOD[2]) {
        with_state(|s| s.tool_mod_held = pressed);
        return;
    }
    if pressed && game.native_playing() && key == game.native_mod_key(ID, TOOL_USE[0], TOOL_USE[2]) && holding_tool(game) {
        tool_use(game);
        return;
    }
    if !pressed || !game.native_window() {
        return;
    }
    if key == game.native_mod_key(ID, MENU[0], MENU[2]) {
        if game.native_playing() {
            let loaded = with_state(|s| s.loaded.is_some());
            game.capture(false);
            game.open_options_at(true, horizon_client::options_ui::Page::Agent);
            game.ui.options.in_world = true;
            menu::open(loaded);
            sync_menu(game);
        } else if game.screen == crate::game::Screen::Options && menu::shows(&game.ui.options) {
            let _ = game.options_key(winit::keyboard::KeyCode::Escape, None);
        }
        return;
    }
    if !game.native_playing() {
        return;
    }
    let step = if key == game.native_mod_key(ID, LAYER_UP[0], LAYER_UP[2]) {
        1
    } else if key == game.native_mod_key(ID, LAYER_DOWN[0], LAYER_DOWN[2]) {
        -1
    } else {
        return;
    };
    let (mode, layer) = with_state(|s| (s.prefs.layer_mode, s.prefs.layer));
    if with_state(|s| s.loaded.is_none()) {
        return;
    }
    if mode == 0 {
        // From "all", the first press shows only the layer where you stand.
        apply(game, menu::Action::LayerMode(1));
    } else {
        apply(game, menu::Action::Layer(layer + step));
    }
    let (mode, layer) = with_state(|s| (s.prefs.layer_mode, s.prefs.layer));
    say(game, &format!("layer {} {layer}", ["all", "only", "below", "above"][mode as usize]));
}

/// Whether the hand that acts holds the tool (a stick).
fn holding_tool(game: &crate::game::Game<'_>) -> bool {
    game.held().is_some_and(|s| materials::short_item(&s.item) == TOOL)
}

/// The stick used: the schematic goes to the block you look at (on the
/// side you look at), or with Ctrl held turns a quarter clockwise.
fn tool_use(game: &mut crate::game::Game<'_>) {
    let Some(place) = with_state(|s| s.loaded.as_ref().map(|l| l.place)) else {
        say(game, "nothing loaded: press K and pick a schematic");
        return;
    };
    if with_state(|s| s.tool_mod_held) {
        let r = (place.rotation + 1) % 4;
        apply(game, menu::Action::Rotation(r));
        say(game, &format!("turned to {}", menu::ROTATIONS[r as usize]));
        return;
    }
    let target = game.aim().and_then(|(origin, dir)| game.clip_ray(origin, dir, TOOL_REACH));
    let Some(hit) = target else {
        say(game, "look at a block to move it there");
        return;
    };
    let to: [i32; 3] = std::array::from_fn(|i| hit.pos[i] + hit.face[i]);
    let o = place.origin;
    for (axis, d) in [(0u8, to[0] - o[0]), (1, to[1] - o[1]), (2, to[2] - o[2])] {
        if d != 0 {
            apply(game, menu::Action::Nudge(axis, d));
        }
    }
    say(game, &format!("moved to {} {} {}", to[0], to[1], to[2]));
}

/// Left Alt + wheel (anything in hand), or Ctrl + wheel with the stick,
/// moves the placement the way you look.
pub(super) fn wheel(game: &mut crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    if !game.native_on(ID) || !game.native_playing() || steps == 0.0 {
        return;
    }
    let (alt, ctrl, loaded) = with_state(|s| (s.nudge_held, s.tool_mod_held, s.loaded.is_some()));
    if !loaded || !(alt || (ctrl && holding_tool(game))) {
        return;
    }
    *claimed = true;
    let (axis, sign) = facing_axis(game);
    let d = if steps > 0.0 { sign } else { -sign };
    apply(game, menu::Action::Nudge(axis as u8, d));
}

pub(super) fn commands(commands: &mut Vec<&'static str>) {
    commands.extend(["litematica", "lm"]);
}

/// `/litematica` (or `/lm`) and what follows.
pub(super) fn command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let mut words = command.split_whitespace();
    if !matches!(words.next(), Some("litematica" | "lm")) || !game.native_on(ID) {
        return;
    }
    *ran = true;
    let rest: Vec<&str> = words.collect();
    use menu::Action as A;
    let action = match rest.as_slice() {
        ["list"] | [] => {
            let files = list_files(&folder(game));
            if files.is_empty() {
                say(game, &format!("no schematics in {}", folder(game).display()));
            } else {
                say(game, &format!("{} schematics:", files.len()));
                for (n, kb) in files.iter().take(20) {
                    game.native_chat(&format!("  {n} ({kb} KB)"));
                }
                say(game, "/lm load <name> places one where you stand");
            }
            return;
        }
        ["load", name @ ..] if !name.is_empty() => A::Load(name.join(" ")),
        ["unload"] => A::Unload,
        ["here"] => A::Here,
        ["move", x, y, z] => match (x.parse::<i32>(), y.parse::<i32>(), z.parse::<i32>()) {
            (Ok(x), Ok(y), Ok(z)) => {
                let Some(o) = with_state(|s| s.loaded.as_ref().map(|l| l.place.origin)) else {
                    say(game, "nothing loaded");
                    return;
                };
                for (axis, d) in [(0u8, x - o[0]), (1, y - o[1]), (2, z - o[2])] {
                    if d != 0 {
                        apply(game, A::Nudge(axis, d));
                    }
                }
                say(game, &format!("moved to {x} {y} {z}"));
                return;
            }
            _ => {
                say(game, "/lm move <x> <y> <z>");
                return;
            }
        },
        ["rotate"] => {
            let r = with_state(|s| s.loaded.as_ref().map_or(0, |l| l.place.rotation));
            A::Rotation((r + 1) % 4)
        }
        ["mirror", m] => match *m {
            "none" => A::Mirror(0),
            "x" | "eastwest" | "east-west" => A::Mirror(1),
            "z" | "northsouth" | "north-south" => A::Mirror(2),
            _ => {
                say(game, "/lm mirror none|x|z");
                return;
            }
        },
        ["overlay", o] => match *o {
            "off" => A::Overlay(0),
            "on" | "all" => A::Overlay(1),
            "wrong" => A::Overlay(2),
            _ => {
                say(game, "/lm overlay off|on|wrong");
                return;
            }
        },
        ["layer", "all"] => A::LayerMode(0),
        ["layer", y] => match y.parse::<i32>() {
            Ok(y) => {
                apply(game, A::LayerMode(1));
                A::Layer(y)
            }
            Err(_) => {
                say(game, "/lm layer <y>|all");
                return;
            }
        },
        _ => {
            say(game, "/lm list | load <name> | here | move <x> <y> <z> | rotate | mirror none|x|z | overlay off|on|wrong | layer <y>|all | unload");
            return;
        }
    };
    let changed = !matches!(action, A::Load(_) | A::Unload | A::Here);
    if changed && with_state(|s| s.loaded.is_none()) && matches!(action, A::Rotation(_) | A::Mirror(_)) {
        say(game, "nothing loaded: /lm load <name>");
        return;
    }
    let shown = format!("{action:?}");
    match apply(game, action) {
        Some((m, _)) => say(game, &m),
        None => say(game, &format!("done ({})", shown.to_lowercase())),
    }
}

/// The on-screen status, top right: the schematic, how far along, the layer.
pub(super) fn hud(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, corners: &mut horizon_client::extras_hud::Corners) {
    use horizon_client::extras::Corner;
    if !game.native_on(ID) || !game.native_corner_shown(Corner::TopRight) {
        return;
    }
    let tool = holding_tool(game);
    let lines: Vec<(String, u32)> = with_state(|s| {
        let Some(l) = &s.loaded else { return Vec::new() };
        if !s.prefs.show_hud {
            return Vec::new();
        }
        let name = if l.schem.name.is_empty() { stem(&l.file).to_owned() } else { l.schem.name.clone() };
        let mut out = vec![(format!("Litematica: {name}"), 0xFF_FFFF)];
        match &l.done {
            Some(c) => {
                let total = c.correct + c.missing + c.wrong_block + c.wrong_state;
                let pct = if total == 0 { 100.0 } else { (c.correct as f32 * 1000.0 / total as f32).floor() / 10.0 };
                out.push((format!("{pct}% built"), if pct >= 100.0 { 0x5CE68A } else { 0xFFFFFF }));
                let mut bad = Vec::new();
                if c.missing > 0 {
                    bad.push(format!("{} missing", c.missing));
                }
                if c.wrong_block + c.wrong_state > 0 {
                    bad.push(format!("{} wrong", c.wrong_block + c.wrong_state));
                }
                if !bad.is_empty() {
                    out.push((bad.join(", "), 0xA0A8B4));
                }
            }
            None => out.push(("Checking...".into(), 0xA0A8B4)),
        }
        if tool {
            out.push(("Stick: right-click moves it here".into(), 0xFFE040));
            out.push(("Ctrl+scroll nudges, Ctrl+right-click turns".into(), 0xFFE040));
        }
        if s.prefs.layer_mode != 0 {
            let mode = ["", "Layer", "Below layer", "Above layer"][s.prefs.layer_mode as usize];
            out.push((format!("{mode} Y {}", s.prefs.layer), 0x5AC3FF));
        }
        out
    });
    if lines.is_empty() {
        return;
    }
    let pad = 3.0;
    let w = lines.iter().map(|(t, _)| game.native_text_width(t)).fold(0.0, f32::max) + pad * 2.0 + 3.0;
    let h = lines.len() as f32 * 10.0 + pad * 2.0 - 1.0;
    let (x, y) = corners.place(Corner::TopRight, w, h);
    game.native_fill(batch, [x, y, w, h], 0x9010_1218);
    game.native_fill(batch, [x, y, 2.0, h], 0xFF5A_BEFF);
    for (k, (text, rgb)) in lines.iter().enumerate() {
        game.native_text(batch, text, [x + pad + 3.0, y + pad + k as f32 * 10.0], *rgb, true);
    }
}

/// Each frame while the menu is open: its clicks, then the latest view.
pub(super) fn frame(game: &mut crate::game::Game<'_>) {
    if !menu::is_open() {
        return;
    }
    if game.screen != crate::game::Screen::Options || !menu::shows(&game.ui.options) {
        menu::closed();
        if game.screen == crate::game::Screen::Playing {
            game.capture(true);
        }
        return;
    }
    if menu::take_done() {
        let _ = game.options_key(winit::keyboard::KeyCode::Escape, None);
        return;
    }
    sync_menu(game);
}

fn sync_menu(game: &mut crate::game::Game<'_>) {
    for a in menu::take_actions() {
        apply(game, a);
    }
    let g = &*game;
    // Your inventory, by item.
    let mut have: HashMap<&str, u32> = HashMap::new();
    for stack in g.inventory.iter().flatten() {
        *have.entry(materials::short_item(&stack.item)).or_default() += stack.count;
    }
    let keys = KEYS
        .iter()
        .map(|k| (k[1].to_owned(), key_text(&g.native_mod_key(ID, k[0], k[2]))))
        .collect();
    let view = with_state(|s| {
        if s.files.is_empty() {
            s.files = list_files(&folder(g));
        }
        let loaded_file = s.loaded.as_ref().map(|l| l.file.clone());
        let p = &s.prefs;
        menu::View {
            folder: folder(g).display().to_string(),
            files: s
                .files
                .iter()
                .map(|(n, kb)| menu::FileRow { name: n.clone(), kb: *kb, loaded: loaded_file.as_deref() == Some(n.as_str()) })
                .collect(),
            info: s.loaded.as_ref().map(|l| menu::Info {
                file: l.file.clone(),
                name: if l.schem.name.is_empty() { stem(&l.file).to_owned() } else { l.schem.name.clone() },
                author: l.schem.author.clone(),
                size: l.schem.size(),
                blocks: l.count,
                origin: l.place.origin,
                rotation: l.place.rotation,
                mirror: l.place.mirror,
            }),
            progress: s.loaded.as_ref().map(|l| {
                let (c, scanning) = match &l.done {
                    Some(c) => (c.clone(), false),
                    None => (l.pass.clone(), true),
                };
                menu::Progress {
                    correct: c.correct,
                    missing: c.missing,
                    wrong_block: c.wrong_block,
                    wrong_state: c.wrong_state,
                    extra: c.extra,
                    unloaded: c.unloaded,
                    total: c.correct + c.missing + c.wrong_block + c.wrong_state + c.unloaded,
                    scanning,
                }
            }),
            materials: s
                .loaded
                .as_ref()
                .map(|l| {
                    let needed = l.done.as_ref().map(|c| &c.needed);
                    l.totals
                        .iter()
                        .map(|(item, total)| menu::Material {
                            item: item.clone(),
                            total: *total,
                            missing: needed.map_or(*total, |n| n.get(item).copied().unwrap_or(0)),
                            have: have.get(item.as_str()).copied().unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            overlay: p.overlay,
            layer_mode: p.layer_mode,
            layer: p.layer,
            radius: p.radius,
            show_hud: p.show_hud,
            show_extra: p.show_extra,
            message: s.message.clone(),
            keys,
        }
    });
    menu::set_view(view);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_filter() {
        let mut p = Prefs { layer: 70, ..Prefs::default() };
        assert!(in_layer(&p, 10));
        p.layer_mode = 1;
        assert!(in_layer(&p, 70) && !in_layer(&p, 71));
        p.layer_mode = 2;
        assert!(in_layer(&p, 69) && !in_layer(&p, 71));
        p.layer_mode = 3;
        assert!(in_layer(&p, 71) && !in_layer(&p, 69));
    }

    #[test]
    fn key_names_read_well() {
        assert_eq!(key_text("key.keyboard.page.up"), "PAGE UP");
        assert_eq!(key_text("key.keyboard.k"), "K");
    }
}
