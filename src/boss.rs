//! The vault's guardians: a hulking boss waiting in every sealed vault room
//! (see `breach.rs`), one of two kinds taking turns down the decks. The vault
//! keeper fights with two floating hands, each sweeping lasers and hurling
//! nova orbs; badly hurt, it enrages and both hands attack together. The
//! vault stalker marks up to three pirates, then lunges through them one after
//! another. Each one further down is tougher and hits harder (see `might`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::class::RiotShield;
use crate::combat::{FxKind, Health, Hurtbox, Knockback, ProjectileLook, Team, fx, hits_shield, shield_between, shield_outlines};
use crate::drops::{EnemyDrop, pick_ammo, spawn_drops};
use crate::guns::Arsenal;
use crate::mission::Banner;
use crate::net::authority;
use crate::perks::{PerkKind, spawn_pickup};
use crate::player::{PirateStatus, Player};
use crate::room::RoomGrid;
use crate::vault::VaultTier;
use crate::{GameState, Level, Rng};

pub const KEEPER_NAME: &str = "VAULT KEEPER";

/// The body's hit radius: a big target.
const KEEPER_HURTBOX: f32 = 15.0;
/// The body's collision half-extent against walls.
const KEEPER_HALF: f32 = 9.0;
/// How fast the body drifts, in pixels per second.
const DRIFT_SPEED: f32 = 22.0;
/// The body backs off from pirates closer than this, and closes in on them past `FAR`.
const NEAR: f32 = 70.0;
const FAR: f32 = 120.0;
/// How far the body strays from where it woke before it's pulled back.
const LEASH: f32 = 56.0;
/// Pirates it can see within this distance are fair game.
const SIGHT: f32 = 280.0;
/// Where each hand floats, for the right hand (the left mirrors it).
const HAND_OFFSET: Vec2 = Vec2::new(30.0, 4.0);
/// Where each hand rests while it sleeps.
const HAND_REST: Vec2 = Vec2::new(24.0, -8.0);
/// How far a hand bobs up and down, and how fast.
const HAND_BOB: f32 = 3.0;
const HAND_BOB_SPEED: f32 = 2.4;
/// Steps `KeeperHand::aim` turns in, in radians, to limit replication traffic.
const AIM_STEP: f32 = 0.05;
/// Steps `KeeperBeam::dir` turns in: fine enough for a smooth sweep.
const BEAM_STEP: f32 = 0.01;

/// Seconds after waking before the first attack.
const WAKE_PAUSE: f32 = 1.6;
/// Seconds between attacks, plus up to `GAP_JITTER` more. Calm, the gap only
/// runs while both hands are idle; enraged, it runs all along, so both attack at once.
const GAP: f32 = 0.8;
const ENRAGED_GAP: f32 = 0.65;
const GAP_JITTER: f32 = 0.5;
/// Share of its health left when it enrages.
const ENRAGE_AT: f32 = 0.5;

/// A laser's warning line tracks its target this long, then swings to where
/// the sweep starts and holds there for `AIM_LOCK` before it burns.
const AIM_TRACK: f32 = 0.7;
const AIM_LOCK: f32 = 0.25;
/// How long a beam burns, and how far it sweeps across where its target was.
const FIRE_TIME: f32 = 1.5;
const ENRAGED_FIRE_TIME: f32 = 1.2;
const SWEEP: f32 = 50.0 * std::f32::consts::PI / 180.0;
const ENRAGED_SWEEP: f32 = 75.0 * std::f32::consts::PI / 180.0;
const BEAM_RANGE: f32 = 320.0;
const BEAM_HALF_WIDTH: f32 = 4.0;
/// A beam's damage each time it burns someone, at most every `BEAM_TICK` seconds each.
const BEAM_HIT: f32 = 18.0;
const BEAM_TICK: f32 = 0.5;
/// How far in front of the hand the beam and orbs start.
const PALM: f32 = 6.0;

/// Seconds gathering a nova orb, then recoiling after letting it go.
const CHARGE_TIME: f32 = 1.0;
const ENRAGED_CHARGE_TIME: f32 = 0.8;
const RECOIL_TIME: f32 = 0.35;
/// Enraged, it hurls three orbs this many radians apart.
const ORB_SPREAD: f32 = 0.35;
const ORB_SPEED: f32 = 85.0;
const ORB_RANGE: f32 = 340.0;
/// Damage at the centre of an orb's burst, halving toward the edge.
const ORB_DAMAGE: f32 = 38.0;
const ORB_RADIUS: f32 = 30.0;
const ORB_KNOCKBACK: f32 = 160.0;
/// Damage of each arc to a pirate the orb passes, how far arcs reach, and how often.
const ORB_ARC_DAMAGE: f32 = 10.0;
const ORB_ARC_REACH: f32 = 40.0;
const ORB_ARC_GAP: f32 = 0.3;
/// The orb is drawn bigger than a pirate's.
const ORB_SCALE: f32 = 1.3;

/// The vault stalker's pace while prowling, and the distance it circles the
/// nearest pirate at: it closes in past `PROWL_FAR` and backs off inside `PROWL_NEAR`.
const PROWL_SPEED: f32 = 48.0;
const PROWL_NEAR: f32 = 40.0;
const PROWL_FAR: f32 = 80.0;
/// Seconds it prowls between lunges, plus up to `PROWL_JITTER` more.
const PROWL_TIME: f32 = 3.4;
const ENRAGED_PROWL_TIME: f32 = 2.3;
const PROWL_JITTER: f32 = 0.6;
/// Seconds crouched with its marks showing before it goes.
const WIND_UP: f32 = 0.95;
const ENRAGED_WIND_UP: f32 = 0.75;
/// The most pirates one lunge goes through.
const MAX_MARKS: usize = 3;
/// How fast it lunges, the longest it spends on any one leg, and the steps it
/// takes so it can't skip past a pirate or a shield.
const LUNGE_SPEED: f32 = 420.0;
const LEG_TIME: f32 = 0.6;
const LUNGE_STEP: f32 = 3.0;
/// Pirates this close to its body's edge as it lunges by are hit.
const LUNGE_REACH: f32 = 4.0;
/// A lunge's damage to each pirate it goes through (once each), and how hard it throws them.
const LUNGE_HIT: f32 = 34.0;
const LUNGE_KNOCKBACK: f32 = 200.0;
/// Seconds spent after a lunge, open to punishment; longer if a riot shield
/// stopped it, throwing it back `REBOUND` pixels.
const RECOVER: f32 = 1.2;
const STAGGER: f32 = 1.8;
const REBOUND: f32 = 8.0;

