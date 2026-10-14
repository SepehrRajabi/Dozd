//! The crew panel: every other pirate's class, hull, shield, haul and state, and
//! over the network how well they're connected. The host measures each
//! client's link and shares it on their pirate, so every machine sees the same.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

use crate::combat::Health;
use crate::hud::{self, HudSlot, Meter};
use crate::mission::MissionStatus;
use crate::net::{LocalId, NetMode, authority};
use crate::player::{PirateStatus, Player, slot_color};

/// Seconds between link measurements; sooner just churns replication.
const LINK_INTERVAL: f32 = 1.0;
/// Seconds a pirate who dropped out stays on the panel, marked as gone.
const GONE_TIME: f32 = 6.0;
const ROW_WIDTH: f32 = 230.0;
const BAR_WIDTH: f32 = 130.0;
const SHIELD_COLOR: Color = Color::srgb(0.45, 0.7, 1.0);
const HULL_COLOR: Color = Color::srgb(0.4, 0.85, 0.45);
const HULL_LOW: Color = Color::srgb(1.0, 0.35, 0.3);
const GOOD: Color = Color::srgb(0.45, 0.9, 0.5);
const FAIR: Color = Color::srgb(1.0, 0.8, 0.3);
const BAD: Color = Color::srgb(1.0, 0.35, 0.3);

pub struct CrewPlugin;

impl Plugin for CrewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, measure_links.run_if(authority));
    }
}

pub struct CrewViewPlugin;

impl Plugin for CrewViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_crew_panel);
    }
}

/// How well a client's pirate is connected to the host. Only on pirates
/// played from another machine.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Link {
    pub ping_ms: u16,
    /// Packet loss, in percent.
    pub loss: u8,
}

impl Link {
    fn quality(self) -> (Color, &'static str) {
        match (self.ping_ms, self.loss) {
            (_, 10..) | (250.., _) => (BAD, "POOR"),
            (_, 3..) | (120.., _) => (FAIR, "FAIR"),
            _ => (GOOD, "GOOD"),
        }
    }
}

/// Host-side: copies each client's round trip and loss onto their pirate.
fn measure_links(
    mut commands: Commands,
    time: Res<Time>,
    mut wait: Local<f32>,
    clients: Query<(&NetworkId, &ConnectedClientStats)>,
    mut players: Query<(Entity, &Player, Option<&mut Link>)>,
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = LINK_INTERVAL;
    for (entity, player, link) in &mut players {
        let Some((_, stats)) = clients.iter().find(|(id, _)| id.get() == player.owner) else { continue };
        let measured = Link {
            // To the nearest 5 ms, so jitter alone doesn't resend it.
            ping_ms: ((stats.rtt * 1000.0 / 5.0).round() * 5.0).min(u16::MAX as f64) as u16,
            loss: (stats.packet_loss * 100.0).round().clamp(0.0, 100.0) as u8,
        };
        match link {
            Some(mut link) => {
                link.set_if_neq(measured);
            }
            None => {
                commands.entity(entity).insert(measured);
            }
        }
    }
}

/// One crewmate's row, and the parts of it that change.
#[derive(Component)]
struct CrewRow {
    owner: u64,
    slot: u8,
    /// Seconds since their pirate vanished mid-run, if it has.
    gone: Option<f32>,
    status: Entity,
    link: Entity,
    shield: Entity,
    hull: Entity,
    credits: Entity,
}

fn spawn_row(commands: &mut Commands, assets: &AssetServer, player: &Player) -> Entity {
    let class = player.class;
    let status = commands.spawn(hud::text("", 11.0, hud::DIM)).id();
    let link = commands.spawn(hud::text("", 11.0, hud::DIM)).id();
    let shield = commands.spawn(hud::meter(BAR_WIDTH, 4.0, SHIELD_COLOR)).id();
    let hull = commands.spawn(hud::meter(BAR_WIDTH, 6.0, HULL_COLOR)).id();
    let credits = commands.spawn(hud::text("", 12.0, hud::GOLD)).id();

    let spacer = || Node { flex_grow: 1.0, ..default() };
    let top = commands
        .spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() })
        .with_children(|line| {
            line.spawn(hud::text(format!("P{}", player.slot + 1), 12.0, slot_color(player.slot)));
            line.spawn(hud::text(class.name(), 12.0, class.color()));
            line.spawn(spacer());
        })
        .add_children(&[status, link])
        .id();
    let bars = commands
        .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() })
        .add_children(&[shield, hull])
        .id();
    let bottom = commands
        .spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() })
        .add_children(&[bars])
        .with_child(spacer())
        .add_children(&[credits])
        .id();
    let body = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            row_gap: Val::Px(3.0),
            ..default()
        })
        .add_children(&[top, bottom])
        .id();

    commands
        .spawn((
            hud::panel_accent(
                Node {
                    width: Val::Px(ROW_WIDTH),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                    ..default()
                },
                class.color(),
            ),
            children![(
                ImageNode::new(assets.load(class.sprite())),
                Node { width: Val::Px(24.0), height: Val::Px(24.0), ..default() },
            )],
        ))
        .add_children(&[body])
        .insert(CrewRow { owner: player.owner, slot: player.slot, gone: None, status, link, shield, hull, credits })
        .id()
}

