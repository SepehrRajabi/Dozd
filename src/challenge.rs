//! Challenges: optional goals the host sets during a heist. Each has a goal,
//! how long it stays open, who it's for, and a reward for completing it. The
//! host tracks progress; everything the HUD needs is replicated on the
//! `Challenge` entity and on each pirate's `Rewards`.

//!
//! To add a challenge, add an entry to `catalog`. A new kind of goal, span,
//! target or reward needs a variant here plus its match arms: for a goal, in
//! `Goal::needed`/`Goal::describe` and the tracking in `track_challenges`; for
//! a reward, in `Reward::secs`/`Reward::describe` and wherever it takes effect
//! (see `Rewards`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::class::PirateClass;
use crate::combat::{CrewHit, Health};
use crate::mission::Banner;
use crate::net::authority;
use crate::player::{PirateStatus, Player};
use crate::room::Heist;
use crate::{GameState, Level};

/// Seconds after the crew reaches a deck before its challenges are offered.
pub const OFFER_DELAY: f32 = 10.0;
/// Seconds the offer and completion banners stay up.
const BANNER_SECS: f32 = 5.0;
const BANNER_COLOR: Color = Color::srgb(1.0, 0.8, 0.3);
/// Rewards count down in steps this long, so they replicate a few times a
/// second rather than every frame.
const REWARD_STEP: f32 = 0.1;

pub struct ChallengePlugin;

impl Plugin for ChallengePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChallengeLog>()
            .add_systems(OnEnter(GameState::Playing), reset_challenges.run_if(authority))
            .add_systems(
                Update,
                (offer_challenges, track_challenges, clear_finished, tick_rewards)
                    .chain()
                    .run_if(in_state(GameState::Playing))
                    .run_if(authority),
            );
    }
}

/// Every challenge the host may offer on reaching `deck`, before checking
/// who's in the crew. Add entries here.
pub fn catalog(deck: u8) -> Vec<Challenge> {
    vec![Challenge::new(
        "KILLING SPREE",
        Goal::KillStreak { kills: 5, window: 15.0 },
        Span::Deck(deck),
        Target::Class(PirateClass::Gunner),
        Recipients::Completer,
        Reward::Damage { bonus: 0.3, secs: 45.0 },
    )]
}

/// What has to be done.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Goal {
    /// Kill `kills` guards with your own weapons within any `window` seconds.
    KillStreak { kills: u32, window: f32 },
}

impl Goal {
    /// What `Progress::count` has to reach.
    pub fn needed(self) -> u32 {
        match self {
            Self::KillStreak { kills, .. } => kills,
        }
    }

    /// The goal in words, e.g. "kill 5 guards within 15 s".
    pub fn describe(self) -> String {
        match self {
            Self::KillStreak { kills, window } => format!("kill {kills} guards within {window:.0} s"),
        }
    }
}

/// How long a challenge stays open before it fails.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Span {
    /// Until the heist ends.
    Heist,
    /// Until the crew leaves the deck it was set on.
    Deck(u8),
    /// For this many seconds after it's set; `Challenge::left` counts down.
    Ttl(f32),
}

impl Span {
    /// Whether it has run out, with the crew on `deck` and `left` seconds on
    /// the clock (`Challenge::left`).
    pub fn over(self, deck: u8, left: Option<f32>) -> bool {
        match self {
            Self::Heist => false,
            Self::Deck(d) => deck != d,
            Self::Ttl(_) => left.is_some_and(|l| l <= 0.0),
        }
    }
}

/// Who can complete a challenge.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// Any pirate in the crew.
    Crew,
    /// Only pirates playing this class.
    Class(PirateClass),
}

impl Target {
    /// Whether a pirate playing `class` can complete it.
    pub fn admits(self, class: PirateClass) -> bool {
        match self {
            Self::Crew => true,
            Self::Class(c) => c == class,
        }
    }

    /// For banners, e.g. "GUNNER".
    pub fn name(self) -> &'static str {
        match self {
            Self::Crew => "CREW",
            Self::Class(c) => c.name(),
        }
    }
}

/// Who gets the reward when it's completed.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recipients {
    /// Only the pirate who completed it.
    Completer,
    /// Every pirate still in the heist.
    Crew,
}

