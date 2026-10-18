//! Every vault room is sealed off behind a gate. A pirate stepping up to it
//! calls the crew to breach; once every living pirate on the deck has accepted,
//! the whole crew is let in together and the gate locks behind them until the
//! vault's keeper (see `boss.rs`) is down. Then it opens for good.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::boss::Keeper;
use crate::combat::Health;
use crate::mission::Banner;
use crate::net::authority;
use crate::player::{Controls, PirateStatus, Player, crew_spot};
use crate::room::{GateSpot, Heist, RoomGrid, TILE};
use crate::vault::VaultTier;
use crate::{GameState, Level};

/// A pirate within this distance of `Breach::outside` calls the vote.
pub const GATE_REACH: f32 = 28.0;
/// Seconds the crew has to answer before the call lapses.
pub const VOTE_TIME: f32 = 20.0;
/// Room a pirate leaves around themselves when let in, so nobody lands in a wall.
const LANDING_HALF: Vec2 = Vec2::new(6.0, 4.0);

pub struct BreachPlugin;

impl Plugin for BreachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                run_breach.run_if(in_state(GameState::Playing)).run_if(authority),
                sync_gate,
            )
                .chain(),
        );
    }
}

/// Where the breach stands.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum BreachState {
    /// The gate is shut and nobody has called for a breach.
    Sealed,
    /// `by` (a `Player::owner`) stepped up to the gate and called the crew in.
    /// `voters` are every living pirate on the deck still in the heist;
    /// `accepted` those who said yes (`by` included). `left` is whole seconds
    /// before the call lapses.
    Vote { by: u64, voters: Vec<u64>, accepted: Vec<u64>, left: f32 },
    /// The crew is inside with the gate locked behind them, fighting the keeper.
    Fight,
    /// The keeper is down: the gate stays open and the vault can be looted.
    Open,
}

/// The vault room's gate and the crew's breach of it, one per deck. Its
/// `Transform` sits at `gate`. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Breach {
    pub state: BreachState,
    pub tier: VaultTier,
    /// World centre of the two-tile gate.
    pub gate: Vec2,
    /// The gate runs up and down, in the room's left or right wall; otherwise
    /// it runs across, in the top or bottom wall.
    pub vertical: bool,
    /// Just outside the gate, where stepping up calls the vote.
    pub outside: Vec2,
    /// Just inside the gate, where the crew is let in.
    pub inside: Vec2,
}

impl Breach {
    /// The gate is open: the keeper is down.
    pub fn is_open(&self) -> bool {
        self.state == BreachState::Open
    }
}

/// Host only: puts the sealed gate of `deck`'s vault room at `gate`.
pub fn spawn_breach(commands: &mut Commands, gate: &GateSpot, deck: u8) {
    commands.spawn((
        Level,
        Replicated,
        Breach {
            state: BreachState::Sealed,
            tier: VaultTier::for_deck(deck),
            gate: gate.centre,
            vertical: gate.vertical,
            outside: gate.outside,
            inside: gate.inside,
        },
        Transform::from_translation(gate.centre.extend(1.0)),
    ));
}

/// A living pirate still in the heist on the current deck, as the breach sees them.
struct Pirate {
    owner: u64,
    slot: u8,
    pos: Vec2,
    accept: bool,
    decline: bool,
}

