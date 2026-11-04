//! The crew panel: every other pirate's class, hull, shield, haul and state, and
//! over the network how well they're connected. The host measures each
//! client's link and shares it on their pirate, so every machine sees the same.
//!
//! Also the roster: a crew has at most one of each class. The host shares who
//! holds which, and a joiner whose class is taken picks another from it.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

use crate::GameState;
use crate::challenge::Rewards;
use crate::challenge_hud::reward_chips;
use crate::abilities::Passive;
use crate::class::{Ability, Boons, CrewNames, LocalLoadout, Loadouts, PirateClass, ability_status, crew_label, effects, passive_tag};
use crate::combat::Health;
use crate::hud::{self, HudSlot, Meter, PirateCard, PirateHeader};
use crate::mission::MissionStatus;
use crate::net::{ChooseLoadout, LocalId, LocalPlayer, NetMode, authority};
use crate::player::{PirateStatus, Player, slot_color};

/// Seconds between link measurements; sooner just churns replication.
const LINK_INTERVAL: f32 = 1.0;
/// Seconds a pirate who dropped out stays on the panel, marked as gone.
const GONE_TIME: f32 = 6.0;
const ROW_WIDTH: f32 = 290.0;
const BAR_WIDTH: f32 = 110.0;
/// The least of a crew row's name that shows: room for `P1`.
const LABEL_MIN: f32 = 15.0;
const SHIELD_COLOR: Color = Color::srgb(0.45, 0.7, 1.0);
const HULL_COLOR: Color = Color::srgb(0.4, 0.85, 0.45);
const HULL_LOW: Color = Color::srgb(1.0, 0.35, 0.3);
const GOOD: Color = Color::srgb(0.45, 0.9, 0.5);
const FAIR: Color = Color::srgb(1.0, 0.8, 0.3);
const BAD: Color = Color::srgb(1.0, 0.35, 0.3);

pub struct CrewPlugin;

impl Plugin for CrewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoinOrder>()
            .add_systems(Update, (measure_links, update_roster).run_if(authority));
    }
}

pub struct CrewViewPlugin;

impl Plugin for CrewViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickedClass>()
            .add_systems(Startup, (spawn_class_picker, spawn_local_card, spawn_local_effects))
            .add_systems(Update, (update_crew_panel, update_local_card, update_local_effects, update_class_picker));
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

/// The crew by `Player::owner`, host first and then in join order, with the
/// class the host accepted for each (`None` while they're still choosing).
/// Lives on the mission status entity. A pirate's place here is their slot.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Eq, Default, Debug)]
pub struct Roster(pub Vec<Crewmate>);

/// One place in the `Roster`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Crewmate {
    pub owner: u64,
    pub class: Option<PirateClass>,
    /// The display name they chose, already clean; empty for none.
    pub name: String,
}

impl Roster {
    /// Who holds `class`, if anyone.
    pub fn holder(&self, class: PirateClass) -> Option<u64> {
        self.0.iter().find(|c| c.class == Some(class)).map(|c| c.owner)
    }

    /// `owner`'s place in the crew, from 0.
    pub fn slot(&self, owner: u64) -> Option<usize> {
        self.0.iter().position(|c| c.owner == owner)
    }

    pub fn class_of(&self, owner: u64) -> Option<PirateClass> {
        self.0.iter().find(|c| c.owner == owner).and_then(|c| c.class)
    }

    /// What to call the crewmate in `slot`.
    pub fn label(&self, slot: usize) -> Option<String> {
        self.0.get(slot).map(|c| crew_label(&c.name, slot))
    }
}

/// Host-side: connected clients' ids, in the order they joined.
#[derive(Resource, Default)]
pub struct JoinOrder(pub Vec<u64>);

