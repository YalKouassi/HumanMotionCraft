# Decision: minimum GPU feature level

- **Status:** done (2026-10-31)
- **Requires:** none
- **Source:** old `ROADMAP.md` open decision 3

## Question (as asked)
The minimum GPU feature level for wgpu (storage buffers are needed, so there's no WebGL2 target).

## Decision
The owner's decision, 2026-10-31: what the code and `README.md` already do. Vulkan, Metal or DX12, with
wgpu's default limits (`DeviceDescriptor::default()`, `crates/ac-render/src/offscreen.rs:188`; no extra
features or limits are asked for), and no WebGL2 target. Windows' WARP software adapter stays supported
for headless rendering and visual tests.
