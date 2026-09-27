//! Perk pickups: short power-ups found around each deck. Walking over one applies
//! it to that pirate; timed perks tick down on the host and show in the HUD.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::enemies::Enemy;
use crate::guns::Arsenal;
use crate::net::{LocalPlayer, authority};
use crate::player::{Dash, PirateStatus, Player};
use crate::{GameState, Hud, Level};

const PICKUP_RADIUS: f32 = 10.0;
/// Enemies this close to a pirate with a shock field are stunned.
pub const SHOCK_RADIUS: f32 = 56.0;
/// How long a stun outlasts leaving the field.
const SHOCK_STUN: f32 = 1.2;
/// How long an instant perk stays listed in the HUD.
const INSTANT_SHOW: f32 = 2.0;

pub struct PerksPlugin;

impl Plugin for PerksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (pick_up_perks, tick_perks, shock_enemies)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(authority),
        );
    }
}

pub struct PerksViewPlugin;

impl Plugin for PerksViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_pickup)
            .add_observer(dress_perks)
            .add_systems(Startup, spawn_perk_hud)
            .add_systems(Update, (bob_pickups, shock_rings, update_perk_hud));
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PerkKind {
    /// Guns never run dry or reload.
    Overdrive,
    /// Hull, shield and dash back to full.
    NanoPatch,
    /// Stuns enemies that come close.
    ShockField,
    /// Enemies can't see you; firing breaks it.
    Cloak,
}

impl PerkKind {
    pub const ALL: [PerkKind; 4] = [Self::Overdrive, Self::NanoPatch, Self::ShockField, Self::Cloak];

    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            '1' => Self::Overdrive,
            '2' => Self::NanoPatch,
            '3' => Self::ShockField,
            '4' => Self::Cloak,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Overdrive => "OVERDRIVE",
            Self::NanoPatch => "NANO PATCH",
            Self::ShockField => "SHOCK FIELD",
            Self::Cloak => "CLOAK",
        }
    }

    /// What sort of perk it is, shown next to the name.
    fn category(self) -> &'static str {
        match self {
            Self::Overdrive => "WEAPON",
            Self::NanoPatch => "REPAIR",
            Self::ShockField => "CONTROL",
            Self::Cloak => "STEALTH",
        }
    }

    fn effect(self) -> &'static str {
        match self {
            Self::Overdrive => "infinite ammo",
            Self::NanoPatch => "hull, shield & dash restored",
            Self::ShockField => "stuns enemies near you",
            Self::Cloak => "unseen until you fire",
        }
    }

    /// Seconds the perk lasts; `None` for instant ones.
    fn duration(self) -> Option<f32> {
        match self {
            Self::Overdrive => Some(10.0),
            Self::NanoPatch => None,
            Self::ShockField => Some(8.0),
            Self::Cloak => Some(12.0),
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Overdrive => Color::srgb(1.0, 0.6, 0.2),
            Self::NanoPatch => Color::srgb(0.35, 0.92, 0.47),
            Self::ShockField => Color::srgb(0.35, 0.82, 1.0),
            Self::Cloak => Color::srgb(0.75, 0.47, 1.0),
        }
    }

    fn sprite(self) -> &'static str {
        match self {
            Self::Overdrive => "sprites/perks/overdrive.png",
            Self::NanoPatch => "sprites/perks/nano_patch.png",
            Self::ShockField => "sprites/perks/shock_field.png",
            Self::Cloak => "sprites/perks/cloak.png",
        }
    }
}

/// A perk lying on the deck.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PerkPickup(pub PerkKind);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct ActivePerk {
    pub kind: PerkKind,
    pub left: f32,
    pub total: f32,
}

/// A pirate's running perks. Picking up one they already have restarts it.
#[derive(Component, Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct Perks(pub Vec<ActivePerk>);

impl Perks {
    pub fn has(&self, kind: PerkKind) -> bool {
        self.0.iter().any(|p| p.kind == kind)
    }

    fn get(&self, kind: PerkKind) -> Option<&ActivePerk> {
        self.0.iter().find(|p| p.kind == kind)
    }

    fn start(&mut self, kind: PerkKind) {
        let total = kind.duration().unwrap_or(INSTANT_SHOW);
        self.end(kind);
        self.0.push(ActivePerk { kind, left: total, total });
    }

    pub fn end(&mut self, kind: PerkKind) {
        self.0.retain(|p| p.kind != kind);
    }
}

