//! Every deck's vault: a stack of bars the crew pulls out one at a time by
//! holding E beside it. Every pirate at it pulls on their own, so a bigger crew
//! empties it faster. The deeper the deck, the more precious the bars. It sits
//! in a sealed room whose keeper has to go down first (see `breach.rs`).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::breach::Breach;
use crate::class::{Laser, RiotShield};
use crate::combat::{FxKind, Health, fx};
use crate::mission::{Alarm, Banner};
use crate::net::authority;
use crate::player::{Controls, PirateStatus, Player};
use crate::room::{DECKS, Heist};
use crate::{GameState, Level};

/// Seconds of holding E for one pirate to pull one bar.
pub const BAR_TIME: f32 = 1.2;
/// Pirates within this distance of the vault's centre can pull bars.
pub const VAULT_REACH: f32 = 30.0;
const GOLD: Color = Color::srgb(1.0, 0.82, 0.35);

pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            pull_bars.run_if(in_state(GameState::Playing)).run_if(authority),
        );
    }
}

/// What a vault holds, by how deep it is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultTier {
    Silver,
    Gold,
    Platinum,
}

impl VaultTier {
    /// The vault on `deck` (0 = top); anything deeper than the third is platinum.
    pub fn for_deck(deck: u8) -> Self {
        match deck {
            0 => Self::Silver,
            1 => Self::Gold,
            _ => Self::Platinum,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Silver => "SILVER",
            Self::Gold => "GOLD",
            Self::Platinum => "PLATINUM",
        }
    }

    /// Bars in a full vault.
    pub fn bars(self) -> u32 {
        match self {
            Self::Silver => 24,
            Self::Gold => 32,
            Self::Platinum => 40,
        }
    }

    /// Credits per bar.
    pub fn bar_value(self) -> u32 {
        match self {
            Self::Silver => 150,
            Self::Gold => 310,
            Self::Platinum => 560,
        }
    }

    /// Tint for credit popups and banners.
    pub fn color(self) -> Color {
        match self {
            Self::Silver => Color::srgb(0.82, 0.86, 0.92),
            Self::Gold => GOLD,
            Self::Platinum => Color::srgb(0.75, 0.95, 1.0),
        }
    }
}

/// The vault, at the centre of each deck's vault room. Stays put once empty.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct Vault {
    pub tier: VaultTier,
    pub bars: u32,
    pub total: u32,
}

impl Vault {
    pub fn full(tier: VaultTier) -> Self {
        Self { tier, bars: tier.bars(), total: tier.bars() }
    }

    /// Credits per bar.
    pub fn bar_value(&self) -> u32 {
        self.tier.bar_value()
    }
}

/// How far a pirate is through pulling their next bar, 0..1 in steps of 0.05.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub struct Looting(pub f32);

/// Host only: puts a full vault for `deck` at `pos`.
pub fn spawn_vault(commands: &mut Commands, pos: Vec2, deck: u8) {
    commands.spawn((
        Level,
        Replicated,
        Vault::full(VaultTier::for_deck(deck)),
        Transform::from_translation(pos.extend(1.0)),
    ));
}

/// Pirates holding E at the vault, hands free, fill their progress; each full
/// bar goes to whoever pulled it. Letting go, stepping away, firing or using
/// their ability starts it over. A sentry already down keeps shooting for them.
/// Nothing comes out until the vault room's gate is open: its keeper is down.
fn pull_bars(
    mut commands: Commands,
    time: Res<Time>,
    heist: Res<Heist>,
    mut alarm: ResMut<Alarm>,
    mut banner: ResMut<Banner>,
    mut vaults: Query<(&mut Vault, &Transform)>,
    mut players: Query<(Entity, &mut Player, &Health, &Controls, &Transform, &mut Looting)>,
    lasers: Query<&Laser>,
    riot_shields: Query<&RiotShield>,
    breaches: Query<&Breach>,
    // Exact seconds pulled so far; `Looting` only carries a rounded copy.
    mut pulled: Local<HashMap<Entity, f32>>,
) {
    let mut vault = vaults.single_mut().ok().filter(|_| breaches.iter().all(Breach::is_open));
    let dt = time.delta_secs();
    for (entity, mut player, health, controls, transform, mut looting) in &mut players {
        let pos = transform.translation.truncate();
        let at_vault = vault.as_ref().is_some_and(|(v, t)| {
            v.bars > 0 && t.translation.truncate().distance(pos) <= VAULT_REACH
        });
        // A laser firing or a riot shield held out takes their hands too.
        let busy = controls.fire
            || controls.ability
            || lasers.iter().any(|l| l.owner == player.owner)
            || riot_shields.iter().any(|s| s.owner == player.owner);
        let pulling = at_vault
            && controls.interact_held
            && !busy
            && !health.is_dead()
            && player.status == PirateStatus::Active;
        let Some((vault, _)) = vault.as_mut().filter(|_| pulling) else {
            pulled.remove(&entity);
            looting.set_if_neq(Looting(0.0));
            continue;
        };

        let seconds = pulled.entry(entity).or_default();
        *seconds += dt;
        if *seconds >= BAR_TIME {
            *seconds = 0.0;
            let tier = vault.tier;
            if vault.bars == vault.total {
                alarm.vault_breached();
                let text = format!("{} VAULT BREACHED  -  security is on its way", tier.name());
                banner.show(text, Color::srgb(1.0, 0.45, 0.3), 4.0);
            }
            vault.bars -= 1;
            player.credits += vault.bar_value();
            player.items += 1;
            alarm.bar_taken();
            fx(&mut commands, FxKind::Popup(vault.bar_value()), pos, 0.0, tier.color());
            if vault.bars == 0 {
                let text = if heist.deck + 1 >= DECKS {
                    "VAULT EMPTIED  -  get to a pad"
                } else {
                    "VAULT EMPTIED  -  ride the lift down together, or leave by a pad"
                };
                banner.show(text, tier.color(), 4.0);
            }
        }
        looting.set_if_neq(Looting((*seconds / BAR_TIME * 20.0).round() / 20.0));
    }
}

