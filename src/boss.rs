//! The vault's guardians: a hulking boss waiting in every sealed vault room
//! (see `breach.rs`), one of three kinds taking turns down the decks. The vault
//! keeper fights with two floating hands, each sweeping lasers and hurling
//! nova orbs; badly hurt, it enrages and both hands attack together. The
//! vault stalker crouches showing several routes it might lunge along (one
//! through up to three pirates in a row), then takes one at random; now and
//! then it rampages instead, dashing wall to wall across the room again and
//! again. The vault brute plods after the crew and slams whoever it reaches,
//! and keeps roaring for guards to do its fighting for it. Each one further
//! down is tougher and hits harder (see `might`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::class::{HACK_COLOR, RiotShield};
use crate::combat::{FxKind, Health, Hurtbox, Knockback, ProjectileLook, Team, fx, hits_shield, shield_between, shield_covers, shield_outlines};
use crate::drops::{EnemyDrop, pick_ammo, spawn_drops};
use crate::enemies::{Enemy, EnemyKind, Summoned, spawn_summoned, whiffs};
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
const PROWL_TIME: f32 = 2.6;
const ENRAGED_PROWL_TIME: f32 = 1.8;
const PROWL_JITTER: f32 = 0.6;
/// Seconds crouched with its routes showing before it goes.
const WIND_UP: f32 = 0.95;
const ENRAGED_WIND_UP: f32 = 0.75;
/// How many routes it threatens while winding up; it takes one at random.
const ROUTES: usize = 3;
const ENRAGED_ROUTES: usize = 4;
/// Every route's first leg heads at least this far from every other's, so
/// each reads as its own direction.
const ROUTE_SPLIT: f32 = 30.0 * std::f32::consts::PI / 180.0;
/// The most pirates the through-the-crew route goes through, and the most legs any route has.
const MAX_MARKS: usize = 3;
const MAX_LEGS: usize = 3;
/// A swerving leg is aimed this far off a pirate, and runs `OVERSHOOT` past them.
const SWERVE_MIN: f32 = 25.0 * std::f32::consts::PI / 180.0;
const SWERVE_MAX: f32 = 50.0 * std::f32::consts::PI / 180.0;
const OVERSHOOT: f32 = 28.0;
/// How far a feint into open floor runs before it turns on someone.
const FEINT_MIN: f32 = 48.0;
const FEINT_MAX: f32 = 100.0;
/// A first leg shorter than this doesn't read as a direction.
const MIN_LEG: f32 = 20.0;
/// How fast it lunges, the longest it spends on any one leg, and the steps it
/// takes so it can't skip past a pirate or a shield.
const LUNGE_SPEED: f32 = 420.0;
const LEG_TIME: f32 = 0.6;
const LUNGE_STEP: f32 = 3.0;
/// Pirates this close to its body's edge as it lunges or dashes by are hit.
const LUNGE_REACH: f32 = 4.0;
/// A lunge's damage to each pirate it goes through (once each), and how hard it throws them.
const LUNGE_HIT: f32 = 48.0;
const LUNGE_KNOCKBACK: f32 = 200.0;
/// Seconds spent after a lunge, open to punishment; longer after a rampage,
/// and longer still if a riot shield stopped it, throwing it back `REBOUND` pixels.
const RECOVER: f32 = 0.9;
const RAMPAGE_RECOVER: f32 = 1.6;
const STAGGER: f32 = 1.8;
const REBOUND: f32 = 8.0;

/// Odds it rampages instead of lunging when its prowl runs out (never twice in a row).
const RAMPAGE_CHANCE: f32 = 0.33;
const ENRAGED_RAMPAGE_CHANCE: f32 = 0.5;
/// Seconds rearing up, its first lane showing, before the rampage starts.
const REAR: f32 = 1.0;
const ENRAGED_REAR: f32 = 0.85;
/// Wall-to-wall dashes in one rampage.
const RAMPAGE_DASHES: u8 = 4;
const ENRAGED_RAMPAGE_DASHES: u8 = 6;
/// How fast it dashes, and the longest one dash may take.
const DASH_SPEED: f32 = 640.0;
const DASH_TIME: f32 = 0.8;
/// Seconds braced at the wall between dashes, the next lane showing.
const BRACE: f32 = 0.45;
const ENRAGED_BRACE: f32 = 0.35;
/// Each lane runs through a pirate, or up to this far to one side of them.
const LANE_JITTER: f32 = 10.0;
/// A lane shorter than this ahead of it isn't worth dashing: it heads for the
/// middle of the room instead.
const MIN_DASH: f32 = 64.0;
/// A dash's damage to each pirate it touches (once each per dash), and how hard it throws them.
const DASH_HIT: f32 = 75.0;
const DASH_KNOCKBACK: f32 = 330.0;

/// The vault brute's plodding pace: slower than the stalker prowls, faster
/// than a regular brute. It stops closing in once a pirate is this close.
const PLOD_SPEED: f32 = 40.0;
const PLOD_NEAR: f32 = 26.0;
/// It starts a flurry of `SLAMS` once a pirate it can see is this close, each
/// landing `SLAM_AHEAD` from it in a random direction, at least `SLAM_TURN`
/// radians round from the one before.
const SLAM_TRIGGER: f32 = 40.0;
const SLAM_AHEAD: f32 = 20.0;
pub const SLAMS: u8 = 3;
const SLAM_TURN: f32 = 1.0;
/// Seconds its fists are raised before the first slam comes down, and before
/// each one after.
const SLAM_WIND_UP: f32 = 0.8;
const ENRAGED_SLAM_WIND_UP: f32 = 0.65;
const FOLLOW_WIND_UP: f32 = 0.5;
const ENRAGED_FOLLOW_WIND_UP: f32 = 0.42;
/// Each slam's damage to everyone within `BRUTE_SLAM_RADIUS` of where it
/// lands and how hard it throws them; the seconds it pauses between slams,
/// and is stuck after the last.
const SLAM_HIT: f32 = 55.0;
const SLAM_KNOCKBACK: f32 = 300.0;
const SLAM_BEAT: f32 = 0.25;
const SLAM_RECOVER: f32 = 1.0;
/// Seconds after waking before its first roar, then between roars.
const FIRST_SUMMON: f32 = 2.0;
const SUMMON_GAP: f32 = 12.0;
const ENRAGED_SUMMON_GAP: f32 = 9.0;
/// Seconds it stands roaring, its guards' spots showing, before they appear.
const ROAR: f32 = 1.2;
/// Guards each roar calls in: `BASE_SUMMONS`, plus one for each pirate it can
/// see up to a crew of `SUMMON_CREW`, plus `ENRAGED_SUMMONS` more enraged. At
/// most `CAP_PER_PIRATE` per pirate are alive at once, never fewer
/// than `MIN_CAP` nor more than `MAX_SUMMONS`: at the cap a roar calls fewer,
/// or none.
const BASE_SUMMONS: usize = 1;
const SUMMON_CREW: usize = 3;
const ENRAGED_SUMMONS: usize = 2;
const CAP_PER_PIRATE: usize = 3;
const MIN_CAP: usize = 3;
const MAX_SUMMONS: usize = 10;
/// Guards appear this far in from the room's walls, at least `SUMMON_CLEAR`
/// from every pirate (half that if the crew leaves no such room), and
/// `SUMMON_SPREAD` from each other.
const SUMMON_INSET: f32 = 12.0;
const SUMMON_CLEAR: f32 = 56.0;
const SUMMON_SPREAD: f32 = 22.0;
/// A summoned guard's collision half-extent, for keeping it out of walls: the biggest guard's.
const SUMMON_HALF: f32 = 6.0;

