//! Shows the challenges in play and the rewards earned from them. Each
//! `Challenge` entity gets a card under the top-left panel: in full when this
//! machine's pirate can complete it, as one dim line when it's a crewmate's.

use bevy::prelude::*;

use crate::challenge::{Challenge, ChallengeState, Goal, Recipients, Reward, Rewards, Span, Target};
use crate::hud::{self, HudSlot, Meter};
use crate::net::{LocalId, LocalPlayer};
use crate::player::{Player, slot_color};

const CARD_WIDTH: f32 = 260.0;
const METER_WIDTH: f32 = 244.0;
/// Accent for challenges anyone in the crew can complete.
const CREW_COLOR: Color = Color::srgb(0.55, 0.8, 1.0);
const DONE_COLOR: Color = Color::srgb(0.45, 0.95, 0.5);
const FAILED_COLOR: Color = Color::srgb(1.0, 0.35, 0.3);
/// Seconds left on a streak window when its meter turns red.
const WINDOW_LOW: f32 = 3.0;

pub struct ChallengeViewPlugin;

impl Plugin for ChallengeViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_challenge_cards);
    }
}

/// The goal in words, e.g. "Kill 5 guards within 15s".
pub fn goal_text(goal: Goal) -> String {
    match goal {
        Goal::KillStreak { kills, window } => {
            let guards = if kills == 1 { "guard" } else { "guards" };
            format!("Kill {kills} {guards} within {window:.0}s")
        }
    }
}

/// The rolling window progress drops out of, if the goal has one.
pub fn goal_window(goal: Goal) -> Option<f32> {
    match goal {
        Goal::KillStreak { window, .. } => Some(window),
    }
}

/// How long it stays open, e.g. "this deck" or "42s left".
pub fn span_text(span: Span, left: Option<f32>) -> String {
    match span {
        Span::Heist => "whole heist".into(),
        Span::Deck(_) => "this deck".into(),
        Span::Ttl(secs) => format!("{:.0}s left", left.unwrap_or(secs).max(0.0).ceil()),
    }
}

/// Who can complete it, as a tag and its colour.
pub fn target_tag(target: Target) -> (&'static str, Color) {
    match target {
        Target::Crew => ("CREW", CREW_COLOR),
        Target::Class(class) => (class.name(), class.color()),
    }
}

/// The reward in words, e.g. "+30% damage for 45s", noting when the whole crew gets it.
pub fn reward_text(reward: Reward, recipients: Recipients) -> String {
    let what = match reward {
        Reward::Damage { bonus, secs } => format!("+{:.0}% damage for {secs:.0}s", bonus * 100.0),
    };
    match recipients {
        Recipients::Completer => what,
        Recipients::Crew => what + " for the crew",
    }
}

/// A reward's short chip label, before its countdown, e.g. "+30% DMG".
pub fn reward_chip(reward: Reward) -> String {
    match reward {
        Reward::Damage { bonus, .. } => format!("+{:.0}% DMG", bonus * 100.0),
    }
}

pub fn reward_color(reward: Reward) -> Color {
    match reward {
        Reward::Damage { .. } => Color::srgb(1.0, 0.5, 0.3),
    }
}

/// A pirate's running rewards as effect chips, e.g. "+30% DMG 32s".
pub fn reward_chips(rewards: Option<&Rewards>) -> Vec<(String, Color)> {
    rewards.map_or_else(Vec::new, |rewards| {
        rewards
            .0
            .iter()
            .map(|r| (format!("{} {:.0}s", reward_chip(r.reward), r.left.max(0.0).ceil()), reward_color(r.reward)))
            .collect()
    })
}

/// One challenge's card, and the parts of it that change.
#[derive(Component)]
struct ChallengeCard {
    challenge: Entity,
    name: Entity,
    tag: Entity,
    status: Entity,
    /// Everything below the title line, hidden on a crewmate's challenge.
    body: Entity,
    goal: Entity,
    window: Entity,
    span: Entity,
    reward: Entity,
}