impl Recipients {
    /// Who gets the reward when `by` completes it, out of the pirates still in
    /// the heist (`Player::owner`s).
    pub fn pick(self, by: u64, crew: &[u64]) -> Vec<u64> {
        match self {
            Self::Completer => vec![by],
            Self::Crew => crew.to_vec(),
        }
    }
}

/// What completing a challenge gives. More kinds will follow.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Reward {
    /// Guns and abilities hit `1 + bonus` times as hard for `secs`.
    Damage { bonus: f32, secs: f32 },
}

impl Reward {
    /// Seconds it lasts once granted.
    pub fn secs(self) -> f32 {
        match self {
            Self::Damage { secs, .. } => secs,
        }
    }

    /// The reward in words, e.g. "+30% damage".
    pub fn describe(self) -> String {
        match self {
            Self::Damage { bonus, .. } => format!("+{:.0}% damage", bonus * 100.0),
        }
    }

    /// Whether `other` is the same kind, so getting it again restarts this.
    pub fn same_kind(self, other: Reward) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

/// Where a challenge stands.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChallengeState {
    Open,
    /// Completed by the pirate with this `Player::owner`.
    Done { by: u64 },
    /// Ran out of time, deck or heist.
    Failed,
}

/// One pirate's progress on a challenge.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Progress {
    /// `Player::owner`.
    pub owner: u64,
    /// How far along: e.g. kills inside the current window.
    pub count: u32,
    /// Seconds until the oldest counted kill drops out of the window; 0 with none.
    pub expires: f32,
}

/// A challenge in play. The host spawns and updates it; every machine sees it.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Challenge {
    /// Short title for the HUD, e.g. "KILLING SPREE".
    pub name: String,
    pub goal: Goal,
    pub span: Span,
    pub target: Target,
    pub recipients: Recipients,
    pub reward: Reward,
    pub state: ChallengeState,
    /// Seconds left before a `Span::Ttl` challenge fails; `None` for other spans.
    pub left: Option<f32>,
    /// Every eligible pirate's progress.
    pub progress: Vec<Progress>,
}

impl Challenge {
    /// A fresh, open challenge with no progress yet.
    pub fn new(name: &str, goal: Goal, span: Span, target: Target, recipients: Recipients, reward: Reward) -> Self {
        Self {
            name: name.into(),
            goal,
            span,
            target,
            recipients,
            reward,
            state: ChallengeState::Open,
            left: match span {
                Span::Ttl(secs) => Some(secs),
                _ => None,
            },
            progress: Vec::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == ChallengeState::Open
    }

    /// The offer banner, e.g. "GUNNER CHALLENGE  -  kill 5 guards within 15 s
    /// for +30% damage".
    pub fn pitch(&self) -> String {
        format!("{} CHALLENGE  -  {} for {}", self.target.name(), self.goal.describe(), self.reward.describe())
    }
}

/// A reward a pirate is enjoying right now.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct ActiveReward {
    pub reward: Reward,
    /// Seconds left, out of `total`.
    pub left: f32,
    pub total: f32,
}

/// The rewards a pirate is enjoying, on their pirate entity.
///
/// To add a reward kind: a `Reward` variant, its arms in `Reward::secs` and
/// `Reward::describe`, and a reader like `Rewards::damage` used wherever it
/// takes effect. `grant` and `tick` handle every kind already.
#[derive(Component, Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct Rewards(pub Vec<ActiveReward>);

impl Rewards {
    /// Hands over `reward`. Getting a kind already running restarts it, with
    /// the new numbers, rather than stacking.
    pub fn grant(&mut self, reward: Reward) {
        let active = ActiveReward { reward, left: reward.secs(), total: reward.secs() };
        match self.0.iter_mut().find(|a| a.reward.same_kind(reward)) {
            Some(existing) => *existing = active,
            None => self.0.push(active),
        }
    }

    /// Counts every reward down by `dt`, dropping the ones that run out.
    pub fn tick(&mut self, dt: f32) {
        for active in &mut self.0 {
            active.left = (active.left - dt).max(0.0);
        }
        self.0.retain(|a| a.left > 0.0);
    }

    /// What their guns' and abilities' damage is multiplied by; 1 with none.
    pub fn damage(&self) -> f32 {
        self.0
            .iter()
            .map(|a| match a.reward {
                Reward::Damage { bonus, .. } => 1.0 + bonus,
            })
            .product()
    }
}

