//! Which in-betweener fills (roadmap 155000): `ai.models` lists the models a session can load
//! (the installed ones, the bundled ones, and any folder it is given) with their cards, and
//! `ai.model` picks one by tag. A model that fails its checks is never loaded: the result says
//! why, and its base (or the default) is used instead (Rule 1). Fixtures, scripts and agent
//! sessions keep the bundled default until a command picks another.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::returns;
use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;

/// Most folders `ai.models` keeps for one session.
const MAX_FOLDERS: usize = 16;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("ai.models", "AI models", ai_models)
        .icon("ai")
        .inputs(&[ParamSpec::new(
            "folder",
            Kind::Text,
            "another folder of models (<folder>/<tag>/weights.json) to list after the installed and bundled ones; kept for ai.model and the AI panel for this session",
        )])
        .outputs(returns::AI_MODELS),
    CommandSpec::new("ai.model", "Choose AI model", ai_model)
        .icon("ai")
        .inputs(&[ParamSpec::new("tag", Kind::Text, "the model's tag (ai.models lists them)").required()])
        .outputs(returns::AI_MODEL),
];

type P = Map<String, Value>;
type Out = Result<Value, String>;

fn source(doc: &Document) -> Result<std::sync::Arc<dyn crate::ai::ModelSource>, String> {
    doc.models.clone().ok_or_else(|| {
        crate::error::hinted(
            "this session has no model source: only the loaded model is known",
            "the app, ac and MCP set one up (SessionBuilder::models); ai.info says what is loaded",
        )
    })
}

/// The model fills run with, as `ai.models` and `session.info` report it.
fn chosen(doc: &Document) -> Value {
    doc.ai
        .as_ref()
        .map_or(Value::Null, |m| json!({ "tag": m.info().tag, "hash": m.hash() }))
}

/// Every model found with its card, and which one fills. `folder` adds a folder to look in.
fn ai_models(doc: &mut Document, p: &P) -> Out {
    let src = source(doc)?;
    if let Some(f) = cmd::text(p, "folder") {
        let f = f.trim();
        if f.is_empty() {
            return Err("folder: an empty name".into());
        }
        let path = PathBuf::from(f);
        if !path.is_dir() {
            return Err(format!("folder {f:?}: not a folder"));
        }
        if !doc.model_folders.contains(&path) {
            if doc.model_folders.len() >= MAX_FOLDERS {
                return Err(format!("no more than {MAX_FOLDERS} model folders in a session"));
            }
            doc.model_folders.push(path);
        }
    }
    let cards = src.cards(&doc.skeleton, &doc.model_folders);
    Ok(json!({
        "models": cards,
        "chosen": chosen(doc),
        "folders": doc.model_folders.iter().map(|f| f.to_string_lossy()).collect::<Vec<_>>(),
    }))
}

/// Fill with the model `tag` from now on. A model its checks refuse is announced and its base
/// (else the default) is used instead; a tag that isn't there is an error.
fn ai_model(doc: &mut Document, p: &P) -> Out {
    let tag = cmd::text(p, "tag")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("tag: the model's tag (ai.models lists them)")?
        .to_string();
    let src = source(doc)?;
    let cards = src.cards(&doc.skeleton, &doc.model_folders);
    let asked = cards.iter().find(|c| c.tag == tag).ok_or_else(|| {
        let tags: Vec<&str> = cards.iter().map(|c| c.tag.as_str()).collect();
        let hint = crate::suggest::did_you_mean(&crate::suggest::closest(&tag, tags.iter().copied()));
        crate::error::hinted(
            format!("no model {tag:?}"),
            format!("{hint} the models here: {}", tags.join(", "))
                .trim()
                .to_string(),
        )
    })?;
    // a refused model: say why, and use its base, else the default, instead (Rule 1)
    let (used, refused) = match &asked.refused {
        None => (asked, None),
        Some(why) => {
            let fallback = asked
                .base
                .as_ref()
                .and_then(|b| cards.iter().find(|c| c.tag == b.tag && c.refused.is_none()))
                .or_else(|| cards.iter().find(|c| c.default && c.refused.is_none()))
                .ok_or_else(|| format!("model {tag}: {why}; and no base or default model loads instead"))?;
            (
                fallback,
                Some(format!("model {tag}: {why}; {} used instead", fallback.tag)),
            )
        }
    };
    let model = src.load(&doc.skeleton, &doc.model_folders, &used.tag)?;
    let mut notes: Vec<String> = refused.iter().cloned().collect();
    let style = doc.ai_settings.style.clone();
    if style != "any" && !model.info().styles.contains(&style) {
        notes.push(format!(
            "style {style:?}: {} doesn't know it; the style is \"any\" now",
            used.tag
        ));
        doc.ai_settings.style = "any".into();
    }
    // a preview made by the model before is no longer what a fill would give
    doc.ai_preview = None;
    doc.ai_precomputed = None;
    doc.ai = Some(model);
    Ok(json!({
        "tag": used.tag,
        "hash": doc.ai.as_ref().and_then(|m| m.hash()).unwrap_or_default(),
        "name": used.name,
        "path": used.path,
        "asked": tag,
        "refused": refused,
        "notes": notes,
    }))
}
