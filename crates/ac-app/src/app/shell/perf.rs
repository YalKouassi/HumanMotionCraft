//! The performance HUD (View > Performance, `--ui overlay=perf`): frames per second, frame time,
//! the last command and how long it took, the last physics rebuild, and the graphics adapter,
//! with a warning when it is the software one (WARP, llvmpipe), which is slow.
//!
//! The window draws only when something changes, so "fps" is the rate of the frames drawn
//! recently, not a limit.

use std::sync::Arc;
use std::time::Instant;

use egui::RichText;

use super::super::App;

pub struct Perf {
    /// Smoothed time between frames (seconds).
    dt: f64,
    /// Smoothed time the app spent in a frame (seconds).
    work: f64,
    started: Option<Instant>,
    pub last_command: Option<(String, f64)>,
    /// The last command that rebuilt the physics pass, and how long it took (with the rebuild).
    pub physics: Option<(String, f64)>,
    pub adapter: String,
    pub software: bool,
}

/// Names of software adapters.
const SOFTWARE: &[&str] = &[
    "microsoft basic render",
    "warp",
    "llvmpipe",
    "swiftshader",
    "softpipe",
];

impl Perf {
    pub fn new(rs: Option<&eframe::egui_wgpu::RenderState>) -> Perf {
        let (adapter, software) = match rs {
            Some(rs) => {
                let info = rs.adapter.get_info();
                let name = info.name.to_lowercase();
                let software = info.device_type == eframe::wgpu::DeviceType::Cpu
                    || SOFTWARE.iter().any(|s| name.contains(s));
                (format!("{} ({:?})", info.name, info.backend), software)
            }
            None => ("unknown".into(), false),
        };
        Perf {
            dt: 0.0,
            work: 0.0,
            started: None,
            last_command: None,
            physics: None,
            adapter,
            software,
        }
    }

    pub fn adapter_warning(&self) -> Option<String> {
        self.software.then(|| {
            format!(
                "drawing on the software renderer ({}): the viewport will be slow. Update the graphics driver, or check the app runs on the graphics card",
                self.adapter
            )
        })
    }

    pub fn frame_start(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.unstable_dt) as f64;
        if dt.is_finite() && dt > 0.0 {
            self.dt = if self.dt == 0.0 {
                dt
            } else {
                self.dt * 0.9 + dt * 0.1
            };
        }
        self.started = Some(Instant::now());
    }

    pub fn frame_end(&mut self) {
        if let Some(t) = self.started.take() {
            let w = t.elapsed().as_secs_f64();
            self.work = if self.work == 0.0 {
                w
            } else {
                self.work * 0.9 + w * 0.1
            };
        }
    }
}

/// What the physics cache was before a command, to tell whether the command rebuilt it.
pub struct Before {
    at: Instant,
    physics: usize,
}

fn physics_key(session: &ac_core::Session) -> usize {
    session
        .document
        .physics_cache
        .as_ref()
        .map_or(0, |c| Arc::as_ptr(c) as usize)
}

impl Perf {
    pub fn before(&self, session: &ac_core::Session) -> Before {
        Before {
            at: Instant::now(),
            physics: physics_key(session),
        }
    }

    pub fn after(&mut self, b: Before, id: &str, session: &ac_core::Session) {
        let ms = b.at.elapsed().as_secs_f64() * 1000.0;
        self.last_command = Some((id.to_string(), ms));
        let now = physics_key(session);
        if now != 0 && now != b.physics {
            self.physics = Some((id.to_string(), ms));
        }
    }
}

impl App {
    pub(in crate::app) fn perf_hud(&mut self, ctx: &egui::Context) {
        if !self.shell.prefs.get().view.perf {
            return;
        }
        let p = &self.shell.perf;
        let warning = self.palette().warning;
        let at = self.shell.viewport_rect.map_or(egui::pos2(300.0, 60.0), |r| {
            egui::pos2(r.right() - 262.0, r.top() + 36.0)
        });
        egui::Area::new(egui::Id::unique("perf-hud"))
            .fixed_pos(at)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(236.0);
                    let fps = if p.dt > 0.0 { 1.0 / p.dt } else { 0.0 };
                    ui.label(
                        RichText::new(format!("{fps:.0} fps · {:.1} ms a frame", p.dt * 1000.0)).monospace(),
                    );
                    ui.label(
                        RichText::new(format!("app work {:.1} ms a frame", p.work * 1000.0)).monospace(),
                    );
                    let cmd = p
                        .last_command
                        .as_ref()
                        .map_or("last command: none".into(), |(id, ms)| {
                            format!("last {id}: {ms:.1} ms")
                        });
                    ui.label(RichText::new(cmd).monospace());
                    let phys = p
                        .physics
                        .as_ref()
                        .map_or("physics rebuild: none yet".into(), |(id, ms)| {
                            format!("physics rebuild: {ms:.1} ms ({id})")
                        });
                    ui.label(RichText::new(phys).monospace());
                    if p.software {
                        ui.label(
                            RichText::new(format!("software renderer: {}", p.adapter))
                                .color(warning)
                                .small(),
                        );
                    } else {
                        ui.label(RichText::new(&p.adapter).weak().small());
                    }
                });
            });
    }
}
