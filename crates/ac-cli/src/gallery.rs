//! `ac fixtures --shots <dir>`: one page (`index.html`) with every fixture's description, outcome
//! and pictures. With `--golden-check` each view also shows its golden, the picture made now and
//! their difference side by side (red: pixels that moved).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{GOLDEN_MAX_DIFFER, GoldenView, Outcome};

/// One fixture on the page.
pub struct Item {
    pub fixture: Value,
    pub outcome: Outcome,
    /// The views at full size (`--shots`).
    pub views: Vec<PathBuf>,
    /// Each view against its golden (`--golden-check`).
    pub goldens: Vec<GoldenView>,
}

fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The file name only: the pictures sit next to the page.
fn src(p: &Path) -> String {
    html(
        &p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

fn figure(caption: &str, pic: Option<&Path>) -> String {
    match pic {
        Some(p) => format!(
            "<figure><img src=\"{0}\" alt=\"{0}\" title=\"{0}\"><figcaption>{1}</figcaption></figure>",
            src(p),
            html(caption)
        ),
        None => format!(
            "<figure><div class=none>none</div><figcaption>{}</figcaption></figure>",
            html(caption)
        ),
    }
}

pub fn page(items: &[Item]) -> String {
    let failed = items.iter().filter(|i| !i.outcome.failures.is_empty()).count();
    let mut s = format!(
        "<!doctype html><meta charset=utf-8><title>{} fixtures</title>\
         <style>body{{font:14px system-ui;background:#1b1d22;color:#ddd;margin:24px}}\
         section{{border:1px solid #333;border-radius:8px;padding:12px 16px;margin:12px 0}}\
         h2{{margin:0 0 4px;font-size:16px}} h3{{font-size:14px;margin:10px 0 2px}} .ok{{color:#7c7}} .fail{{color:#e77}} p{{max-width:70em;line-height:1.45}}\
         img{{max-width:420px;border-radius:4px;margin:4px 8px 0 0;background:#000}} code{{color:#9bd}}\
         .row{{display:flex;flex-wrap:wrap}} figure{{margin:0 8px 0 0}} figcaption{{color:#999;font-size:12px}}\
         .row img{{width:320px}} .none{{width:320px;height:240px;display:flex;align-items:center;justify-content:center;background:#000;color:#666}}</style>\
         <h1>Fixtures: {} run, {} failed</h1>",
        ac_core::product::NAME,
        items.len(),
        failed
    );
    for it in items {
        let out = &it.outcome;
        let ok = out.failures.is_empty();
        s += &format!(
            "<section id=\"{0}\"><h2><span class={1}>{2}</span> {0} <code>{3}</code></h2><p>{4}</p>",
            html(&out.name),
            if ok { "ok" } else { "fail" },
            if ok { "ok" } else { "FAIL" },
            html(&out.feature),
            html(it.fixture["description"].as_str().unwrap_or(""))
        );
        if !ok {
            s += "<ul>";
            for f in &out.failures {
                s += &format!("<li class=fail>{}</li>", html(f));
            }
            s += "</ul>";
        }
        for v in &it.views {
            s += &format!("<img src=\"{0}\" alt=\"{0}\" title=\"{0}\">", src(v));
        }
        for g in &it.goldens {
            let moved = g.moved > GOLDEN_MAX_DIFFER;
            s += &format!(
                "<h3>{} <span class={}>{:.2}% of pixels moved</span></h3><div class=row>{}{}{}</div>",
                html(&g.view),
                if moved { "fail" } else { "ok" },
                g.moved * 100.0,
                figure("golden (tests/visual)", g.golden.as_deref()),
                figure("actual", Some(&g.actual)),
                figure("diff", g.diff.as_deref()),
            );
        }
        s += "</section>";
    }
    s
}