/// Credit drops it leaves behind, sharing its bounty, and ammo boxes.
const BOUNTY_DROPS: u32 = 10;
const BOUNTY_AMMO: u32 = 2;
pub const KEEPER_COLOR: Color = Color::srgb(1.0, 0.3, 0.5);

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (drive_keeper, drive_stalker, move_keeper_orbs, keeper_deaths)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(authority),
        );
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandSide {
    Left,
    Right,
}

impl HandSide {
    /// -1 for the left hand, 1 for the right.
    pub fn sign(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// What a hand is doing, for the view.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum HandAction {
    /// Floating beside the body between attacks.
    Idle,
    /// Winding up a laser; its `KeeperBeam` shows as a warning line.
    LaserAim,
    /// Its beam is burning.
    LaserFire,
    /// Gathering a nova orb in its palm; `charge` runs 0..1 (in steps of 0.1).
    NovaCharge { charge: f32 },
    /// Just let an orb go: the palm recoils for a moment.
    NovaRecoil,
}

/// Which guardian waits in a vault room.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeeperKind {
    /// The vault keeper: two floating hands sweeping lasers and hurling nova orbs.
    Hands,
    /// A hulking, buffed-up stalker: winds up, then lunges through up to three
    /// pirates in a row, with a moderate cooldown between.
    Stalker,
}

impl KeeperKind {
    pub const ALL: [KeeperKind; 2] = [Self::Hands, Self::Stalker];

    /// The guardian of `deck`'s vault in the heist from `seed`. The kinds take
    /// turns down the decks, so a heist always meets both.
    pub fn pick(seed: u32, deck: u8) -> Self {
        Self::ALL[(seed as usize + deck as usize) % Self::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hands => KEEPER_NAME,
            Self::Stalker => "VAULT STALKER",
        }
    }

    /// How it fights, in a word, for the enemies screen.
    pub fn role(self) -> &'static str {
        match self {
            Self::Hands => "BOSS - RANGED",
            Self::Stalker => "BOSS - MELEE",
        }
    }

    /// What to expect from it, for the enemies screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::Hands => "Guards a vault. Two floating hands sweep lasers and hurl nova orbs; badly hurt, both attack at once.",
            Self::Stalker => "Guards a vault. Marks up to three pirates, then lunges through them one after another. Spread out.",
        }
    }

    pub fn sprite(self) -> &'static str {
        match self {
            Self::Hands => "sprites/boss/keeper.png",
            Self::Stalker => "sprites/boss/stalker.png",
        }
    }
}

/// A vault's guardian's body, whatever its kind. Carries `Team::Enemy`,
/// `Health` and a `Hurtbox`, so the crew's weapons hurt it. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Keeper {
    pub kind: KeeperKind,
    pub tier: VaultTier,
    /// Asleep until the crew is let into its room.
    pub awake: bool,
    /// Badly hurt: the keeper's hands attack together, and faster; the
    /// stalker lunges sooner and winds up quicker.
    pub enraged: bool,
}

impl Keeper {
    /// Wakes it as `crew` pirates are let in, sizing its health to them.
    pub fn wake(&mut self, health: &mut Health, crew: usize) {
        *health = Health::new(keeper_health(self.kind, self.tier, crew), 0.0);
        self.awake = true;
    }
}

/// How much tougher and harder-hitting a vault's guardian is than Silver's.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Might {
    /// Scales its health.
    health: f32,
    /// Scales every hit it lands: beams, orbs, arcs and lunges.
    power: f32,
}

/// How far past the trend the last vault's guardian is pushed, on purpose.
const OVERTUNE: f32 = 1.4;

/// Each guardian further down has a bit more health and hits a bit harder,
/// whatever its kind. The trend would put Platinum's at 1.5 health and 1.3
/// power, but the last vault's guardian is deliberately overtuned, `OVERTUNE`
/// past that, so the richest vault is a brutal fight.
fn might(tier: VaultTier) -> Might {
    match tier {
        VaultTier::Silver => Might { health: 1.0, power: 1.0 },
        VaultTier::Gold => Might { health: 1.25, power: 1.15 },
        VaultTier::Platinum => Might { health: 1.5 * OVERTUNE, power: 1.3 * OVERTUNE },
    }
}

/// The guardian's health for a vault of `tier` and `crew` pirates let in: a
/// lot for one pirate, and more for each crewmate, though not quite their share.
fn keeper_health(kind: KeeperKind, tier: VaultTier, crew: usize) -> f32 {
    let base = match kind {
        KeeperKind::Hands => 1800.0,
        KeeperKind::Stalker => 1800.0,
    };
    base * might(tier).health * (0.4 + 0.6 * crew.max(1) as f32)
}

/// The credits it leaves behind for a vault of `tier`.
fn keeper_bounty(tier: VaultTier) -> u32 {
    match tier {
        VaultTier::Silver => 900,
        VaultTier::Gold => 1800,
        VaultTier::Platinum => 3200,
    }
}

/// One of the keeper's two hands; its `Transform` is where it floats. There's
/// only ever one keeper on a deck. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct KeeperHand {
    pub side: HandSide,
    pub action: HandAction,
    /// Which way its palm faces.
    pub aim: Vec2,
}

/// What a Vault Stalker is doing, for the view.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StalkerState {
    /// Circling, waiting out its cooldown.
    Prowl,
    /// Crouched, winding up: its `marks` show who it'll go through.
    WindUp,
    /// Lunging through its marks, `hit` of them reached so far.
    Lunge { hit: u8 },
    /// Spent after a lunge: a moment to punish it.
    Recover,
}

/// Beside a Vault Stalker's `Keeper`: what it's doing and, while it winds up
/// and lunges, where the pirates it marked stood, in the order it goes
/// through them. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StalkerLook {
    pub state: StalkerState,
    pub marks: Vec<Vec2>,
}

/// A hand's laser: a warning line while it aims, then the burning beam, from
/// `from` along `dir` until the first wall (or riot shield) `length` away.
/// Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct KeeperBeam {
    pub side: HandSide,
    pub from: Vec2,
    pub dir: Vec2,
    pub length: f32,
    pub firing: bool,
}

