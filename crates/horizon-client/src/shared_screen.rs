//! The pause menu's Friends button in a shared world (`game/shared_worlds.rs` fills it), every
//! player's, host or guest. Top to bottom: the world and who hosts it; who is in it and how each
//! one's connection goes; what is going on (a slow route and why, a backup to load, the storage);
//! what can be done (answers to requests, Give Host to a player, Ask to Host); and at the foot
//! Copy Invite (the world's invite, for a friend to add it) beside Done. Drawn as Play with
//! Friends is (`friends_screen.rs`).
use crate::{
    hud::{HudAtlas, HudBatch, rgba},
    multiplayer::wrap,
    text::Text,
    ui,
};

/// The English title, and what the screen's widgets are named by.
pub const TITLE: &str = "Shared World";

fn tr(key: &str, english: &str) -> String {
    crate::lang::get_or(&format!("horizon.shared.{key}"), english)
}

/// A button's job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The host gives the world to this player (a UUID).
    GiveHost(String),
    /// The host lets this player (a UUID) host, as they asked.
    Accept(String),
    /// The host says no to this player (a UUID).
    Deny(String),
    /// A guest asks the host for the world.
    AskHost,
    /// The world's invite to the clipboard, for a friend to add the world.
    CopyInvite,
    /// The owner's answers to a request to load a backup.
    ApproveLoad,
    DeclineLoad,
    Done,
}

/// What the screen shows.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub world: String,
    /// Who hosts ("You host this world.", "Hosted by Alex.").
    pub status: String,
    /// Who is in it, with how each connects ("You (host), Steve (direct)").
    pub players: String,
    /// What is going on, a note each: a slow route and why, a request, the storage.
    pub notes: Vec<String>,
    /// Buttons above the footer: the others a row each, then Give Host's (labelled with the
    /// player's name) two to a row under "Give Host to:".
    pub buttons: Vec<(String, Action, bool)>,
    /// The world's invite can be copied.
    pub invite: bool,
    /// It was just copied.
    pub copied: bool,
}

const WHITE: u32 = u32::MAX;
fn grey() -> u32 {
    rgba(170, 170, 170, 255)
}
fn soft() -> u32 {
    rgba(255, 255, 160, 255)
}

struct Button {
    x: f32,
    y: f32,
    w: f32,
    label: String,
    action: Action,
    active: bool,
}

fn layout(
    atlas: &HudAtlas,
    (w, h): (f32, f32),
    view: &View,
) -> (Vec<(String, u32, f32)>, Vec<Button>) {
    let width = (w - 40.0).min(310.0);
    let cx = (w / 2.0).floor();
    let mut lines = Vec::new();
    let mut y = 32.0;
    let text = |lines: &mut Vec<(String, u32, f32)>, y: &mut f32, what: &str, color: u32| {
        for line in wrap(what, width, atlas) {
            lines.push((line, color, *y));
            *y += 10.0;
        }
    };
    text(&mut lines, &mut y, &view.world, WHITE);
    text(&mut lines, &mut y, &view.status, grey());
    if !view.players.is_empty() {
        y += 3.0;
        text(&mut lines, &mut y, &view.players, WHITE);
    }
    if !view.notes.is_empty() {
        y += 3.0;
        for note in &view.notes {
            text(&mut lines, &mut y, note, soft());
        }
    }
    y += 6.0;
    let mut buttons = Vec::new();
    let fits = |y: f32| y + 20.0 <= h - 33.0;
    let (give, rows): (Vec<_>, Vec<_>) = view
        .buttons
        .iter()
        .partition(|(_, action, _)| matches!(action, Action::GiveHost(_)));
    for (label, action, active) in rows.into_iter().take(6) {
        if !fits(y) {
            break;
        }
        buttons.push(Button {
            x: cx - 100.0,
            y,
            w: 200.0,
            label: label.clone(),
            action: action.clone(),
            active: *active,
        });
        y += 24.0;
    }
    if !give.is_empty() && fits(y + 12.0) {
        lines.push((tr("give_host_to", "Give Host to:"), WHITE, y));
        y += 12.0;
        for pair in give.chunks(2) {
            if !fits(y) {
                break;
            }
            for (i, (label, action, active)) in pair.iter().enumerate() {
                buttons.push(Button {
                    x: cx - 100.0 + i as f32 * 102.0,
                    y,
                    w: 98.0,
                    label: label.clone(),
                    action: action.clone(),
                    active: *active,
                });
            }
            y += 24.0;
        }
    }
    let done = if view.invite {
        buttons.push(Button {
            x: cx - 100.0,
            y: h - 27.0,
            w: 98.0,
            label: if view.copied {
                tr("copied", "Copied")
            } else {
                tr("copy_invite_short", "Copy Invite")
            },
            action: Action::CopyInvite,
            active: true,
        });
        (cx + 2.0, 98.0)
    } else {
        (cx - 100.0, 200.0)
    };
    buttons.push(Button {
        x: done.0,
        y: h - 27.0,
        w: done.1,
        label: "gui.done".into(),
        action: Action::Done,
        active: true,
    });
    (lines, buttons)
}

fn label(key: &str) -> Text {
    if key.starts_with("gui.") {
        Text::translate(key, &[])
    } else {
        Text::literal(key)
    }
}

