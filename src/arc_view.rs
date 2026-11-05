//! How a Conductor's arc field looks: a hard ring on the true edge of the
//! damage, twitching rather than breathing, with short bolts leaping off its
//! prongs and crawling the rim. Up when the field is, gone when it isn't, on
//! every client alike. The jolts that land are the host's `FxKind::Zap`s.

use bevy::prelude::*;

use crate::Level;
use crate::Rng;
use crate::class::HACK_COLOR;
use crate::combat::Fade;
use crate::enemies::{ARC_COLOR, ARC_RADIUS, EMPOWERED_COLOR, EnemyKind, EnemyLook};
use crate::ring_view::AuraRing;

/// Seconds the field holds a brightness before it twitches to the next.
const TWITCH: f32 = 0.05;
/// The ring's alpha and inner glow at the field's dimmest. A surge adds these
/// on top, so the worst flicker still shows exactly where the danger ends.
const LINE: f32 = 0.7;
const SURGE_LINE: f32 = 0.3;
const FILL: f32 = 0.16;
const SURGE_FILL: f32 = 0.1;
/// How far a surge whitens the field.
const SURGE_WHITE: f32 = 0.35;
/// Seconds a stray arc lasts, and the shortest and longest wait for the next.
const ARC_LIFE: f32 = 0.12;
const ARC_GAP: (f32, f32) = (0.06, 0.16);
/// How far toward white the arcs burn, over the field they're in.
const ARC_WHITE: f32 = 0.45;
/// Over the Conductor but under its health bar and the pirates.
const ARC_Z: f32 = 8.5;
/// The tip of its right prong, from its centre, on the sprite.
const PRONG: Vec2 = Vec2::new(4.5, 7.5);
/// Pixels between a bolt's kinks, and how far off line each kinks.
const BOLT_STEP: f32 = 10.0;
const BOLT_KINK: f32 = 3.5;
/// A rim arc's links, the radians each spans, and how far in from the ring
/// they sag. Nothing is drawn past the ring: it stays the honest edge.
const RIM_LINKS: usize = 4;
const RIM_LINK: f32 = 0.2;
const RIM_SAG: f32 = 4.0;

pub struct ArcViewPlugin;

impl Plugin for ArcViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_conductor).add_systems(Update, draw_arc_fields);
    }
}

/// A Conductor's ring, and when it next twitches and throws an arc.
#[derive(Component, Default)]
struct ArcField {
    twitch: f32,
    arc: f32,
    /// This moment's surge, 0..1.
    surge: f32,
}

/// Gives a Conductor its ring: centred on it, not at `RING_AT` under its
/// feet, because the line has to be where the damage stops.
fn dress_conductor(add: On<Add, EnemyLook>, mut commands: Commands, looks: Query<&EnemyLook>) {
    let Ok(look) = looks.get(add.entity) else { return };
    if look.kind != EnemyKind::Conductor {
        return;
    }
    commands.entity(add.entity).with_child((
        ArcField::default(),
        AuraRing::new(ARC_RADIUS, FILL),
        Transform::from_xyz(0.0, 0.0, -0.2),
        Visibility::Hidden,
    ));
}