/// Host only: where the body wanders and when it next attacks.
#[derive(Component)]
struct KeeperBrain {
    /// Where it woke; it never strays far from here.
    home: Vec2,
    /// Seconds until the next attack may start.
    cooldown: f32,
    /// The hand whose turn it is.
    next: HandSide,
    /// Seconds awake, driving the bob.
    age: f32,
    velocity: Vec2,
    /// Which way it circles the crew, flipped now and then.
    strafe: f32,
    strafe_timer: f32,
}

/// Host only: a hand's attack in progress.
#[derive(Component, Default)]
struct HandBrain {
    /// Seconds into its current action.
    timer: f32,
    /// The pirate it's after, and where they were last seen.
    target: Option<Entity>,
    mark: Vec2,
    /// A laser's angle through the target, and its signed sweep across it.
    lock: f32,
    sweep: f32,
    fire_time: f32,
    beam: Option<Entity>,
    /// Who the beam burned, and seconds before it may burn them again.
    burned: Vec<(Entity, f32)>,
}

/// Host only: a hand's nova orb. Drifts straight on, lashing pirates it passes
/// with arcs, and bursts on the first wall, pirate or riot shield it meets.
#[derive(Component)]
struct KeeperOrb {
    velocity: Vec2,
    remaining: f32,
    /// Seconds until it lashes out again.
    arc_timer: f32,
    /// Scales its arcs and burst: its keeper's `Might::power`.
    power: f32,
}

/// Host only: the vault stalker's hidden side; what it's doing is its `StalkerLook`.
#[derive(Component)]
struct StalkerBrain {
    /// Seconds into its current state, or its current lunge's leg.
    timer: f32,
    /// Seconds of prowling left before it may wind up again.
    cooldown: f32,
    /// How long this recovery lasts: longer if a riot shield stopped it.
    recover: f32,
    velocity: Vec2,
    /// Which way it circles the crew, flipped now and then.
    strafe: f32,
    strafe_timer: f32,
    /// Who this lunge has hit already: nobody's hit twice by one.
    hits: Vec<Entity>,
}

/// Host only: puts the sleeping guardian of `deck`'s vault at `pos`: the
/// keeper with its two hands, or the stalker.
pub fn spawn_keeper(commands: &mut Commands, pos: Vec2, deck: u8, kind: KeeperKind) {
    let tier = VaultTier::for_deck(deck);
    let body = commands
        .spawn((
            Level,
            Replicated,
            Keeper { kind, tier, awake: false, enraged: false },
            Team::Enemy,
            Health::new(keeper_health(kind, tier, 1), 0.0),
            Hurtbox(KEEPER_HURTBOX),
            Knockback::default(),
            Transform::from_translation(pos.extend(8.0)),
        ))
        .id();
    if kind == KeeperKind::Stalker {
        commands.entity(body).insert((
            StalkerBrain {
                timer: 0.0,
                cooldown: WAKE_PAUSE,
                recover: RECOVER,
                velocity: Vec2::ZERO,
                strafe: 1.0,
                strafe_timer: 3.0,
                hits: Vec::new(),
            },
            StalkerLook { state: StalkerState::Prowl, marks: Vec::new() },
        ));
        return;
    }
    commands.entity(body).insert(KeeperBrain {
        home: pos,
        cooldown: WAKE_PAUSE,
        next: HandSide::Right,
        age: 0.0,
        velocity: Vec2::ZERO,
        strafe: 1.0,
        strafe_timer: 3.0,
    });
    for side in [HandSide::Left, HandSide::Right] {
        let at = pos + HAND_REST * Vec2::new(side.sign(), 1.0);
        commands.spawn((
            Level,
            Replicated,
            KeeperHand { side, action: HandAction::Idle, aim: Vec2::NEG_Y },
            HandBrain::default(),
            Transform::from_translation(at.extend(9.0)),
        ));
    }
}

/// A pirate (or sentry) on the crew's side, alive and in play.
fn in_play(team: &Team, health: &Health, player: Option<&Player>) -> bool {
    *team == Team::Player && !health.is_dead() && player.is_none_or(|p| p.status == PirateStatus::Active)
}

/// Snaps a direction to steps of `step` radians, so it only changes, and
/// replicates, when it turns noticeably.
fn quantize(dir: Vec2, step: f32) -> Vec2 {
    Vec2::from_angle((dir.to_angle() / step).round() * step)
}

/// The laser's angle `t` (0..1) of the way through its sweep: it passes
/// `lock`, where the target was, halfway through.
fn sweep_angle(lock: f32, sweep: f32, t: f32) -> f32 {
    lock + sweep * (t.clamp(0.0, 1.0) - 0.5)
}

/// Which way the body wants to drift, at most unit length: away from pirates
/// too close, toward them if too far, circling `strafe`-wise at a fighting
/// distance, and back toward `home` once past the leash.
fn drift_dir(pos: Vec2, home: Vec2, crew: &[Vec2], strafe: f32) -> Vec2 {
    let mut want = Vec2::ZERO;
    if let Some(nearest) = crew.iter().min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos))) {
        let offset = pos - *nearest;
        let away = offset.normalize_or(Vec2::Y);
        if offset.length() < NEAR {
            want += away;
        } else if offset.length() > FAR {
            want -= away * 0.6;
        }
        want += away.perp() * strafe * 0.5;
    } else {
        want += (home - pos).clamp_length_max(16.0) / 16.0;
    }
    let from_home = pos - home;
    if from_home.length() > LEASH {
        want -= from_home.normalize_or_zero() * ((from_home.length() - LEASH) / 16.0).min(2.0);
    }
    want.clamp_length_max(1.0)
}

/// How far a beam from `from` along `dir` reaches before a wall or a riot shield stops it.
fn beam_length(grid: &RoomGrid, shields: &[[Vec2; crate::class::RIOT_POINTS]], from: Vec2, dir: Vec2) -> f32 {
    let mut length = 0.0;
    while length < BEAM_RANGE {
        let at = from + dir * (length + 2.0);
        if grid.is_solid_at(at) || hits_shield(shields, at) {
            break;
        }
        length += 2.0;
    }
    length
}

