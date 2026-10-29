//! The renderer: the skinned mannequin, the floor and ghosts, drawn offscreen with
//! wgpu (the UI shows the texture; screenshots read it back). No egui here.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod anim;
pub mod mesh;
pub mod offscreen;
pub mod renderer;
pub mod skin;

pub use glam;
pub use mesh::Mesh;
pub use renderer::{Body, Camera, Renderer};
pub use wgpu;
