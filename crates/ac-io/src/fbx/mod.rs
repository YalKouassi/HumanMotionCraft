//! FBX import (roadmap 152000): our own readers of the binary container ([`binary`]) and the text
//! one ([`text`]), FBX 6's scene put in the shape of 7's ([`legacy`], roadmap 164000), the scene's skeleton and animation sampled to world joint positions in the rig's
//! axes ([`scene`]), and the retarget of those onto the rig as a clip ([`import`]), plus
//! `clip.import_batch` ([`commands`]). `clip.import` (in `crate::commands`) reads `.fbx` through
//! [`import::import_file`]. A Unity `.meta` beside the file ([`unity_meta`], roadmap 164000) says
//! which bone is which and cuts the take into named clips.
//!
//! Written from the public layout of the format and our own retarget; no other product's reader
//! was read or copied. Test files are written by `training/ac_train/fbxwrite.py`.

pub mod binary;
pub mod commands;
pub mod import;
pub mod legacy;
pub mod scene;
pub mod text;
pub mod unity_meta;

pub use import::{Imported, Source, import_bytes, import_file, open_bytes, open_file};

/// Read FBX bytes, binary or text, of any version read, as a 7.x scene for [`scene::motion`].
pub fn read(bytes: &[u8]) -> Result<binary::Document, String> {
    let doc = if text::is_text(bytes) {
        text::parse(bytes)?
    } else {
        binary::parse(bytes)?
    };
    if doc.version < legacy::FIRST_MODERN {
        legacy::modern(&doc)
    } else {
        Ok(doc)
    }
}