/// Which way the stalker wants to prowl, at most unit length: in on the
/// nearest pirate if too far, away if too close, always circling `strafe`-wise.
fn prowl_dir(pos: Vec2, crew: &[Vec2], strafe: f32) -> Vec2 {
    let Some(nearest) = crew.iter().min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos))) else {
        return Vec2::ZERO;
    };
    let offset = pos - *nearest;
    let away = offset.normalize_or(Vec2::Y);
    let mut want = away.perp() * strafe * 0.6;
    if offset.length() < PROWL_NEAR {
        want += away;
    } else if offset.length() > PROWL_FAR {
        want -= away;
    }
    want.clamp_length_max(1.0)
}

/// Which of the pirates at `crew` a lunge from `pos` goes through, as indices
/// in the order it reaches them: the `MAX_MARKS` nearest, each next the
/// closest to the last.
fn pick_marks(pos: Vec2, crew: &[Vec2]) -> Vec<usize> {
    let mut nearest: Vec<usize> = (0..crew.len()).collect();
    nearest.sort_by(|&a, &b| crew[a].distance(pos).total_cmp(&crew[b].distance(pos)));
    nearest.truncate(MAX_MARKS);
    let mut order = Vec::with_capacity(nearest.len());
    let mut from = pos;
    while !nearest.is_empty() {
        let (i, _) = nearest
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| crew[**a].distance(from).total_cmp(&crew[**b].distance(from)))
            .unwrap();
        let next = nearest.remove(i);
        from = crew[next];
        order.push(next);
    }
    order
}

/// Keeps a body of half-extent `half` on the vault room's floor, `room`.
fn in_room(room: Rect, half: Vec2, p: Vec2) -> Vec2 {
    if room.width() <= half.x * 2.0 || room.height() <= half.y * 2.0 {
        return p;
    }
    p.clamp(room.min + half, room.max - half)
}

