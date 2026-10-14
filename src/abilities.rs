//! The abilities a pirate picks between in their loadout: one passive, always
//! on, and one active, on Q. Each class offers its own; the class rules in
//! `class` look at the pick rather than the class.

use serde::{Deserialize, Serialize};

use crate::class::PirateClass;

/// Odds a Gunner's bullet that hits a guard bounces off into others nearby.
pub const RICOCHET_CHANCE: f32 = 0.3;
/// How many guards a ricochet goes on to, how far it looks for them, and the
/// share of the shot's damage each bounce carries.
pub const RICOCHET_TARGETS: usize = 2;
pub const RICOCHET_REACH: f32 = 70.0;
pub const RICOCHET_DAMAGE: f32 = 0.6;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Passive {
    /// Gunner: shots by the whole crew sometimes cost no ammo.
    Supply,
    /// Gunner: their bullets sometimes bounce on into nearby guards.
    Ricochet,
    /// Engineer: a heavier shield.
    Armor,
    /// Bulwark: crewmates nearby hit harder.
    Rally,
    /// Hacker: guards nearby sometimes miss.
    Jamming,
    /// Doctor: crewmates nearby slowly heal.
    Healing,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Active {
    /// Engineer: drops a sentry gun.
    Sentry,
    /// Bulwark: raises a bulletproof shield.
    RiotShield,
    /// Hacker: turns a guard against the station.
    Hack,
    /// Doctor: wards the crew nearby against all harm.
    Ward,
}

impl Passive {
    pub const ALL: [Passive; 6] = [Self::Supply, Self::Ricochet, Self::Armor, Self::Rally, Self::Jamming, Self::Healing];

    /// By its code name, e.g. `ricochet`, as saved and typed on the command line.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| format!("{p:?}").eq_ignore_ascii_case(raw.trim()))
    }

    pub fn class(self) -> PirateClass {
        match self {
            Self::Supply | Self::Ricochet => PirateClass::Gunner,
            Self::Armor => PirateClass::Engineer,
            Self::Rally => PirateClass::Bulwark,
            Self::Jamming => PirateClass::Hacker,
            Self::Healing => PirateClass::Doctor,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Supply => "SUPPLY",
            Self::Ricochet => "RICOCHET",
            Self::Armor => "ARMOUR",
            Self::Rally => "RALLY",
            Self::Jamming => "JAMMING",
            Self::Healing => "FIELD MEDIC",
        }
    }

    /// What it does, in a line, for the loadout screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::Supply => "Crew shots may cost no ammo",
            Self::Ricochet => "Hits may bounce to others",
            Self::Armor => "80% more shield",
            Self::Rally => "Crew nearby hit 25% harder",
            Self::Jamming => "Guards nearby may miss",
            Self::Healing => "Heals crew nearby",
        }
    }
}

impl Active {
    pub const ALL: [Active; 4] = [Self::Sentry, Self::RiotShield, Self::Hack, Self::Ward];

    /// By its code name, e.g. `riotshield`, as saved and typed on the command line.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| format!("{a:?}").eq_ignore_ascii_case(raw.trim()))
    }

    pub fn class(self) -> PirateClass {
        match self {
            Self::Sentry => PirateClass::Engineer,
            Self::RiotShield => PirateClass::Bulwark,
            Self::Hack => PirateClass::Hacker,
            Self::Ward => PirateClass::Doctor,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Sentry => "SENTRY GUN",
            Self::RiotShield => "RIOT SHIELD",
            Self::Hack => "HACK",
            Self::Ward => "WARD",
        }
    }

    /// What it does, in a line, for the loadout screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::Sentry => "Q: drop a gun that shoots guards",
            Self::RiotShield => "Q: raise a bulletproof shield",
            Self::Hack => "Q: turn a guard to your side",
            Self::Ward => "Q: shield the crew from harm",
        }
    }
}

impl PirateClass {
    /// The passives this class can pick from; the first is the default.
    pub fn passives(self) -> impl Iterator<Item = Passive> {
        Passive::ALL.into_iter().filter(move |p| p.class() == self)
    }

    /// The actives this class can pick from; the first is the default. The
    /// Gunner has none: they carry a 4th gun instead.
    pub fn actives(self) -> impl Iterator<Item = Active> {
        Active::ALL.into_iter().filter(move |a| a.class() == self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_passive() {
        for class in PirateClass::ALL {
            assert!(class.passives().next().is_some(), "{class:?} has no passive");
        }
        assert_eq!(PirateClass::Gunner.actives().count(), 0);
    }
}
