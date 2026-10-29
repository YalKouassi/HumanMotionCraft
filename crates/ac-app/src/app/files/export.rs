//! File → Export… (Ctrl+E): rigged .glb files for engines. The open animation and/or any library
//! animations, with a joint profile (full, game, body) or a bone list, into one file or a file
//! each (`clip.export`, `clip.export_batch`). The folder is remembered; after an export, "Show in
//! folder" opens it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::super::App;
use super::super::ui_ids::Tag;
use super::{documents, reveal};

const PROFILES: &[(&str, &str, &str)] = &[
    ("full", "Full", "every joint of the rig (104)"),
    ("game", "Game", "the joints a game engine usually wants (68)"),
    ("body", "Body", "the body without fingers and face (36)"),
    ("bones", "Bone list", "exactly the joints you list"),
];

pub struct Export {
    /// The open animation as it is now (unsaved changes included).
    open: bool,
    /// Library animations, by id.
    picked: BTreeSet<String>,
    filter: String,
    profile: String,
    bones: String,
    /// One .glb holding every animation (else a file each).
    one_file: bool,
    folder: String,
    name: String,
    /// What the last export wrote (to show it) and said.
    done: Option<(PathBuf, String)>,
}

impl App {
    pub(in crate::app) fn open_export(&mut self) {
        let st = &self.files.state;
        let doc = &self.session.document;
        let name = doc
            .clip_id
            .as_deref()
            .and_then(|i| i.split_once('/'))
            .map_or(doc.clip.name.clone(), |x| x.1.to_string());
        self.files.export = Some(Export {
            open: true,
            picked: BTreeSet::new(),
            filter: String::new(),
            profile: st.export_profile.clone().unwrap_or_else(|| "full".into()),
            bones: st.export_bones.clone(),
            one_file: true,
            folder: st
                .export_dir
                .clone()
                .unwrap_or_else(|| documents().to_string_lossy().to_string()),
            name: format!("{name}.glb"),
            done: None,
        });
    }

    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        let Some(mut ex) = self.files.export.take() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        let anims: Vec<(String, String, bool)> = self.library["animations"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|it| {
                (
                    it["id"].as_str().unwrap_or_default().to_string(),
                    it["group"].as_str().unwrap_or_default().to_string(),
                    it["builtin"].as_bool().unwrap_or(false),
                )
            })
            .collect();
        let open_name = self
            .session
            .document
            .clip_id
            .clone()
            .unwrap_or_else(|| "untitled".into());
        egui::Window::new("Export")
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(super::dialog_pos(ctx))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Animations").strong());
                ui.checkbox(
                    &mut ex.open,
                    format!("The open animation as it is now ({open_name})"),
                )
                .tag("dialog.export.open", "The open animation as it is now");
                ui.add(
                    egui::TextEdit::singleline(&mut ex.filter)
                        .hint_text("Search the library")
                        .desired_width(f32::INFINITY),
                )
                .tag("dialog.export.search", "Search the library");
                let needle = ex.filter.trim().to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut group = String::new();
                        for (id, g, builtin) in &anims {
                            if !needle.is_empty() && !id.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if *g != group {
                                group = g.clone();
                                let tag = if *builtin { " (built-in)" } else { "" };
                                ui.label(egui::RichText::new(format!("{group}{tag}")).weak().small());
                            }
                            let mut on = ex.picked.contains(id);
                            let short = id.split_once('/').map_or(id.as_str(), |x| x.1);
                            if ui
                                .checkbox(&mut on, short)
                                .tag(format!("dialog.export.anim.{id}"), short)
                                .changed()
                            {
                                if on {
                                    ex.picked.insert(id.clone());
                                } else {
                                    ex.picked.remove(id);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} from the library", ex.picked.len())).weak());
                    if !ex.picked.is_empty()
                        && ui
                            .small_button("Clear")
                            .tag("dialog.export.clear", "Clear")
                            .clicked()
                    {
                        ex.picked.clear();
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("Joints").strong());
                ui.horizontal(|ui| {
                    for (key, label, tip) in PROFILES {
                        ui.selectable_value(&mut ex.profile, (*key).to_string(), *label)
                            .on_hover_text(*tip)
                            .tag(format!("dialog.export.profile.{key}"), *label);
                    }
                });
                if ex.profile == "bones" {
                    ui.add(
                        egui::TextEdit::singleline(&mut ex.bones)
                            .hint_text("root, spine01, upperarm01.L, …")
                            .desired_width(f32::INFINITY),
                    )
                    .tag("dialog.export.bones", "Bone list");
                }
                ui.separator();
                let count = ex.picked.len() + usize::from(ex.open);
                if count > 1 {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut ex.one_file, true, "One .glb, an animation each")
                            .on_hover_text("One file holding every animation (an engine sees them as clips)")
                            .tag("dialog.export.one_file", "One .glb, an animation each");
                        ui.radio_value(&mut ex.one_file, false, "A .glb per animation")
                            .tag("dialog.export.file_each", "A .glb per animation");
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Folder");
                    ui.add(egui::TextEdit::singleline(&mut ex.folder).desired_width(260.0))
                        .tag("dialog.export.folder", "Folder");
                    if ui
                        .button("Choose…")
                        .tag("dialog.export.choose", "Choose…")
                        .clicked()
                        && let Some(d) = rfd::FileDialog::new()
                            .set_title("Export to folder")
                            .set_directory(&ex.folder)
                            .pick_folder()
                    {
                        ex.folder = d.to_string_lossy().to_string();
                    }
                });
                if count <= 1 || ex.one_file {
                    ui.horizontal(|ui| {
                        ui.label("File");
                        ui.add(egui::TextEdit::singleline(&mut ex.name).desired_width(260.0))
                            .tag("dialog.export.file", "File");
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(count > 0, egui::Button::new("Export"))
                        .on_disabled_hover_text("Pick the open animation or some from the library")
                        .tag("dialog.export.export", "Export")
                        .clicked()
                    {
                        go = true;
                    }
                    if let Some((path, said)) = &ex.done {
                        ui.label(egui::RichText::new(said).weak());
                        if ui
                            .button("Show in folder")
                            .tag("dialog.export.show", "Show in folder")
                            .clicked()
                            && let Err(e) = reveal(path)
                        {
                            self.status = e;
                        }
                    }
                });
            });
        if go {
            self.run_export(&mut ex);
        }
        if open {
            self.files.export = Some(ex);
        }
    }

    /// The command (and its parameters) the dialog's choices make.
    fn run_export(&mut self, ex: &mut Export) {
        let folder = PathBuf::from(ex.folder.trim());
        let mut joints = json!({});
        if ex.profile == "bones" {
            let list: Vec<String> = ex
                .bones
                .split([',', ' ', ';'])
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_string)
                .collect();
            joints["bones"] = json!(list);
        } else {
            joints["profile"] = json!(ex.profile);
        }
        let doc = &self.session.document;
        let ids: Vec<String> = ex.picked.iter().cloned().collect();
        // the open animation goes in by id when the library has it as it is
        let open_id = doc.clip_id.clone().filter(|_| !doc.unsaved());
        let count = ids.len() + usize::from(ex.open);
        let (cmd, params, shown) = if count > 1 && !ex.one_file {
            if ex.open && open_id.is_none() {
                self.status =
                    "save the open animation first to export it with others as a file each (or untick it)"
                        .into();
                return;
            }
            let mut all = ids.clone();
            if let Some(id) = open_id.filter(|i| ex.open && !all.contains(i)) {
                all.insert(0, id);
            }
            let mut p = json!({ "ids": all, "folder": folder.to_string_lossy() });
            merge(&mut p, &joints);
            ("clip.export_batch", p, folder.clone())
        } else {
            let mut name = ex.name.trim().to_string();
            if !name.to_lowercase().ends_with(".glb") {
                name.push_str(".glb");
            }
            let path = folder.join(&name);
            let mut p = json!({ "path": path.to_string_lossy() });
            if !ids.is_empty() {
                let mut all = ids.clone();
                if ex.open {
                    match open_id {
                        Some(id) if !all.contains(&id) => all.insert(0, id),
                        Some(_) => {}
                        None => {
                            self.status = "save the open animation first to put it in one file with others (or untick it)".into();
                            return;
                        }
                    }
                }
                p["ids"] = json!(all);
            }
            merge(&mut p, &joints);
            ("clip.export", p, path)
        };
        let Some(r) = self.run(cmd, params) else { return };
        let said = match cmd {
            "clip.export_batch" => {
                let n = r["files"].as_array().map_or(0, Vec::len);
                let failed: Vec<String> = r["failed"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| {
                        format!(
                            "{}: {}",
                            f["id"].as_str().unwrap_or_default(),
                            f["error"].as_str().unwrap_or_default()
                        )
                    })
                    .collect();
                if failed.is_empty() {
                    format!("exported {n} files to {}", folder.display())
                } else {
                    format!(
                        "exported {n} files to {}; not exported: {}",
                        folder.display(),
                        failed.join("; ")
                    )
                }
            }
            _ => format!(
                "exported {} ({} animation(s), {} joints)",
                shown.display(),
                r["animations"],
                r["joints"]
            ),
        };
        self.status = said.clone();
        ex.done = Some((shown, said));
        self.files.state.export_dir = Some(folder.to_string_lossy().to_string());
        self.files.state.export_profile = Some(ex.profile.clone());
        self.files.state.export_bones = ex.bones.clone();
        self.save_state();
    }
}

fn merge(into: &mut Value, from: &Value) {
    if let (Some(a), Some(b)) = (into.as_object_mut(), from.as_object()) {
        for (k, v) in b {
            a.insert(k.clone(), v.clone());
        }
    }
}