/// Wakes, moves and fights the keeper: the body drifts within its room, the
/// hands float beside it and take turns attacking, and its beams burn.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_keeper(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut banner: ResMut<Banner>,
    mut keepers: Query<(&mut Keeper, &Health, &mut Transform, &mut Knockback, &mut KeeperBrain), Without<KeeperHand>>,
    mut hands: Query<(&mut KeeperHand, &mut HandBrain, &mut Transform), Without<Keeper>>,
    mut beams: Query<(&mut KeeperBeam, &mut Transform), (Without<Keeper>, Without<KeeperHand>)>,
    mut crew: Query<
        (Entity, &Team, &Hurtbox, &Transform, &mut Health, Option<&Player>),
        (Without<Keeper>, Without<KeeperHand>, Without<KeeperBeam>),
    >,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Keeper>, Without<KeeperHand>, Without<KeeperBeam>)>,
) {
    let Ok((mut keeper, health, mut body, mut knockback, mut brain)) = keepers.single_mut() else { return };
    let dt = time.delta_secs();
    // Nothing pushes it around.
    if knockback.0 != Vec2::ZERO {
        knockback.0 = Vec2::ZERO;
    }
    let pos = body.translation.truncate();

    if !keeper.awake {
        // Hands resting low at its sides, palms down.
        for (mut hand, _, mut transform) in &mut hands {
            let at = pos + HAND_REST * Vec2::new(hand.side.sign(), 1.0);
            transform.set_if_neq(Transform::from_translation(at.extend(9.0)));
            let side = hand.side;
            hand.set_if_neq(KeeperHand { side, action: HandAction::Idle, aim: Vec2::NEG_Y });
        }
        return;
    }
    brain.age += dt;

    if !keeper.enraged && health.current <= health.max * ENRAGE_AT {
        keeper.enraged = true;
        fx(&mut commands, FxKind::Explosion, pos, 0.0, KEEPER_COLOR);
        banner.show(format!("THE {} IS ENRAGED", keeper.kind.name()), KEEPER_COLOR, 3.0);
    }
    let enraged = keeper.enraged;
    let power = might(keeper.tier).power;

    // The pirates it can see.
    let targets: Vec<(Entity, Vec2)> = crew
        .iter()
        .filter(|(_, team, _, t, health, player)| {
            let at = t.translation.truncate();
            player.is_some() && in_play(team, health, *player) && at.distance(pos) <= SIGHT && grid.line_of_sight(pos, at)
        })
        .map(|(e, _, _, t, ..)| (e, t.translation.truncate()))
        .collect();
    let spots: Vec<Vec2> = targets.iter().map(|(_, at)| *at).collect();

    // Drift, keeping its distance and its room.
    brain.strafe_timer -= dt;
    if brain.strafe_timer <= 0.0 {
        brain.strafe = -brain.strafe;
        brain.strafe_timer = 3.0 + rng.unit() * 2.0;
    }
    let want = drift_dir(pos, brain.home, &spots, brain.strafe) * DRIFT_SPEED;
    brain.velocity = brain.velocity.lerp(want, (3.0 * dt).min(1.0));
    let new = grid.slide(pos, brain.velocity * dt, Vec2::splat(KEEPER_HALF));
    if new != pos {
        body.translation.x = new.x;
        body.translation.y = new.y;
    }
    let pos = new;

    // Start the next attack once the gap runs out: calm, the gap waits for
    // both hands to be idle; enraged, it runs regardless.
    let idle: Vec<HandSide> = hands
        .iter()
        .filter(|(h, ..)| h.action == HandAction::Idle)
        .map(|(h, ..)| h.side)
        .collect();
    if enraged || idle.len() == 2 {
        brain.cooldown -= dt;
    }
    let starting = (brain.cooldown <= 0.0 && !targets.is_empty())
        .then(|| {
            if idle.contains(&brain.next) {
                Some(brain.next)
            } else {
                idle.first().copied()
            }
        })
        .flatten();
    if let Some(side) = starting {
        brain.next = side.other();
        brain.cooldown = if enraged { ENRAGED_GAP } else { GAP } + rng.unit() * GAP_JITTER;
    }

    let shields = shield_outlines(&riot_shields);
    for (mut hand, mut hb, mut transform) in &mut hands {
        let side = hand.side;
        let sign = side.sign();
        let mut action = hand.action;
        let mut aim = hand.aim;

        if starting == Some(side) {
            let (target, at) = targets[(rng.unit() * targets.len() as f32) as usize % targets.len()];
            hb.timer = 0.0;
            hb.target = Some(target);
            hb.mark = at;
            hb.burned.clear();
            hb.sweep = 0.0;
            if rng.unit() < 0.5 {
                action = HandAction::LaserAim;
            } else {
                action = HandAction::NovaCharge { charge: 0.0 };
            }
        }

        // Keep an eye on the target while it's in sight.
        if let Some(target) = hb.target
            && let Some((_, at)) = targets.iter().find(|(e, _)| *e == target)
        {
            let tracking = match action {
                HandAction::LaserAim => hb.timer < AIM_TRACK,
                HandAction::NovaCharge { .. } => true,
                _ => false,
            };
            if tracking {
                hb.mark = *at;
            }
        }

        // Float beside the body, bobbing; a recoiling palm is thrown back.
        let bob = HAND_BOB * (brain.age * HAND_BOB_SPEED + sign).sin();
        let mut hand_pos = pos + HAND_OFFSET * Vec2::new(sign, 1.0) + Vec2::Y * bob;
        if action == HandAction::NovaRecoil {
            hand_pos -= aim * 4.0 * (1.0 - hb.timer / RECOIL_TIME);
        }
        let hand_pos = (hand_pos * 2.0).round() / 2.0;
        let palm_dir = (hb.mark - hand_pos).normalize_or(aim);
        let from = hand_pos + palm_dir * PALM;

        hb.timer += dt;
        match action {
            HandAction::Idle => {
                if let Some(nearest) = spots.iter().min_by(|a, b| a.distance(hand_pos).total_cmp(&b.distance(hand_pos))) {
                    aim = (*nearest - hand_pos).normalize_or(aim);
                }
            }
            HandAction::LaserAim | HandAction::LaserFire => {
                let firing = action == HandAction::LaserFire;
                if !firing && hb.timer >= AIM_TRACK && hb.sweep == 0.0 {
                    // Lock on where the target was, and pick which way to sweep.
                    let turn = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
                    hb.lock = (hb.mark - hand_pos).normalize_or(aim).to_angle();
                    hb.sweep = turn * if enraged { ENRAGED_SWEEP } else { SWEEP };
                    hb.fire_time = if enraged { ENRAGED_FIRE_TIME } else { FIRE_TIME };
                }
                let angle = if firing {
                    sweep_angle(hb.lock, hb.sweep, hb.timer / hb.fire_time)
                } else if hb.sweep != 0.0 {
                    // Locked: the warning line waits where the sweep will start.
                    sweep_angle(hb.lock, hb.sweep, 0.0)
                } else {
                    palm_dir.to_angle()
                };
                let dir = quantize(Vec2::from_angle(angle), BEAM_STEP);
                aim = dir;
                let from = hand_pos + dir * PALM;
                let length = beam_length(&grid, &shields, from, dir).round();
                let line = KeeperBeam { side, from, dir, length, firing };
                let placed = Transform::from_translation(from.extend(12.0)).with_rotation(Quat::from_rotation_z(dir.to_angle()));
                match hb.beam.map(|b| beams.get_mut(b)) {
                    Some(Ok((mut beam, mut beam_transform))) => {
                        beam.set_if_neq(line);
                        beam_transform.set_if_neq(placed);
                    }
                    // Spawned last frame: not in the query until its commands apply.
                    Some(Err(_)) => {}
                    None => hb.beam = Some(commands.spawn((Level, Replicated, line, placed)).id()),
                }

                if firing {
                    // Burns everyone on the crew's side it touches.
                    for (_, left) in &mut hb.burned {
                        *left -= dt;
                    }
                    hb.burned.retain(|(_, left)| *left > 0.0);
                    for (entity, team, hurtbox, t, mut health, player) in &mut crew {
                        let at = t.translation.truncate();
                        let along = (at - from).dot(dir).clamp(0.0, length);
                        let in_beam = at.distance(from + dir * along) <= hurtbox.0 + BEAM_HALF_WIDTH;
                        if in_beam
                            && in_play(team, &health, player)
                            && !hb.burned.iter().any(|(e, _)| *e == entity)
                            && !shield_between(&shields, from, at)
                            && health.hurt(BEAM_HIT * power)
                        {
                            hb.burned.push((entity, BEAM_TICK));
                            fx(&mut commands, FxKind::Spark, at, 0.0, KEEPER_COLOR);
                        }
                    }
                    if hb.timer >= hb.fire_time {
                        if let Some(beam) = hb.beam.take() {
                            commands.entity(beam).despawn();
                        }
                        fx(&mut commands, FxKind::Burst, from, 0.0, KEEPER_COLOR);
                        action = HandAction::Idle;
                        hb.sweep = 0.0;
                        hb.target = None;
                    }
                } else if hb.timer >= AIM_TRACK + AIM_LOCK {
                    action = HandAction::LaserFire;
                    hb.timer = 0.0;
                    fx(&mut commands, FxKind::MuzzleFlash, from, dir.to_angle(), KEEPER_COLOR);
                }
            }
            HandAction::NovaCharge { .. } => {
                aim = palm_dir;
                let charge_time = if enraged { ENRAGED_CHARGE_TIME } else { CHARGE_TIME };
                let charge = ((hb.timer / charge_time * 10.0).floor() / 10.0).min(1.0);
                action = HandAction::NovaCharge { charge };
                if hb.timer >= charge_time {
                    let spread: &[f32] = if enraged { &[-ORB_SPREAD, 0.0, ORB_SPREAD] } else { &[0.0] };
                    for offset in spread {
                        let dir = Vec2::from_angle(palm_dir.to_angle() + offset);
                        commands.spawn((
                            Level,
                            Replicated,
                            KeeperOrb { velocity: dir * ORB_SPEED, remaining: ORB_RANGE, arc_timer: ORB_ARC_GAP, power },
                            ProjectileLook::NovaOrb,
                            Transform::from_translation(from.extend(15.0)).with_scale(Vec3::splat(ORB_SCALE)),
                        ));
                    }
                    fx(&mut commands, FxKind::Burst, from, 0.0, KEEPER_COLOR);
                    action = HandAction::NovaRecoil;
                    hb.timer = 0.0;
                }
            }
            HandAction::NovaRecoil => {
                if hb.timer >= RECOIL_TIME {
                    action = HandAction::Idle;
                    hb.target = None;
                }
            }
        }

        transform.set_if_neq(Transform::from_translation(hand_pos.extend(9.0)));
        let aim = quantize(aim, AIM_STEP);
        hand.set_if_neq(KeeperHand { side, action, aim });
    }
}