#[derive(Component)]
struct PickupSprite {
    phase: f32,
}

/// The ring drawn around a pirate while their shock field is up.
#[derive(Component)]
struct ShockRing;

#[derive(Component)]
struct PerkRow(PerkKind);

#[derive(Component)]
struct PerkTime(PerkKind);

#[derive(Component)]
struct PerkFill(PerkKind);

pub fn spawn_pickup(commands: &mut Commands, kind: PerkKind, pos: Vec2) {
    commands.spawn((Level, Replicated, PerkPickup(kind), Transform::from_translation(pos.extend(5.0))));
}

fn dress_pickup(
    add: On<Add, PerkPickup>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    pickups: Query<(&PerkPickup, &Transform)>,
) {
    let Ok((pickup, transform)) = pickups.get(add.entity) else { return };
    let pos = transform.translation.truncate();
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, -2.0, -0.1),
        ));
        parent.spawn((
            PickupSprite { phase: pos.x * 0.29 + pos.y * 0.17 },
            Sprite::from_image(assets.load(pickup.0.sprite())),
        ));
    });
}

/// Gives every pirate (here or replicated) their shock ring, hidden until needed.
fn dress_perks(add: On<Add, Perks>, mut commands: Commands, assets: Res<AssetServer>) {
    commands.entity(add.entity).with_child((
        ShockRing,
        Sprite {
            image: assets.load("sprites/fx/shock_ring.png"),
            color: Color::srgba(1.0, 1.0, 1.0, 0.0),
            ..default()
        },
        Transform::from_xyz(0.0, -4.0, -0.2),
        Visibility::Hidden,
    ));
}

