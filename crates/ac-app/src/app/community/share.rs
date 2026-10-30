//! "Share to community…" on one of your own animations: a small window for its title,
//! description, tags, licence, credit and the name shown as yours. Share draws its preview strip
//! (as the popover's) and runs `community.share`; the first time, the server asks for the captcha.

use serde_json::json;

use super::super::App;
use crate::app::library::preview::{CELL, FRAMES};
use crate::app::ui_ids::Tag;

pub struct Form {
    /// The library id being shared.
    pub id: String,
    pub title: String,
    pub description: String,
    /// Comma-separated.
    pub tags: String,
    pub licence: &'static str,
    pub credit: String,
    pub name: String,
    /// Sent, waiting for the server.
    pub sending: bool,
}

const LICENCES: &[(&str, &str, &str)] = &[
    (
        "CC0-1.0",
        "CC0",
        "Anyone may use it for anything, no credit needed",
    ),
    (
        "CC-BY-4.0",
        "CC BY 4.0",
        "Anyone may use it for anything, crediting you",
    ),
];

impl App {
    /// Open the form for library animation `id`.
    pub(in crate::app) fn community_share_open(&mut self, id: &str) {
        let name = id.rsplit('/').next().unwrap_or(id).to_string();
        let author = self
            .session
            .run("community.account", &json!({}))
            .ok()
            .and_then(|a| a["name"].as_str().map(str::to_string))
            .unwrap_or_default();
        self.community.share = Some(Form {
            id: id.to_string(),
            title: name,
            description: String::new(),
            tags: String::new(),
            licence: "CC0-1.0",
            credit: String::new(),
            name: author,
            sending: false,
        });
    }

    pub(in crate::app) fn community_share_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.community.share.take() else {
            return;
        };
        let mut open = true;
        let mut send = false;
        let mut cancel = false;
        egui::Window::new("Share to community")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&f.id).weak());
                egui::Grid::new("community.share.grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Title");
                        ui.add(egui::TextEdit::singleline(&mut f.title).char_limit(ac_community::MAX_TITLE))
                            .tag("community.share.title", "Title");
                        ui.end_row();
                        ui.label("Description");
                        ui.add(
                            egui::TextEdit::multiline(&mut f.description)
                                .desired_rows(3)
                                .char_limit(ac_community::MAX_DESCRIPTION),
                        )
                        .tag("community.share.description", "Description");
                        ui.end_row();
                        ui.label("Tags");
                        ui.add(egui::TextEdit::singleline(&mut f.tags).hint_text("walk, tired, loop"))
                            .on_hover_text("Up to 10, separated by commas")
                            .tag("community.share.tags", "Tags");
                        ui.end_row();
                        ui.label("Licence");
                        ui.vertical(|ui| {
                            for (id, label, what) in LICENCES {
                                ui.radio_value(&mut f.licence, *id, *label)
                                    .on_hover_text(*what)
                                    .tag(format!("community.share.licence.{id}"), *label);
                            }
                        });
                        ui.end_row();
                        ui.label("Credit");
                        ui.add(
                            egui::TextEdit::singleline(&mut f.credit)
                                .hint_text("who or what to credit (optional)")
                                .char_limit(ac_community::MAX_CREDIT),
                        )
                        .tag("community.share.credit", "Credit");
                        ui.end_row();
                        ui.label("Your name");
                        ui.add(
                            egui::TextEdit::singleline(&mut f.name)
                                .hint_text("shown as the author")
                                .char_limit(ac_community::MAX_NAME),
                        )
                        .tag("community.share.name", "Your name");
                        ui.end_row();
                    });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "It goes public once the checks pass. Share only animations you made or may share.",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    let ready = !f.sending && !f.title.trim().is_empty();
                    if ui
                        .add_enabled(
                            ready,
                            egui::Button::new(if f.sending { "Sharing…" } else { "Share" }),
                        )
                        .tag("community.share.send", "Share")
                        .clicked()
                    {
                        send = true;
                    }
                    if ui
                        .button("Cancel")
                        .tag("community.share.cancel", "Cancel")
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if !open || cancel {
            return;
        }
        if send {
            f.sending = true;
            let preview = self.community_share_preview(&f.id);
            let tags: Vec<&str> = f
                .tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect();
            let mut p = json!({
                "id": f.id, "title": f.title.trim(), "description": f.description.trim(), "tags": tags,
                "licence": f.licence, "credit": f.credit.trim(), "name": f.name.trim(),
            });
            match preview {
                Ok(path) => p["preview"] = json!(path),
                Err(e) => self.status = format!("Sharing without a preview: {e}"),
            }
            self.community.share = Some(f);
            self.run("community.share", p);
            return;
        }
        self.community.share = Some(f);
    }

    /// The popover's strip of `id`, written beside the library for `community.share` to send.
    fn community_share_preview(&mut self, id: &str) -> Result<String, String> {
        let png = self.thumb_png("animations", id, true, FRAMES, CELL, "preview")?;
        let root = self.session.document.library.clone().ok_or("no library")?;
        let dir = root.join(ac_community::record::CACHE);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("share-preview.png");
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }
}