/// Wakes, moves and fights the stalker: it prowls around the crew, winds up
/// marking up to three of them, lunges through each mark in turn, then stands
/// spent for a moment. A riot shield in its way stops a lunge dead.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_stalker(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut banner: ResMut<Banner>,
    mut stalkers: Query<(&mut Keeper, &Health, &mut Transform, &mut Knockback, &mut StalkerBrain, &mut StalkerLook)>,
    mut crew: Query<
        (Entity, &Team, &Hurtbox, &Transform, &mut Health, Option<&Player>, Option<&mut Knockback>),
        Without<Keeper>,
    >,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Keeper>, Without<Hurtbox>)>,
) {
    let Ok((mut keeper, health, mut body, mut knockback, mut brain, mut look)) = stalkers.single_mut() else { return };
    let dt = time.delta_secs();
    // Nothing pushes it around.
    if knockback.0 != Vec2::ZERO {
        knockback.0 = Vec2::ZERO;
    }
    if !keeper.awake {
        look.set_if_neq(StalkerLook { state: StalkerState::Prowl, marks: Vec::new() });
        return;
    }
    let start = body.translation.truncate();
    let mut pos = start;

    if !keeper.enraged && health.current <= health.max * ENRAGE_AT {
        keeper.enraged = true;
        fx(&mut commands, FxKind::Explosion, pos, 0.0, KEEPER_COLOR);
        banner.show(format!("THE {} IS ENRAGED", keeper.kind.name()), KEEPER_COLOR, 3.0);
    }
    let enraged = keeper.enraged;
    let power = might(keeper.tier).power;
    let half = Vec2::splat(KEEPER_HALF);
    let room = grid.vault_room;
    let shields = shield_outlines(&riot_shields);

    // The pirates it can see.
    let spots: Vec<Vec2> = crew
        .iter()
        .filter(|(_, team, _, t, health, player, _)| {
            let at = t.translation.truncate();
            player.is_some() && in_play(team, health, *player) && at.distance(pos) <= SIGHT && grid.line_of_sight(pos, at)
        })
        .map(|(_, _, _, t, ..)| t.translation.truncate())
        .collect();

    let mut state = look.state;
    let mut marks = look.marks.clone();
    brain.timer += dt;
    match state {
        StalkerState::Prowl => {
            brain.strafe_timer -= dt;
            if brain.strafe_timer <= 0.0 {
                brain.strafe = -brain.strafe;
                brain.strafe_timer = 2.0 + rng.unit() * 2.0;
            }
            let want = prowl_dir(pos, &spots, brain.strafe) * PROWL_SPEED;
            brain.velocity = brain.velocity.lerp(want, (4.0 * dt).min(1.0));
            pos = in_room(room, half, grid.slide(pos, brain.velocity * dt, half));
            brain.cooldown -= dt;
            if brain.cooldown <= 0.0 && !spots.is_empty() {
                // Locked in now, so whoever sidesteps in time is missed.
                marks = pick_marks(pos, &spots).into_iter().map(|i| in_room(room, half, spots[i]).round()).collect();
                state = StalkerState::WindUp;
                brain.timer = 0.0;
                brain.velocity = Vec2::ZERO;
            }
        }
        StalkerState::WindUp => {
            if brain.timer >= if enraged { ENRAGED_WIND_UP } else { WIND_UP } {
                state = StalkerState::Lunge { hit: 0 };
                brain.timer = 0.0;
                brain.hits.clear();
                fx(&mut commands, FxKind::Burst, pos, 0.0, KEEPER_COLOR);
            }
        }
        StalkerState::Lunge { hit } => {
            let mark = marks.get(hit as usize).copied().unwrap_or(pos);
            let mut left = LUNGE_SPEED * dt;
            let mut reached = false;
            let mut blocked = None;
            while left > 0.0 {
                let to = mark - pos;
                if to.length() < 0.5 {
                    reached = true;
                    break;
                }
                let dir = to.normalize();
                let step = left.min(LUNGE_STEP).min(to.length());
                // A raised riot shield in its way stops it dead.
                let front = pos + dir * (KEEPER_HALF + step);
                if hits_shield(&shields, front) || shield_between(&shields, pos, front) {
                    blocked = Some(dir);
                    break;
                }
                let next = in_room(room, half, grid.slide(pos, dir * step, half));
                if next.distance(pos) < step * 0.5 {
                    // Up against a wall: as far as this leg goes.
                    reached = true;
                    break;
                }
                pos = next;
                left -= step;
                // Hits everyone on the crew's side it touches, once each per lunge.
                for (entity, team, hurtbox, t, mut victim, player, victim_knockback) in &mut crew {
                    let at = t.translation.truncate();
                    if brain.hits.contains(&entity)
                        || !in_play(team, &victim, player)
                        || at.distance(pos) > KEEPER_HALF + LUNGE_REACH + hurtbox.0
                        || shield_between(&shields, pos, at)
                    {
                        continue;
                    }
                    brain.hits.push(entity);
                    if victim.hurt(LUNGE_HIT * power) {
                        fx(&mut commands, FxKind::Spark, at, dir.to_angle(), KEEPER_COLOR);
                        if let Some(mut victim_knockback) = victim_knockback {
                            victim_knockback.0 += dir * LUNGE_KNOCKBACK;
                        }
                    }
                }
            }
            if let Some(dir) = blocked {
                fx(&mut commands, FxKind::Spark, pos + dir * KEEPER_HALF, dir.to_angle(), KEEPER_COLOR);
                pos = in_room(room, half, grid.slide(pos, -dir * REBOUND, half));
                state = StalkerState::Recover;
                brain.recover = STAGGER;
                brain.timer = 0.0;
            } else if reached || brain.timer >= LEG_TIME {
                let hit = hit + 1;
                brain.timer = 0.0;
                if hit as usize >= marks.len() {
                    state = StalkerState::Recover;
                    brain.recover = RECOVER;
                } else {
                    state = StalkerState::Lunge { hit };
                }
            }
        }
        StalkerState::Recover => {
            if brain.timer >= brain.recover {
                state = StalkerState::Prowl;
                brain.timer = 0.0;
                brain.cooldown = if enraged { ENRAGED_PROWL_TIME } else { PROWL_TIME } + rng.unit() * PROWL_JITTER;
            }
        }
    }
    if !matches!(state, StalkerState::WindUp | StalkerState::Lunge { .. }) {
        marks.clear();
    }

    if pos != start {
        body.translation.x = pos.x;
        body.translation.y = pos.y;
    }
    look.set_if_neq(StalkerLook { state, marks });
}

