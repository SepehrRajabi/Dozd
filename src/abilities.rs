//! The abilities a pirate picks between in their loadout: one passive, always
//! on, and one active, mostly on Q. Each class offers its own; the class rules in
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
/// Odds a hit on a guard by a Doctor's crewmate, or the Doctor, heals them.
pub const LIFESTEAL_CHANCE: f32 = 0.3;
/// The share of the damage dealt that such a hit gives back as hull.
pub const LIFESTEAL_SHARE: f32 = 0.5;

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
    /// Doctor: hits by the whole crew sometimes heal the shooter.
    Lifesteal,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Active {
    /// Gunner: no Q, but a 4th gun.
    SpareGun,
    /// Gunner: fires a lethal, shaky laser beam for a few seconds.
    Laser,
    /// Engineer: drops a sentry gun.
    Sentry,
    /// Bulwark: raises a bulletproof shield.
    RiotShield,
    /// Hacker: turns the guards around the crosshair against the station.
    Hack,
    /// Hacker: freezes every guard on the deck for a few seconds.
    Blackout,
    /// Doctor: wards the crew nearby against all harm.
    Ward,
}

impl Passive {
    pub const ALL: [Passive; 7] =
        [Self::Supply, Self::Ricochet, Self::Armor, Self::Rally, Self::Jamming, Self::Healing, Self::Lifesteal];

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
            Self::Healing | Self::Lifesteal => PirateClass::Doctor,
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
            Self::Lifesteal => "LIFESTEAL",
        }
    }

    /// What it does, in a line, for the loadout screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::Supply => "Crew shots may cost no ammo",
            Self::Ricochet => "Hits may bounce to others",
            Self::Armor => "80% more shield",
            Self::Rally => "You and crew nearby hit 25% harder",
            Self::Jamming => "Guards nearby may miss",
            Self::Healing => "Heals crew nearby",
            Self::Lifesteal => "Crew hits may heal the shooter",
        }
    }
}

impl Active {
    pub const ALL: [Active; 7] =
        [Self::SpareGun, Self::Laser, Self::Sentry, Self::RiotShield, Self::Hack, Self::Blackout, Self::Ward];

    /// By its code name, e.g. `riotshield`, as saved and typed on the command line.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| format!("{a:?}").eq_ignore_ascii_case(raw.trim()))
    }

    pub fn class(self) -> PirateClass {
        match self {
            Self::SpareGun | Self::Laser => PirateClass::Gunner,
            Self::Sentry => PirateClass::Engineer,
            Self::RiotShield => PirateClass::Bulwark,
            Self::Hack | Self::Blackout => PirateClass::Hacker,
            Self::Ward => PirateClass::Doctor,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::SpareGun => "SPARE GUN",
            Self::Laser => "LASER",
            Self::Sentry => "SENTRY GUN",
            Self::RiotShield => "RIOT SHIELD",
            Self::Hack => "HACK",
            Self::Blackout => "BLACKOUT",
            Self::Ward => "WARD",
        }
    }

    /// What it does, in a line, for the loadout screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::SpareGun => "No Q: carry a 4th gun",
            Self::Laser => "Q: a lethal beam, with kick",
            Self::Sentry => "Q: drop a gun that shoots guards",
            Self::RiotShield => "Q: raise a bulletproof shield",
            Self::Hack => "Q: turn guards near the aim",
            Self::Blackout => "Q: freeze every guard on deck",
            Self::Ward => "Q: shield the crew from harm",
        }
    }
}

impl PirateClass {
    /// The passives this class can pick from; the first is the default.
    pub fn passives(self) -> impl Iterator<Item = Passive> {
        Passive::ALL.into_iter().filter(move |p| p.class() == self)
    }

    /// The actives this class can pick from; the first is the default.
    pub fn actives(self) -> impl Iterator<Item = Active> {
        Active::ALL.into_iter().filter(move |a| a.class() == self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_abilities() {
        for class in PirateClass::ALL {
            assert!(class.passives().next().is_some(), "{class:?} has no passive");
        }
        for class in PirateClass::ALL {
            assert!(class.actives().next().is_some(), "{class:?} has no active");
        }
    }

    #[test]
    fn abilities_parse_by_code_name() {
        for passive in Passive::ALL {
            assert_eq!(Passive::parse(&format!("{passive:?}").to_lowercase()), Some(passive));
        }
        for active in Active::ALL {
            assert_eq!(Active::parse(&format!("{active:?}").to_lowercase()), Some(active));
        }
        assert_eq!(PirateClass::Doctor.passives().next(), Some(Passive::Healing), "field medic stays the default");
    }
}
