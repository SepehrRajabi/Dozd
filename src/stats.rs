//! What each pirate did over a run, for the results screen.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{CrewHit, SentryHit};
use crate::net::authority;
use crate::player::Player;

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        // Not tied to `GameState::Playing`, and after `Update`, so whatever
        // lands on the frame the run ends is in before the results go up.
        app.add_systems(PostUpdate, tally_hits.run_if(authority));
    }
}

/// A pirate's kills and damage done this run, kept by the host and replicated
/// for the results screen. Their damage taken is `Health::taken`, and the
/// credits they picked up are `Player::credits`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Debug)]
pub struct RunStats {
    pub kills: u32,
    pub dealt: f32,
}

/// Adds each hit to the stats of the pirate who landed it, their own weapons
/// and their sentries alike.
fn tally_hits(mut crew: MessageReader<CrewHit>, mut sentries: MessageReader<SentryHit>, mut players: Query<(&Player, &mut RunStats)>) {
    let own = crew.read().map(|hit| (hit.owner, hit.dealt, hit.killed));
    let sentry = sentries.read().map(|hit| (hit.owner, hit.dealt, hit.killed));
    for (owner, dealt, killed) in own.chain(sentry) {
        if dealt <= 0.0 && !killed {
            continue;
        }
        if let Some((_, mut stats)) = players.iter_mut().find(|(p, _)| p.owner == owner) {
            stats.dealt += dealt;
            stats.kills += killed as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::abilities::Passive;
    use crate::class::{Ability, Loadout, Loadouts, PirateClass, Sentry};
    use crate::combat::Health;
    use crate::enemies::{Enemy, EnemyKind, spawn_enemy};
    use crate::net::{HOST_ID, NetMode, StartSession};
    use crate::player::{Controls, CrewSize, PirateStatus};
    use crate::room::{NextSeed, RoomGrid};
    use crate::{GamePlugins, GameState, Rng};

    /// Just the tally, in `state`, with a pirate for each of `owners`.
    fn tally(state: GameState, owners: &[u64]) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(NetMode::Solo)
            .init_state::<GameState>()
            .add_message::<CrewHit>()
            .add_message::<SentryHit>()
            .add_plugins(StatsPlugin);
        for &owner in owners {
            app.world_mut().spawn(Player {
                owner,
                class: PirateClass::Engineer,
                passive: Passive::Overclock,
                active: None,
                slot: 0,
                credits: 0,
                items: 0,
                status: PirateStatus::Active,
                left_early: false,
                payout: 0,
                deck: 0,
                name: String::new(),
            });
        }
        app.world_mut().resource_mut::<NextState<GameState>>().set(state);
        app.update();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), state);
        app
    }

    fn hit(app: &mut App, owner: u64, dealt: f32, killed: bool) {
        app.world_mut().write_message(CrewHit { owner, dealt, killed, drains: true });
    }

    fn stats(app: &mut App, owner: u64) -> RunStats {
        let mut q = app.world_mut().query::<(&Player, &RunStats)>();
        q.iter(app.world()).find(|(p, _)| p.owner == owner).map(|(_, s)| *s).expect("that pirate")
    }

    #[test]
    fn hits_add_up_per_pirate_and_a_kill_is_whoever_landed_it() {
        let mut app = tally(GameState::Playing, &[1, 2]);
        hit(&mut app, 1, 10.0, false);
        hit(&mut app, 2, 4.0, false);
        hit(&mut app, 1, 6.5, true);
        app.update();
        hit(&mut app, 2, 1.0, false);
        app.update();
        assert_eq!(stats(&mut app, 1), RunStats { kills: 1, dealt: 16.5 });
        assert_eq!(stats(&mut app, 2), RunStats { kills: 0, dealt: 5.0 });
    }

    #[test]
    fn a_hit_counts_once_the_run_is_over() {
        let mut app = tally(GameState::Over, &[1]);
        hit(&mut app, 1, 8.0, true);
        app.update();
        assert_eq!(stats(&mut app, 1), RunStats { kills: 1, dealt: 8.0 });
    }

    #[test]
    fn nothing_to_add_leaves_the_stats_unchanged_for_replication() {
        let mut app = tally(GameState::Playing, &[1]);
        app.update();
        let mut q = app.world_mut().query_filtered::<Ref<RunStats>, With<Player>>();
        let before = q.single(app.world()).unwrap().last_changed();
        hit(&mut app, 1, 0.0, false);
        hit(&mut app, 2, 9.0, true);
        app.update();
        assert_eq!(q.single(app.world()).unwrap().last_changed(), before);
        assert_eq!(stats(&mut app, 1), RunStats::default());
    }

    #[test]
    fn a_sentrys_hits_go_to_its_engineer_without_a_crew_hit() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(GamePlugins)
            .insert_resource(CrewSize(1))
            .insert_resource(Loadouts([(HOST_ID, Loadout::new(PirateClass::Engineer))].into_iter().collect()))
            .insert_resource(NextSeed(7))
            .insert_resource(Rng::new(7));
        app.world_mut().trigger(StartSession(NetMode::Solo));
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Playing);

        // The Engineer, safe from stray guards, drops their sentry.
        let mut engineer = app.world_mut().query_filtered::<(&mut Health, &mut Ability, &mut Controls), With<Player>>();
        let (mut health, mut ability, mut controls) = engineer.single_mut(app.world_mut()).unwrap();
        health.warded = 1000.0;
        ability.cooldown = 0.0;
        controls.ability = true;
        app.update();
        engineer.single_mut(app.world_mut()).unwrap().2.ability = false;

        // A guard held still in its sights, one hit from death.
        let mut sentries = app.world_mut().query_filtered::<&Transform, With<Sentry>>();
        let spot = sentries.single(app.world()).expect("a sentry down").translation.truncate();
        let at = (0..16)
            .map(|i| spot + Vec2::from_angle(i as f32 / 16.0 * std::f32::consts::TAU) * 30.0)
            .find(|at| app.world().resource::<RoomGrid>().line_of_sight(spot, *at))
            .expect("open floor beside it");
        let guard = spawn_enemy(&mut app.world_mut().commands(), EnemyKind::Warden, at, false);
        app.world_mut().flush();
        app.world_mut().get_mut::<Enemy>(guard).unwrap().stun(10.0);
        let mut body = app.world_mut().get_mut::<Health>(guard).unwrap();
        body.shield = 0.0;
        body.current = 1.0;

        for _ in 0..300 {
            app.update();
            assert!(app.world().resource::<Messages<CrewHit>>().is_empty(), "a sentry's hit is no crew hit");
            if app.world().get::<Health>(guard).is_none_or(Health::is_dead) {
                break;
            }
        }
        assert_eq!(stats(&mut app, HOST_ID), RunStats { kills: 1, dealt: 1.0 });
    }

    #[test]
    fn damage_taken_is_what_was_actually_lost() {
        let mut health = Health::new(100.0, 0.0).with_shield(20.0);
        assert!(health.hurt(30.0));
        assert_eq!(health.taken, 30.0, "shield and hull alike");
        assert!(health.hurt(5000.0));
        assert_eq!(health.taken, 120.0, "only what was left, not the overkill");
        assert!(!health.hurt(50.0), "dead");
        assert_eq!(health.taken, 120.0);

        let ignored: [fn(&mut Health); 3] = [|h| h.warded = 1.0, |h| h.invulnerable = 1.0, |h| h.negating = true];
        for ignore in ignored {
            let mut health = Health::new(100.0, 0.0);
            ignore(&mut health);
            assert!(!health.hurt(10.0));
            assert_eq!(health.taken, 0.0);
        }
    }
}