/// Kill times inside a rolling window, oldest first.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Streak(Vec<f32>);

impl Streak {
    /// Forgets kills more than `window` seconds before `now`; whether any went.
    pub fn prune(&mut self, now: f32, window: f32) -> bool {
        let before = self.0.len();
        self.0.retain(|t| now - t < window);
        self.0.len() != before
    }

    pub fn push(&mut self, now: f32) {
        self.0.push(now);
    }

    pub fn count(&self) -> u32 {
        self.0.len() as u32
    }

    /// Seconds until the oldest kill drops out of the window; 0 with none.
    pub fn expires(&self, now: f32, window: f32) -> f32 {
        self.0.first().map_or(0.0, |t| (t + window - now).max(0.0))
    }
}

/// Host only: this run's challenge bookkeeping, and tallies a headless run
/// can read. Reset when a run starts.
#[derive(Resource, Default)]
pub struct ChallengeLog {
    /// Seconds played this run: the clock kill windows run on.
    clock: f32,
    /// The deck the crew was last seen on, and seconds since it got there.
    deck: Option<u8>,
    on_deck: f32,
    /// Whether this deck's challenges have been offered yet.
    offered_here: bool,
    /// Challenges offered, completed and failed this run.
    pub offered: u32,
    pub completed: u32,
    pub failed: u32,
}

/// Host only, beside a `Challenge`: the exact numbers behind what it shows.
#[derive(Component, Default)]
struct Tally {
    /// Each eligible pirate's recent kills, by `Player::owner`.
    streaks: Vec<(u64, Streak)>,
    /// Exact seconds left on a `Span::Ttl`; `Challenge::left` is rounded.
    left: Option<f32>,
}

/// Still in the heist and able to act.
fn in_play(player: &Player, health: &Health) -> bool {
    player.status == PirateStatus::Active && !health.is_dead()
}

/// To the nearest tenth of a second, so the HUD's numbers don't replicate
/// every frame.
fn tenths(secs: f32) -> f32 {
    (secs * 10.0).round() / 10.0
}

fn reset_challenges(mut log: ResMut<ChallengeLog>) {
    *log = ChallengeLog::default();
}

/// Offers the catalog `OFFER_DELAY` after the crew reaches each deck: every
/// challenge someone in the crew can complete, once per deck.
fn offer_challenges(
    mut commands: Commands,
    time: Res<Time>,
    heist: Res<Heist>,
    mut log: ResMut<ChallengeLog>,
    mut banner: ResMut<Banner>,
    players: Query<(&Player, &Health)>,
) {
    let dt = time.delta_secs();
    log.clock += dt;
    if log.deck != Some(heist.deck) {
        log.deck = Some(heist.deck);
        log.on_deck = 0.0;
        log.offered_here = false;
    }
    log.on_deck += dt;
    if log.offered_here || log.on_deck < OFFER_DELAY {
        return;
    }
    log.offered_here = true;

    let classes: Vec<PirateClass> = players.iter().filter(|(p, h)| in_play(p, h)).map(|(p, _)| p.class).collect();
    let mut pitches = Vec::new();
    for challenge in catalog(heist.deck) {
        if !classes.iter().any(|c| challenge.target.admits(*c)) {
            continue;
        }
        pitches.push(challenge.pitch());
        log.offered += 1;
        let tally = Tally { left: challenge.left, ..default() };
        commands.spawn((Level, Replicated, tally, challenge));
    }
    if !pitches.is_empty() {
        banner.show(pitches.join("\n"), BANNER_COLOR, BANNER_SECS);
    }
}