/// Host-side: keeps the roster in step with the loadouts the host accepted.
fn update_roster(
    mut commands: Commands,
    mode: Res<NetMode>,
    local: Res<LocalId>,
    loadout: Res<LocalLoadout>,
    loadouts: Res<Loadouts>,
    names: Res<CrewNames>,
    order: Res<JoinOrder>,
    clients: Query<&NetworkId, With<AuthorizedClient>>,
    mut status: Query<(Entity, Option<&mut Roster>), With<MissionStatus>>,
) {
    if !matches!(*mode, NetMode::Host { .. }) {
        return;
    }
    let Ok((entity, roster)) = status.single_mut() else { return };
    let joiners = order
        .0
        .iter()
        .filter(|id| clients.iter().any(|c| c.get() == **id))
        .map(|&id| Crewmate {
            owner: id,
            class: loadouts.0.get(&id).map(|l| l.class),
            name: names.0.get(&id).cloned().unwrap_or_default(),
        });
    let host = Crewmate { owner: local.0, class: Some(loadout.0.class), name: loadout.0.name.clone() };
    let current = Roster([host].into_iter().chain(joiners).collect());
    match roster {
        Some(mut roster) => {
            roster.set_if_neq(current);
        }
        None => {
            commands.entity(entity).insert(current);
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
    /// Their ability, how it stands, and their passive.
    ability: Entity,
    ready: Entity,
    passive: Entity,
    effects: Entity,
}

fn spawn_row(commands: &mut Commands, assets: &AssetServer, player: &Player) -> Entity {
    let class = player.class;
    // One line each: a big haul or long name never wraps the row taller.
    let line = || TextLayout::no_wrap();
    // A pirate's state and link stay whole; their name gives way to them.
    let fixed = || Node { flex_shrink: 0.0, ..default() };
    let status = commands.spawn((hud::text("", 11.0, hud::DIM), line(), fixed())).id();
    let link = commands.spawn((hud::text("", 11.0, hud::DIM), line(), fixed())).id();
    let shield = commands.spawn(hud::meter(BAR_WIDTH, 4.0, SHIELD_COLOR)).id();
    let hull = commands.spawn(hud::meter_ticked(Val::Px(BAR_WIDTH), 7.0, HULL_COLOR, 4)).id();
    let credits = commands.spawn((hud::text("", 12.0, hud::GOLD), line(), fixed())).id();
    let ability = commands.spawn((hud::text("", 11.0, hud::TEXT), line())).id();
    let ready = commands.spawn((hud::text("", 11.0, hud::DIM), line())).id();
    let passive = commands.spawn((hud::text("", 11.0, hud::LABEL), line())).id();
    let effects = commands.spawn(hud::chip_row()).id();

    let spacer = || Node { flex_grow: 1.0, ..default() };
    let top = commands
        .spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() })
        .with_children(|line| {
            // The text can't clip itself: its box does, so a long name stops short.
            line.spawn(Node { min_width: Val::Px(LABEL_MIN), overflow: Overflow::clip_x(), ..default() })
                .with_child((hud::text(player.label(), 12.0, slot_color(player.slot)), TextLayout::no_wrap()));
            line.spawn((hud::text(class.name(), 13.0, class.color()), fixed()));
            line.spawn(spacer());
        })
        .add_children(&[status, link])
        .id();
    let bars = commands
        .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() })
        .add_children(&[shield, hull])
        .id();
    let middle = commands
        .spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() })
        .add_children(&[bars])
        .with_child(spacer())
        .add_children(&[credits])
        .id();
    let kit = commands
        .spawn(Node { column_gap: Val::Px(5.0), align_items: AlignItems::Center, ..default() })
        .add_children(&[ability, ready])
        .with_child(spacer())
        .add_children(&[passive])
        .id();
    let body = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            overflow: Overflow::clip_x(),
            row_gap: Val::Px(4.0),
            ..default()
        })
        .add_children(&[top, middle, kit, effects])
        .id();

    commands
        .spawn((
            hud::panel_class(
                Node {
                    width: Val::Px(ROW_WIDTH),
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::FlexStart,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    ..default()
                },
                class.color(),
            ),
            hud::wash(class.color()),
            children![hud::portrait(assets.load(class.sprite()), class.color(), 24.0)],
        ))
        .add_children(&[body])
        .insert(CrewRow {
            owner: player.owner,
            slot: player.slot,
            gone: None,
            status,
            link,
            shield,
            hull,
            credits,
            ability,
            ready,
            passive,
            effects,
        })
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
    players: Query<(&Player, &Health, Option<&Link>, &Boons, Option<&Rewards>, &Ability)>,
    mut rows: Query<(Entity, &mut CrewRow)>,
    mut chips: hud::ChipParts,
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
        let texts = &mut chips.2;
        let Some((player, health, link, boons, rewards, ability)) = player else {
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
            set_text(texts, row.status, "DISCONNECTED", BAD);
            set_text(texts, row.link, "", hud::DIM);
            hud::show_chips(&mut chips, row.effects, &[]);
            continue;
        };
        row.gone = None;

        let other_deck = mission.as_ref().is_some_and(|m| m.deck != player.deck);
        let (status, color) = if health.is_dead() {
            ("DOWN", BAD)
        } else {
            match player.status {
                // Their own credits, shown at the end of the row, are theirs to keep.
                PirateStatus::Extracted if player.left_early => ("LEFT EARLY", hud::GOLD),
                PirateStatus::Active if other_deck => ("OTHER DECK", hud::DIM),
                PirateStatus::Active => ("", hud::DIM),
                PirateStatus::Extracted => ("EXTRACTED", hud::GOLD),
            }
        };
        set_text(texts, row.status, status, color);

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
        set_text(texts, row.link, &link_text, link_color);

        if let Ok(mut meter) = meters.get_mut(row.shield) {
            meter.frac = if health.max_shield > 0.0 { health.shield / health.max_shield } else { 0.0 };
        }
        if let Ok(mut meter) = meters.get_mut(row.hull) {
            let frac = if health.is_dead() { 0.0 } else { health.current / health.max };
            meter.frac = frac;
            meter.color = if frac < 0.35 { HULL_LOW } else { HULL_COLOR };
        }
        set_text(&mut chips.2, row.credits, &hud::credits(player.credits), hud::GOLD);

        // What they can bring to bear: their ability and whether it's ready,
        // and their passive, lit while it's reaching anyone.
        let texts = &mut chips.2;
        let (name, state) = match player.active {
            Some(active) => match ability_status(active, ability, boons) {
                Some((state, ready)) => {
                    let color = active.stats().map_or(hud::TEXT, |s| s.color);
                    ((active.name(), if ready { color } else { hud::DIM }), (state, if ready { Color::WHITE } else { hud::DIM }))
                }
                None => ((active.name(), hud::DIM), (String::new(), hud::DIM)),
            },
            None => (("", hud::DIM), (String::new(), hud::DIM)),
        };
        set_text(texts, row.ability, name.0, name.1);
        set_text(texts, row.ready, &state.0, state.1);
        let reaching = passive_tag(player.passive, boons).is_some() && reaches_crew(player.passive);
        set_text(texts, row.passive, player.passive.name(), if reaching { player.class.color() } else { hud::LABEL });
        hud::show_chips(&mut chips, row.effects, &with_rewards(rewards, effects(health, boons)));
    }
}