/// Shows the ring while `arcing()`, twitches it, and throws arcs. The `Rng`
/// is this system's own, not the host's: what's drawn must never steer the
/// simulation.
fn draw_arc_fields(
    mut commands: Commands,
    time: Res<Time>,
    mut rng: Local<Rng>,
    conductors: Query<(&EnemyLook, &Transform)>,
    mut fields: Query<(&ChildOf, &mut ArcField, &mut AuraRing, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    for (parent, mut field, mut ring, mut visibility) in &mut fields {
        let Ok((look, transform)) = conductors.get(parent.parent()) else { continue };
        let live = look.arcing();
        visibility.set_if_neq(if live { Visibility::Inherited } else { Visibility::Hidden });
        if !live {
            continue;
        }
        let color = if look.hacked {
            HACK_COLOR
        } else if look.empowered {
            EMPOWERED_COLOR
        } else {
            ARC_COLOR
        };

        // Stepped, not smoothed, and mostly low with sudden spikes: nervous.
        field.twitch -= dt;
        if field.twitch <= 0.0 {
            field.twitch = TWITCH;
            field.surge = rng.unit().powi(3);
        }
        ring.color = color.mix(&Color::WHITE, SURGE_WHITE * field.surge).with_alpha(LINE + SURGE_LINE * field.surge);
        ring.fill = FILL + SURGE_FILL * field.surge;

        field.arc -= dt;
        if field.arc <= 0.0 {
            field.arc = ARC_GAP.0 + rng.unit() * (ARC_GAP.1 - ARC_GAP.0);
            let centre = transform.translation.truncate();
            let points = if rng.unit() < 0.5 { prong_bolt(&mut rng) } else { rim_arc(&mut rng) };
            let tint = color.mix(&Color::WHITE, ARC_WHITE);
            for pair in points.windows(2) {
                let (a, b) = (centre + pair[0], centre + pair[1]);
                // In the world, like a `FxKind::Zap`: gone in a blink either
                // way, and `Level` clears any caught by a teardown.
                commands.spawn((
                    Level,
                    Fade { age: 0.0, life: ARC_LIFE },
                    Sprite::from_color(tint, Vec2::new(a.distance(b) + 1.0, 1.0)),
                    Transform::from_translation(((a + b) / 2.0).extend(ARC_Z))
                        .with_rotation(Quat::from_rotation_z((b - a).to_angle())),
                ));
            }
        }
    }
}

/// A kinked bolt off one of its prongs to the ring, from its centre.
fn prong_bolt(rng: &mut Rng) -> Vec<Vec2> {
    let from = if rng.unit() < 0.5 { PRONG } else { PRONG * Vec2::new(-1.0, 1.0) };
    let to = Vec2::from_angle(rng.unit() * std::f32::consts::TAU) * (ARC_RADIUS - 1.0);
    let links = (from.distance(to) / BOLT_STEP).ceil().max(2.0) as usize;
    let normal = (to - from).normalize_or(Vec2::X).perp();
    let mut points: Vec<Vec2> = (0..=links).map(|i| from.lerp(to, i as f32 / links as f32)).collect();
    for p in &mut points[1..links] {
        *p += normal * rng.signed() * BOLT_KINK;
    }
    points
}

/// A jagged run along the inside of the ring, from its centre.
fn rim_arc(rng: &mut Rng) -> Vec<Vec2> {
    let start = rng.unit() * std::f32::consts::TAU;
    (0..=RIM_LINKS)
        .map(|i| Vec2::from_angle(start + i as f32 * RIM_LINK) * (ARC_RADIUS - 1.0 - rng.unit() * RIM_SAG))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::enemies::AiState;

    fn look(state: AiState, stunned: bool, hacked: bool) -> EnemyLook {
        EnemyLook {
            kind: EnemyKind::Conductor,
            state,
            stunned,
            hacked,
            empowered: false,
            frenzied: false,
            jammed: false,
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>().add_plugins(ArcViewPlugin);
        app
    }

    fn ring(app: &mut App) -> (AuraRing, Visibility) {
        let mut rings = app.world_mut().query_filtered::<(&AuraRing, &Visibility), With<ArcField>>();
        let (ring, visibility) = rings.single(app.world()).unwrap();
        (*ring, *visibility)
    }

    #[test]
    fn only_conductors_get_a_ring() {
        let mut app = app();
        app.world_mut().spawn((EnemyLook { kind: EnemyKind::Warden, ..look(AiState::Hunt, false, false) }, Transform::default()));
        app.update();
        assert_eq!(app.world_mut().query::<&ArcField>().iter(app.world()).count(), 0);
    }

    #[test]
    fn ring_is_up_only_while_the_field_is() {
        let mut app = app();
        let conductor = app.world_mut().spawn((look(AiState::Idle, false, false), Transform::default())).id();
        app.update();
        assert_eq!(ring(&mut app).1, Visibility::Hidden);

        app.world_mut().entity_mut(conductor).insert(look(AiState::Hunt, false, false));
        app.update();
        let (shown, visibility) = ring(&mut app);
        assert_eq!(visibility, Visibility::Inherited);
        assert_eq!(shown.radius, ARC_RADIUS);
        assert!(shown.color.alpha() >= LINE);

        app.world_mut().entity_mut(conductor).insert(look(AiState::Hunt, true, false));
        app.update();
        assert_eq!(ring(&mut app).1, Visibility::Hidden);
    }

    #[test]
    fn hacked_field_wears_the_crews_green() {
        let mut app = app();
        app.world_mut().spawn((look(AiState::Hunt, false, true), Transform::default()));
        app.update();
        let green = Srgba::from(ring(&mut app).0.color);
        assert!(green.green > green.red && green.green > green.blue);
    }

    #[test]
    fn arcs_never_pass_the_ring() {
        let mut rng = Rng::new(7);
        for _ in 0..2000 {
            let bolt = prong_bolt(&mut rng);
            let rim = rim_arc(&mut rng);
            assert!(bolt.iter().chain(&rim).all(|p| p.length() <= ARC_RADIUS));
        }
    }
}
