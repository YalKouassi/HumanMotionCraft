//! The window's look: dark or light, and every colour the app paints itself (the timeline, the
//! curve editor, the panels, the overlays and ghosts). The colour-safe palette (Settings >
//! Colours) takes its colours from Okabe and Ito's set, which people with any kind of colour
//! vision tell apart; it never pairs red with green.
//!
//! Drawing code asks `self.palette()` for a colour instead of keeping its own constant, so every
//! theme stays complete: add a field to [`Palette`] (or [`Surface`] for the greys a theme sets)
//! and a value to each. Four palettes come out: standard or colour-safe hues, each on the dark or
//! the light greys ([`palette`]); on light, the hues are darkened to read on a pale ground.

use egui::Color32;

use super::App;
use super::prefs::{Colours, Theme};

/// The greys the painted areas use (the timeline's tracks, the curve graph, thumbnails); the
/// theme sets them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// The timeline's and curve graph's ground.
    pub track: Color32,
    /// The timeline's summary row (every key).
    pub summary: Color32,
    /// Every other part row (dope sheet), and the two foot rows.
    pub row: Color32,
    pub row_alt: Color32,
    /// Row names and secondary text painted on a track.
    pub ink: Color32,
    /// Ruler and grid numbers.
    pub ink_faint: Color32,
    /// The ruler's ticks and the graph's grid.
    pub grid: Color32,
    /// The graph's zero line.
    pub grid_zero: Color32,
    /// Shade past the animation's end, and (lighter) outside the play range.
    pub past_end: Color32,
    pub outside_range: Color32,
    /// The box-select rectangle.
    pub select_fill: Color32,
    pub select_stroke: Color32,
    /// The outline of key glyphs, dots and markers.
    pub outline: Color32,
    /// A play range handle while no range is set.
    pub range_off: Color32,
    /// A library thumbnail still being drawn.
    pub thumb: Color32,
    /// The picker's bones, and its figure's outline.
    pub bone: Color32,
    pub figure: Color32,
    /// A dot on a coloured span (a pinned foot).
    pub on_colour: Color32,
}

/// The colours with a meaning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The support area while the centre of mass is over it.
    pub support_ok: Color32,
    /// The support area while the body is off balance.
    pub support_off: Color32,
    /// The keyed pose behind the physics pass (RGBA, 0..1, for the renderer).
    pub ghost_keyed: [f32; 4],
    /// The AI preview's ghost (RGBA, 0..1).
    pub ghost_preview: [f32; 4],
    /// The AI preview: its root path and marks, the timeline's preview span, a panel's
    /// "previewing" text.
    pub preview: Color32,
    /// A refusal or failure (toasts, the message list, a panel's refusal, "unavailable").
    pub error: Color32,
    /// A warning (a fallback, something not set up).
    pub warning: Color32,
    /// Keys: set by hand, filled by the AI, selected (outline and the chosen row's name).
    pub key_manual: Color32,
    pub key_ai: Color32,
    pub key_selected: Color32,
    /// The playhead, on the timeline and the curve graph.
    pub playhead: Color32,
    /// The timeline's outline while it has the keyboard.
    pub focus: Color32,
    /// The AI bands on the summary row, and their label.
    pub ai_band: Color32,
    pub ai_band_text: Color32,
    /// Markers, and the play range's handles.
    pub marker: Color32,
    pub range: Color32,
    /// The foot-contact strip: planted, and pinned (foot-locked).
    pub planted: Color32,
    pub pinned: Color32,
    /// The curve editor's six channels: X, Y, Z angle, then hips X, Y, Z.
    pub channels: [Color32; 6],
    /// The Save button while there are unsaved changes (black text on it).
    pub unsaved: Color32,
    /// Physics problems: penetration, sliding, off balance, anything else.
    pub problems: [Color32; 4],
    /// Scene boxes, the chosen one, and the travel path.
    pub scene_box: Color32,
    pub scene_box_selected: Color32,
    pub travel: Color32,
    /// The picker's joints: idle, picked, the active one.
    pub joint: Color32,
    pub joint_picked: Color32,
    pub joint_active: Color32,
    /// A control the app points out for a moment (it glows this colour).
    pub hint: Color32,
    /// The greys.
    pub surface: Surface,
}

pub const DARK: Surface = Surface {
    track: Color32::from_gray(26),
    summary: Color32::from_gray(32),
    row: Color32::from_gray(29),
    row_alt: Color32::from_gray(27),
    ink: Color32::from_gray(150),
    ink_faint: Color32::from_gray(135),
    grid: Color32::from_gray(44),
    grid_zero: Color32::from_gray(64),
    past_end: Color32::from_black_alpha(70),
    outside_range: Color32::from_black_alpha(45),
    select_fill: Color32::from_rgba_premultiplied(20, 20, 20, 20),
    select_stroke: Color32::from_gray(200),
    outline: Color32::BLACK,
    range_off: Color32::from_gray(80),
    thumb: Color32::from_gray(35),
    bone: Color32::from_gray(120),
    figure: Color32::from_gray(60),
    on_colour: Color32::from_gray(30),
};