fn spawn_card(commands: &mut Commands, challenge: Entity) -> Entity {
    let name = commands.spawn((hud::text("", 14.0, hud::TEXT), TextLayout::no_wrap())).id();
    let tag = commands.spawn(hud::text("", 10.0, hud::DIM)).id();
    let status = commands.spawn(hud::text("", 13.0, hud::TEXT)).id();
    let goal = commands.spawn(hud::text("", 11.0, hud::TEXT)).id();
    let window = commands.spawn(hud::meter(METER_WIDTH, 4.0, CREW_COLOR)).id();
    let span = commands.spawn(hud::text("", 11.0, hud::LABEL)).id();
    let reward = commands.spawn(hud::text("", 11.0, hud::TEXT)).id();

    let spacer = || Node { flex_grow: 1.0, ..default() };
    let title = commands
        .spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Baseline, ..default() })
        .add_children(&[name, tag])
        .with_child(spacer())
        .add_children(&[status])
        .id();
    let footer = commands
        .spawn(Node { column_gap: Val::Px(6.0), align_items: AlignItems::Baseline, ..default() })
        .add_children(&[reward])
        .with_child(spacer())
        .add_children(&[span])
        .id();
    let body = commands
        .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() })
        .add_children(&[goal, window, footer])
        .id();

    commands
        .spawn(hud::panel_accent(
            Node {
                width: Val::Px(CARD_WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                ..default()
            },
            hud::EDGE,
        ))
        .add_children(&[title, body])
        .insert(ChallengeCard { challenge, name, tag, status, body, goal, window, span, reward })
        .id()
}

/// Keeps a card per challenge in step with it: spawned as one appears, filled
/// in every frame, and gone once the host despawns it.
fn update_challenge_cards(
    mut commands: Commands,
    time: Res<Time>,
    local: Option<Res<LocalId>>,
    slots: Query<(Entity, &HudSlot)>,
    challenges: Query<(Entity, &Challenge)>,
    me: Option<Single<&Player, With<LocalPlayer>>>,
    players: Query<&Player>,
    mut cards: Query<(Entity, &ChallengeCard, &mut BorderColor)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut nodes: Query<&mut Node, Without<ChallengeCard>>,
    mut meters: Query<&mut Meter>,
) {
    let container = hud::slot(&slots, HudSlot::Challenges);
    for (entity, _) in &challenges {
        if !cards.iter().any(|(_, card, _)| card.challenge == entity) {
            let card = spawn_card(&mut commands, entity);
            commands.entity(container).add_child(card);
        }
    }

    let local = local.map(|id| id.0);
    let my_class = me.map(|p| p.class);
    let blink = (time.elapsed_secs() * 6.0).sin() > 0.0;
    for (entity, card, mut border) in &mut cards {
        let Ok((_, challenge)) = challenges.get(card.challenge) else {
            commands.entity(entity).despawn();
            continue;
        };
        let mine = match challenge.target {
            Target::Crew => true,
            Target::Class(class) => my_class == Some(class),
        };
        let (tag, tag_color) = target_tag(challenge.target);
        let needs = challenge.goal.needed();
        // Your own progress on yours; the furthest along on a crewmate's.
        let progress = challenge
            .progress
            .iter()
            .filter(|p| !mine || Some(p.owner) == local)
            .max_by_key(|p| p.count)
            .copied();
        let count = progress.map_or(0, |p| p.count);

        if let Ok(mut node) = nodes.get_mut(card.body) {
            let display = if mine { Display::Flex } else { Display::None };
            if node.display != display {
                node.display = display;
            }
        }

        let (status, status_color, accent) = match challenge.state {
            ChallengeState::Open => {
                let color = if mine { hud::TEXT } else { hud::DIM };
                (format!("{count}/{needs}"), color, tag_color)
            }
            ChallengeState::Done { .. } => {
                let color = if blink || !mine { DONE_COLOR } else { hud::TEXT };
                ("COMPLETE".to_string(), color, DONE_COLOR)
            }
            ChallengeState::Failed => ("FAILED".to_string(), FAILED_COLOR.with_alpha(0.6), hud::EDGE),
        };
        let faded = !mine || challenge.state == ChallengeState::Failed;
        let fade = |color: Color| if faded { color.with_alpha(0.5) } else { color };

        set_text(&mut texts, card.name, &challenge.name, fade(if mine { accent } else { hud::TEXT }));
        set_text(&mut texts, card.tag, tag, fade(tag_color));
        set_text(&mut texts, card.status, &status, status_color);
        border.set_if_neq(BorderColor { top: fade(accent), ..BorderColor::all(hud::EDGE) });
        if !mine {
            continue;
        }

        set_text(&mut texts, card.goal, &goal_text(challenge.goal), fade(hud::TEXT));
        let reward = reward_text(challenge.reward, challenge.recipients);
        set_text(&mut texts, card.reward, &reward, fade(reward_color(challenge.reward)));
        let span = match challenge.state {
            ChallengeState::Open => span_text(challenge.span, challenge.left),
            ChallengeState::Done { by } if Some(by) == local => "by you".into(),
            ChallengeState::Done { by } => match players.iter().find(|p| p.owner == by) {
                Some(p) => format!("by P{} {}", p.slot + 1, p.class.name()),
                None => "by a crewmate".into(),
            },
            ChallengeState::Failed => "missed".into(),
        };
        let span_color = match challenge.state {
            ChallengeState::Done { by } => players
                .iter()
                .find(|p| p.owner == by && Some(by) != local)
                .map_or(DONE_COLOR, |p| slot_color(p.slot)),
            _ => fade(hud::LABEL),
        };
        set_text(&mut texts, card.span, &span, span_color);

        if let Ok(mut meter) = meters.get_mut(card.window) {
            let (frac, color) = match (challenge.state, goal_window(challenge.goal), progress) {
                (ChallengeState::Done { .. }, ..) => (1.0, DONE_COLOR),
                (ChallengeState::Failed, ..) => (0.0, hud::SPENT),
                // A rolling window: how long until the streak starts slipping.
                (ChallengeState::Open, Some(window), Some(p)) if p.count > 0 => {
                    let color = if p.expires < WINDOW_LOW { FAILED_COLOR } else { accent };
                    (p.expires / window, color)
                }
                (ChallengeState::Open, Some(_), _) => (0.0, accent),
                // No window: plain progress towards the goal.
                (ChallengeState::Open, None, _) => (count as f32 / needs.max(1) as f32, accent),
            };
            meter.frac = frac;
            meter.color = color;
        }
    }
}

