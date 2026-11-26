// game-module-cfg: target_os = "ios"
//! iOS: what this release brought, on a card over the title screen at the first start with it,
//! after an update and after a new install alike (`horizon_client::whats_new` has the words).
//! "Got it" closes it for good: the version is written down in the private folder.
//!
//! The card is drawn under the update's card and takes its touches after it, so a newer release
//! is offered first; it steps aside for the catalog and the options.
use super::Game;
use horizon_client::{
    hud::HudBatch,
    ui_theme::{Face, Ui, color},
    whats_new::{self, Notes},
};
use std::sync::{Mutex, PoisonError};
use winit::event::{Touch, TouchPhase};

/// The version whose notes were closed, in the private folder.
const SEEN_FILE: &str = "whats-new-seen";

struct Card {
    /// The file was read.
    checked: bool,
    notes: Option<&'static Notes>,
    /// The button as last drawn (UI units).
    button: Option<[f32; 4]>,
}

/// The card is one for the whole app, as the title screen is.
static CARD: Mutex<Card> = Mutex::new(Card {
    checked: false,
    notes: None,
    button: None,
});

fn card() -> std::sync::MutexGuard<'static, Card> {
    CARD.lock().unwrap_or_else(PoisonError::into_inner)
}

fn seen_path() -> std::path::PathBuf {
    horizon_client::logging::private_home().join(SEEN_FILE)
}

/// One line of the card.
enum Line<'a> {
    Heading(&'a str),
    /// An item's line; its first has the dot.
    Item(String, bool),
    Gap(f32),
}

const PAD: f32 = 22.0;
const BUTTON_H: f32 = 40.0;
const ITEM_INDENT: f32 = 16.0;

impl Game<'_> {
    /// The notes to show now: over the title's main page only, and not over the catalog or the
    /// options.
    fn whats_new_shown(&self) -> Option<&'static Notes> {
        let on_title = self
            .title
            .as_ref()
            .is_some_and(|t| matches!(t.mode, horizon_client::title::Mode::Main))
            && !matches!(self.screen, super::Screen::Options | super::Screen::Win);
        if !on_title || self.catalog_open() {
            return None;
        }
        let mut card = card();
        if !card.checked {
            card.checked = true;
            let seen = std::fs::read_to_string(seen_path()).ok();
            card.notes = crate::android::app_version()
                .and_then(|(version, _)| whats_new::due(&version, seen.as_deref()));
        }
        card.notes
    }

    pub(super) fn draw_whats_new(&self, batch: &mut HudBatch) {
        let Some(notes) = self.whats_new_shown() else {
            return;
        };
        let mut ui = Ui::new(&self.gui.atlas, batch, (-1.0, -1.0));
        let (w, h) = (ui.w, ui.h);
        ui.fill(0.0, 0.0, w, h, color::with_alpha(color::bg(), 0.72));
        let card_w = (w - 48.0).min(520.0);
        let room = card_w - 2.0 * PAD - ITEM_INDENT;
        let mut lines = Vec::new();
        for (heading, items) in [("New", notes.new), ("Fixed", notes.fixed)] {
            if !lines.is_empty() {
                lines.push(Line::Gap(10.0));
            }
            lines.push(Line::Heading(heading));
            for item in items {
                let mut line = String::new();
                let mut first = true;
                for word in item.split(' ') {
                    let next = if line.is_empty() {
                        word.to_owned()
                    } else {
                        format!("{line} {word}")
                    };
                    if !line.is_empty() && ui.measure(Face::Regular, 14.0, &next) > room {
                        let full = std::mem::replace(&mut line, word.to_owned());
                        lines.push(Line::Item(full, first));
                        first = false;
                    } else {
                        line = next;
                    }
                }
                lines.push(Line::Item(line, first));
                lines.push(Line::Gap(4.0));
            }
        }
        let body: f32 = lines
            .iter()
            .map(|line| match line {
                Line::Heading(_) => 24.0,
                Line::Item(..) => 20.0,
                Line::Gap(gap) => *gap,
            })
            .sum();
        let card_h = PAD + 26.0 + 12.0 + body + 14.0 + BUTTON_H + PAD;
        let (x, y) = ((w - card_w) / 2.0, ((h - card_h) / 2.0).max(8.0));
        ui.rounded(x, y, card_w, card_h, 14.0, color::bg());
        ui.border(x, y, card_w, card_h, 14.0, color::border());
        let title = format!("What's new in {}", notes.version);
        ui.text(
            Face::Semibold,
            18.0,
            x + PAD,
            y + PAD,
            color::text(),
            &title,
        );
        let mut ty = y + PAD + 26.0 + 12.0;
        for line in &lines {
            match line {
                Line::Heading(words) => {
                    ui.text(Face::Semibold, 14.0, x + PAD, ty, color::text(), words);
                    ty += 24.0;
                }
                Line::Item(words, first) => {
                    if *first {
                        ui.rounded(x + PAD + 2.0, ty + 7.0, 5.0, 5.0, 2.5, color::ACCENT);
                    }
                    let left = x + PAD + ITEM_INDENT;
                    ui.text(Face::Regular, 14.0, left, ty, color::MUTED, words);
                    ty += 20.0;
                }
                Line::Gap(gap) => ty += gap,
            }
        }
        let button = [
            x + card_w - PAD - 160.0,
            y + card_h - PAD - BUTTON_H,
            160.0,
            BUTTON_H,
        ];
        ui.rounded(
            button[0],
            button[1],
            button[2],
            button[3],
            10.0,
            color::ACCENT,
        );
        ui.text_centered(
            Face::Semibold,
            15.0,
            button[0] + button[2] / 2.0,
            button[1] + (button[3] - 15.0) / 2.0 - 2.0,
            color::ON_ACCENT,
            "Got it",
        );
        ui.finish();
        card().button = Some(button);
    }

    /// While the card shows it takes every touch; a finger lifted on "Got it" closes it for good.
    pub(super) fn whats_new_touch(&mut self, touch: &Touch) -> bool {
        let Some(notes) = self.whats_new_shown() else {
            return false;
        };
        if touch.phase != TouchPhase::Ended {
            return true;
        }
        let [w, h] = self.touch_size();
        // The UI's scale for a window of this size (as `Ui::new`).
        let d = (w as f32 / 1040.0).min(h as f32 / 700.0).clamp(1.0, 4.0);
        let p = [touch.location.x as f32 / d, touch.location.y as f32 / d];
        let mut card = card();
        let pressed = card.button.is_some_and(|r| {
            p[0] >= r[0] && p[0] <= r[0] + r[2] && p[1] >= r[1] && p[1] <= r[1] + r[3]
        });
        if pressed {
            card.notes = None;
            card.button = None;
            if let Err(e) = std::fs::write(seen_path(), notes.version) {
                log::warn!("Horizon PE: the notes of {} show again: {e}", notes.version);
            }
        }
        true
    }
}