/// Credit drops it leaves behind, sharing its bounty, and ammo boxes.
const BOUNTY_DROPS: u32 = 10;
const BOUNTY_AMMO: u32 = 2;
pub const KEEPER_COLOR: Color = Color::srgb(1.0, 0.3, 0.5);

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (drive_keeper, drive_stalker, drive_brute, move_keeper_orbs, keeper_deaths)
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
    /// A hulking, buffed-up stalker: winds up showing several lunge routes and
    /// takes one at random, or rampages wall to wall across the room, with a
    /// moderate cooldown between.
    Stalker,
    /// A beefy, buffed-up brute: melee only, it has to get up close to slam,
    /// and keeps calling in guards to do its fighting for it.
    Brute,
}

impl KeeperKind {
    pub const ALL: [KeeperKind; 3] = [Self::Hands, Self::Stalker, Self::Brute];

    /// The guardian of `deck`'s vault in the heist from `seed`. The kinds take
    /// turns down the decks, so a heist meets each of them once.
    pub fn pick(seed: u32, deck: u8) -> Self {
        Self::ALL[(seed as usize + deck as usize) % Self::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hands => KEEPER_NAME,
            Self::Stalker => "VAULT STALKER",
            Self::Brute => "VAULT BRUTE",
        }
    }

    /// How it fights, in a word, for the enemies screen.
    pub fn role(self) -> &'static str {
        match self {
            Self::Hands => "BOSS - RANGED",
            Self::Stalker => "BOSS - MELEE",
            Self::Brute => "BOSS - SUMMONER",
        }
    }

    /// What to expect from it, for the enemies screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::Hands => "Guards a vault. Two floating hands sweep lasers and hurl nova orbs; badly hurt, both attack at once.",
            Self::Stalker => "Guards a vault. Shows several ways it might lunge, then takes one at random: get off them all. Sometimes rampages wall to wall instead. Riot shields stop it.",
            Self::Brute => "Guards a vault. A beefy brute that slams whoever it reaches, and keeps calling in guards to fight for it.",
        }
    }

    pub fn sprite(self) -> &'static str {
        match self {
            Self::Hands => "sprites/boss/keeper.png",
            Self::Stalker => "sprites/boss/stalker.png",
            Self::Brute => "sprites/boss/brute.png",
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
    /// stalker lunges sooner and winds up quicker; the brute slams quicker
    /// and calls in more guards, more often.
    pub enraged: bool,
}

/// Host only: a Hacker nearby is jamming the boss, so each of its blows,
/// beam burns, orb arcs and bursts may whiff (see `enemies::whiffs`).
#[derive(Component, Default)]
pub struct KeeperJam(pub bool);

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
    /// Scales every hit it lands: beams, orbs, arcs, lunges and slams.
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
        KeeperKind::Stalker => 2400.0,
        KeeperKind::Brute => 3000.0,
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
    /// Crouched, winding up: `StalkerLook::routes` shows every way it might
    /// lunge; nobody knows which until it goes.
    WindUp,
    /// Lunging along the one route it picked (`routes[0]`), `hit` of its legs done.
    Lunge { hit: u8 },
    /// Spent after a lunge or rampage: a moment to punish it.
    Recover,
    /// Rearing up before a rampage: `StalkerLook::lane` shows the first dash.
    Rear,
    /// Rampaging: dashing wall to wall across the room, `dash` (from 0) of
    /// `total` dashes. While `moving` is false it's braced at a wall, lining up
    /// the next dash along `lane`.
    Rampage { dash: u8, total: u8, moving: bool },
}

/// Beside a Vault Stalker's `Keeper`: what it's doing and where it's going.
/// Replicated.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StalkerLook {
    pub state: StalkerState,
    /// While it winds up, every route it might lunge along, each the points
    /// it would pass through in order, starting after its body. While it
    /// lunges, only the route it picked. Empty otherwise.
    pub routes: Vec<Vec<Vec2>>,
    /// While it rears up or rampages, the current (or next) dash's lane,
    /// from wall to wall. `None` otherwise.
    pub lane: Option<[Vec2; 2]>,
}

/// What a Vault Brute is doing, for the view.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BruteState {
    /// Plodding after the nearest pirate.
    Plod,
    /// Fists raised over `BruteLook::at` for the `blow`th of its `SLAMS`
    /// slams, about to bring them down: everyone within `BRUTE_SLAM_RADIUS`
    /// of it when they land is hit.
    WindUp { blow: u8 },
    /// Just landed the `blow`th slam: a beat before the next, or, after the
    /// last, stuck a moment, open to punishment.
    Recover { blow: u8 },
    /// Roaring, calling in guards: they appear at `BruteLook::summons` when it ends.
    Summon,
}

/// How far from `BruteLook::at` a Vault Brute's slam reaches.
pub const BRUTE_SLAM_RADIUS: f32 = 32.0;

/// Seconds a Vault Brute's fists stay raised before its `blow`th slam.
pub fn slam_wind_up(blow: u8, enraged: bool) -> f32 {
    match (blow, enraged) {
        (0, false) => SLAM_WIND_UP,
        (0, true) => ENRAGED_SLAM_WIND_UP,
        (_, false) => FOLLOW_WIND_UP,
        (_, true) => ENRAGED_FOLLOW_WIND_UP,
    }
}

/// Beside a Vault Brute's `Keeper`: what it's doing, where its slam lands,
/// and where the guards it's calling in will appear. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct BruteLook {
    pub state: BruteState,
    /// While it winds up or recovers, where its slam lands.
    pub at: Vec2,
    /// While it roars, where each guard it's calling will appear. Empty otherwise.
    pub summons: Vec<Vec2>,
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
    /// Thrown by a jammed keeper: its arcs and burst may whiff.
    jammed: bool,
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
    /// Who this lunge or dash has hit already: nobody's hit twice by one.
    hits: Vec<Entity>,
    /// Its last attack was a rampage: the next is a lunge.
    rampaged: bool,
}

/// Host only: the vault brute's hidden side; what it's doing is its `BruteLook`.
#[derive(Component)]
struct BruteBrain {
    /// Seconds into its current state.
    timer: f32,
    /// Seconds until it may roar for guards again.
    summon_cd: f32,
    velocity: Vec2,
}