/// Counts each eligible pirate's progress, publishes it when it changes, and
/// settles open challenges: done, with the reward handed out, or failed when
/// their span runs out or nobody who could complete them is left.
fn track_challenges(
    time: Res<Time>,
    heist: Res<Heist>,
    mut hits: MessageReader<CrewHit>,
    mut log: ResMut<ChallengeLog>,
    mut banner: ResMut<Banner>,
    mut challenges: Query<(&mut Challenge, &mut Tally)>,
    mut players: Query<(&Player, &Health, &mut Rewards)>,
) {
    let now = log.clock;
    let dt = time.delta_secs();
    let kills: Vec<u64> = hits.read().filter(|h| h.killed).map(|h| h.owner).collect();
    let crew: Vec<u64> = players.iter().filter(|(p, h, _)| in_play(p, h)).map(|(p, ..)| p.owner).collect();

    for (mut challenge, mut tally) in &mut challenges {
        if !challenge.is_open() {
            continue;
        }
        let eligible: Vec<u64> = players
            .iter()
            .filter(|(p, h, _)| in_play(p, h) && challenge.target.admits(p.class))
            .map(|(p, ..)| p.owner)
            .collect();

        // New goals get an arm here, feeding their own tally. `lapsed` is
        // whether someone's attempt ran out: a streak's window closed on them.
        let mut lapsed = false;
        let progress: Vec<Progress> = match challenge.goal {
            Goal::KillStreak { window, .. } => {
                tally.streaks.retain(|(owner, _)| eligible.contains(owner));
                for owner in &eligible {
                    if !tally.streaks.iter().any(|(o, _)| o == owner) {
                        tally.streaks.push((*owner, Streak::default()));
                    }
                }
                tally
                    .streaks
                    .iter_mut()
                    .map(|(owner, streak)| {
                        lapsed |= streak.prune(now, window);
                        for _ in kills.iter().filter(|k| *k == owner) {
                            streak.push(now);
                        }
                        Progress { owner: *owner, count: streak.count(), expires: tenths(streak.expires(now, window)) }
                    })
                    .collect()
            }
        };
        if challenge.progress != progress {
            challenge.progress = progress;
        }

        tally.left = tally.left.map(|l| (l - dt).max(0.0));
        // Rounded up, so it only reads 0 once it's actually out.
        let left = tally.left.map(|l| (l * 10.0).ceil() / 10.0);
        if challenge.left != left {
            challenge.left = left;
        }

        let needed = challenge.goal.needed();
        if let Some(by) = challenge.progress.iter().find(|p| p.count >= needed).map(|p| p.owner) {
            challenge.state = ChallengeState::Done { by };
            log.completed += 1;
            let recipients = challenge.recipients.pick(by, &crew);
            for (_, _, mut rewards) in players.iter_mut().filter(|(p, ..)| recipients.contains(&p.owner)) {
                rewards.grant(challenge.reward);
            }
            if let Some((player, ..)) = players.iter().find(|(p, ..)| p.owner == by) {
                let whom = match challenge.recipients {
                    Recipients::Completer => "",
                    Recipients::Crew => " for the crew",
                };
                banner.show(
                    format!(
                        "P{} {} completed {}  -  {}{whom} for {:.0} s",
                        player.slot + 1,
                        player.class.name(),
                        challenge.name,
                        challenge.reward.describe(),
                        challenge.reward.secs()
                    ),
                    BANNER_COLOR,
                    BANNER_SECS,
                );
            }
        } else if lapsed || eligible.is_empty() || challenge.span.over(heist.deck, tally.left) {
            if lapsed {
                banner.show(format!("{} FAILED  -  out of time", challenge.name), BANNER_COLOR, BANNER_SECS);
            }
            challenge.state = ChallengeState::Failed;
            log.failed += 1;
        }
    }
}

/// Clears away challenges as soon as they're done or failed; the banner has
/// already told the crew how it went.
fn clear_finished(mut commands: Commands, challenges: Query<(Entity, &Challenge)>) {
    for (entity, challenge) in &challenges {
        if !challenge.is_open() {
            commands.entity(entity).despawn();
        }
    }
}