fn set_text(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, content: &str, color: Color) {
    let Ok((mut text, mut text_color)) = texts.get_mut(entity) else { return };
    if text.0 != content {
        text.0 = content.into();
    }
    text_color.set_if_neq(TextColor(color));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::ActiveReward;
    use crate::class::PirateClass;

    #[test]
    fn challenges_read_in_words() {
        let goal = Goal::KillStreak { kills: 5, window: 15.0 };
        assert_eq!(goal_text(goal), "Kill 5 guards within 15s");
        assert_eq!(goal.needed(), 5);
        assert_eq!(span_text(Span::Deck(2), None), "this deck");
        assert_eq!(span_text(Span::Heist, None), "whole heist");
        assert_eq!(span_text(Span::Ttl(60.0), Some(41.2)), "42s left");
        assert_eq!(target_tag(Target::Class(PirateClass::Gunner)).0, "GUNNER");
        assert_eq!(target_tag(Target::Crew).0, "CREW");
        let reward = Reward::Damage { bonus: 0.3, secs: 45.0 };
        assert_eq!(reward_text(reward, Recipients::Completer), "+30% damage for 45s");
        assert_eq!(reward_text(reward, Recipients::Crew), "+30% damage for 45s for the crew");
    }

    #[test]
    fn rewards_show_as_chips_with_a_countdown() {
        let rewards = Rewards(vec![ActiveReward { reward: Reward::Damage { bonus: 0.3, secs: 45.0 }, left: 31.4, total: 45.0 }]);
        assert_eq!(reward_chips(Some(&rewards))[0].0, "+30% DMG 32s");
        assert!(reward_chips(None).is_empty());
    }
}