/// Drifts the keeper's orbs on, lashing nearby pirates with arcs, and bursts
/// them on the first wall, pirate or riot shield they meet. Neither arcs nor
/// bursts reach anyone through a riot shield.
#[allow(clippy::type_complexity)]
fn move_keeper_orbs(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut orbs: Query<(Entity, &mut KeeperOrb, &mut Transform)>,
    mut crew: Query<(&Team, &Hurtbox, &Transform, &mut Health, Option<&Player>, Option<&mut Knockback>), Without<KeeperOrb>>,
    riot_shields: Query<(&RiotShield, &Transform), (Without<KeeperOrb>, Without<Hurtbox>)>,
) {
    const MAX_STEP: f32 = 3.0;
    let dt = time.delta_secs();
    let shields = shield_outlines(&riot_shields);
    'orbs: for (entity, mut orb, mut transform) in &mut orbs {
        transform.rotate_z(dt * 5.0);

        // Lashes the nearest pirate in reach that it can see.
        orb.arc_timer -= dt;
        if orb.arc_timer <= 0.0 {
            orb.arc_timer = ORB_ARC_GAP;
            let pos = transform.translation.truncate();
            let nearest = crew
                .iter_mut()
                .filter(|(team, _, t, health, player, _)| {
                    let at = t.translation.truncate();
                    in_play(team, health, *player)
                        && at.distance(pos) <= ORB_ARC_REACH
                        && grid.line_of_sight(pos, at)
                        && !shield_between(&shields, pos, at)
                })
                .min_by(|a, b| a.2.translation.truncate().distance(pos).total_cmp(&b.2.translation.truncate().distance(pos)));
            if let Some((_, _, t, mut health, ..)) = nearest {
                let at = t.translation.truncate();
                if health.hurt(ORB_ARC_DAMAGE * orb.power) {
                    fx(&mut commands, FxKind::Zap(at), pos, 0.0, KEEPER_COLOR);
                }
            }
        }

        let travel = orb.velocity * dt;
        let steps = (travel.length() / MAX_STEP).ceil().max(1.0);
        for _ in 0..steps as u32 {
            transform.translation += (travel / steps).extend(0.0);
            let pos = transform.translation.truncate();
            let touching = crew.iter().any(|(team, hurtbox, t, health, player, _)| {
                in_play(team, health, player) && t.translation.truncate().distance(pos) <= hurtbox.0 + 3.0
            });
            orb.remaining -= travel.length() / steps;
            if grid.is_solid_at(pos) || hits_shield(&shields, pos) || touching || orb.remaining <= 0.0 {
                // Bursts just short of a wall, not inside it.
                let center = pos - orb.velocity.normalize_or_zero() * 2.0;
                for (team, hurtbox, t, mut health, player, knockback) in &mut crew {
                    let offset = t.translation.truncate() - center;
                    let reach = ORB_RADIUS + hurtbox.0;
                    if !in_play(team, &health, player)
                        || offset.length() > reach
                        || shield_between(&shields, center, t.translation.truncate())
                    {
                        continue;
                    }
                    let falloff = 1.0 - 0.5 * offset.length() / reach;
                    if health.hurt(ORB_DAMAGE * orb.power * falloff)
                        && let Some(mut knockback) = knockback
                    {
                        knockback.0 += offset.normalize_or(Vec2::X) * ORB_KNOCKBACK * falloff;
                    }
                }
                fx(&mut commands, FxKind::Explosion, center, 0.0, KEEPER_COLOR);
                commands.entity(entity).despawn();
                continue 'orbs;
            }
        }
    }
}