/// Runs the deck's breach: a pirate stepping up to the gate calls the vote,
/// everyone on the deck answers (Y / N), and once they all say yes the crew is
/// let in together and the keeper wakes. Once it's down the gate opens for good.
fn run_breach(
    time: Res<Time>,
    heist: Res<Heist>,
    grid: Option<Res<RoomGrid>>,
    mut banner: ResMut<Banner>,
    mut breaches: Query<&mut Breach>,
    mut players: Query<(&Player, &Health, &Controls, &mut Transform)>,
    mut keepers: Query<(&mut Keeper, &mut Health), Without<Player>>,
    // Exact seconds left on the call; `Vote::left` only carries whole ones.
    mut clock: Local<f32>,
    // Pirates near the gate when a call was turned down or lapsed: they can't
    // call again until they've stepped away from it.
    mut held_back: Local<Vec<u64>>,
) {
    let Some(grid) = grid else { return };
    let Ok(mut breach) = breaches.single_mut() else { return };
    let crew: Vec<Pirate> = players
        .iter()
        .filter(|(p, h, ..)| !h.is_dead() && p.status == PirateStatus::Active && p.deck == heist.deck)
        .map(|(p, _, c, t)| Pirate {
            owner: p.owner,
            slot: p.slot,
            pos: t.translation.truncate(),
            accept: c.accept,
            decline: c.decline,
        })
        .collect();
    let outside = breach.outside;
    held_back.retain(|&owner| crew.iter().any(|p| p.owner == owner && p.pos.distance(outside) <= 2.0 * GATE_REACH));

    let mut next = breach.state.clone();
    let caller = crew
        .iter()
        .find(|p| p.pos.distance(outside) <= GATE_REACH && !held_back.contains(&p.owner));
    if let Some(caller) = caller.filter(|_| next == BreachState::Sealed) {
        *clock = VOTE_TIME;
        next = BreachState::Vote {
            by: caller.owner,
            voters: Vec::new(),
            accepted: vec![caller.owner],
            left: VOTE_TIME,
        };
    }

    match &mut next {
        BreachState::Vote { voters, accepted, left, .. } => {
            let mut owners: Vec<u64> = crew.iter().map(|p| p.owner).collect();
            owners.sort_unstable();
            *voters = owners;
            accepted.retain(|owner| voters.contains(owner));
            for pirate in crew.iter().filter(|p| p.accept) {
                if !accepted.contains(&pirate.owner) {
                    accepted.push(pirate.owner);
                }
            }
            *clock -= time.delta_secs();
            *left = clock.max(0.0).ceil();

            let declined = crew.iter().find(|p| p.decline);
            if voters.is_empty() {
                next = BreachState::Sealed;
            } else if voters.iter().all(|owner| accepted.contains(owner)) {
                let voters = voters.clone();
                let_in(&grid, &voters, &mut players);
                let mut name = "VAULT KEEPER";
                for (mut keeper, mut health) in &mut keepers {
                    keeper.wake(&mut health, voters.len());
                    name = keeper.kind.name();
                }
                let text = format!("THE {name} WAKES  -  the gate locks behind you");
                banner.show(text, Color::srgb(1.0, 0.4, 0.3), 4.0);
                next = BreachState::Fight;
            } else if declined.is_some() || *clock <= 0.0 {
                let text = match declined {
                    Some(p) => format!("P{} ISN'T READY TO BREACH THE VAULT", p.slot + 1),
                    None => "THE CALL TO BREACH THE VAULT LAPSED".to_string(),
                };
                banner.show(text, Color::srgb(1.0, 0.75, 0.35), 3.0);
                // Whoever's at the gate has to step away before calling again.
                *held_back = crew
                    .iter()
                    .filter(|p| p.pos.distance(outside) <= 2.0 * GATE_REACH)
                    .map(|p| p.owner)
                    .collect();
                next = BreachState::Sealed;
            }
        }
        BreachState::Fight => {
            // The keeper's body is despawned once it falls.
            if keepers.iter().all(|(_, health)| health.is_dead()) {
                banner.show("KEEPER DOWN  -  the vault is yours", breach.tier.color(), 4.0);
                next = BreachState::Open;
            }
        }
        BreachState::Sealed | BreachState::Open => {}
    }
    if breach.state != next {
        breach.state = next;
    }
}

/// Moves every voter through the gate onto open floor just inside, packed
/// around a spot a couple of tiles in.
fn let_in(grid: &RoomGrid, voters: &[u64], players: &mut Query<(&Player, &Health, &Controls, &mut Transform)>) {
    let inward = (grid.gate.inside - grid.gate.outside).normalize_or_zero();
    let landing = grid.gate.inside + inward * TILE * 2.0;
    let mut crew: Vec<_> = players.iter_mut().filter(|(p, ..)| voters.contains(&p.owner)).collect();
    crew.sort_by_key(|(p, ..)| p.slot);
    let mut spot = 0;
    for (_, _, _, transform) in &mut crew {
        let pos = loop {
            // Past the packed spots, everyone left shares the landing.
            if spot >= 9 {
                break landing;
            }
            let pos = crew_spot(landing, spot);
            spot += 1;
            if !grid.collides(pos, LANDING_HALF) {
                break pos;
            }
        };
        transform.translation = pos.extend(transform.translation.z);
    }
}

/// Every machine: lets the gate's tiles through once the deck's `Breach` is open.
fn sync_gate(grid: Option<ResMut<RoomGrid>>, breaches: Query<&Breach>) {
    let Some(mut grid) = grid else { return };
    // A replicated gate from the deck before can linger a moment on a client.
    let centre = grid.gate.centre;
    let open = breaches.iter().find(|b| b.gate.distance(centre) < 1.0).is_some_and(Breach::is_open);
    if grid.gate_open() != open {
        grid.set_gate_open(open);
    }
}