/// Host only: puts the sleeping guardian of `deck`'s vault at `pos`: the
/// keeper with its two hands, the stalker, or the brute.
pub fn spawn_keeper(commands: &mut Commands, pos: Vec2, deck: u8, kind: KeeperKind) {
    let tier = VaultTier::for_deck(deck);
    let body = commands
        .spawn((
            Level,
            Replicated,
            Keeper { kind, tier, awake: false, enraged: false },
            KeeperJam::default(),
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
                rampaged: false,
            },
            StalkerLook { state: StalkerState::Prowl, routes: Vec::new(), lane: None },
        ));
        return;
    }
    if kind == KeeperKind::Brute {
        commands.entity(body).insert((
            BruteBrain { timer: 0.0, summon_cd: FIRST_SUMMON, velocity: Vec2::ZERO },
            BruteLook { state: BruteState::Plod, at: pos.round(), summons: Vec::new() },
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

/// Which way each pirate holding a riot shield up faces it, by owner.
fn raised_shields<F: bevy::ecs::query::QueryFilter>(riot_shields: &Query<(&RiotShield, &Transform), F>) -> Vec<(u64, Vec2)> {
    riot_shields.iter().map(|(shield, _)| (shield.owner, shield.facing)).collect()
}

/// Whether `player`, at `at`, holds a shield up toward a blow from `from`.
fn covered(held: &[(u64, Vec2)], player: Option<&Player>, at: Vec2, from: Vec2) -> bool {
    player.is_some_and(|p| held.iter().any(|&(owner, facing)| owner == p.owner && shield_covers(facing, at, from)))
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

/// A uniformly random index below `n` (which must be above zero).
fn roll(rng: &mut Rng, n: usize) -> usize {
    (rng.unit() * n as f32) as usize % n
}

/// Up to `n` of the pirates at `crew`, picked at random, in a random order.
fn shuffled(rng: &mut Rng, crew: &[Vec2], n: usize) -> Vec<Vec2> {
    let mut left = crew.to_vec();
    let mut picked = Vec::with_capacity(n.min(left.len()));
    while picked.len() < n && !left.is_empty() {
        picked.push(left.swap_remove(roll(rng, left.len())));
    }
    picked
}

/// Which way a route from `pos` sets off: toward its first point at least
/// `MIN_LEG` away. `None` if it never gets that far.
fn heading(pos: Vec2, route: &[Vec2]) -> Option<Vec2> {
    route.iter().find(|p| p.distance(pos) >= MIN_LEG).map(|p| (*p - pos).normalize())
}

/// The routes a lunge from `pos` might take, `count` of them if there's room:
/// each the points it passes through in order, on the room floor `room` (for
/// a body of half-extent `half`). With pirates at `crew`, the first goes
/// through up to `MAX_MARKS` of them in a chain (see `pick_marks`); the rest
/// take them in other orders, swerve past one and cut back through, or feint
/// into open floor before turning on someone. No two set off within
/// `ROUTE_SPLIT` of each other, so even a lone pirate can't just stand aside
/// from one line.
fn plan_routes(room: Rect, half: Vec2, pos: Vec2, crew: &[Vec2], count: usize, rng: &mut Rng) -> Vec<Vec<Vec2>> {
    let fit = |p: Vec2| in_room(room, half, p).round();
    let mut routes: Vec<Vec<Vec2>> = Vec::new();
    // Takes a route if it sets off its own way.
    let offer = |routes: &mut Vec<Vec<Vec2>>, route: Vec<Vec2>| {
        let mut route: Vec<Vec2> = route.into_iter().map(fit).collect();
        route.dedup_by(|a, b| a.distance(*b) < 1.0);
        route.truncate(MAX_LEGS);
        let Some(dir) = heading(pos, &route) else { return };
        let apart = routes
            .iter()
            .filter_map(|r| heading(pos, r))
            .all(|other| dir.angle_to(other).abs() >= ROUTE_SPLIT);
        if apart && routes.len() < count {
            routes.push(route);
        }
    };

    if !crew.is_empty() {
        let chain: Vec<Vec2> = pick_marks(pos, crew).into_iter().map(|i| fit(crew[i])).collect();
        // Always on offer, however close they stand.
        if heading(pos, &chain).is_some() {
            offer(&mut routes, chain);
        } else {
            routes.push(chain);
        }
    }
    for _ in 0..48 {
        if routes.len() >= count {
            break;
        }
        let kind = if crew.is_empty() { 2 } else { roll(rng, 3) };
        let route = match kind {
            // The crew in another order.
            0 => shuffled(rng, crew, MAX_MARKS),
            // Past a pirate, off to one side, then back through them and on.
            1 => {
                let target = crew[roll(rng, crew.len())];
                let to = target - pos;
                let swerve = (SWERVE_MIN + rng.unit() * (SWERVE_MAX - SWERVE_MIN)) * rng.signed().signum();
                let past = pos + Vec2::from_angle(swerve).rotate(to.normalize_or(Vec2::X)) * (to.length() + OVERSHOOT);
                let mut route = vec![past, target];
                let others: Vec<Vec2> = crew.iter().copied().filter(|p| *p != target).collect();
                if !others.is_empty() && rng.unit() < 0.5 {
                    route.push(others[roll(rng, others.len())]);
                } else {
                    route.push(target + (target - fit(past)).normalize_or(Vec2::X) * OVERSHOOT);
                }
                route
            }
            // Into open floor, then through one or two of them.
            _ => {
                let dir = Vec2::from_angle(rng.unit() * std::f32::consts::TAU);
                let feint = pos + dir * (FEINT_MIN + rng.unit() * (FEINT_MAX - FEINT_MIN));
                let mut route = vec![feint];
                if crew.is_empty() {
                    route.push(feint + Vec2::from_angle(rng.signed() * 2.0).rotate(dir) * FEINT_MAX);
                } else {
                    let n = 1 + roll(rng, 2);
                    route.extend(shuffled(rng, crew, n));
                }
                route
            }
        };
        offer(&mut routes, route);
    }
    // Cornered or crowded: feints fanned out all around, then at someone.
    let start = rng.unit() * std::f32::consts::TAU;
    for k in 0..24 {
        if routes.len() >= count {
            break;
        }
        let dir = Vec2::from_angle(start + k as f32 * std::f32::consts::TAU / 24.0);
        let mut route = vec![pos + dir * FEINT_MAX];
        if let Some(target) = crew.first() {
            route.push(*target);
        }
        offer(&mut routes, route);
    }
    routes
}

/// How far a body at `p` can go along unit `dir` before leaving `floor`.
fn edge_reach(floor: Rect, p: Vec2, dir: Vec2) -> f32 {
    let mut reach = f32::INFINITY;
    for (at, d, min, max) in [(p.x, dir.x, floor.min.x, floor.max.x), (p.y, dir.y, floor.min.y, floor.max.y)] {
        if d > 1e-6 {
            reach = reach.min((max - at) / d);
        } else if d < -1e-6 {
            reach = reach.min((min - at) / d);
        }
    }
    if reach.is_finite() { reach.max(0.0) } else { 0.0 }
}

/// A rampage dash's lane from `from` toward `aim`, from wall to wall of the
/// room floor `room` (for a body of half-extent `half`): it starts behind
/// `from` and ends past `aim`, where the dash ends. If that's hardly any way
/// ahead, it runs toward the middle of the room instead.
fn rampage_lane(room: Rect, half: Vec2, from: Vec2, aim: Vec2) -> [Vec2; 2] {
    let floor = Rect { min: room.min + half, max: room.max - half };
    if floor.width() <= 0.0 || floor.height() <= 0.0 {
        return [from, from];
    }
    let from = in_room(room, half, from);
    let mut dir = (aim - from).normalize_or(Vec2::X);
    if edge_reach(floor, from, dir) < MIN_DASH {
        dir = (floor.center() - from).normalize_or(-dir);
    }
    [from - dir * edge_reach(floor, from, -dir), from + dir * edge_reach(floor, from, dir)]
}

/// Cuts a lane through `from` short at the first walls (or vault) either way,
/// for a body of half-extent `half`.
fn clip_lane(grid: &RoomGrid, half: Vec2, from: Vec2, lane: [Vec2; 2]) -> [Vec2; 2] {
    lane.map(|end| {
        let dir = (end - from).normalize_or_zero();
        let mut at = from;
        while at.distance(end) > 2.0 {
            let next = at + dir * at.distance(end).min(2.0);
            if grid.collides(next, half) {
                break;
            }
            at = next;
        }
        at.round()
    })
}

/// Where the vault brute's slam from `pos` lands, struck `angle` radians round.
fn slam_spot(pos: Vec2, angle: f32) -> Vec2 {
    pos + Vec2::from_angle(angle) * SLAM_AHEAD
}

/// A random way round for the next slam, at least `SLAM_TURN` from `last`'s.
fn slam_angle(rng: &mut Rng, last: Option<f32>) -> f32 {
    let Some(last) = last else { return rng.unit() * std::f32::consts::TAU };
    last + SLAM_TURN + rng.unit() * (std::f32::consts::TAU - 2.0 * SLAM_TURN)
}

/// Whether a slam landing at `at` reaches someone at `victim` with a hurtbox of `hurtbox`.
fn slam_reaches(at: Vec2, victim: Vec2, hurtbox: f32) -> bool {
    victim.distance(at) <= BRUTE_SLAM_RADIUS + hurtbox
}

/// The most of its guards alive at once, facing `crew` pirates.
fn summon_cap(crew: usize) -> usize {
    (CAP_PER_PIRATE * crew).clamp(MIN_CAP, MAX_SUMMONS)
}

/// How many guards a roar calls in facing `crew` pirates, with `alive` of its
/// summons still up: a lone pirate isn't swamped, and never past the cap.
fn summon_count(enraged: bool, alive: usize, crew: usize) -> usize {
    let wanted = BASE_SUMMONS + crew.clamp(1, SUMMON_CREW) + if enraged { ENRAGED_SUMMONS } else { 0 };
    wanted.min(summon_cap(crew).saturating_sub(alive))
}

/// Every kind of guard a roar can call in.
const SUMMON_KINDS: [EnemyKind; 5] =
    [EnemyKind::SentryDrone, EnemyKind::Stalker, EnemyKind::Warden, EnemyKind::Brute, EnemyKind::Herald];

/// Which guards a roar of `count` calls in: every kind in turn, in a random
/// order, so a roar of five or more brings one of each.
fn summon_kinds(rng: &mut Rng, count: usize) -> Vec<EnemyKind> {
    let mut kinds = SUMMON_KINDS;
    for i in (1..kinds.len()).rev() {
        kinds.swap(i, roll(rng, i + 1));
    }
    kinds.iter().copied().cycle().take(count).collect()
}

/// Where up to `count` summoned guards appear on the room floor `room`: near
/// its walls, in whole pixels, at spots `open` says a guard fits, at least
/// `SUMMON_CLEAR` from every pirate at `crew` (or half that, if the crew
/// leaves no such room), and `SUMMON_SPREAD` apart. Fewer if there's no room.
fn summon_spots(room: Rect, crew: &[Vec2], count: usize, open: impl Fn(Vec2) -> bool, rng: &mut Rng) -> Vec<Vec2> {
    let floor = Rect { min: room.min + SUMMON_INSET, max: room.max - SUMMON_INSET };
    if floor.width() <= 0.0 || floor.height() <= 0.0 {
        return Vec::new();
    }
    // Random spots along the walls, up to a tile in from them.
    let candidates: Vec<Vec2> = (0..64)
        .map(|_| {
            let (t, inward) = (rng.unit(), rng.unit() * 16.0);
            let p = match roll(rng, 4) {
                0 => Vec2::new(floor.min.x.lerp(floor.max.x, t), floor.min.y + inward),
                1 => Vec2::new(floor.min.x.lerp(floor.max.x, t), floor.max.y - inward),
                2 => Vec2::new(floor.min.x + inward, floor.min.y.lerp(floor.max.y, t)),
                _ => Vec2::new(floor.max.x - inward, floor.min.y.lerp(floor.max.y, t)),
            };
            p.clamp(floor.min, floor.max).round()
        })
        .filter(|p| open(*p))
        .collect();
    let clearance = |p: Vec2| crew.iter().map(|c| c.distance(p)).fold(f32::INFINITY, f32::min);
    let mut picked: Vec<Vec2> = Vec::with_capacity(count);
    for clear in [SUMMON_CLEAR, SUMMON_CLEAR / 2.0] {
        for p in &candidates {
            if picked.len() >= count {
                return picked;
            }
            if clearance(*p) >= clear && picked.iter().all(|q| q.distance(*p) >= SUMMON_SPREAD) {
                picked.push(*p);
            }
        }
    }
    picked
}

/// The pirates on the crew's side, who a lunge or dash can hurt.
type Crew<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Team,
        &'static Hurtbox,
        &'static Transform,
        &'static mut Health,
        Option<&'static Player>,
        Option<&'static mut Knockback>,
    ),
    Without<Keeper>,
>;

/// How a frame of a lunge leg or rampage dash ended.
enum Charge {
    Going,
    /// At the point it was charging for, or up against a wall.
    There,
    /// A raised riot shield stopped it dead, heading this way.
    Blocked(Vec2),
}

/// One frame of a lunge leg or rampage dash: carries the body at `pos` up to
/// `left` pixels on toward `to`, in small steps, and hits everyone it touches
/// who isn't in `hits` yet for `damage`, throwing them `throw` hard: onward,
/// and if `fling`, off to the side too.
#[allow(clippy::too_many_arguments)]
fn charge(
    commands: &mut Commands,
    grid: &RoomGrid,
    shields: &[[Vec2; crate::class::RIOT_POINTS]],
    pos: &mut Vec2,
    to: Vec2,
    mut left: f32,
    hits: &mut Vec<Entity>,
    crew: &mut Crew,
    damage: f32,
    throw: f32,
    fling: bool,
    jammed: bool,
    rng: &mut Rng,
) -> Charge {
    let half = Vec2::splat(KEEPER_HALF);
    while left > 0.0 {
        let ahead = to - *pos;
        if ahead.length() < 0.5 {
            return Charge::There;
        }
        let dir = ahead.normalize();
        let step = left.min(LUNGE_STEP).min(ahead.length());
        // A raised riot shield in its way stops it dead.
        let front = *pos + dir * (KEEPER_HALF + step);
        if hits_shield(shields, front) || shield_between(shields, *pos, front) {
            return Charge::Blocked(dir);
        }
        let next = in_room(grid.vault_room, half, grid.slide(*pos, dir * step, half));
        if next.distance(*pos) < step * 0.5 {
            // Up against a wall: as far as this goes.
            return Charge::There;
        }
        *pos = next;
        left -= step;
        // Hits everyone on the crew's side it touches, once each.
        for (entity, team, hurtbox, t, mut victim, player, victim_knockback) in crew.iter_mut() {
            let at = t.translation.truncate();
            if hits.contains(&entity)
                || !in_play(team, &victim, player)
                || at.distance(*pos) > KEEPER_HALF + LUNGE_REACH + hurtbox.0
                || shield_between(shields, *pos, at)
            {
                continue;
            }
            hits.push(entity);
            if whiffs(rng, jammed) {
                fx(commands, FxKind::Spark, at, dir.to_angle(), HACK_COLOR);
                continue;
            }
            if victim.hurt(damage) {
                fx(commands, FxKind::Spark, at, dir.to_angle(), KEEPER_COLOR);
                if let Some(mut victim_knockback) = victim_knockback {
                    let side = if fling { dir.perp() * dir.perp().dot(at - *pos).signum() } else { Vec2::ZERO };
                    victim_knockback.0 += (dir + side).normalize_or(dir) * throw;
                }
            }
        }
    }
    Charge::Going
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
    mut keepers: Query<(&mut Keeper, &KeeperJam, &Health, &mut Transform, &mut Knockback, &mut KeeperBrain), Without<KeeperHand>>,
    mut hands: Query<(&mut KeeperHand, &mut HandBrain, &mut Transform), Without<Keeper>>,
    mut beams: Query<(&mut KeeperBeam, &mut Transform), (Without<Keeper>, Without<KeeperHand>)>,
    mut crew: Query<
        (Entity, &Team, &Hurtbox, &Transform, &mut Health, Option<&Player>),
        (Without<Keeper>, Without<KeeperHand>, Without<KeeperBeam>),
    >,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Keeper>, Without<KeeperHand>, Without<KeeperBeam>)>,
) {
    let Ok((mut keeper, jam, health, mut body, mut knockback, mut brain)) = keepers.single_mut() else { return };
    let jammed = jam.0;
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
                        {
                            hb.burned.push((entity, BEAM_TICK));
                            if whiffs(&mut rng, jammed) {
                                fx(&mut commands, FxKind::Spark, at, 0.0, HACK_COLOR);
                            } else if health.hurt(BEAM_HIT * power) {
                                fx(&mut commands, FxKind::Spark, at, 0.0, KEEPER_COLOR);
                            }
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
                            KeeperOrb { velocity: dir * ORB_SPEED, remaining: ORB_RANGE, arc_timer: ORB_ARC_GAP, power, jammed },
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

/// Wakes, moves and fights the stalker: it prowls around the crew, then
/// either winds up showing several routes and lunges along one picked at
/// random, or rears up and rampages wall to wall across the room, then stands
/// spent for a moment. A riot shield in its way stops a lunge, or a whole
/// rampage, dead.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_stalker(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut banner: ResMut<Banner>,
    mut stalkers: Query<(&mut Keeper, &KeeperJam, &Health, &mut Transform, &mut Knockback, &mut StalkerBrain, &mut StalkerLook)>,
    mut crew: Crew,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Keeper>, Without<Hurtbox>)>,
) {
    let Ok((mut keeper, jam, health, mut body, mut knockback, mut brain, mut look)) = stalkers.single_mut() else { return };
    let jammed = jam.0;
    let dt = time.delta_secs();
    // Nothing pushes it around.
    if knockback.0 != Vec2::ZERO {
        knockback.0 = Vec2::ZERO;
    }
    if !keeper.awake {
        look.set_if_neq(StalkerLook { state: StalkerState::Prowl, routes: Vec::new(), lane: None });
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
    // A dash's lane from `from`: wall to wall, through or just beside someone.
    let next_lane = |rng: &mut Rng, from: Vec2| {
        let target = spots[roll(rng, spots.len())];
        let aside = (target - from).normalize_or(Vec2::X).perp() * rng.signed() * LANE_JITTER;
        clip_lane(&grid, half, from, rampage_lane(room, half, from, target + aside))
    };

    let mut state = look.state;
    let mut routes = look.routes.clone();
    let mut lane = look.lane;
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
                brain.timer = 0.0;
                brain.velocity = Vec2::ZERO;
                // Routes and lanes are locked in now, so whoever gets off them in time is missed.
                let chance = if enraged { ENRAGED_RAMPAGE_CHANCE } else { RAMPAGE_CHANCE };
                if !brain.rampaged && rng.unit() < chance {
                    brain.rampaged = true;
                    lane = Some(next_lane(&mut rng, pos));
                    state = StalkerState::Rear;
                } else {
                    brain.rampaged = false;
                    let count = if enraged { ENRAGED_ROUTES } else { ROUTES };
                    routes = plan_routes(room, half, pos, &spots, count, &mut rng);
                    state = StalkerState::WindUp;
                }
            }
        }
        StalkerState::WindUp => {
            if routes.is_empty() {
                state = StalkerState::Prowl;
            } else if brain.timer >= if enraged { ENRAGED_WIND_UP } else { WIND_UP } {
                // Only now does anyone find out which way it's going.
                let pick = roll(&mut rng, routes.len());
                routes = vec![routes.swap_remove(pick)];
                state = StalkerState::Lunge { hit: 0 };
                brain.timer = 0.0;
                brain.hits.clear();
                fx(&mut commands, FxKind::Burst, pos, 0.0, KEEPER_COLOR);
            }
        }
        StalkerState::Lunge { hit } => {
            let legs = routes.first().map_or(0, Vec::len);
            let to = routes.first().and_then(|r| r.get(hit as usize)).copied().unwrap_or(pos);
            let speed = LUNGE_SPEED * dt;
            match charge(&mut commands, &grid, &shields, &mut pos, to, speed, &mut brain.hits, &mut crew, LUNGE_HIT * power, LUNGE_KNOCKBACK, false, jammed, &mut rng) {
                Charge::Blocked(dir) => {
                    fx(&mut commands, FxKind::Spark, pos + dir * KEEPER_HALF, dir.to_angle(), KEEPER_COLOR);
                    pos = in_room(room, half, grid.slide(pos, -dir * REBOUND, half));
                    state = StalkerState::Recover;
                    brain.recover = STAGGER;
                    brain.timer = 0.0;
                }
                done => {
                    if matches!(done, Charge::There) || brain.timer >= LEG_TIME {
                        let hit = hit + 1;
                        brain.timer = 0.0;
                        if hit as usize >= legs {
                            state = StalkerState::Recover;
                            brain.recover = RECOVER;
                        } else {
                            state = StalkerState::Lunge { hit };
                        }
                    }
                }
            }
        }
        StalkerState::Rear => {
            if brain.timer >= if enraged { ENRAGED_REAR } else { REAR } {
                let total = if enraged { ENRAGED_RAMPAGE_DASHES } else { RAMPAGE_DASHES };
                state = StalkerState::Rampage { dash: 0, total, moving: true };
                brain.timer = 0.0;
                brain.hits.clear();
                fx(&mut commands, FxKind::Burst, pos, 0.0, KEEPER_COLOR);
            }
        }
        StalkerState::Rampage { dash, total, moving: true } => {
            let to = lane.map_or(pos, |l| l[1]);
            let speed = DASH_SPEED * dt;
            match charge(&mut commands, &grid, &shields, &mut pos, to, speed, &mut brain.hits, &mut crew, DASH_HIT * power, DASH_KNOCKBACK, true, jammed, &mut rng) {
                Charge::Blocked(dir) => {
                    // A shield ends the whole rampage.
                    fx(&mut commands, FxKind::Spark, pos + dir * KEEPER_HALF, dir.to_angle(), KEEPER_COLOR);
                    pos = in_room(room, half, grid.slide(pos, -dir * REBOUND, half));
                    state = StalkerState::Recover;
                    brain.recover = STAGGER;
                    brain.timer = 0.0;
                }
                done => {
                    if matches!(done, Charge::There) || brain.timer >= DASH_TIME {
                        fx(&mut commands, FxKind::Burst, pos, 0.0, KEEPER_COLOR);
                        let dash = dash + 1;
                        brain.timer = 0.0;
                        if dash >= total || spots.is_empty() {
                            state = StalkerState::Recover;
                            brain.recover = RAMPAGE_RECOVER;
                        } else {
                            // Braced at the wall, lining up the next.
                            lane = Some(next_lane(&mut rng, pos));
                            state = StalkerState::Rampage { dash, total, moving: false };
                        }
                    }
                }
            }
        }
        StalkerState::Rampage { dash, total, moving: false } => {
            if brain.timer >= if enraged { ENRAGED_BRACE } else { BRACE } {
                state = StalkerState::Rampage { dash, total, moving: true };
                brain.timer = 0.0;
                brain.hits.clear();
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
        routes.clear();
    }
    if !matches!(state, StalkerState::Rear | StalkerState::Rampage { .. }) {
        lane = None;
    }

    if pos != start {
        body.translation.x = pos.x;
        body.translation.y = pos.y;
    }
    look.set_if_neq(StalkerLook { state, routes, lane });
}

/// Wakes, moves and fights the vault brute: it plods after the nearest pirate
/// and, once one is close, raises its fists and slams them down in front of
/// it, then stands stuck a moment. Every so often (and as soon as it enrages)
/// it stops to roar instead, calling in guards at the room's edges to do its
/// fighting for it. Riot shields stop its slam.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_brute(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut banner: ResMut<Banner>,
    mut brutes: Query<(&mut Keeper, &KeeperJam, &Health, &mut Transform, &mut Knockback, &mut BruteBrain, &mut BruteLook)>,
    mut crew: Crew,
    summoned: Query<&Enemy, With<Summoned>>,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Keeper>, Without<Hurtbox>)>,
) {
    let Ok((mut keeper, jam, health, mut body, mut knockback, mut brain, mut look)) = brutes.single_mut() else { return };
    let jammed = jam.0;
    let dt = time.delta_secs();
    // Nothing pushes it around.
    if knockback.0 != Vec2::ZERO {
        knockback.0 = Vec2::ZERO;
    }
    if !keeper.awake {
        let at = look.at;
        look.set_if_neq(BruteLook { state: BruteState::Plod, at, summons: Vec::new() });
        return;
    }
    let start = body.translation.truncate();
    let mut pos = start;

    if !keeper.enraged && health.current <= health.max * ENRAGE_AT {
        keeper.enraged = true;
        // Enraging, it roars for help as soon as it's free.
        brain.summon_cd = 0.0;
        fx(&mut commands, FxKind::Explosion, pos, 0.0, KEEPER_COLOR);
        banner.show(format!("THE {} IS ENRAGED", keeper.kind.name()), KEEPER_COLOR, 3.0);
    }
    let enraged = keeper.enraged;
    let power = might(keeper.tier).power;
    let half = Vec2::splat(KEEPER_HALF);
    let room = grid.vault_room;

    // Every pirate in play, who its guards keep clear of, and the ones it can see.
    let pirates: Vec<Vec2> = crew
        .iter()
        .filter(|(_, team, _, _, health, player, _)| player.is_some() && in_play(team, health, *player))
        .map(|(_, _, _, t, ..)| t.translation.truncate())
        .collect();
    let nearest = pirates
        .iter()
        .filter(|at| at.distance(pos) <= SIGHT && grid.line_of_sight(pos, **at))
        .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)))
        .copied();

    let mut state = look.state;
    let mut at = look.at;
    let mut summons = look.summons.clone();
    brain.timer += dt;
    brain.summon_cd -= dt;
    match state {
        BruteState::Plod => {
            if brain.summon_cd <= 0.0 {
                brain.summon_cd = if enraged { ENRAGED_SUMMON_GAP } else { SUMMON_GAP };
                // At the cap, it skips this roar.
                let count = summon_count(enraged, summoned.iter().filter(|g| !g.is_hacked()).count(), pirates.len());
                if count > 0 {
                    let open = |p: Vec2| !grid.collides(p, Vec2::splat(SUMMON_HALF));
                    summons = summon_spots(room, &pirates, count, open, &mut rng);
                    if !summons.is_empty() {
                        state = BruteState::Summon;
                        brain.timer = 0.0;
                        brain.velocity = Vec2::ZERO;
                        fx(&mut commands, FxKind::Burst, pos, 0.0, KEEPER_COLOR);
                    }
                }
            }
            if state == BruteState::Plod {
                let mut want = Vec2::ZERO;
                if let Some(target) = nearest {
                    let dist = target.distance(pos);
                    if dist <= SLAM_TRIGGER {
                        // Locked in now: whoever steps out of reach in time is missed.
                        state = BruteState::WindUp { blow: 0 };
                        at = in_room(room, Vec2::ZERO, slam_spot(pos, slam_angle(&mut rng, None))).round();
                        brain.timer = 0.0;
                        brain.velocity = Vec2::ZERO;
                    } else if dist > PLOD_NEAR {
                        want = (target - pos).normalize_or_zero() * PLOD_SPEED;
                    }
                }
                if state == BruteState::Plod {
                    brain.velocity = brain.velocity.lerp(want, (4.0 * dt).min(1.0));
                    pos = in_room(room, half, grid.slide(pos, brain.velocity * dt, half));
                }
            }
        }
        BruteState::WindUp { blow } => {
            if brain.timer >= slam_wind_up(blow, enraged) {
                // Down come the fists, on everyone in reach a raised shield doesn't cover.
                let shields = shield_outlines(&riot_shields);
                let held = raised_shields(&riot_shields);
                let ahead = (at - pos).normalize_or(Vec2::NEG_Y);
                for (_, team, hurtbox, t, mut victim, player, victim_knockback) in crew.iter_mut() {
                    let spot = t.translation.truncate();
                    if !in_play(team, &victim, player) || !slam_reaches(at, spot, hurtbox.0) {
                        continue;
                    }
                    if shield_between(&shields, at, spot) || covered(&held, player, spot, pos) {
                        fx(&mut commands, FxKind::Spark, spot, (spot - at).to_angle(), KEEPER_COLOR);
                        continue;
                    }
                    if whiffs(&mut rng, jammed) {
                        fx(&mut commands, FxKind::Spark, spot, 0.0, HACK_COLOR);
                        continue;
                    }
                    if victim.hurt(SLAM_HIT * power) {
                        fx(&mut commands, FxKind::Spark, spot, 0.0, KEEPER_COLOR);
                        if let Some(mut victim_knockback) = victim_knockback {
                            victim_knockback.0 += (spot - at).normalize_or(ahead) * SLAM_KNOCKBACK;
                        }
                    }
                }
                fx(&mut commands, FxKind::Explosion, at, 0.0, KEEPER_COLOR);
                state = BruteState::Recover { blow };
                brain.timer = 0.0;
            }
        }
        BruteState::Recover { blow } if blow + 1 < SLAMS => {
            if brain.timer >= SLAM_BEAT {
                // The next slam, somewhere else round it.
                let last = (at - pos).to_angle();
                state = BruteState::WindUp { blow: blow + 1 };
                at = in_room(room, Vec2::ZERO, slam_spot(pos, slam_angle(&mut rng, Some(last)))).round();
                brain.timer = 0.0;
            }
        }
        BruteState::Recover { .. } => {
            if brain.timer >= SLAM_RECOVER {
                state = BruteState::Plod;
                brain.timer = 0.0;
            }
        }
        BruteState::Summon => {
            if brain.timer >= ROAR {
                // The cap was checked as the roar began: nothing else calls guards in.
                let kinds = summon_kinds(&mut rng, summons.len());
                for (spot, kind) in summons.iter().zip(kinds) {
                    spawn_summoned(&mut commands, kind, *spot);
                    fx(&mut commands, FxKind::Burst, *spot, 0.0, KEEPER_COLOR);
                }
                state = BruteState::Plod;
                brain.timer = 0.0;
            }
        }
    }
    if state != BruteState::Summon {
        summons.clear();
    }

    if pos != start {
        body.translation.x = pos.x;
        body.translation.y = pos.y;
    }
    look.set_if_neq(BruteLook { state, at, summons });
}

