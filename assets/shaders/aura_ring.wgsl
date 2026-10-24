// A soft ring on the floor (see src/ring_view.rs): a thin line with feathered
// sides and a faint glow rising toward it from inside, worked out per world
// pixel so it stays as crisp as the sprites, cut to an arc when rings share it.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::view,
}

#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif
#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif

struct RingParams {
    color: vec4<f32>,
    radius: f32,
    size: f32,
    fill: f32,
    start: f32,
    sweep: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> ring: RingParams;

const TAU: f32 = 6.28318530718;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // The middle of this world pixel, from the centre, y up.
    let cell = floor(mesh.uv * ring.size) + 0.5 - ring.size * 0.5;
    let p = vec2(cell.x, -cell.y);
    let d = length(p);

    let line = max(0.0, 1.0 - abs(d - (ring.radius - 0.5)) / 0.85);
    let t = clamp((d - 0.5 * ring.radius) / (0.5 * ring.radius), 0.0, 1.0);
    let glow = select(0.0, ring.fill * t * t * (3.0 - 2.0 * t), d < ring.radius);
    var alpha = max(line, glow);

    if ring.sweep < TAU - 0.001 {
        // How far round from the arc's start, and so how near either end.
        let along = fract((atan2(p.y, p.x) - ring.start) / TAU) * TAU;
        if along > ring.sweep {
            discard;
        }
        let end = min(along, ring.sweep - along) * max(d, 1.0);
        alpha *= clamp(end / 1.5, 0.0, 1.0);
    }

    var color = vec4(ring.color.rgb, ring.color.a * alpha);
#ifdef TONEMAP_IN_SHADER
    color = tonemapping::tone_mapping(color, view.color_grading);
#endif
#ifdef SRGB_OUTPUT
    color = vec4(linear_to_srgb(color.rgb), color.a);
#endif
    return color;
}