/// Whether a passive works through the crew, so its tag says how many it reaches.
fn reaches_crew(passive: Passive) -> bool {
    !matches!(passive, Passive::Armor | Passive::Ricochet)
}

/// The parts of this machine's pirate's card that `update_local_card` keeps
/// in step: who they are in its header, and their passive under the ability.
#[derive(Resource)]
struct LocalCard {
    frame: Entity,
    portrait: Entity,
    class: Entity,
    slot: Entity,
    role: Entity,
    pill: Entity,
    passive: Entity,
    tag: Entity,
    details: Entity,
}

fn spawn_local_card(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    let portrait = commands.spawn((ImageNode::default(), Node { width: Val::Px(30.0), height: Val::Px(30.0), ..default() })).id();
    let frame = commands
        .spawn((
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                padding: UiRect::all(Val::Px(2.0)),
                border: UiRect { bottom: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(hud::EDGE),
        ))
        .add_children(&[portrait])
        .id();
    let class = commands.spawn(hud::text("", 17.0, hud::TEXT)).id();
    let slot = commands.spawn((hud::text("", 10.0, hud::TEXT), TextLayout::no_wrap())).id();
    let role = commands.spawn((hud::text("", 10.0, hud::DIM), TextLayout::no_wrap())).id();
    // One line each, cut off at the haul rather than wrapped under it.
    let who = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            min_width: Val::Px(0.0),
            flex_shrink: 1.0,
            overflow: Overflow::clip_x(),
            ..default()
        })
        .add_children(&[class])
        .with_children(|who| {
            who.spawn(Node { column_gap: Val::Px(6.0), ..default() }).add_children(&[slot, role]);
        })
        .id();
    commands
        .entity(hud::slot(&slots, HudSlot::Pirate))
        .insert(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, flex_grow: 1.0, min_width: Val::Px(0.0), ..default() })
        .add_children(&[frame, who]);

    let pill = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(4.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(hud::EDGE),
            children![hud::text("PASSIVE", 9.0, hud::LABEL)],
        ))
        .id();
    let passive = commands.spawn(hud::text("", 13.0, hud::TEXT)).id();
    let tag = commands.spawn(hud::text("", 11.0, hud::DIM)).id();
    let details = commands.spawn(hud::text("", 11.0, hud::DIM)).id();
    commands.entity(hud::slot(&slots, HudSlot::Passive)).with_children(|slot| {
        slot.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::Center, ..default() })
            .add_children(&[pill, passive])
            .with_child(Node { flex_grow: 1.0, ..default() })
            .add_children(&[tag]);
    }).add_children(&[details]);
    commands.insert_resource(LocalCard { frame, portrait, class, slot, role, pill, passive, tag, details });
}

