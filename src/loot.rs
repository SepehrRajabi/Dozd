use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::mission::Alarm;
use crate::net::{LocalPlayer, authority};
use crate::player::{Controls, PirateStatus, Player};
use crate::hud::{self, HudSlot};
use crate::vault::{VAULT_REACH, Vault};
use crate::{GameState, Level};

const PICKUP_RADIUS: f32 = 14.0;

pub struct LootPlugin;

impl Plugin for LootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            pick_up_loot.run_if(in_state(GameState::Playing)).run_if(authority),
        );
    }
}

pub struct LootViewPlugin;

impl Plugin for LootViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_loot)
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (bob_loot, update_hud));
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LootKind {
    CreditChip,
    GoldIngot,
    DataCrystal,
    AlienRelic,
    PlasmaCell,
    WeaponCrate,
}

impl LootKind {
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'c' => Self::CreditChip,
            'i' => Self::GoldIngot,
            'x' => Self::DataCrystal,
            'r' => Self::AlienRelic,
            'p' => Self::PlasmaCell,
            'w' => Self::WeaponCrate,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CreditChip => "Credit Chip",
            Self::GoldIngot => "Gold Ingot",
            Self::DataCrystal => "Data Crystal",
            Self::AlienRelic => "Alien Relic",
            Self::PlasmaCell => "Plasma Cell",
            Self::WeaponCrate => "Weapon Crate",
        }
    }

    pub fn value(self) -> u32 {
        match self {
            Self::CreditChip => 100,
            Self::PlasmaCell => 250,
            Self::GoldIngot => 400,
            Self::DataCrystal => 650,
            Self::WeaponCrate => 800,
            Self::AlienRelic => 1500,
        }
    }

    fn sprite(self) -> &'static str {
        match self {
            Self::CreditChip => "sprites/loot/credit_chip.png",
            Self::GoldIngot => "sprites/loot/gold_ingot.png",
            Self::DataCrystal => "sprites/loot/data_crystal.png",
            Self::AlienRelic => "sprites/loot/alien_relic.png",
            Self::PlasmaCell => "sprites/loot/plasma_cell.png",
            Self::WeaponCrate => "sprites/loot/weapon_crate.png",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::CreditChip => Color::srgb(0.45, 0.65, 1.0),
            Self::GoldIngot => Color::srgb(1.0, 0.84, 0.3),
            Self::DataCrystal => Color::srgb(0.5, 0.92, 1.0),
            Self::AlienRelic => Color::srgb(0.75, 0.45, 1.0),
            Self::PlasmaCell => Color::srgb(0.45, 1.0, 0.5),
            Self::WeaponCrate => Color::srgb(0.7, 0.8, 0.45),
        }
    }
}

#[derive(Component)]
struct LootSprite {
    phase: f32,
}

#[derive(Component)]
struct HaulText;

#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct PromptPanel;

#[derive(Component)]
struct ItemsText;

pub fn spawn_loot(commands: &mut Commands, kind: LootKind, pos: Vec2) {
    commands.spawn((Level, Replicated, kind, Transform::from_translation(pos.extend(5.0))));
}

fn dress_loot(
    add: On<Add, LootKind>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    loot: Query<(&LootKind, &Transform)>,
) {
    let Ok((kind, transform)) = loot.get(add.entity) else { return };
    let pos = transform.translation.truncate();
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, -1.0, -0.1),
        ));
        parent.spawn((
            LootSprite { phase: pos.x * 0.37 + pos.y * 0.11 },
            Sprite::from_image(assets.load(kind.sprite())),
        ));
    });
}

