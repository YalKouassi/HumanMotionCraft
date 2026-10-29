//! Skinning matrices from a posed rig: for bone i with world rotation R, head P and rest head H,
//! `T(P) R T(-H)` maps the rest mesh onto the pose (at rest R = I and P = H: the identity).

use glam::{Mat3, Mat4, Vec3};

/// One matrix per bone, from world rotations (row-major 3x3) and joint positions (f64, rig space).
pub fn skin_matrices(rotations: &[[[f64; 3]; 3]], positions: &[[f64; 3]], rest: &[[f64; 3]]) -> Vec<Mat4> {
    rotations
        .iter()
        .zip(positions)
        .zip(rest)
        .map(|((r, p), h)| {
            // glam is column-major: column c is (r[0][c], r[1][c], r[2][c])
            let m = Mat3::from_cols(
                Vec3::new(r[0][0] as f32, r[1][0] as f32, r[2][0] as f32),
                Vec3::new(r[0][1] as f32, r[1][1] as f32, r[2][1] as f32),
                Vec3::new(r[0][2] as f32, r[1][2] as f32, r[2][2] as f32),
            );
            let p = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
            let h = Vec3::new(h[0] as f32, h[1] as f32, h[2] as f32);
            Mat4::from_translation(p) * Mat4::from_mat3(m) * Mat4::from_translation(-h)
        })
        .collect()
}
