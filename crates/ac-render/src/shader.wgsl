// The skinned mannequin and the floor grid. Rig space: metres, Z up.

struct Frame {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // rgb + alpha of the body (alpha < 1: a ghost of another frame)
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> joints: array<mat4x4<f32>>;

struct VIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) joints: vec4<u32>,
    @location(3) weights: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_body(v: VIn) -> VOut {
    let skin = joints[v.joints.x] * v.weights.x + joints[v.joints.y] * v.weights.y
        + joints[v.joints.z] * v.weights.z + joints[v.joints.w] * v.weights.w;
    let world = skin * vec4<f32>(v.position, 1.0);
    var out: VOut;
    out.clip = frame.view_proj * world;
    out.world = world.xyz;
    out.normal = normalize((skin * vec4<f32>(v.normal, 0.0)).xyz);
    return out;
}

@fragment
fn fs_body(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let view = normalize(frame.eye.xyz - i.world);
    let key = normalize(vec3<f32>(0.4, -0.6, 0.8));
    let fill = normalize(vec3<f32>(-0.6, 0.3, 0.3));
    let diffuse = max(dot(n, key), 0.0) * 0.75 + max(dot(n, fill), 0.0) * 0.25;
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0) * 0.35;
    let c = frame.tint.rgb * (0.28 + diffuse) + vec3<f32>(rim);
    return vec4<f32>(c, frame.tint.a);
}

struct FOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) xy: vec2<f32>,
};

// A 40 m floor quad at z = 0 (two triangles from the vertex index).
@vertex
fn vs_floor(@builtin(vertex_index) i: u32) -> FOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let xy = corners[i] * 20.0;
    var out: FOut;
    out.clip = frame.view_proj * vec4<f32>(xy, 0.0, 1.0);
    out.xy = xy;
    return out;
}

fn grid_line(x: vec2<f32>, spacing: f32) -> f32 {
    let g = abs(fract(x / spacing - 0.5) - 0.5) / fwidth(x / spacing);
    return 1.0 - min(min(g.x, g.y), 1.0);
}

@fragment
fn fs_floor(i: FOut) -> @location(0) vec4<f32> {
    let fine = grid_line(i.xy, 0.25) * 0.25;
    let coarse = grid_line(i.xy, 1.0) * 0.55;
    let fade = clamp(1.0 - length(i.xy - frame.eye.xy) / 18.0, 0.0, 1.0);
    let line = max(fine, coarse) * fade;
    var c = vec3<f32>(0.20, 0.21, 0.23);
    c = mix(c, vec3<f32>(0.42, 0.44, 0.48), line);
    // the axes: X red, Y green
    let ax = 1.0 - min(abs(i.xy.y) / fwidth(i.xy.y), 1.0);
    let ay = 1.0 - min(abs(i.xy.x) / fwidth(i.xy.x), 1.0);
    c = mix(c, vec3<f32>(0.75, 0.30, 0.30), ax * fade);
    c = mix(c, vec3<f32>(0.35, 0.70, 0.35), ay * fade);
    return vec4<f32>(c, 1.0);
}