pub struct VaultViewPlugin;

impl Plugin for VaultViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_vault)
            .add_observer(dress_looting)
            .add_systems(Update, (draw_vault, lift_bars, draw_looting));
    }
}

/// The bars drawn in the pile, bottom layer first and back to front.
const PILE: [(u32, u32); 4] = [(5, 4), (4, 3), (3, 2), (2, 1)];

/// The pile on top of the vault's floor; shakes for a moment when a bar leaves.
#[derive(Component)]
struct VaultPile {
    shown: u32,
    shake: f32,
}

/// One bar of the pile, by its place in `PILE` order.
#[derive(Component)]
struct PileBar(u32);

/// A bar just pulled out, rising and fading away in its tier's tint.
#[derive(Component)]
struct LiftedBar {
    age: f32,
    tint: Color,
}

/// A pirate's pull bar under their feet, shown while `Looting` is filling.
#[derive(Component)]
struct LootingBar {
    fill: bool,
}

/// Where each bar of the pile sits, relative to the vault's centre: a stepped
/// pyramid, every layer two pixels right and up on the one below.
fn pile_spots() -> Vec<Vec3> {
    let mut spots = Vec::new();
    for (layer, &(cols, rows)) in PILE.iter().enumerate() {
        let k = layer as f32;
        for row in 0..rows {
            for col in 0..cols {
                // Pixel offsets of a 6x5 bar in the 32x32 floor, image rows down.
                let x = 3.0 + col as f32 * 5.0 + k * 2.0;
                let y = 11.0 + row as f32 * 3.0 - k;
                spots.push(Vec3::new(x - 13.0, 13.5 - y, 0.1 + k * 0.05 + row as f32 * 0.01));
            }
        }
    }
    spots
}

/// The pile's bar sprite and tint: gold as drawn, the others from plain steel.
fn bar_look(tier: VaultTier) -> (&'static str, Color) {
    match tier {
        VaultTier::Gold => ("sprites/vault_bar.png", Color::WHITE),
        _ => ("sprites/vault_bar_plain.png", tier.color()),
    }
}

/// A light wash over the strongroom floor, so the tier reads at a glance.
fn floor_tint(tier: VaultTier) -> Color {
    match tier {
        VaultTier::Silver => Color::srgb(0.88, 0.9, 0.96),
        VaultTier::Gold => Color::WHITE,
        VaultTier::Platinum => Color::srgb(0.84, 0.95, 1.0),
    }
}

/// Bars of the pile still drawn for `bars` left out of `total`.
fn bars_shown(vault: &Vault, spots: u32) -> u32 {
    if vault.total == 0 {
        return 0;
    }
    (vault.bars * spots).div_ceil(vault.total)
}

