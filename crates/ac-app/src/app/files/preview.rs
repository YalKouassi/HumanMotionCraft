//! File → Export preview… (Ctrl+Shift+E): the open animation as it plays, physics included, into
//! an MP4 (needs ffmpeg) or an animated GIF (`clip.video`), or a contact sheet PNG (`clip.sheet`),
//! the same files `ac video` and MCP make.
//!
//! Rendering runs on the window's thread: the dialog says "Rendering…" for a frame, then renders
//! (a few seconds for a long animation).

use std::path::PathBuf;

use serde_json::json;

use super::super::App;
use super::super::ui_ids::Tag;
use super::{documents, reveal};

const VIEWS: &[&str] = &["side", "front", "three-quarter", "back", "left", "right", "top"];
/// (label, mp4 size, gif size)
/// A picture size, (width, height).
type Size = (u32, u32);
const SIZES: &[(&str, Size, Size)] = &[
    ("Small", (640, 360), (320, 180)),
    ("Medium", (960, 540), (480, 270)),
    ("Large", (1280, 720), (640, 360)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Video,
    Sheet,
}

pub struct Preview {
    gif: bool,
    view: String,
    size: usize,
    fixed: bool,
    floor: bool,
    /// One colour behind the body instead of the viewport's.
    flat: bool,
    colour: [u8; 3],
    /// Frames per second (starts at the animation's own).
    fps: f64,
    sheet_count: u32,
    folder: String,
    name: String,
    /// Asked for, rendered next frame (after "Rendering…" is on screen).
    pending: Option<Job>,
    done: Option<(PathBuf, String)>,
}

impl App {
    pub(in crate::app) fn open_preview(&mut self) {
        let st = &self.files.state;
        let doc = &self.session.document;
        let name = doc
            .clip_id
            .as_deref()
            .and_then(|i| i.split_once('/'))
            .map_or(doc.clip.name.clone(), |x| x.1.to_string());
        self.files.preview = Some(Preview {
            gif: st.preview_format.as_deref() == Some("gif"),
            view: "side".into(),
            size: 1,
            fixed: false,
            floor: true,
            flat: false,
            colour: [0, 255, 0],
            fps: if doc.clip.fps > 0.0 { doc.clip.fps } else { 30.0 },
            sheet_count: 6,
            folder: st
                .preview_dir
                .clone()
                .unwrap_or_else(|| documents().to_string_lossy().to_string()),
            name,
            pending: None,
            done: None,
        });
    }

    pub(super) fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(mut pv) = self.files.preview.take() else {
            return;
        };
        // asked for last frame, "Rendering…" is on screen now
        if let Some(job) = pv.pending.take() {
            self.render_preview(&mut pv, job);
        }
        let mut open = true;
        let (frames, fps) = {
            let doc = &self.session.document;
            (doc.clip.length_with(&doc.links).max(1), doc.clip.fps)
        };
        egui::Window::new("Export preview")
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(super::dialog_pos(ctx))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "The open animation as it plays ({frames} frames at {fps} fps, physics included)"
                    ))
                    .weak(),
                );
                ui.horizontal(|ui| {
                    ui.label("Format");
                    ui.radio_value(&mut pv.gif, false, "MP4")
                        .on_hover_text("H.264 video; needs ffmpeg installed (or AC_FFMPEG set)")
                        .tag("dialog.preview.format.mp4", "MP4");
                    ui.radio_value(&mut pv.gif, true, "Animated GIF")
                        .tag("dialog.preview.format.gif", "Animated GIF");
                });
                ui.horizontal(|ui| {
                    ui.label("Camera");
                    egui::ComboBox::from_id_salt("preview-view")
                        .selected_text(pv.view.clone())
                        .show_ui(ui, |ui| {
                            for v in VIEWS {
                                ui.selectable_value(&mut pv.view, (*v).to_string(), *v)
                                    .tag(format!("dialog.preview.camera.{v}"), *v);
                            }
                        })
                        .response
                        .tag("dialog.preview.camera", "Camera");
                    ui.checkbox(&mut pv.fixed, "Fixed")
                        .on_hover_text(
                            "One camera that sees the whole shot, so travel shows; else it follows the body",
                        )
                        .tag("dialog.preview.fixed", "Fixed");
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut pv.floor, "Floor")
                        .on_hover_text("The floor and its grid")
                        .tag("dialog.preview.floor", "Floor");
                    ui.checkbox(&mut pv.flat, "Flat background")
                        .on_hover_text("One colour behind the body, to key it out later")
                        .tag("dialog.preview.flat", "Flat background");
                    if pv.flat {
                        ui.color_edit_button_srgb(&mut pv.colour)
                            .tag("dialog.preview.colour", "Background colour");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Size");
                    for (i, (label, mp4, gif)) in SIZES.iter().enumerate() {
                        let (w, h) = if pv.gif { gif } else { mp4 };
                        ui.selectable_value(&mut pv.size, i, *label)
                            .on_hover_text(format!("{w} x {h}"))
                            .tag(format!("dialog.preview.size.{}", label.to_lowercase()), *label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Frames per second");
                    ui.add(egui::DragValue::new(&mut pv.fps).range(1.0..=120.0).speed(1.0))
                        .on_hover_text(format!("The animation's own is {fps}"))
                        .tag("dialog.preview.fps", "Frames per second");
                });
                ui.horizontal(|ui| {
                    ui.label("Folder");
                    ui.add(egui::TextEdit::singleline(&mut pv.folder).desired_width(220.0))
                        .tag("dialog.preview.folder", "Folder");
                    if ui
                        .button("Choose…")
                        .tag("dialog.preview.choose", "Choose…")
                        .clicked()
                        && let Some(d) = rfd::FileDialog::new()
                            .set_title("Export preview to folder")
                            .set_directory(&pv.folder)
                            .pick_folder()
                    {
                        pv.folder = d.to_string_lossy().to_string();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut pv.name).desired_width(220.0))
                        .tag("dialog.preview.name", "Name");
                });
                ui.add_space(6.0);
                if pv.pending.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Rendering…");
                    });
                    return;
                }
                ui.horizontal(|ui| {
                    let what = if pv.gif { "Export GIF" } else { "Export MP4" };
                    if ui.button(what).tag("dialog.preview.export", what).clicked() {
                        pv.pending = Some(Job::Video);
                    }
                    ui.separator();
                    ui.add(
                        egui::DragValue::new(&mut pv.sheet_count)
                            .range(1..=64)
                            .suffix(" frames"),
                    )
                    .tag("dialog.preview.sheet_count", "Sheet frames");
                    if ui
                        .button("Export contact sheet")
                        .on_hover_text(
                            "One PNG of frames spread over the animation, side by side from one camera",
                        )
                        .tag("dialog.preview.sheet", "Export contact sheet")
                        .clicked()
                    {
                        pv.pending = Some(Job::Sheet);
                    }
                });
                if let Some((path, said)) = &pv.done {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(said).weak());
                        if ui
                            .button("Show in folder")
                            .tag("dialog.preview.show", "Show in folder")
                            .clicked()
                            && let Err(e) = reveal(path)
                        {
                            self.status = e;
                        }
                    });
                }
            });
        if pv.pending.is_some() {
            ctx.request_repaint();
        }
        if open {
            self.files.preview = Some(pv);
        }
    }

    fn render_preview(&mut self, pv: &mut Preview, job: Job) {
        let folder = PathBuf::from(pv.folder.trim());
        let stem = pv.name.trim();
        let stem = if stem.is_empty() { "preview" } else { stem };
        let (_, mp4, gif) = SIZES
            .get(pv.size)
            .copied()
            .unwrap_or(("Medium", (960, 540), (480, 270)));
        let (cmd, path, mut params) = match job {
            Job::Video => {
                let ext = if pv.gif { "gif" } else { "mp4" };
                let (w, h) = if pv.gif { gif } else { mp4 };
                let path = folder.join(format!("{stem}.{ext}"));
                let p = json!({ "path": path.to_string_lossy(), "view": pv.view, "width": w, "height": h, "fixed": pv.fixed });
                ("clip.video", path, p)
            }
            Job::Sheet => {
                let path = folder.join(format!("{stem} sheet.png"));
                let p = json!({ "path": path.to_string_lossy(), "view": pv.view, "count": pv.sheet_count, "cols": pv.sheet_count.min(8) });
                ("clip.sheet", path, p)
            }
        };
        if job == Job::Video {
            params["fps"] = json!(pv.fps);
        }
        params["floor"] = json!(pv.floor);
        if pv.flat {
            let [r, g, b] = pv.colour;
            params["background"] = json!(format!("#{r:02x}{g:02x}{b:02x}"));
        }
        let Some(r) = self.run(cmd, params) else {
            pv.done = None;
            return;
        };
        let mut said = match job {
            Job::Video => format!("wrote {} ({} frames)", path.display(), r["frames"]),
            Job::Sheet => format!(
                "wrote {} ({} frames)",
                path.display(),
                r["frames"].as_array().map_or(0, Vec::len)
            ),
        };
        if let Some(n) = r["note"].as_str() {
            said.push_str(&format!("; {n}"));
        }
        self.status = said.clone();
        pv.done = Some((path, said));
        self.files.state.preview_dir = Some(folder.to_string_lossy().to_string());
        self.files.state.preview_format = Some(if pv.gif { "gif" } else { "mp4" }.into());
        self.save_state();
    }
}