fn bob_loot(time: Res<Time>, mut sprites: Query<(&LootSprite, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (loot, mut transform) in &mut sprites {
        transform.translation.y = ((t * 2.5 + loot.phase).sin() * 1.5 + 1.5).round();
    }
}

fn nearest_loot<'a>(
    player: Vec2,
    loot: impl Iterator<Item = (Entity, &'a LootKind, &'a Transform)>,
) -> Option<(Entity, LootKind, Vec2)> {
    loot.map(|(e, k, t)| (e, *k, t.translation.truncate()))
        .filter(|(_, _, p)| p.distance(player) <= PICKUP_RADIUS)
        .min_by(|a, b| a.2.distance(player).total_cmp(&b.2.distance(player)))
}

fn pick_up_loot(
    mut commands: Commands,
    mut alarm: ResMut<Alarm>,
    mut players: Query<(&mut Player, &Health, &Controls, &Transform)>,
    loot: Query<(Entity, &LootKind, &Transform)>,
) {
    let mut taken = Vec::new();
    for (mut player, health, controls, transform) in &mut players {
        if !controls.interact || health.is_dead() || player.status != PirateStatus::Active {
            continue;
        }
        let available = loot.iter().filter(|(e, _, _)| !taken.contains(e));
        let Some((entity, kind, pos)) = nearest_loot(transform.translation.truncate(), available) else {
            continue;
        };

        taken.push(entity);
        commands.entity(entity).despawn();
        player.credits += kind.value();
        player.items += 1;
        alarm.loot_taken(kind);
        fx(&mut commands, FxKind::Popup(kind.value()), pos, 0.0, kind.color());
    }
}

fn spawn_hud(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Haul)).with_child((
        Node {
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Baseline,
            ..default()
        },
        children![
            hud::text("HAUL", 12.0, hud::LABEL),
            (HaulText, hud::text("", 22.0, hud::GOLD)),
            (ItemsText, hud::text("", 13.0, hud::DIM)),
        ],
    ));

    commands.entity(hud::slot(&slots, HudSlot::Prompt)).with_child((
        PromptPanel,
        hud::panel(Node {
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            display: Display::None,
            ..default()
        }),
        children![hud::key_cap("E"), (PromptText, hud::text("", 17.0, Color::WHITE))],
    ));
}

fn update_hud(
    player: Option<Single<(&Player, &Transform), With<LocalPlayer>>>,
    loot: Query<(Entity, &LootKind, &Transform)>,
    vaults: Query<(&Vault, &Transform)>,
    mut haul_text: Single<&mut Text, (With<HaulText>, Without<ItemsText>, Without<PromptText>)>,
    mut items_text: Single<&mut Text, (With<ItemsText>, Without<PromptText>)>,
    mut panel: Single<&mut Node, With<PromptPanel>>,
    prompt: Single<(&mut Text, &mut TextColor), With<PromptText>>,
) {
    let Some(player) = player else { return };
    let (player, transform) = *player;
    haul_text.set_if_neq(Text(format!("{} cr", player.credits)));
    let items = if player.items == 1 { "1 item".into() } else { format!("{} items", player.items) };
    items_text.set_if_neq(Text(items));

    // Loose loot in reach comes first; the vault takes a while anyway.
    let pos = transform.translation.truncate();
    let shown = if let Some((_, kind, _)) = nearest_loot(pos, loot.iter()) {
        Some((format!("Take {}  ({} cr)", kind.name(), kind.value()), kind.color()))
    } else {
        vaults
            .iter()
            .find(|(v, t)| v.bars > 0 && t.translation.truncate().distance(pos) <= VAULT_REACH)
            .map(|(v, _)| {
                let left = if v.bars == 1 { "1 bar left".into() } else { format!("{} bars left", v.bars) };
                (format!("Hold to loot the vault  ({left})"), hud::GOLD)
            })
    };
    let display = if shown.is_some() { Display::Flex } else { Display::None };
    if panel.display != display {
        panel.display = display;
    }
    if let Some((line, tint)) = shown {
        let (mut text, mut color) = prompt.into_inner();
        text.set_if_neq(Text(line));
        color.set_if_neq(TextColor(tint));
    }
}