/// Fits the card to this machine's pirate: their portrait, class and slot,
/// backed and washed in their class's colour, and how their passive is doing.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_local_card(
    assets: Res<AssetServer>,
    card: Res<LocalCard>,
    player: Option<Single<(&Player, &Boons), With<LocalPlayer>>>,
    mut shown: Local<Option<(PirateClass, Passive, u8)>>,
    panel: Single<Entity, With<PirateCard>>,
    mut header: Single<&mut BackgroundGradient, With<PirateHeader>>,
    mut borders: Query<&mut BorderColor>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut images: Query<&mut ImageNode>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let Some(player) = player else { return };
    let (player, boons) = *player;
    let (class, passive) = (player.class, player.passive);
    let tint = class.color();

    if *shown != Some((class, passive, player.slot)) {
        *shown = Some((class, passive, player.slot));
        if let Ok(mut border) = borders.get_mut(*panel) {
            border.set_if_neq(BorderColor { top: tint, ..BorderColor::all(hud::EDGE) });
        }
        if let Ok(mut background) = backgrounds.get_mut(*panel) {
            background.set_if_neq(BackgroundColor(hud::card_back(tint)));
        }
        header.0 = hud::wash(tint).0;
        if let Ok(mut border) = borders.get_mut(card.frame) {
            border.set_if_neq(BorderColor::all(tint.with_alpha(0.7)));
        }
        if let Ok(mut background) = backgrounds.get_mut(card.frame) {
            background.set_if_neq(BackgroundColor(tint.with_alpha(0.14)));
        }
        if let Ok(mut image) = images.get_mut(card.portrait) {
            image.image = assets.load(class.sprite());
        }
        if let Ok(mut border) = borders.get_mut(card.pill) {
            border.set_if_neq(BorderColor::all(tint.with_alpha(0.5)));
        }
        set_text(&mut texts, card.class, class.name(), tint);
        set_text(&mut texts, card.role, class.perk(), hud::DIM);
        set_text(&mut texts, card.passive, passive.name(), tint);
        set_text(&mut texts, card.details, passive.details(), hud::DIM);
    }
    // Out here because the next run's pirate can differ by their name alone.
    set_text(&mut texts, card.slot, &player.label(), slot_color(player.slot));
    // Lit in their colour while it's reaching anyone.
    let (tag, color) = match passive_tag(passive, boons) {
        Some(tag) => (tag, if reaches_crew(passive) { tint } else { hud::TEXT }),
        None => (String::new(), hud::DIM),
    };
    set_text(&mut texts, card.tag, &tag, color);
}

/// This machine's pirate's effects, under their passive in the top-left card.
#[derive(Component)]
struct LocalEffects;

fn spawn_local_effects(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Effects)).with_child((LocalEffects, hud::chip_row()));
}

fn update_local_effects(
    player: Option<Single<(&Player, &Health, &Boons, Option<&Rewards>), With<LocalPlayer>>>,
    row: Single<Entity, With<LocalEffects>>,
    mut chips: hud::ChipParts,
) {
    let list = player.map_or_else(Vec::new, |p| with_rewards(p.3, effects(p.1, p.2)));
    hud::show_chips(&mut chips, *row, &list);
}

/// Challenge rewards first, since they run out, then the rest of the effects.
fn with_rewards(rewards: Option<&Rewards>, effects: Vec<(String, Color)>) -> Vec<(String, Color)> {
    let mut chips = reward_chips(rewards);
    chips.extend(effects);
    chips
}

fn set_text(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, content: &str, color: Color) {
    let Ok((mut text, mut text_color)) = texts.get_mut(entity) else { return };
    if text.0 != content {
        text.0 = content.into();
    }
    text_color.set_if_neq(TextColor(color));
}

/// The class this machine last asked the host for from the picker.
#[derive(Resource, Default)]
struct PickedClass(Option<PirateClass>);

#[derive(Component)]
struct ClassPicker;

#[derive(Component)]
struct PickerHint;

/// A class on the picker; free ones are buttons.
#[derive(Component)]
struct ClassCard(PirateClass);

#[derive(Component)]
struct CardImage;

/// Who holds a card's class, or that it's free.
#[derive(Component)]
struct CardNote;

