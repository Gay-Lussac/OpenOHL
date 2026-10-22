// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

// Positions relative to the scene origin (f32); colours sRGB per vertex,
// converted to linear here (the target is an sRGB texture). Lines and
// points are quads of a width in pixels, built from instances.

struct Uniforms {
    view_proj: mat4x4<f32>,
    viewport: vec2<f32>,
    _pad: vec2<f32>,
    // The clip box: x, y of its origin (relative to the scene origin), z
    // 1 when on.
    clip_origin: vec4<f32>,
    // Its direction ahead (x, y), half depth, half width.
    clip_box: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Where, relative to the scene origin (for the clip box).
    @location(1) world: vec3<f32>,
};

fn linear(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(pow(c.rgb, vec3<f32>(2.2)), c.a);
}

// Corners of a quad as two triangles: x along (0 start, 1 end), y across.
fn corner(i: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    return c[i];
}

@vertex
fn vs_surface(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.color = linear(color);
    out.world = position;
    return out;
}

@vertex
fn vs_line(
    @builtin(vertex_index) i: u32,
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width: f32,
) -> VertexOut {
    var out: VertexOut;
    out.color = linear(color);
    var p0 = u.view_proj * vec4<f32>(a, 1.0);
    var p1 = u.view_proj * vec4<f32>(b, 1.0);
    var w0 = a;
    var w1 = b;
    // Clip at the near plane (z = 0 in wgpu clip space).
    if (p0.z < 0.0 && p1.z < 0.0) {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.world = a;
        return out;
    }
    if (p0.z < 0.0) {
        let t = (0.0 - p0.z) / (p1.z - p0.z);
        p0 = mix(p0, p1, t);
        w0 = mix(a, b, t);
    }
    if (p1.z < 0.0) {
        let t = (0.0 - p1.z) / (p0.z - p1.z);
        p1 = mix(p1, p0, t);
        w1 = mix(b, a, t);
    }
    let half = 0.5 * u.viewport;
    let s0 = p0.xy / p0.w * half;
    let s1 = p1.xy / p1.w * half;
    var dir = s1 - s0;
    if (length(dir) < 1e-6) {
        dir = vec2<f32>(1.0, 0.0);
    }
    dir = normalize(dir);
    let n = vec2<f32>(-dir.y, dir.x);
    let c = corner(i);
    let p = select(p0, p1, c.x > 0.5);
    out.world = select(w0, w1, c.x > 0.5);
    let offset = n * c.y * 0.5 * width / half * p.w;
    out.clip = vec4<f32>(p.xy + offset, p.z, p.w);
    return out;
}

@vertex
fn vs_point(
    @builtin(vertex_index) i: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) size: f32,
) -> VertexOut {
    var out: VertexOut;
    out.color = linear(color);
    let p = u.view_proj * vec4<f32>(position, 1.0);
    out.world = position;
    let c = corner(i);
    let d = vec2<f32>(c.x * 2.0 - 1.0, c.y) * 0.5 * size / (0.5 * u.viewport) * p.w;
    out.clip = vec4<f32>(p.xy + d, p.z, p.w);
    return out;
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    if (u.clip_origin.z > 0.5) {
        let d = v.world.xy - u.clip_origin.xy;
        let ahead = u.clip_box.xy;
        let along = dot(d, ahead);
        let across = dot(d, vec2<f32>(ahead.y, -ahead.x));
        if (abs(along) > u.clip_box.z || abs(across) > u.clip_box.w) {
            discard;
        }
    }
    return v.color;
}
