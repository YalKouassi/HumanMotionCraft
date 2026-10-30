//! What the community commands return: their declared outputs, kept apart so the command table
//! stays short.

use ac_core::command::{Kind as K, ParamSpec};

const CLIP_FIELDS: &str = "{id, title, description, author, tags, licence, credit, fps, frames, \
                           seconds, rating, ratings, downloads, published, preview, installed}";

pub const BROWSE: &[ParamSpec] = &[
    ParamSpec::new("clips", K::Text, CLIP_FIELDS).json().required(),
    ParamSpec::new("count", K::Integer, "how many on this page").required(),
];

pub const FETCH: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the clip").required(),
    ParamSpec::new(
        "path",
        K::Text,
        "the unpacked clip file, to play (not in the library)",
    )
    .required(),
    ParamSpec::new("clip", K::Text, CLIP_FIELDS).json().required(),
];

pub const INSTALL: &[ParamSpec] = &[
    ParamSpec::new(
        "id",
        K::Text,
        "the library id it was installed as (Community/<title>)",
    )
    .required(),
    ParamSpec::new("clip", K::Text, "the community clip").required(),
    ParamSpec::new("already", K::Bool, "it was installed before: nothing changed").required(),
    ParamSpec::new("notes", K::TextList, "what didn't go to plan (empty: nothing)").required(),
];

pub const RATE: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the clip").required(),
    ParamSpec::new("rating", K::Number, "its average now, to two decimals").required(),
    ParamSpec::new("ratings", K::Integer, "how many have rated it").required(),
];

pub const REPORT: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the clip").required(),
    ParamSpec::new("reported", K::Bool, "true").required(),
];

pub const VERIFY: &[ParamSpec] = &[ParamSpec::new("verified", K::Bool, "true").required()];

pub const SHARE: &[ParamSpec] = &[
    ParamSpec::new("clip", K::Text, "its community id").required(),
    ParamSpec::new("status", K::Text, "checking: it goes live once the checks pass").required(),
    ParamSpec::new("title", K::Text, "the title it was shared with").required(),
    ParamSpec::new("tags", K::TextList, "the tags, cleaned").required(),
];

pub const MINE: &[ParamSpec] = &[ParamSpec::new(
    "clips",
    K::Text,
    "[{id, title, status (checking, published, failed, hidden, removed), reason, created}], newest first",
)
.json()
.required()];

pub const REMOVE: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the clip").required(),
    ParamSpec::new("status", K::Text, "removed").required(),
];

pub const ACCOUNT: &[ParamSpec] = &[
    ParamSpec::new("signed_in", K::Bool, "an account exists on this computer").required(),
    ParamSpec::new("verified", K::Bool, "it passed the captcha: may share and report").required(),
    ParamSpec::new("name", K::Text, "the name shown on its clips").required(),
    ParamSpec::new("captcha", K::Text, "the captcha page").required(),
];
