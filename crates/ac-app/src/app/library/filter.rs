//! The library panel's filters: where items come from (all, yours, the built-ins), the chips
//! (length, loops, changed, part, starred) and the order. The words, source, length, loops, part
//! and stars become an `ac_io::find::Query`, so the panel finds what `library.find` finds;
//! "changed" (a built-in you edited, or the open animation with unsaved changes) and "recently
//! used" (the window's recent list) are the window's own.

use ac_io::find::{Length, Query, Sort, Source};

use super::super::App;
use super::super::ui_ids::Tag;

/// The order the list is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Order {
    /// By group, then name.
    #[default]
    Name,
    /// The ones used last first.
    Recent,
    /// The files changed last first.
    Modified,
    /// Shortest first (animations).
    Length,
}

const ORDERS: [(Order, &str, &str); 4] = [
    (Order::Name, "name", "Name"),
    (Order::Recent, "recent", "Recently used"),
    (Order::Modified, "modified", "Recently modified"),
    (Order::Length, "length", "Length"),
];

const SOURCES: [(Source, &str, &str, &str); 3] = [
    (Source::All, "all", "All", "Yours and the built-ins"),
    (Source::Mine, "mine", "Mine", "Only your own"),
    (
        Source::Builtin,
        "builtin",
        "Built-in",
        "Only the built-ins: for watching and learning (Save as makes your own copy)",
    ),
];

const LENGTHS: [(Length, &str, &str); 3] = [
    (Length::Short, "Short", "Under 2 s"),
    (Length::Medium, "Medium", "2 to 6 s"),
    (Length::Long, "Long", "Over 6 s"),
];

impl Order {
    pub fn name(self) -> &'static str {
        ORDERS.iter().find(|o| o.0 == self).map_or("name", |o| o.1)
    }

    pub fn parse(s: &str) -> Result<Order, String> {
        ORDERS
            .iter()
            .find(|o| o.1 == s)
            .map(|o| o.0)
            .ok_or_else(|| "name, recent, modified or length".into())
    }

    /// Grouped under group headers (the other orders are one list).
    pub fn grouped(self) -> bool {
        self == Order::Name
    }
}

pub fn source_name(s: Source) -> &'static str {
    SOURCES.iter().find(|x| x.0 == s).map_or("all", |x| x.1)
}

pub fn parse_source(s: &str) -> Result<Source, String> {
    SOURCES
        .iter()
        .find(|x| x.1 == s)
        .map(|x| x.0)
        .ok_or_else(|| "all, mine or builtin".into())
}

/// The panel's filters (the source is the same on both tabs; the chips are per kind).
#[derive(Default)]
pub struct Filters {
    pub source: Source,
    pub length: Option<Length>,
    pub loops: bool,
    pub changed: bool,
    pub part: Option<String>,
    pub stars: bool,
    pub order: Order,
}

impl Filters {
    /// The query for the words typed and these filters (`anims`: the animations tab).
    pub fn query(&self, words: &str, anims: bool) -> Query {
        Query {
            words: words.to_string(),
            source: self.source,
            length: self.length.filter(|_| anims),
            looping: (anims && self.loops).then_some(true),
            part: self.part.clone().filter(|_| !anims),
            favourites: self.stars,
            sort: match self.order {
                Order::Modified => Sort::Modified,
                Order::Length if anims => Sort::Length,
                _ => Sort::Name,
            },
            ..Query::default()
        }
    }

    /// A chip narrows the list.
    pub fn chips_on(&self, anims: bool) -> bool {
        self.stars
            || if anims {
                self.length.is_some() || self.loops || self.changed
            } else {
                self.part.is_some()
            }
    }

    fn clear_chips(&mut self) {
        self.length = None;
        self.loops = false;
        self.changed = false;
        self.part = None;
        self.stars = false;
    }
}

/// A chip: a small toggle.
fn chip(ui: &mut egui::Ui, on: bool, text: &str, tip: &str, id: &str) -> bool {
    ui.selectable_label(on, egui::RichText::new(text).small())
        .on_hover_text(tip)
        .tag(id, text)
        .clicked()
}

impl App {
    /// The source switch, the order and the chips (`parts`: the poses' parts, for its chip).
    pub(super) fn library_filters(&mut self, ui: &mut egui::Ui, anims: bool, parts: &[String]) {
        let f = &mut self.lib_view.filters;
        ui.horizontal(|ui| {
            for (s, name, label, tip) in SOURCES {
                if ui
                    .selectable_label(f.source == s, label)
                    .on_hover_text(tip)
                    .tag(format!("library.source.{name}"), label)
                    .clicked()
                {
                    f.source = s;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = ORDERS.iter().find(|o| o.0 == f.order).map_or("Name", |o| o.2);
                egui::ComboBox::from_id_salt("library.sort")
                    .selected_text(egui::RichText::new(label).small())
                    .show_ui(ui, |ui| {
                        for (o, name, label) in ORDERS {
                            if o == Order::Length && !anims {
                                continue;
                            }
                            ui.selectable_value(&mut f.order, o, label)
                                .tag(format!("library.sort.{name}"), label);
                        }
                    })
                    .response
                    .on_hover_text("The order of the list")
                    .tag("library.sort", label);
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if anims {
                for (l, label, tip) in LENGTHS {
                    if chip(
                        ui,
                        f.length == Some(l),
                        label,
                        tip,
                        &format!("library.chip.{}", label.to_lowercase()),
                    ) {
                        f.length = if f.length == Some(l) { None } else { Some(l) };
                    }
                }
                if chip(
                    ui,
                    f.loops,
                    "Loops",
                    "Only animations that loop",
                    "library.chip.loops",
                ) {
                    f.loops = !f.loops;
                }
                if chip(
                    ui,
                    f.changed,
                    "Changed",
                    "Built-ins you edited, and the open animation if it has unsaved changes",
                    "library.chip.changed",
                ) {
                    f.changed = !f.changed;
                }
            } else {
                let label = f.part.clone().unwrap_or_else(|| "Any part".into());
                egui::ComboBox::from_id_salt("library.chip.part")
                    .selected_text(egui::RichText::new(&label).small())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.part, None, "Any part")
                            .tag("library.chip.part.any", "Any part");
                        for p in parts {
                            ui.selectable_value(&mut f.part, Some(p.clone()), p)
                                .tag(format!("library.chip.part.{p}"), p.as_str());
                        }
                    })
                    .response
                    .on_hover_text("Only poses of this body part")
                    .tag("library.chip.part", label);
            }
            if chip(
                ui,
                f.stars,
                "Starred",
                "Only the ones you starred",
                "library.chip.starred",
            ) {
                f.stars = !f.stars;
            }
            if f.chips_on(anims)
                && ui
                    .small_button("Clear")
                    .on_hover_text("Turn every filter off")
                    .tag("library.chip.clear", "Clear")
                    .clicked()
            {
                f.clear_chips();
            }
        });
    }
}
