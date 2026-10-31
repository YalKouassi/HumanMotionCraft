//! Binary FBX import (roadmap 152000): our own reader of the binary container
//! ([`binary`]), the scene's skeleton and animation sampled to world joint positions in the rig's
//! axes ([`scene`]), and the retarget of those onto the rig as a clip ([`import`]), plus
//! `clip.import_batch` ([`commands`]). `clip.import` (in `crate::commands`) reads `.fbx` through
//! [`import::import_file`].
//!
//! Written from the public layout of the format and our own retarget; no other product's reader
//! was read or copied. Test files are written by `training/ac_train/fbxwrite.py`.

pub mod binary;
pub mod commands;
pub mod import;
pub mod scene;

pub use import::{Imported, import_bytes, import_file};