/// Dresses a vault as a strongroom floor with its bars piled up on it.
fn dress_vault(add: On<Add, Vault>, mut commands: Commands, assets: Res<AssetServer>, vaults: Query<&Vault>) {
    let Ok(vault) = vaults.get(add.entity) else { return };
    let spots = pile_spots();
    let shown = bars_shown(vault, spots.len() as u32);
    let (bar_sprite, tint) = bar_look(vault.tier);
    let bar = Sprite { color: tint, ..Sprite::from_image(assets.load(bar_sprite)) };
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        parent.spawn(Sprite { color: floor_tint(vault.tier), ..Sprite::from_image(assets.load("sprites/vault.png")) });
        parent
            .spawn((VaultPile { shown, shake: 0.0 }, Transform::default(), Visibility::default()))
            .with_children(|pile| {
                pile.spawn((
                    PileBar(0),
                    Sprite::from_image(assets.load("sprites/shadow.png")),
                    Transform::from_xyz(0.0, 3.0, 0.05).with_scale(Vec3::new(2.0, 2.0, 1.0)),
                ));
                for (i, &spot) in spots.iter().enumerate() {
                    let visibility = if (i as u32) < shown { Visibility::Inherited } else { Visibility::Hidden };
                    pile.spawn((PileBar(i as u32), visibility, bar.clone(), Transform::from_translation(spot)));
                }
            });
    });
}

/// Hides bars as the vault empties, and lifts the ones just taken off the pile.
fn draw_vault(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    vaults: Query<&Vault>,
    mut piles: Query<(Entity, &ChildOf, &mut VaultPile, &mut Transform)>,
    mut bars: Query<(&ChildOf, &PileBar, &mut Visibility)>,
) {
    let spots = pile_spots();
    for (entity, parent, mut pile, mut transform) in &mut piles {
        let Ok(vault) = vaults.get(parent.parent()) else { continue };
        let shown = bars_shown(vault, spots.len() as u32);
        if shown < pile.shown {
            pile.shake = 0.3;
            let (bar_sprite, tint) = bar_look(vault.tier);
            for &spot in &spots[shown as usize..pile.shown as usize] {
                commands.entity(parent.parent()).with_child((
                    LiftedBar { age: 0.0, tint },
                    Sprite { color: tint, ..Sprite::from_image(assets.load(bar_sprite)) },
                    Transform::from_translation(spot.with_z(0.5)),
                ));
            }
        }
        if shown != pile.shown {
            pile.shown = shown;
            for (bar_parent, bar, mut visibility) in &mut bars {
                if bar_parent.parent() != entity {
                    continue;
                }
                // Bar 0 also holds the shadow, which goes with the last bar.
                let on = bar.0 < shown;
                visibility.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        pile.shake = (pile.shake - time.delta_secs()).max(0.0);
        let jolt = if pile.shake > 0.0 { (pile.shake * 60.0).sin().signum() } else { 0.0 };
        transform.translation.x = jolt;
    }
}

/// Bars just taken float up and fade out.
fn lift_bars(
    mut commands: Commands,
    time: Res<Time>,
    mut bars: Query<(Entity, &mut LiftedBar, &mut Transform, &mut Sprite)>,
) {
    const LIFE: f32 = 0.5;
    let dt = time.delta_secs();
    for (entity, mut bar, mut transform, mut sprite) in &mut bars {
        bar.age += dt;
        if bar.age >= LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.y += 24.0 * dt;
        sprite.color = bar.tint.with_alpha(1.0 - bar.age / LIFE);
    }
}

/// Gives every pirate a slim bar under their feet for pulling bars, in the
/// vault's tier colour (see `draw_looting`).
fn dress_looting(add: On<Add, Looting>, mut commands: Commands) {
    commands.entity(add.entity).with_children(|parent| {
        parent.spawn((
            LootingBar { fill: false },
            Visibility::Hidden,
            Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.75), Vec2::new(14.0, 3.0)),
            Transform::from_xyz(0.0, -11.0, 1.0),
        ));
        parent.spawn((
            LootingBar { fill: true },
            Visibility::Hidden,
            Sprite::from_color(GOLD, Vec2::new(0.0, 1.0)),
            Transform::from_xyz(0.0, -11.0, 1.1),
        ));
    });
}

fn draw_looting(
    players: Query<&Looting>,
    vault: Option<Single<&Vault>>,
    mut bars: Query<(&ChildOf, &LootingBar, &mut Visibility, &mut Transform, &mut Sprite)>,
) {
    for (parent, bar, mut visibility, mut transform, mut sprite) in &mut bars {
        let Ok(looting) = players.get(parent.parent()) else { continue };
        let frac = looting.0.clamp(0.0, 1.0);
        visibility.set_if_neq(if frac > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        if bar.fill {
            sprite.color = vault.as_ref().map_or(GOLD, |v| v.tier.color());
            let width = (12.0 * frac).round();
            sprite.custom_size = Some(Vec2::new(width, 1.0));
            transform.translation.x = (width - 12.0) / 2.0;
        }
    }
}