/// Drifts the keeper's orbs on, lashing nearby pirates with arcs, and bursts
/// them on the first wall, pirate or riot shield they meet. Neither arcs nor
/// bursts reach anyone through a riot shield.
#[allow(clippy::type_complexity)]
fn move_keeper_orbs(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut orbs: Query<(Entity, &mut KeeperOrb, &mut Transform)>,
    mut crew: Query<(&Team, &Hurtbox, &Transform, &mut Health, Option<&Player>, Option<&mut Knockback>), Without<KeeperOrb>>,
    riot_shields: Query<(&RiotShield, &Transform), (Without<KeeperOrb>, Without<Hurtbox>)>,
) {
    const MAX_STEP: f32 = 3.0;
    let dt = time.delta_secs();
    let shields = shield_outlines(&riot_shields);
    let held = raised_shields(&riot_shields);
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
                        && !covered(&held, *player, at, pos)
                })
                .min_by(|a, b| a.2.translation.truncate().distance(pos).total_cmp(&b.2.translation.truncate().distance(pos)));
            if let Some((_, _, t, mut health, ..)) = nearest {
                let at = t.translation.truncate();
                if whiffs(&mut rng, orb.jammed) {
                    fx(&mut commands, FxKind::Zap(at), pos, 0.0, HACK_COLOR);
                } else if health.hurt(ORB_ARC_DAMAGE * orb.power) {
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
                        || covered(&held, player, t.translation.truncate(), center)
                    {
                        continue;
                    }
                    if whiffs(&mut rng, orb.jammed) {
                        fx(&mut commands, FxKind::Spark, t.translation.truncate(), 0.0, HACK_COLOR);
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
/// patch, and the body, hands, beams, orbs and summoned guards all gone, so
/// the breach opens the gate.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn keeper_deaths(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    keepers: Query<(Entity, &Keeper, &Health, &Transform)>,
    hands: Query<(Entity, &Transform), With<KeeperHand>>,
    beams: Query<Entity, With<KeeperBeam>>,
    orbs: Query<Entity, With<KeeperOrb>>,
    summoned: Query<(Entity, &Enemy, &Transform), (With<Summoned>, Without<Keeper>)>,
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
        // Its guards go down with it, leaving nothing behind; those a Hacker
        // turned are the crew's now, and fight on.
        for (guard, _, t) in summoned.iter().filter(|(_, g, _)| !g.is_hacked()) {
            fx(&mut commands, FxKind::Burst, t.translation.truncate(), 0.0, KEEPER_COLOR);
            commands.entity(guard).despawn();
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
            let base = match kind {
                KeeperKind::Hands => 1800.0,
                KeeperKind::Stalker => 2400.0,
                KeeperKind::Brute => 3000.0,
            };
            assert_eq!(keeper_health(kind, VaultTier::Silver, 1), base);
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
            let mut met: Vec<_> = (1..=3).map(|deck| KeeperKind::pick(seed, deck)).collect();
            met.sort_by_key(|k| *k as u8);
            assert_eq!(met, KeeperKind::ALL, "every heist meets each of them once");
        }
        assert_eq!(KeeperKind::pick(0, 3), KeeperKind::Hands);
        assert_eq!(KeeperKind::pick(0, 1), KeeperKind::Stalker);
        assert_eq!(KeeperKind::pick(0, 2), KeeperKind::Brute);
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

    /// A vault-sized floor for the route and lane tests: 15 by 11 tiles.
    const ROOM: Rect = Rect { min: Vec2::new(0.0, 0.0), max: Vec2::new(240.0, 176.0) };

    fn on_floor(p: Vec2) -> bool {
        let half = Vec2::splat(KEEPER_HALF);
        p.cmpge(ROOM.min + half).all() && p.cmple(ROOM.max - half).all()
    }

    fn check_routes(pos: Vec2, crew: &[Vec2], count: usize, seed: u32) -> Vec<Vec<Vec2>> {
        let mut rng = Rng::new(seed);
        let routes = plan_routes(ROOM, Vec2::splat(KEEPER_HALF), pos, crew, count, &mut rng);
        let cornered = pos.distance(ROOM.min) < 40.0;
        if cornered {
            // Only a quarter of the directions are open: not always room for four.
            assert!((ROUTES..=count).contains(&routes.len()), "seed {seed}: cornered, still a few routes");
        } else {
            assert_eq!(routes.len(), count, "seed {seed}: as many routes as asked for");
        }
        for route in &routes {
            assert!((1..=MAX_LEGS).contains(&route.len()), "seed {seed}: 1 to {MAX_LEGS} legs");
            assert!(route.iter().all(|p| on_floor(*p) && *p == p.round()), "seed {seed}: on the floor, whole pixels");
        }
        for (i, a) in routes.iter().enumerate() {
            for b in &routes[i + 1..] {
                let (a, b) = (heading(pos, a).unwrap(), heading(pos, b).unwrap());
                assert!(a.angle_to(b).abs() >= ROUTE_SPLIT - 1e-4, "seed {seed}: first legs head apart");
            }
        }
        routes
    }

    #[test]
    fn routes_head_apart_on_the_floor_and_one_goes_through_the_crew() {
        let pos = Vec2::new(120.0, 88.0);
        let crew = [Vec2::new(170.0, 90.0), Vec2::new(100.0, 140.0), Vec2::new(60.0, 40.0)];
        let chain: Vec<Vec2> = pick_marks(pos, &crew).into_iter().map(|i| crew[i]).collect();
        let mut firsts = Vec::new();
        for seed in 1..200 {
            for count in [ROUTES, ENRAGED_ROUTES] {
                let routes = check_routes(pos, &crew, count, seed);
                assert_eq!(routes[0], chain, "the through-the-crew chain is always one of them");
                firsts.extend(routes[1..].iter().map(|r| r[0]));
            }
        }
        firsts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        firsts.dedup();
        assert!(firsts.len() > 50, "the other routes vary from lunge to lunge");
        // Cornered, there's still room for a few.
        for seed in 1..50 {
            check_routes(Vec2::new(12.0, 12.0), &crew, ENRAGED_ROUTES, seed);
        }
        // Nobody about: feints only.
        check_routes(pos, &[], ROUTES, 7);
    }

    #[test]
    fn a_lone_pirate_cant_just_stand_aside() {
        let pos = Vec2::new(120.0, 88.0);
        let pirate = Vec2::new(180.0, 88.0);
        for seed in 1..200 {
            let routes = check_routes(pos, &[pirate], ROUTES, seed);
            let straight = routes.iter().filter(|r| heading(pos, r).unwrap().angle_to(pirate - pos).abs() < 0.1).count();
            assert_eq!(straight, 1, "seed {seed}: only one route sets off straight at them");
            assert!(routes.iter().all(|r| r.contains(&pirate)), "seed {seed}: but every route comes for them");
        }
    }

    #[test]
    fn rampage_lanes_run_wall_to_wall() {
        let half = Vec2::splat(KEEPER_HALF);
        let floor = Rect { min: ROOM.min + half, max: ROOM.max - half };
        // Mid-room at someone to the right: from the left wall to the right.
        let [a, b] = rampage_lane(ROOM, half, Vec2::new(120.0, 88.0), Vec2::new(170.0, 88.0));
        assert!((a - Vec2::new(floor.min.x, 88.0)).length() < 1e-3);
        assert!((b - Vec2::new(floor.max.x, 88.0)).length() < 1e-3);
        // From a wall, across through someone and on to the far side.
        let from = Vec2::new(floor.min.x, 40.0);
        let pirate = Vec2::new(150.0, 120.0);
        let [a, b] = rampage_lane(ROOM, half, from, pirate);
        assert!(a.distance(from) < 1e-3, "starts where it's braced");
        let along = (b - a).normalize();
        assert!((pirate - a).perp_dot(along).abs() < 1e-3, "through them");
        assert!(b.distance(a) > pirate.distance(a), "and out past them");
        assert!((b.x - floor.max.x).abs() < 1e-3 || (b.y - floor.max.y).abs() < 1e-3, "to the far wall");
        // Someone hugging the same wall: it charges out across the room instead.
        let [_, b] = rampage_lane(ROOM, half, from, from + Vec2::new(-4.0, 6.0));
        assert!(b.distance(from) >= MIN_DASH);
        assert!(on_floor(b));
    }

    #[test]
    fn slams_land_round_it_and_reach_only_so_far() {
        let pos = Vec2::new(100.0, 100.0);
        let at = slam_spot(pos, 0.0);
        assert!(at.distance(pos + Vec2::new(SLAM_AHEAD, 0.0)) < 1e-3, "SLAM_AHEAD out, the way it struck");
        assert!(slam_reaches(at, at + Vec2::new(BRUTE_SLAM_RADIUS + 5.0, 0.0), 5.0));
        assert!(!slam_reaches(at, at + Vec2::new(BRUTE_SLAM_RADIUS + 6.0, 0.0), 5.0));
        // Whoever set it off, on the far side of the slam, is clear of it.
        let behind = pos - Vec2::new(SLAM_TRIGGER, 0.0);
        assert!(!slam_reaches(at, behind, 6.0), "it doesn't reach behind it");
        // Each slam swings well round from the one before.
        let mut rng = Rng::new(9);
        let mut last = slam_angle(&mut rng, None);
        for _ in 0..200 {
            let next = slam_angle(&mut rng, Some(last));
            let turn = Vec2::from_angle(next).angle_to(Vec2::from_angle(last)).abs();
            assert!(turn >= SLAM_TURN - 1e-3, "turned only {turn}");
            last = next;
        }
    }

    #[test]
    fn roars_never_raise_more_than_the_cap() {
        assert_eq!((summon_count(false, 0, 1), summon_count(true, 0, 1)), (2, MIN_CAP), "a lone pirate isn't swamped");
        assert_eq!((summon_count(false, 0, 3), summon_count(true, 0, 3)), (4, 6));
        assert_eq!(summon_count(false, 0, 5), BASE_SUMMONS + SUMMON_CREW, "a big crew calls no more each roar");
        assert_eq!((summon_cap(1), summon_cap(2), summon_cap(3), summon_cap(9)), (MIN_CAP, 6, 9, MAX_SUMMONS));
        assert_eq!(summon_count(true, summon_cap(3) - 1, 3), 1, "shrinks near the cap");
        for crew in 0..5 {
            for alive in 0..MAX_SUMMONS + 3 {
                for enraged in [false, true] {
                    assert!(alive + summon_count(enraged, alive, crew) <= summon_cap(crew).max(alive));
                }
            }
        }
        assert_eq!(summon_count(false, MAX_SUMMONS, 3), 0);
        assert_eq!(summon_count(true, summon_cap(1), 1), 0);
        // A roar brings as many different kinds as it can, and over a fight, all of them.
        let mut rng = Rng::new(3);
        for count in 0..12 {
            let kinds = summon_kinds(&mut rng, count);
            assert_eq!(kinds.len(), count);
            for kind in SUMMON_KINDS {
                let n = kinds.iter().filter(|k| **k == kind).count();
                assert!(n == count / 5 || n == count / 5 + 1, "{count} guards bring {n} of {kind:?}");
            }
        }
    }

    #[test]
    fn guards_appear_on_open_floor_away_from_the_crew() {
        // The vault itself, in the middle of the room, is no place for a guard.
        let vault = Rect::new(104.0, 72.0, 136.0, 104.0);
        let open = |p: Vec2| ROOM.contains(p) && !vault.inflate(SUMMON_HALF).contains(p);
        let crew = [Vec2::new(40.0, 40.0), Vec2::new(200.0, 140.0)];
        for seed in 1..200 {
            let mut rng = Rng::new(seed);
            let spots = summon_spots(ROOM, &crew, ENRAGED_SUMMONS, open, &mut rng);
            assert_eq!(spots.len(), ENRAGED_SUMMONS, "seed {seed}: room for them all");
            for (i, p) in spots.iter().enumerate() {
                assert!(open(*p) && *p == p.round(), "seed {seed}: on open floor, whole pixels");
                assert!(p.cmpge(ROOM.min + SUMMON_INSET).all() && p.cmple(ROOM.max - SUMMON_INSET).all(), "seed {seed}: off the walls");
                assert!(crew.iter().all(|c| c.distance(*p) >= SUMMON_CLEAR), "seed {seed}: well clear of the crew");
                assert!(spots[i + 1..].iter().all(|q| q.distance(*p) >= SUMMON_SPREAD), "seed {seed}: spread out");
            }
        }
        // The crew all over the room: closer in, but never on top of anyone.
        let crowd: Vec<Vec2> = (0..5).flat_map(|x| (0..4).map(move |y| Vec2::new(24.0 + x as f32 * 48.0, 22.0 + y as f32 * 44.0))).collect();
        let mut rng = Rng::new(9);
        let spots = summon_spots(ROOM, &crowd, SUMMON_CREW, open, &mut rng);
        assert!(spots.iter().all(|p| crowd.iter().all(|c| c.distance(*p) >= SUMMON_CLEAR / 2.0)));
        // No floor to speak of, or none open: nobody.
        assert!(summon_spots(Rect::new(0.0, 0.0, 16.0, 16.0), &[], SUMMON_CREW, |_| true, &mut rng).is_empty());
        assert!(summon_spots(ROOM, &[], SUMMON_CREW, |_| false, &mut rng).is_empty());
        assert_eq!(summon_spots(ROOM, &crew, 0, open, &mut rng).len(), 0);
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