fn update_crew_panel(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    mode: Res<NetMode>,
    local: Res<LocalId>,
    slots: Query<(Entity, &HudSlot)>,
    mission: Option<Single<&MissionStatus>>,
    players: Query<(&Player, &Health, Option<&Link>)>,
    mut rows: Query<(Entity, &mut CrewRow)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut meters: Query<&mut Meter>,
    mut resort: Local<bool>,
) {
    let networked = *mode != NetMode::Solo;
    let anyone = players.iter().any(|(p, ..)| p.owner == local.0);
    let container = hud::slot(&slots, HudSlot::Crew);

    // Rows spawned last frame are in the query now: put them all in join order.
    if *resort {
        *resort = false;
        let mut order: Vec<(u8, Entity)> = rows.iter().map(|(e, row)| (row.slot, e)).collect();
        order.sort();
        let order: Vec<Entity> = order.into_iter().map(|(_, e)| e).collect();
        commands.entity(container).replace_children(&order);
    }
    for (player, ..) in &players {
        if player.owner == local.0 || rows.iter().any(|(_, row)| row.owner == player.owner) {
            continue;
        }
        let row = spawn_row(&mut commands, &assets, player);
        commands.entity(container).add_child(row);
        *resort = true;
    }

    for (entity, mut row) in &mut rows {
        let player = players.iter().find(|(p, ..)| p.owner == row.owner);
        let Some((player, health, link)) = player else {
            // The whole run is gone (back to the menu or a new run): no ghosts.
            if !anyone || !networked {
                commands.entity(entity).despawn();
                continue;
            }
            let gone = row.gone.get_or_insert(0.0);
            *gone += time.delta_secs();
            if *gone >= GONE_TIME {
                commands.entity(entity).despawn();
                continue;
            }
            set_text(&mut texts, row.status, "DISCONNECTED", BAD);
            set_text(&mut texts, row.link, "", hud::DIM);
            continue;
        };
        row.gone = None;

        let other_deck = mission.as_ref().is_some_and(|m| m.deck != player.deck);
        let (status, color) = if health.is_dead() {
            ("DOWN", BAD)
        } else {
            match player.status {
                PirateStatus::Extracted => ("EXTRACTED", hud::GOLD),
                PirateStatus::LeftBehind => ("LEFT BEHIND", BAD),
                PirateStatus::Active if other_deck => ("OTHER DECK", hud::DIM),
                PirateStatus::Active => ("", hud::DIM),
            }
        };
        set_text(&mut texts, row.status, status, color);

        let (link_text, link_color) = match (networked, link) {
            (false, _) => (String::new(), hud::DIM),
            // No link of their own: they're on the host's machine.
            (true, None) => ("HOST".into(), hud::LABEL),
            // Nothing getting through: they're likely gone, and about to time out.
            (true, Some(link)) if link.loss >= 50 => ("NO SIGNAL".into(), BAD),
            (true, Some(link)) => {
                let (color, word) = link.quality();
                let loss = if link.loss > 0 { format!(" {}%", link.loss) } else { String::new() };
                (format!("{word} {}ms{loss}", link.ping_ms), color)
            }
        };
        set_text(&mut texts, row.link, &link_text, link_color);

        if let Ok(mut meter) = meters.get_mut(row.shield) {
            meter.frac = if health.max_shield > 0.0 { health.shield / health.max_shield } else { 0.0 };
        }
        if let Ok(mut meter) = meters.get_mut(row.hull) {
            let frac = if health.is_dead() { 0.0 } else { health.current / health.max };
            meter.frac = frac;
            meter.color = if frac < 0.35 { HULL_LOW } else { HULL_COLOR };
        }
        set_text(&mut texts, row.credits, &format!("{} cr", player.credits), hud::GOLD);
    }
}

fn set_text(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, content: &str, color: Color) {
    let Ok((mut text, mut text_color)) = texts.get_mut(entity) else { return };
    if text.0 != content {
        text.0 = content.into();
    }
    text_color.set_if_neq(TextColor(color));
}