fn spawn_class_picker(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            ClassPicker,
            ZIndex(10),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|root| {
            root.spawn(hud::panel_accent(
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    ..default()
                },
                hud::GOLD,
            ))
            .with_children(|panel| {
                panel.spawn(hud::text("CLASS TAKEN", 28.0, hud::GOLD));
                panel.spawn((PickerHint, hud::text("", 15.0, hud::DIM)));
                panel
                    .spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(8.0)), ..default() })
                    .with_children(|row| {
                        for class in PirateClass::ALL {
                            row.spawn((
                                Button,
                                ClassCard(class),
                                Node {
                                    width: Val::Px(150.0),
                                    height: Val::Px(150.0),
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    row_gap: Val::Px(4.0),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(hud::PANEL),
                                BorderColor::all(hud::EDGE),
                            ))
                            .with_children(|card| {
                                card.spawn((
                                    CardImage,
                                    ImageNode::new(assets.load(class.sprite())),
                                    Node { width: Val::Px(56.0), height: Val::Px(56.0), ..default() },
                                ));
                                card.spawn(hud::text(class.name(), 18.0, class.color()));
                                card.spawn((CardNote, hud::text("", 12.0, hud::DIM), TextLayout::no_wrap()));
                            });
                        }
                    });
            });
        });
}

/// Shows the picker to a joiner whose class someone else holds, marks each
/// taken class with who took it, and asks the host for the one they click.
fn update_class_picker(
    mode: Res<NetMode>,
    state: Res<State<GameState>>,
    local: Res<LocalId>,
    loadout: Res<LocalLoadout>,
    mut picked: ResMut<PickedClass>,
    roster: Option<Single<&Roster>>,
    players: Query<&Player>,
    mut picker: Single<&mut Node, With<ClassPicker>>,
    mut hint: Single<&mut Text, (With<PickerHint>, Without<CardNote>)>,
    mut cards: Query<(&ClassCard, &Interaction, &Children, &mut BackgroundColor, &mut BorderColor)>,
    mut images: Query<&mut ImageNode, With<CardImage>>,
    mut notes: Query<(&mut Text, &mut TextColor), With<CardNote>>,
    mut messages: MessageWriter<ChooseLoadout>,
) {
    // The lobby has its own class cards; this is for joining mid-run.
    let joining = matches!(*mode, NetMode::Join { .. })
        && matches!(state.get(), GameState::Playing | GameState::Over);
    if !joining {
        picked.0 = None;
    }
    let wanted = picked.0.unwrap_or(loadout.0.class);
    let roster = roster.filter(|_| joining);
    let show = roster.as_ref().is_some_and(|roster| {
        !players.iter().any(|p| p.owner == local.0)
            && roster.holder(wanted).is_some_and(|owner| owner != local.0)
            && roster.class_of(local.0).is_none()
    });
    let display = if show { Display::Flex } else { Display::None };
    if picker.display != display {
        picker.display = display;
    }
    let Some(roster) = roster.filter(|_| show) else { return };

    let hint_text = format!("Someone already plays the {}. Pick another pirate.", wanted.name());
    if hint.0 != hint_text {
        hint.0 = hint_text;
    }
    for (card, interaction, children, mut background, mut border) in &mut cards {
        let class = card.0;
        let holder = roster.holder(class).filter(|owner| *owner != local.0);
        let (note, note_color) = match holder {
            None => ("FREE".to_string(), class.color()),
            Some(owner) => match roster.slot(owner).and_then(|slot| roster.label(slot)) {
                Some(label) => (format!("TAKEN  {label}"), hud::DIM),
                None => ("TAKEN".to_string(), hud::DIM),
            },
        };
        for child in children.iter() {
            if let Ok((mut text, mut color)) = notes.get_mut(child) {
                if text.0 != note {
                    text.0.clone_from(&note);
                }
                color.set_if_neq(TextColor(note_color));
            }
            if let Ok(mut image) = images.get_mut(child) {
                let tint = Color::WHITE.with_alpha(if holder.is_some() { 0.3 } else { 1.0 });
                if image.color != tint {
                    image.color = tint;
                }
            }
        }

        let free = holder.is_none();
        let (bg, edge) = match (free, interaction) {
            (false, _) => (hud::PANEL, hud::EDGE),
            (true, Interaction::None) => (hud::PANEL, class.color().with_alpha(0.4)),
            (true, _) => (Color::srgb(0.14, 0.15, 0.25), class.color()),
        };
        background.set_if_neq(BackgroundColor(bg));
        border.set_if_neq(BorderColor::all(edge));

        if free && *interaction == Interaction::Pressed && picked.0 != Some(class) {
            picked.0 = Some(class);
            let mut choice = loadout.0.clone();
            choice.set_class(class);
            messages.write(ChooseLoadout(choice));
        }
    }
}