fn bob_pickups(time: Res<Time>, mut sprites: Query<(&PickupSprite, &mut Transform, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (pickup, mut transform, mut sprite) in &mut sprites {
        transform.translation.y = ((t * 3.0 + pickup.phase).sin() * 2.0 + 3.0).round();
        // A slow glow pulse so perks read differently from loot.
        let glow = 1.0 + 0.25 * (t * 5.0 + pickup.phase).sin().max(0.0);
        sprite.color = Color::srgb(glow, glow, glow);
    }
}

fn pick_up_perks(
    mut commands: Commands,
    mut players: Query<(&Player, &Transform, &mut Perks, &mut Health, &mut Dash, &mut Arsenal)>,
    pickups: Query<(Entity, &PerkPickup, &Transform), Without<Player>>,
) {
    for (entity, pickup, transform) in &pickups {
        let pos = transform.translation.truncate();
        let Some((_, _, mut perks, mut health, mut dash, mut arsenal)) = players
            .iter_mut()
            .filter(|(player, t, _, health, ..)| {
                player.status == PirateStatus::Active
                    && !health.is_dead()
                    && t.translation.truncate().distance(pos) <= PICKUP_RADIUS
            })
            .min_by(|a, b| a.1.translation.distance(transform.translation).total_cmp(&b.1.translation.distance(transform.translation)))
        else {
            continue;
        };

        let kind = pickup.0;
        commands.entity(entity).despawn();
        perks.start(kind);
        match kind {
            PerkKind::Overdrive => arsenal.refill(),
            PerkKind::NanoPatch => {
                health.current = health.max;
                health.shield = health.max_shield;
                dash.cooldown = 0.0;
            }
            PerkKind::ShockField | PerkKind::Cloak => {}
        }
        fx(&mut commands, FxKind::Perk(kind), pos, 0.0, kind.color());
    }
}

fn tick_perks(time: Res<Time>, mut perks: Query<(&mut Perks, &Health, &Player)>) {
    let dt = time.delta_secs();
    for (mut perks, health, player) in &mut perks {
        if perks.0.is_empty() {
            continue;
        }
        // Dying or leaving the heist ends everything.
        if health.is_dead() || player.status != PirateStatus::Active {
            perks.0.clear();
            continue;
        }
        for perk in &mut perks.0 {
            perk.left -= dt;
        }
        perks.0.retain(|p| p.left > 0.0);
    }
}

fn shock_enemies(
    players: Query<(&Transform, &Perks)>,
    mut enemies: Query<(&mut Enemy, &Transform), Without<Perks>>,
) {
    let fields: Vec<Vec2> = players
        .iter()
        .filter(|(_, perks)| perks.has(PerkKind::ShockField))
        .map(|(t, _)| t.translation.truncate())
        .collect();
    if fields.is_empty() {
        return;
    }
    for (mut enemy, transform) in &mut enemies {
        let pos = transform.translation.truncate();
        if fields.iter().any(|f| f.distance(pos) <= SHOCK_RADIUS) {
            enemy.stun(SHOCK_STUN);
        }
    }
}

fn shock_rings(
    time: Res<Time>,
    players: Query<&Perks>,
    mut rings: Query<(&ChildOf, &mut Sprite, &mut Transform, &mut Visibility), With<ShockRing>>,
) {
    let t = time.elapsed_secs();
    for (parent, mut sprite, mut transform, mut visibility) in &mut rings {
        let Ok(perks) = players.get(parent.parent()) else { continue };
        let Some(perk) = perks.get(PerkKind::ShockField) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.rotation = Quat::from_rotation_z(t * 0.8);
        // Flickers as it runs out.
        let fading = perk.left < 2.0 && (t * 12.0).sin() < 0.0;
        sprite.color = Color::srgba(1.0, 1.0, 1.0, if fading { 0.25 } else { 0.55 + 0.2 * (t * 6.0).sin() });
    }
}

fn spawn_perk_hud(mut commands: Commands, assets: Res<AssetServer>) {
    // Bottom left, above the controls hint and clear of the centre banners.
    commands
        .spawn((Hud, Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(44.0),
            left: Val::Px(18.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        }))
        .with_children(|panel| {
            for kind in PerkKind::ALL {
                panel
                    .spawn((
                        PerkRow(kind),
                        Node {
                            display: Display::None,
                            column_gap: Val::Px(8.0),
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 0.7)),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            ImageNode::new(assets.load(kind.sprite())),
                            Node {
                                width: Val::Px(32.0),
                                height: Val::Px(32.0),
                                ..default()
                            },
                        ));
                        row.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            ..default()
                        })
                        .with_children(|info| {
                            info.spawn(Node {
                                column_gap: Val::Px(8.0),
                                align_items: AlignItems::Baseline,
                                ..default()
                            })
                            .with_children(|title| {
                                title.spawn((
                                    Text::new(kind.name()),
                                    TextFont::from_font_size(15.0),
                                    TextColor(kind.color()),
                                ));
                                title.spawn((
                                    Text::new(kind.category()),
                                    TextFont::from_font_size(11.0),
                                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                                ));
                                title.spawn((
                                    PerkTime(kind),
                                    Text::new(""),
                                    TextFont::from_font_size(13.0),
                                    TextColor(Color::WHITE),
                                ));
                            });
                            info.spawn((
                                Text::new(kind.effect()),
                                TextFont::from_font_size(11.0),
                                TextColor(Color::srgba(0.85, 0.9, 1.0, 0.7)),
                            ));
                            info.spawn((
                                Node {
                                    width: Val::Px(150.0),
                                    height: Val::Px(4.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                            ))
                            .with_child((
                                PerkFill(kind),
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(kind.color()),
                            ));
                        });
                    });
            }
        });
}

fn update_perk_hud(
    time: Res<Time>,
    perks: Option<Single<&Perks, With<LocalPlayer>>>,
    mut rows: Query<(&PerkRow, &mut Node), (Without<PerkFill>, Without<PerkTime>)>,
    mut times: Query<(&PerkTime, &mut Text, &mut TextColor)>,
    mut fills: Query<(&PerkFill, &mut Node), Without<PerkRow>>,
) {
    let active = |kind| perks.as_ref().and_then(|p| p.get(kind)).copied();
    for (row, mut node) in &mut rows {
        let display = if active(row.0).is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    let blink = (time.elapsed_secs() * 8.0).sin() > 0.0;
    for (tag, mut text, mut color) in &mut times {
        let Some(perk) = active(tag.0) else { continue };
        if tag.0.duration().is_some() {
            text.0 = format!("{:.1}s", perk.left.max(0.0));
            color.0 = if perk.left < 2.0 && blink { Color::srgb(1.0, 0.4, 0.35) } else { Color::WHITE };
        } else {
            text.0 = "INSTANT".into();
            color.0 = Color::srgba(1.0, 1.0, 1.0, 0.8);
        }
    }
    for (fill, mut node) in &mut fills {
        let Some(perk) = active(fill.0) else { continue };
        node.width = Val::Percent((perk.left / perk.total).clamp(0.0, 1.0) * 100.0);
    }
}