/// Counts rewards down in `REWARD_STEP`s, and takes them from the fallen.
fn tick_rewards(time: Res<Time>, log: Res<ChallengeLog>, mut players: Query<(&Health, &mut Rewards)>) {
    let steps = (log.clock / REWARD_STEP).floor() - ((log.clock - time.delta_secs()) / REWARD_STEP).floor();
    for (health, mut rewards) in &mut players {
        if rewards.0.is_empty() {
            continue;
        }
        if health.is_dead() {
            rewards.0.clear();
        } else if steps > 0.0 {
            rewards.tick(steps * REWARD_STEP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPREE: Goal = Goal::KillStreak { kills: 5, window: 15.0 };

    /// Feeds kills at `times` into a streak, returning the count after each.
    fn run(times: &[f32], window: f32) -> Vec<u32> {
        let mut streak = Streak::default();
        times
            .iter()
            .map(|&t| {
                streak.prune(t, window);
                streak.push(t);
                streak.count()
            })
            .collect()
    }

    #[test]
    fn kills_expire_after_the_window() {
        let mut streak = Streak::default();
        streak.push(0.0);
        streak.push(5.0);
        streak.prune(14.9, 15.0);
        assert_eq!(streak.count(), 2);
        assert!((streak.expires(14.9, 15.0) - 0.1).abs() < 1e-4);
        streak.prune(15.0, 15.0);
        assert_eq!(streak.count(), 1);
        streak.prune(20.0, 15.0);
        assert_eq!(streak.count(), 0);
        assert_eq!(streak.expires(20.0, 15.0), 0.0);
    }

    #[test]
    fn exactly_n_kills_inside_the_window_completes() {
        let counts = run(&[0.0, 3.0, 6.0, 9.0, 14.9], 15.0);
        assert_eq!(*counts.last().unwrap(), SPREE.needed());
        // The fifth a moment too late: the first has dropped out.
        let counts = run(&[0.0, 3.0, 6.0, 9.0, 15.0], 15.0);
        assert_eq!(*counts.last().unwrap(), SPREE.needed() - 1);
    }

    #[test]
    fn spans_run_out() {
        assert!(!Span::Heist.over(2, None));
        assert!(!Span::Deck(1).over(1, None));
        assert!(Span::Deck(1).over(2, None));
        assert!(!Span::Ttl(30.0).over(0, Some(0.1)));
        assert!(Span::Ttl(30.0).over(0, Some(0.0)));
        let challenge = Challenge::new("T", SPREE, Span::Ttl(30.0), Target::Crew, Recipients::Crew, Reward::Damage { bonus: 0.1, secs: 1.0 });
        assert_eq!(challenge.left, Some(30.0));
    }

    #[test]
    fn recipients_are_the_completer_or_the_crew() {
        let crew = [1, 2, 3];
        assert_eq!(Recipients::Completer.pick(2, &crew), vec![2]);
        assert_eq!(Recipients::Crew.pick(2, &crew), vec![1, 2, 3]);
    }

    #[test]
    fn targets_admit_their_class() {
        assert!(Target::Crew.admits(PirateClass::Doctor));
        assert!(Target::Class(PirateClass::Gunner).admits(PirateClass::Gunner));
        assert!(!Target::Class(PirateClass::Gunner).admits(PirateClass::Hacker));
    }

    #[test]
    fn rewards_refresh_rather_than_stack() {
        let mut rewards = Rewards::default();
        rewards.grant(Reward::Damage { bonus: 0.3, secs: 45.0 });
        rewards.tick(30.0);
        assert_eq!(rewards.0[0].left, 15.0);
        rewards.grant(Reward::Damage { bonus: 0.3, secs: 45.0 });
        assert_eq!(rewards.0.len(), 1);
        assert_eq!(rewards.0[0].left, 45.0);
        rewards.tick(45.0);
        assert!(rewards.0.is_empty());
    }

    #[test]
    fn damage_multiplier() {
        let mut rewards = Rewards::default();
        assert_eq!(rewards.damage(), 1.0);
        rewards.grant(Reward::Damage { bonus: 0.3, secs: 45.0 });
        rewards.grant(Reward::Damage { bonus: 0.3, secs: 45.0 });
        assert!((rewards.damage() - 1.3).abs() < 1e-6);
        rewards.tick(46.0);
        assert_eq!(rewards.damage(), 1.0);
    }

    /// The challenge systems alone, on a host, with time stepping 0.1 s a frame.
    mod sim {
        use std::time::Duration;

        use bevy::state::app::StatesPlugin;
        use bevy::time::TimeUpdateStrategy;

        use super::super::*;
        use crate::abilities::Passive;
        use crate::net::NetMode;

        pub const STEP: f32 = 0.1;

        pub fn app(classes: &[PirateClass]) -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatesPlugin))
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(STEP)))
                .insert_resource(NetMode::Solo)
                .init_resource::<Heist>()
                .init_resource::<Banner>()
                .add_message::<CrewHit>()
                .init_state::<GameState>()
                .add_plugins(ChallengePlugin);
            for (i, &class) in classes.iter().enumerate() {
                app.world_mut().spawn((
                    Player {
                        owner: i as u64,
                        class,
                        passive: Passive::Supply,
                        active: None,
                        slot: i as u8,
                        credits: 0,
                        items: 0,
                        status: PirateStatus::Active,
                        left_early: false,
                        payout: 0,
                        deck: 0,
                    },
                    Health::new(100.0, 0.0),
                    Rewards::default(),
                ));
            }
            app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
            app.update();
            app
        }

        pub fn run_for(app: &mut App, secs: f32) {
            for _ in 0..(secs / STEP).round() as u32 {
                app.update();
            }
        }

        /// Kills by `owner`, all landing on the next frame.
        pub fn kill(app: &mut App, owner: u64, kills: u32) {
            for _ in 0..kills {
                app.world_mut().write_message(CrewHit { owner, dealt: 10.0, killed: true });
            }
            app.update();
        }

        pub fn challenges(app: &mut App) -> Vec<Challenge> {
            app.world_mut().query::<&Challenge>().iter(app.world()).cloned().collect()
        }

        pub fn log(app: &App) -> (u32, u32, u32) {
            let log = app.world().resource::<ChallengeLog>();
            (log.offered, log.completed, log.failed)
        }

        pub fn damage(app: &mut App, owner: u64) -> f32 {
            let mut q = app.world_mut().query::<(&Player, &Rewards)>();
            q.iter(app.world()).find(|(p, _)| p.owner == owner).map(|(_, r)| r.damage()).unwrap()
        }

        pub fn set_player(app: &mut App, owner: u64, change: impl Fn(&mut Player, &mut Health)) {
            let mut q = app.world_mut().query::<(&mut Player, &mut Health)>();
            for (mut p, mut h) in q.iter_mut(app.world_mut()) {
                if p.owner == owner {
                    change(&mut p, &mut h);
                }
            }
        }

        /// Up to just after the deck's challenges are offered.
        pub fn offered(classes: &[PirateClass]) -> App {
            let mut app = app(classes);
            run_for(&mut app, OFFER_DELAY + 0.5);
            app
        }
    }

    #[test]
    fn nothing_is_offered_without_a_gunner() {
        let mut app = sim::offered(&[PirateClass::Doctor, PirateClass::Bulwark]);
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (0, 0, 0));
    }

    #[test]
    fn offered_once_per_deck_after_the_delay() {
        let mut app = sim::app(&[PirateClass::Gunner]);
        sim::run_for(&mut app, OFFER_DELAY - 1.0);
        assert!(sim::challenges(&mut app).is_empty());
        sim::run_for(&mut app, 1.5);
        assert_eq!(sim::challenges(&mut app).len(), 1);
        sim::run_for(&mut app, 30.0);
        assert_eq!(sim::log(&app).0, 1);
    }

    #[test]
    fn only_the_gunners_kills_complete_it_and_only_they_get_the_reward() {
        let mut app = sim::offered(&[PirateClass::Gunner, PirateClass::Doctor]);
        sim::kill(&mut app, 1, 6);
        sim::kill(&mut app, 0, 4);
        let challenge = &sim::challenges(&mut app)[0];
        assert!(challenge.is_open());
        assert_eq!(challenge.progress, vec![Progress { owner: 0, count: 4, expires: 15.0 }]);
        sim::kill(&mut app, 0, 1);
        // Gone the moment it's done; the reward runs on for its 45 s, then ends.
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (1, 1, 0));
        assert!((sim::damage(&mut app, 0) - 1.3).abs() < 1e-6);
        assert_eq!(sim::damage(&mut app, 1), 1.0);
        sim::run_for(&mut app, 43.0);
        assert!(sim::damage(&mut app, 0) > 1.0);
        sim::run_for(&mut app, 2.5);
        assert_eq!(sim::damage(&mut app, 0), 1.0);
    }

    #[test]
    fn kills_before_the_offer_dont_count_and_a_lapsed_window_fails_it() {
        let mut app = sim::app(&[PirateClass::Gunner]);
        sim::run_for(&mut app, 2.0);
        sim::kill(&mut app, 0, 4);
        sim::run_for(&mut app, OFFER_DELAY);
        sim::kill(&mut app, 0, 4);
        sim::run_for(&mut app, 14.0);
        let challenge = &sim::challenges(&mut app)[0];
        assert!(challenge.is_open());
        assert_eq!(challenge.progress[0].count, 4, "only the kills since the offer");
        // The window closes one short: the attempt is over, and it's cleared away.
        sim::run_for(&mut app, 1.5);
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (1, 0, 1));
    }

    #[test]
    fn no_kills_yet_leaves_it_open() {
        let mut app = sim::offered(&[PirateClass::Gunner]);
        sim::run_for(&mut app, 30.0);
        assert!(sim::challenges(&mut app)[0].is_open(), "the window only starts with a kill");
    }

    #[test]
    fn leaving_the_deck_fails_it_and_the_next_deck_offers_again() {
        let mut app = sim::offered(&[PirateClass::Gunner]);
        sim::kill(&mut app, 0, 4);
        app.world_mut().resource_mut::<Heist>().deck = 1;
        app.update();
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (1, 0, 1));
        sim::run_for(&mut app, OFFER_DELAY);
        let open: Vec<_> = sim::challenges(&mut app).into_iter().filter(Challenge::is_open).collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].span, Span::Deck(1));
        // A fresh streak: the kills from deck 1 don't carry over.
        assert_eq!(open[0].progress[0].count, 0);
        assert_eq!(sim::log(&app).0, 2);
    }

    #[test]
    fn the_last_gunner_dying_fails_it_and_takes_their_reward() {
        let mut app = sim::offered(&[PirateClass::Gunner, PirateClass::Doctor]);
        sim::kill(&mut app, 0, 3);
        sim::set_player(&mut app, 0, |_, h| h.current = 0.0);
        app.update();
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (1, 0, 1));

        // A Gunner who completed it loses the reward on dying.
        let mut app = sim::offered(&[PirateClass::Gunner]);
        sim::kill(&mut app, 0, 5);
        assert!(sim::damage(&mut app, 0) > 1.0);
        sim::set_player(&mut app, 0, |_, h| h.current = 0.0);
        app.update();
        assert_eq!(sim::damage(&mut app, 0), 1.0);
    }

    #[test]
    fn one_gunner_dying_leaves_it_open_for_another() {
        let mut app = sim::offered(&[PirateClass::Gunner, PirateClass::Gunner]);
        sim::kill(&mut app, 0, 4);
        sim::set_player(&mut app, 0, |_, h| h.current = 0.0);
        sim::kill(&mut app, 1, 5);
        assert_eq!(sim::log(&app), (1, 1, 0));
        assert!(sim::damage(&mut app, 1) > 1.0);
    }

    #[test]
    fn extracting_with_it_open_fails_it() {
        let mut app = sim::offered(&[PirateClass::Gunner, PirateClass::Doctor]);
        sim::kill(&mut app, 0, 2);
        sim::set_player(&mut app, 0, |p, _| p.status = PirateStatus::Extracted);
        app.update();
        assert!(sim::challenges(&mut app).is_empty());
        assert_eq!(sim::log(&app), (1, 0, 1));
    }

    #[test]
    fn a_new_run_resets_the_log() {
        let mut app = sim::offered(&[PirateClass::Gunner]);
        sim::kill(&mut app, 0, 5);
        assert_eq!(sim::log(&app), (1, 1, 0));
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Over);
        app.update();
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        assert_eq!(sim::log(&app), (0, 0, 0));
        // Challenges are `Level` entities, so `room::clear_level` sweeps them
        // away as the new run starts (that system isn't part of this app).
        let mut level = app.world_mut().query_filtered::<(), (With<Challenge>, Without<Level>)>();
        assert_eq!(level.iter(app.world()).count(), 0);
        // And the new run offers afresh.
        sim::run_for(&mut app, OFFER_DELAY + 0.5);
        assert_eq!(sim::log(&app).0, 1);
    }

    #[test]
    fn the_pitch_reads_right() {
        let challenge = &catalog(0)[0];
        assert_eq!(challenge.pitch(), "GUNNER CHALLENGE  -  kill 5 guards within 15 s for +30% damage");
        assert_eq!(challenge.span, Span::Deck(0));
    }
}
