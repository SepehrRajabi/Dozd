//! The soft rings drawn on the floor around a pirate: how far a class aura
//! reaches, or a perk's field. Each is a thin pixel line with a faint glow
//! rising toward it from inside, drawn by `shaders/aura_ring.wgsl`. Rings of
//! a pirate's that share a radius split the circle into equal arcs, one each,
//! and spin together so every one of them still shows.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

/// Where a pirate's rings sit, under their feet; every ring around one has to
/// share it so arcs of the same circle line up.
pub const RING_AT: Vec3 = Vec3::new(0.0, -5.0, -0.2);
/// Rings whose radii are this close share the circle.
const SHARE_WITHIN: f32 = 3.0;
/// Pixels left clear between neighbouring arcs.
const ARC_GAP: f32 = 4.0;
/// How fast shared rings turn, in radians a second.
const SPIN: f32 = 0.7;

pub struct RingViewPlugin;

impl Plugin for RingViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<RingMaterial>::default())
            .add_observer(dress_ring)
            .add_systems(PostUpdate, share_rings);
    }
}

/// A ring around its parent. Whoever spawns it sets its colour (alpha
/// included) and `Visibility`; the arc it gets is worked out here.
#[derive(Component, Clone, Copy)]
pub struct AuraRing {
    /// Pixels from the centre to the middle of the line.
    pub radius: f32,
    /// How bright the glow inside gets toward the line, 0..1.
    pub fill: f32,
    pub color: Color,
}

impl AuraRing {
    pub fn new(radius: f32, fill: f32) -> Self {
        Self { radius, fill, color: Color::NONE }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct RingMaterial {
    #[uniform(0)]
    params: RingParams,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq)]
struct RingParams {
    color: Vec4,
    radius: f32,
    /// The quad's width in pixels.
    size: f32,
    fill: f32,
    /// Where the arc starts, anticlockwise from +x, and how far it sweeps.
    start: f32,
    sweep: f32,
}

impl Material2d for RingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/aura_ring.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// A pixel wider than the line on each side, so its feathering fits.
fn quad_size(radius: f32) -> f32 {
    2.0 * radius.round() + 2.0
}

fn dress_ring(
    add: On<Add, AuraRing>,
    mut commands: Commands,
    rings: Query<&AuraRing>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RingMaterial>>,
) {
    let Ok(ring) = rings.get(add.entity) else { return };
    let size = quad_size(ring.radius);
    let params = RingParams { radius: ring.radius.round(), size, fill: ring.fill, sweep: std::f32::consts::TAU, ..default() };
    commands.entity(add.entity).insert((
        Mesh2d(meshes.add(Rectangle::new(size, size))),
        MeshMaterial2d(materials.add(RingMaterial { params })),
    ));
}

/// Gives each shown ring its arc: the whole circle when it's alone at its
/// radius, else an equal share of it, turning.
fn share_rings(
    time: Res<Time>,
    rings: Query<(Entity, &ChildOf, &AuraRing, &Visibility, &MeshMaterial2d<RingMaterial>)>,
    mut materials: ResMut<Assets<RingMaterial>>,
) {
    use std::f32::consts::TAU;
    let mut shown: Vec<(Entity, Entity, f32)> =
        rings.iter().filter(|r| *r.3 != Visibility::Hidden).map(|(e, parent, ring, ..)| (parent.parent(), e, ring.radius)).collect();
    // Same pirate and radius side by side, in a steady order so arcs don't swap.
    shown.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.total_cmp(&b.2)).then(a.1.cmp(&b.1)));
    let spin = time.elapsed_secs() * SPIN;
    let mut i = 0;
    while i < shown.len() {
        let (owner, _, radius) = shown[i];
        let n = shown[i..].iter().take_while(|s| s.0 == owner && s.2 - radius <= SHARE_WITHIN).count();
        for (k, &(_, entity, _)) in shown[i..i + n].iter().enumerate() {
            let Ok((.., ring, _, handle)) = rings.get(entity) else { continue };
            let (start, sweep) = if n == 1 {
                (0.0, TAU)
            } else {
                let share = TAU / n as f32;
                ((spin + k as f32 * share) % TAU, share - ARC_GAP / ring.radius)
            };
            let params = RingParams {
                color: ring.color.to_linear().to_vec4(),
                radius: ring.radius.round(),
                size: quad_size(ring.radius),
                fill: ring.fill,
                start,
                sweep,
            };
            // Only touch the material when something changed, to spare re-uploads.
            if materials.get(&handle.0).is_some_and(|m| m.params != params)
                && let Some(mut material) = materials.get_mut(&handle.0)
            {
                material.params = params;
            }
        }
        i += n;
    }
}