pub fn draw(batch: &mut HudBatch, atlas: &HudAtlas, cursor: (f32, f32), view: &View) {
    let (w, h) = (batch.width, batch.height);
    ui::menu_background(batch, atlas, 0.0, 0.0, w, h, true);
    let title = Text::literal(tr("world_title", TITLE));
    let tw = atlas.font.width_text(&title);
    batch.text_styled(atlas, &title, ((w - tw) / 2.0).floor(), 15.0, WHITE, true);
    let (lines, buttons) = layout(atlas, (w, h), view);
    for (line, color, y) in lines {
        let lw = HudBatch::text_width(atlas, &line);
        batch.text(atlas, &line, ((w - lw) / 2.0).floor(), y, color, true);
    }
    for b in buttons {
        let hover = b.active && ui::hovered(cursor, b.x, b.y, b.w, 20.0);
        ui::button(
            batch,
            atlas,
            b.x,
            b.y,
            b.w,
            20.0,
            &label(&b.label),
            b.active,
            hover,
        );
    }
}

/// The button under a click, if it is active.
pub fn click(
    atlas: &HudAtlas,
    size: (f32, f32),
    cursor: (f32, f32),
    view: &View,
) -> Option<Action> {
    layout(atlas, size, view)
        .1
        .into_iter()
        .find(|b| b.active && ui::hovered(cursor, b.x, b.y, b.w, 20.0))
        .map(|b| b.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atlas() -> HudAtlas {
        HudAtlas::build(std::path::Path::new("/nonexistent/horizon-test-assets")).unwrap()
    }

    /// A host's screen with as much on it as there can be.
    fn busy() -> View {
        View {
            world: "Island".into(),
            status: "You host this world.".into(),
            players: "You (host), Steve (direct), Alex (bore.pub), Sam (direct, UDP)".into(),
            notes: vec![
                "Friends elsewhere may go through bore.pub (slower): your router didn't open a port (turn on UPnP). Now: 2 direct, 1 bore.pub.".into(),
            ],
            buttons: (0..7)
                .map(|i| (format!("Player{i}"), Action::GiveHost(i.to_string()), true))
                .collect(),
            invite: true,
            copied: false,
        }
    }

    #[test]
    fn every_button_fits_above_the_footer_and_done_is_always_there() {
        let atlas = atlas();
        let mut view = busy();
        // Answers to players who asked come first, a row each.
        view.buttons
            .insert(0, ("Let P1 Host".into(), Action::Accept("1".into()), true));
        let (_, buttons) = layout(&atlas, (427.0, 240.0), &view);
        assert_eq!(buttons.last().unwrap().action, Action::Done);
        assert_eq!(buttons[0].action, Action::Accept("1".into()));
        for b in &buttons[..buttons.len() - 2] {
            assert!(b.y + 20.0 <= 240.0 - 33.0);
        }
        // Give Host's two to a row, side by side.
        let give: Vec<&Button> = buttons
            .iter()
            .filter(|b| matches!(b.action, Action::GiveHost(_)))
            .collect();
        assert!(give.len() >= 2, "{}", give.len());
        assert_eq!(give[0].y, give[1].y);
        assert!(give[0].x + give[0].w <= give[1].x);
    }

    #[test]
    fn the_sections_come_in_order_and_copy_invite_sits_beside_done() {
        let atlas = atlas();
        let view = busy();
        // Phones (the smallest GUI is 240 high), an iPad's, a large one.
        for size in [
            (427.0, 240.0),
            (520.0, 240.0),
            (341.0, 256.0),
            (640.0, 360.0),
        ] {
            let (lines, buttons) = layout(&atlas, size, &view);
            let at = |text: &str| {
                lines
                    .iter()
                    .find(|(l, _, _)| l.contains(text))
                    .map(|(_, _, y)| *y)
                    .unwrap_or_else(|| panic!("{size:?}: no line with {text}"))
            };
            assert!(at("Island") < at("You host") && at("You host") < at("Steve"));
            assert!(at("Steve") < at("Friends elsewhere"));
            let copy = &buttons[buttons.len() - 2];
            let done = &buttons[buttons.len() - 1];
            assert_eq!(
                (&copy.action, &done.action),
                (&Action::CopyInvite, &Action::Done)
            );
            assert!(copy.active && copy.x + copy.w <= done.x, "{size:?}");
            assert_eq!(
                click(&atlas, size, (copy.x + 5.0, copy.y + 5.0), &view),
                Some(Action::CopyInvite)
            );
            // Under the text: at least two Give Host buttons, above the footer.
            let give: Vec<&Button> = buttons
                .iter()
                .filter(|b| matches!(b.action, Action::GiveHost(_)))
                .collect();
            assert!(give.len() >= 2, "{size:?}: {}", give.len());
            let bottom = lines.iter().map(|(_, _, y)| *y + 10.0).fold(0.0, f32::max);
            assert!(
                give.iter()
                    .all(|b| b.y >= bottom && b.y + 20.0 <= size.1 - 33.0),
                "{size:?}"
            );
        }
        // Without an invite to copy, Done alone, full width.
        let (_, buttons) = layout(
            &atlas,
            (427.0, 240.0),
            &View {
                invite: false,
                ..view
            },
        );
        assert!(buttons.iter().all(|b| b.action != Action::CopyInvite));
        assert_eq!(buttons.last().unwrap().w, 200.0);
    }

    #[test]
    fn a_guests_screen_has_ask_to_host_and_its_own_route() {
        let atlas = atlas();
        let view = View {
            world: "Island".into(),
            status: "Hosted by Alex.".into(),
            players: "Alex (host), You (direct)".into(),
            notes: vec!["Your connection: direct to the host (no bore.pub).".into()],
            buttons: vec![("Ask to Host".into(), Action::AskHost, true)],
            invite: true,
            copied: false,
        };
        let (lines, buttons) = layout(&atlas, (427.0, 240.0), &view);
        assert!(lines.iter().any(|(l, _, _)| l.contains("Hosted by Alex")));
        assert!(buttons.iter().any(|b| b.action == Action::AskHost));
        assert!(buttons.iter().any(|b| b.action == Action::CopyInvite));
    }
}