pub const LIGHT: Surface = Surface {
    track: Color32::from_gray(238),
    summary: Color32::from_gray(226),
    row: Color32::from_gray(232),
    row_alt: Color32::from_gray(244),
    ink: Color32::from_gray(80),
    ink_faint: Color32::from_gray(105),
    grid: Color32::from_gray(205),
    grid_zero: Color32::from_gray(160),
    past_end: Color32::from_black_alpha(28),
    outside_range: Color32::from_black_alpha(16),
    select_fill: Color32::from_rgba_premultiplied(0, 0, 0, 14),
    select_stroke: Color32::from_gray(70),
    outline: Color32::from_gray(40),
    range_off: Color32::from_gray(170),
    thumb: Color32::from_gray(225),
    bone: Color32::from_gray(150),
    figure: Color32::from_gray(185),
    on_colour: Color32::from_gray(250),
};

/// The colours the app has always used.
pub const STANDARD: Palette = Palette {
    support_ok: Color32::from_rgb(90, 200, 120),
    support_off: Color32::from_rgb(230, 90, 80),
    ghost_keyed: [0.45, 0.65, 1.0, 0.30],
    ghost_preview: [0.45, 1.0, 0.55, 0.35],
    preview: Color32::from_rgb(110, 230, 130),
    error: Color32::from_rgb(235, 100, 90),
    warning: Color32::from_rgb(235, 190, 80),
    key_manual: Color32::from_rgb(235, 190, 80),
    key_ai: Color32::from_rgb(120, 160, 230),
    key_selected: Color32::WHITE,
    playhead: Color32::from_rgb(230, 90, 80),
    focus: Color32::from_rgb(120, 150, 210),
    ai_band: Color32::from_rgb(120, 160, 230),
    ai_band_text: Color32::from_rgb(170, 195, 245),
    marker: Color32::from_rgb(90, 205, 190),
    range: Color32::from_rgb(120, 205, 120),
    planted: Color32::from_rgb(70, 150, 100),
    pinned: Color32::from_rgb(130, 220, 150),
    channels: [
        Color32::from_rgb(230, 95, 90),
        Color32::from_rgb(110, 200, 100),
        Color32::from_rgb(95, 145, 235),
        Color32::from_rgb(240, 170, 160),
        Color32::from_rgb(170, 230, 160),
        Color32::from_rgb(160, 190, 245),
    ],
    unsaved: Color32::from_rgb(235, 190, 80),
    problems: [
        Color32::from_rgb(235, 110, 90),
        Color32::from_rgb(240, 180, 60),
        Color32::from_rgb(200, 120, 230),
        Color32::from_gray(180),
    ],
    scene_box: Color32::from_rgb(130, 150, 175),
    scene_box_selected: Color32::from_rgb(255, 205, 70),
    travel: Color32::from_rgb(250, 150, 60),
    joint: Color32::from_rgb(90, 200, 255),
    joint_picked: Color32::from_rgb(255, 160, 60),
    joint_active: Color32::from_rgb(255, 214, 70),
    hint: Color32::from_rgb(255, 150, 40),
    surface: DARK,
};

/// Okabe–Ito: blue against vermilion for balance, sky blue and orange for the two ghosts, and
/// no red–green pair anywhere (the curve channels are vermilion, bluish green and blue).
pub const SAFE: Palette = Palette {
    support_ok: Color32::from_rgb(0, 114, 178),
    support_off: Color32::from_rgb(213, 94, 0),
    ghost_keyed: [0.34, 0.71, 0.91, 0.32],
    ghost_preview: [0.90, 0.62, 0.0, 0.38],
    preview: Color32::from_rgb(230, 159, 0),
    error: Color32::from_rgb(213, 94, 0),
    warning: Color32::from_rgb(240, 228, 66),
    key_manual: Color32::from_rgb(230, 159, 0),
    key_ai: Color32::from_rgb(86, 180, 233),
    key_selected: Color32::WHITE,
    playhead: Color32::from_rgb(213, 94, 0),
    focus: Color32::from_rgb(86, 180, 233),
    ai_band: Color32::from_rgb(86, 180, 233),
    ai_band_text: Color32::from_rgb(150, 210, 240),
    marker: Color32::from_rgb(204, 121, 167),
    range: Color32::from_rgb(0, 158, 115),
    planted: Color32::from_rgb(0, 114, 178),
    pinned: Color32::from_rgb(86, 180, 233),
    channels: [
        Color32::from_rgb(213, 94, 0),
        Color32::from_rgb(0, 158, 115),
        Color32::from_rgb(0, 114, 178),
        Color32::from_rgb(230, 159, 0),
        Color32::from_rgb(86, 180, 233),
        Color32::from_rgb(204, 121, 167),
    ],
    unsaved: Color32::from_rgb(230, 159, 0),
    problems: [
        Color32::from_rgb(213, 94, 0),
        Color32::from_rgb(240, 228, 66),
        Color32::from_rgb(204, 121, 167),
        Color32::from_gray(180),
    ],
    scene_box: Color32::from_rgb(130, 150, 175),
    scene_box_selected: Color32::from_rgb(240, 228, 66),
    travel: Color32::from_rgb(230, 159, 0),
    joint: Color32::from_rgb(86, 180, 233),
    joint_picked: Color32::from_rgb(230, 159, 0),
    joint_active: Color32::from_rgb(240, 228, 66),
    hint: Color32::from_rgb(230, 159, 0),
    surface: DARK,
};