/// When the keeper falls: a string of explosions, a generous bounty, a nano
/// patch, and the body, hands, beams and orbs all gone, so the breach opens the gate.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn keeper_deaths(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    keepers: Query<(Entity, &Keeper, &Health, &Transform)>,
    hands: Query<(Entity, &Transform), With<KeeperHand>>,
    beams: Query<Entity, With<KeeperBeam>>,
    orbs: Query<Entity, With<KeeperOrb>>,
    pirates: Query<(&Player, &Health, &Arsenal), Without<Keeper>>,
) {
    for (entity, keeper, health, transform) in &keepers {
        if !health.is_dead() {
            continue;
        }
        let pos = transform.translation.truncate();
        fx(&mut commands, FxKind::Explosion, pos, 0.0, KEEPER_COLOR);
        for i in 0..5 {
            let offset = Vec2::from_angle(i as f32 / 5.0 * std::f32::consts::TAU + rng.signed()) * (8.0 + rng.unit() * 10.0);
            fx(&mut commands, FxKind::Explosion, pos + offset, 0.0, keeper.tier.color());
        }
        for (hand, t) in &hands {
            fx(&mut commands, FxKind::Explosion, t.translation.truncate(), 0.0, KEEPER_COLOR);
            commands.entity(hand).despawn();
        }
        for e in beams.iter().chain(orbs.iter()) {
            commands.entity(e).despawn();
        }

        let share = keeper_bounty(keeper.tier) / BOUNTY_DROPS;
        let mut drops = vec![EnemyDrop::Credits(share); BOUNTY_DROPS as usize];
        let crew = || {
            pirates
                .iter()
                .filter(|(player, health, _)| player.status == PirateStatus::Active && !health.is_dead())
                .map(|(.., arsenal)| arsenal)
        };
        for _ in 0..BOUNTY_AMMO {
            if let Some(kind) = pick_ammo(&mut rng, crew()) {
                drops.push(EnemyDrop::Ammo(kind));
            }
        }
        spawn_drops(&mut commands, &mut rng, pos, &drops);
        // A nano patch where it fell, to patch the crew back up.
        spawn_pickup(&mut commands, PerkKind::NanoPatch, pos);
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_grows_with_tier_and_crew() {
        for kind in KeeperKind::ALL {
            assert_eq!(keeper_health(kind, VaultTier::Silver, 1), 1800.0);
            assert_eq!(keeper_health(kind, VaultTier::Silver, 0), keeper_health(kind, VaultTier::Silver, 1), "never less than solo");
            for crew in 1..5 {
                let (now, more) = (keeper_health(kind, VaultTier::Gold, crew), keeper_health(kind, VaultTier::Gold, crew + 1));
                assert!(more > now, "each crewmate adds health");
                assert!(more / now < (crew + 1) as f32 / crew as f32, "but less than their share");
            }
            assert!(keeper_health(kind, VaultTier::Silver, 3) < keeper_health(kind, VaultTier::Gold, 3));
            assert!(keeper_health(kind, VaultTier::Gold, 3) < keeper_health(kind, VaultTier::Platinum, 3));

            let mut keeper = Keeper { kind, tier: VaultTier::Platinum, awake: false, enraged: false };
            let mut health = Health::new(1.0, 0.0);
            keeper.wake(&mut health, 2);
            assert!(keeper.awake);
            let full = keeper_health(kind, VaultTier::Platinum, 2);
            assert_eq!((health.current, health.max), (full, full));
        }
    }

    #[test]
    fn deeper_guardians_are_mightier_and_the_last_overtuned() {
        let [silver, gold, platinum] = [VaultTier::Silver, VaultTier::Gold, VaultTier::Platinum].map(might);
        assert_eq!(silver, Might { health: 1.0, power: 1.0 });
        for (now, next) in [(silver, gold), (gold, platinum)] {
            assert!(next.health > now.health && next.power > now.power, "more of both each deck down");
        }
        // Platinum sits well past where the Silver-to-Gold step would put it.
        let trend = |a: f32, b: f32| b + (b - a);
        assert!(platinum.health >= trend(silver.health, gold.health) * 1.35);
        assert!(platinum.power >= trend(silver.power, gold.power) * 1.35);
        assert!(platinum.health <= trend(silver.health, gold.health) * 1.5);
        assert!(platinum.power <= trend(silver.power, gold.power) * 1.5);
    }

    #[test]
    fn kinds_take_turns_down_the_decks() {
        for seed in [0, 1, 500, 501] {
            assert_ne!(KeeperKind::pick(seed, 1), KeeperKind::pick(seed, 2));
            assert_ne!(KeeperKind::pick(seed, 2), KeeperKind::pick(seed, 3));
        }
        assert_eq!(KeeperKind::pick(0, 2), KeeperKind::Hands);
        assert_eq!(KeeperKind::pick(0, 1), KeeperKind::Stalker);
    }

    #[test]
    fn marks_the_nearest_three_in_a_chain() {
        let pos = Vec2::ZERO;
        assert!(pick_marks(pos, &[]).is_empty());
        assert_eq!(pick_marks(pos, &[Vec2::new(50.0, 0.0)]), vec![0]);
        // Four pirates: the farthest is spared, and after the nearest it goes
        // on to whoever's closest to them, not to itself.
        let crew = [Vec2::new(200.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(-60.0, 0.0), Vec2::new(70.0, 0.0)];
        assert_eq!(pick_marks(pos, &crew), vec![1, 3, 2]);
        assert_eq!(pick_marks(pos, &crew).len(), MAX_MARKS);
    }

    #[test]
    fn prowls_in_close_and_circles() {
        assert_eq!(prowl_dir(Vec2::ZERO, &[], 1.0), Vec2::ZERO, "nobody to prowl around");
        let closer = prowl_dir(Vec2::ZERO, &[Vec2::new(200.0, 0.0)], 0.0);
        assert!(closer.x > 0.0);
        let away = prowl_dir(Vec2::ZERO, &[Vec2::new(20.0, 0.0)], 0.0);
        assert!(away.x < 0.0);
        let circling = prowl_dir(Vec2::ZERO, &[Vec2::new(60.0, 0.0)], 1.0);
        assert!(circling.x.abs() < 1e-6 && circling.y.abs() > 0.0, "at a fighting distance it only circles");
        assert!(prowl_dir(Vec2::ZERO, &[Vec2::new(5.0, 5.0)], 1.0).length() <= 1.0 + 1e-6);
    }

    #[test]
    fn stays_on_the_vault_floor() {
        let room = Rect::new(0.0, 0.0, 100.0, 80.0);
        let half = Vec2::splat(KEEPER_HALF);
        assert_eq!(in_room(room, half, Vec2::new(50.0, 40.0)), Vec2::new(50.0, 40.0));
        assert_eq!(in_room(room, half, Vec2::new(-30.0, 200.0)), Vec2::new(KEEPER_HALF, 80.0 - KEEPER_HALF));
        let tiny = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert_eq!(in_room(tiny, half, Vec2::new(9.0, 9.0)), Vec2::new(9.0, 9.0), "no room to keep it in: left be");
    }

    #[test]
    fn sweep_crosses_the_target_halfway() {
        let (lock, sweep) = (1.0, SWEEP);
        assert!((sweep_angle(lock, sweep, 0.5) - lock).abs() < 1e-6);
        assert!((sweep_angle(lock, sweep, 1.0) - sweep_angle(lock, sweep, 0.0) - sweep).abs() < 1e-6);
        assert_eq!(sweep_angle(lock, -sweep, 2.0), sweep_angle(lock, -sweep, 1.0), "stops at the end");
    }

    #[test]
    fn drifts_to_a_fighting_distance_within_the_leash() {
        let home = Vec2::ZERO;
        // A pirate right next to it: it backs off.
        let away = drift_dir(Vec2::ZERO, home, &[Vec2::new(20.0, 0.0)], 0.0);
        assert!(away.x < 0.0);
        // Far off: it closes in.
        let closer = drift_dir(Vec2::ZERO, home, &[Vec2::new(200.0, 0.0)], 0.0);
        assert!(closer.x > 0.0);
        // Well past the leash, it heads home even backing off from a pirate.
        let back = drift_dir(Vec2::new(LEASH + 40.0, 0.0), home, &[Vec2::new(LEASH + 20.0, 0.0)], 0.0);
        assert!(back.x < 0.0);
        let back = drift_dir(Vec2::new(-LEASH - 40.0, 0.0), home, &[Vec2::new(-LEASH - 20.0, 0.0)], 0.0);
        assert!(back.x > 0.0);
        assert!(drift_dir(Vec2::new(5.0, 5.0), home, &[Vec2::ZERO], 1.0).length() <= 1.0 + 1e-6);
    }

    #[test]
    fn quantized_aim_stays_unit_and_close() {
        let dir = Vec2::from_angle(0.123);
        let q = quantize(dir, AIM_STEP);
        assert!((q.length() - 1.0).abs() < 1e-5);
        assert!(q.angle_to(dir).abs() <= AIM_STEP / 2.0 + 1e-5);
        assert_eq!(q, quantize(Vec2::from_angle(0.124), AIM_STEP), "small turns don't change it");
    }
}