/// A hue darkened to read on the light greys.
const fn deep(c: Color32) -> Color32 {
    let [r, g, b, _] = c.to_array();
    Color32::from_rgb(
        (r as u16 * 3 / 5) as u8,
        (g as u16 * 3 / 5) as u8,
        (b as u16 * 3 / 5) as u8,
    )
}

/// `p` on the light greys: text and thin lines on the pale tracks get deeper hues; fills (the
/// key glyphs, the contact spans, the Save button) keep theirs, outlined.
const fn on_light(p: Palette) -> Palette {
    let c = p.channels;
    Palette {
        preview: deep(p.preview),
        error: deep(p.error),
        warning: deep(p.warning),
        key_selected: Color32::from_gray(15),
        playhead: deep(p.playhead),
        focus: deep(p.focus),
        ai_band_text: deep(p.key_ai),
        marker: deep(p.marker),
        range: deep(p.range),
        channels: [
            deep(c[0]),
            deep(c[1]),
            deep(c[2]),
            deep(c[3]),
            deep(c[4]),
            deep(c[5]),
        ],
        problems: [
            deep(p.problems[0]),
            deep(p.problems[1]),
            deep(p.problems[2]),
            Color32::from_gray(90),
        ],
        travel: deep(p.travel),
        hint: deep(p.hint),
        surface: LIGHT,
        ..p
    }
}

pub const STANDARD_LIGHT: Palette = on_light(STANDARD);
pub const SAFE_LIGHT: Palette = on_light(SAFE);

pub fn palette(theme: Theme, colours: Colours) -> &'static Palette {
    match (theme, colours) {
        (Theme::Dark, Colours::Standard) => &STANDARD,
        (Theme::Dark, Colours::Safe) => &SAFE,
        (Theme::Light, Colours::Standard) => &STANDARD_LIGHT,
        (Theme::Light, Colours::Safe) => &SAFE_LIGHT,
    }
}

/// `colour` with `alpha` (0..255), for fills under an outline.
pub fn faded(colour: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha)
}

pub fn visuals(theme: Theme) -> egui::Visuals {
    match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    }
}

impl App {
    /// The colours for the theme and the palette the user chose.
    pub(super) fn palette(&self) -> &'static Palette {
        let g = &self.shell.prefs.get().general;
        palette(g.theme, g.colours)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rough red–green check: under deuteranopia (no M cones) red and green both read as
    /// yellowish; the safe palette's in/out pair must differ in blue, not only in red and green.
    #[test]
    fn the_safe_balance_pair_differs_in_blue() {
        let (a, b) = (SAFE.support_ok, SAFE.support_off);
        assert!((a.b() as i32 - b.b() as i32).abs() > 120);
        let (c, d) = (STANDARD.support_ok, STANDARD.support_off);
        assert!(
            (c.b() as i32 - d.b() as i32).abs() < 60,
            "the standard pair is the red/green one"
        );
        // the curve channels X and Y, drawn side by side, differ in blue too
        let (x, y) = (SAFE.channels[0], SAFE.channels[1]);
        assert!((x.b() as i32 - y.b() as i32).abs() > 100);
    }

    /// Text and lines stand out from the ground they're painted on, in every palette.
    #[test]
    fn ink_contrasts_with_its_ground() {
        let luma = |c: Color32| 0.3 * c.r() as f32 + 0.59 * c.g() as f32 + 0.11 * c.b() as f32;
        for p in [&STANDARD, &SAFE, &STANDARD_LIGHT, &SAFE_LIGHT] {
            let ground = luma(p.surface.track);
            for (name, c) in [
                ("ink", p.surface.ink),
                ("selected", p.key_selected),
                ("playhead", p.playhead),
                ("marker", p.marker),
                ("channel X", p.channels[0]),
                ("channel Z", p.channels[2]),
            ] {
                assert!((luma(c) - ground).abs() > 60.0, "{name} on {:?}", p.surface.track);
            }
        }
    }
}
