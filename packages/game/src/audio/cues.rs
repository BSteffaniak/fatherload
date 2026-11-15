//! What the game asks to hear: [`SoundCue`]s placed at a source, the cue -> patch table, and the
//! gameplay event -> cue table. Pure data, no device access.

use fatherload_simulation::{
    ExplosiveKind, GameplayEventKind, PlayerId, PoiKind, explosive::Deployment,
};

use super::patch::Patch;
use crate::shell::SoundCue;

/// Where a cue comes from, which decides its pan and distance attenuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundSource {
    /// Interface and global sounds: centred, never attenuated.
    Interface,
    /// A player's rig: a local seat's sounds pan toward its split-screen view; other players'
    /// rigs are placed in the world.
    Player(PlayerId),
    /// A point in the world (world pixels): attenuated by distance to the nearest local rig and
    /// panned by its side.
    World { x: i32, y: i32 },
}

/// A cue and where it comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacedCue {
    pub cue: SoundCue,
    pub source: SoundSource,
}

impl PlacedCue {
    /// An interface (centred) cue.
    #[must_use]
    pub const fn interface(cue: SoundCue) -> Self {
        Self {
            cue,
            source: SoundSource::Interface,
        }
    }

    /// A cue from `player`'s rig.
    #[must_use]
    pub const fn player(cue: SoundCue, player: PlayerId) -> Self {
        Self {
            cue,
            source: SoundSource::Player(player),
        }
    }

    /// A cue at world position `(x, y)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::missing_const_for_fn,
        reason = "world pixel positions are far inside i32; float rounding is not const"
    )]
    pub fn world(cue: SoundCue, x: f32, y: f32) -> Self {
        Self {
            cue,
            source: SoundSource::World {
                x: x.round() as i32,
                y: y.round() as i32,
            },
        }
    }
}

/// The designed patch each cue plays.
#[must_use]
pub const fn cue_patch(cue: SoundCue) -> Patch {
    match cue {
        SoundCue::Drill => Patch::DrillBite,
        SoundCue::Sell => Patch::Coins,
        SoundCue::Upgrade => Patch::Purchase,
        SoundCue::Damage => Patch::HullHit,
        SoundCue::Milestone => Patch::Milestone,
        SoundCue::Rescue => Patch::Rescue,
        SoundCue::Explosion => Patch::Explosion,
        SoundCue::Ui => Patch::UiConfirm,
        SoundCue::UiMove => Patch::UiMove,
        SoundCue::UiBack => Patch::UiBack,
        SoundCue::UiError => Patch::UiError,
        SoundCue::Pickup => Patch::Pickup,
        SoundCue::BombPlaced => Patch::BombArm,
        SoundCue::BombBounce => Patch::LandLight,
        SoundCue::InfrastructurePlaced => Patch::Deploy,
        SoundCue::Scanner => Patch::ScannerPulse,
        SoundCue::PoiDiscovered => Patch::PoiDiscovered,
        SoundCue::CrateOpened => Patch::CrateOpen,
        SoundCue::SalvageOpened => Patch::SalvageOpen,
        SoundCue::VaultOpened => Patch::VaultFanfare,
        SoundCue::VaultReset => Patch::VaultReset,
        SoundCue::SprayPaint => Patch::SprayPaint,
        SoundCue::ExplosiveThrow => Patch::ThrowWhoosh,
        SoundCue::ChargeStuck => Patch::ChargeStick,
        SoundCue::Detonator => Patch::Detonator,
        SoundCue::SmallBlast => Patch::SmallBlast,
        SoundCue::ClusterBurst => Patch::ClusterBurst,
        SoundCue::ShapedBlast => Patch::ShapedBlast,
        SoundCue::DeepBlast => Patch::DeepBlast,
        SoundCue::FireWhoosh => Patch::FireWhoosh,
        SoundCue::FireCrackle => Patch::FireCrackle,
        SoundCue::EmpPulse => Patch::EmpPulse,
        SoundCue::FlareIgnite => Patch::FlareIgnite,
    }
}

/// The blast each explosive kind goes off with (exhaustive: a new kind needs a sound).
#[must_use]
pub const fn detonation_cue(kind: ExplosiveKind) -> SoundCue {
    match kind {
        ExplosiveKind::Bomb | ExplosiveKind::Sticky | ExplosiveKind::C4 => SoundCue::Explosion,
        ExplosiveKind::Cluster | ExplosiveKind::Bomblet => SoundCue::SmallBlast,
        ExplosiveKind::Shaped => SoundCue::ShapedBlast,
        ExplosiveKind::Mining => SoundCue::DeepBlast,
        ExplosiveKind::Incendiary => SoundCue::FireWhoosh,
        ExplosiveKind::Emp => SoundCue::EmpPulse,
        ExplosiveKind::Flare => SoundCue::FlareIgnite,
    }
}

/// The sound of deploying `kind`: a whoosh for throws, the arming clank for drops and placed
/// charges.
#[must_use]
pub const fn deploy_cue(kind: ExplosiveKind) -> SoundCue {
    match kind.deployment() {
        Deployment::Throw => SoundCue::ExplosiveThrow,
        Deployment::Drop | Deployment::Place => SoundCue::BombPlaced,
    }
}

/// The one-shot cue a gameplay event plays, if any. Exhaustive on purpose: a new event kind
/// fails to compile until someone decides how it sounds. Events answered `None` are covered
/// elsewhere (noted per arm).
#[must_use]
pub const fn event_cue(kind: &GameplayEventKind) -> Option<SoundCue> {
    match kind {
        GameplayEventKind::DrillStarted { .. } => Some(SoundCue::Drill),
        // Only when the bay grew (selling also changes cargo; it gets coins).
        GameplayEventKind::CargoChanged { .. } => Some(SoundCue::Pickup),
        GameplayEventKind::BombPlaced { .. } => Some(SoundCue::BombPlaced),
        GameplayEventKind::InfrastructurePlaced { .. } => Some(SoundCue::InfrastructurePlaced),
        GameplayEventKind::ScannerUsed { .. } => Some(SoundCue::Scanner),
        // Milestones, including the quest fanfare when a job is ready or paid.
        GameplayEventKind::EscapeStarted { .. }
        | GameplayEventKind::DeepRewardGranted { .. }
        | GameplayEventKind::QuestReady { .. }
        | GameplayEventKind::QuestCompleted { .. } => Some(SoundCue::Milestone),
        GameplayEventKind::RescueTriggered { .. } => Some(SoundCue::Rescue),
        GameplayEventKind::PlayerDamaged { .. } => Some(SoundCue::Damage),
        GameplayEventKind::BombDetonated { .. } => Some(SoundCue::Explosion),
        GameplayEventKind::BombBounced { .. } => Some(SoundCue::BombBounce),
        GameplayEventKind::ExplosiveDeployed { kind, .. } => Some(deploy_cue(*kind)),
        GameplayEventKind::RemoteDetonation { .. } => Some(SoundCue::Detonator),
        GameplayEventKind::ChargeStuck { .. } => Some(SoundCue::ChargeStuck),
        GameplayEventKind::ExplosiveDetonated { kind, .. } => Some(detonation_cue(*kind)),
        GameplayEventKind::ClusterSplit { .. } => Some(SoundCue::ClusterBurst),
        GameplayEventKind::FireSpread { .. } => Some(SoundCue::FireCrackle),
        GameplayEventKind::PoiOpened { poi, .. } => Some(match poi {
            PoiKind::SupplyCache | PoiKind::RivalClaim => SoundCue::CrateOpened,
            PoiKind::DerelictRig | PoiKind::Vault | PoiKind::Formation => SoundCue::SalvageOpened,
        }),
        GameplayEventKind::PoiDiscovered { .. } => Some(SoundCue::PoiDiscovered),
        GameplayEventKind::VaultOpened { .. } => Some(SoundCue::VaultOpened),
        GameplayEventKind::VaultLockReset { .. } => Some(SoundCue::VaultReset),
        // Quests: a tick per finished stage, the error buzz when a twist fails a job, and the
        // upgrade chime for a completed blueprint.
        GameplayEventKind::QuestStageDone { .. } => Some(SoundCue::Ui),
        GameplayEventKind::QuestFailed { .. } => Some(SoundCue::UiError),
        GameplayEventKind::BlueprintCompleted { .. } => Some(SoundCue::Upgrade),
        // A garage paint job: the spray can, heard from the rig (other players too).
        GameplayEventKind::RigCustomized { .. } => Some(SoundCue::SprayPaint),
        // The shell plays coins/purchase when the service it requested resolves. Hazards are
        // continuous: the director's gas-hiss and boulder-rumble loops follow them directly.
        // Tunnel edits are terrain changes with no sound of their own. Accepting a quest and
        // test-mode toggles/warps are answered by the counter or menu that requested them.
        GameplayEventKind::PurchaseCompleted { .. }
        | GameplayEventKind::QuestAccepted { .. }
        | GameplayEventKind::TestModeChanged { .. }
        | GameplayEventKind::HazardChanged
        | GameplayEventKind::BouldersChanged
        | GameplayEventKind::EscapeTunnelChanged => None,
    }
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::{
        BlueprintId, PoiReward, QuestFailure, QuestId, worldgen::FeatureId,
    };

    use super::*;

    /// One of every event kind (the match below must stay exhaustive, so adding a kind breaks
    /// this test until it is listed).
    fn every_event() -> Vec<GameplayEventKind> {
        let player_id = PlayerId::new(1);
        let feature = FeatureId(0);
        let quest = QuestId(101);
        let sample = GameplayEventKind::HazardChanged;
        match sample {
            GameplayEventKind::DrillStarted { .. }
            | GameplayEventKind::PurchaseCompleted { .. }
            | GameplayEventKind::CargoChanged { .. }
            | GameplayEventKind::BombPlaced { .. }
            | GameplayEventKind::InfrastructurePlaced { .. }
            | GameplayEventKind::ScannerUsed { .. }
            | GameplayEventKind::EscapeStarted { .. }
            | GameplayEventKind::EscapeTunnelChanged
            | GameplayEventKind::DeepRewardGranted { .. }
            | GameplayEventKind::RescueTriggered { .. }
            | GameplayEventKind::PlayerDamaged { .. }
            | GameplayEventKind::HazardChanged
            | GameplayEventKind::BouldersChanged
            | GameplayEventKind::BombDetonated { .. }
            | GameplayEventKind::BombBounced { .. }
            | GameplayEventKind::ExplosiveDeployed { .. }
            | GameplayEventKind::RemoteDetonation { .. }
            | GameplayEventKind::ChargeStuck { .. }
            | GameplayEventKind::ExplosiveDetonated { .. }
            | GameplayEventKind::ClusterSplit { .. }
            | GameplayEventKind::FireSpread { .. }
            | GameplayEventKind::PoiOpened { .. }
            | GameplayEventKind::PoiDiscovered { .. }
            | GameplayEventKind::VaultOpened { .. }
            | GameplayEventKind::VaultLockReset { .. }
            | GameplayEventKind::TestModeChanged { .. }
            | GameplayEventKind::QuestAccepted { .. }
            | GameplayEventKind::QuestStageDone { .. }
            | GameplayEventKind::QuestReady { .. }
            | GameplayEventKind::QuestCompleted { .. }
            | GameplayEventKind::QuestFailed { .. }
            | GameplayEventKind::BlueprintCompleted { .. }
            | GameplayEventKind::RigCustomized { .. } => {}
        }
        let mut events = vec![
            GameplayEventKind::DrillStarted { player_id },
            GameplayEventKind::PurchaseCompleted { player_id },
            GameplayEventKind::CargoChanged { player_id },
            GameplayEventKind::BombPlaced { player_id },
            GameplayEventKind::InfrastructurePlaced { player_id },
            GameplayEventKind::ScannerUsed { player_id },
            GameplayEventKind::EscapeStarted { player_id },
            GameplayEventKind::EscapeTunnelChanged,
            GameplayEventKind::DeepRewardGranted { player_id },
            GameplayEventKind::RescueTriggered { player_id },
            GameplayEventKind::PlayerDamaged { player_id },
            GameplayEventKind::HazardChanged,
            GameplayEventKind::BouldersChanged,
            GameplayEventKind::BombDetonated {
                bomb_id: 1,
                x: 0,
                y: 0,
            },
            GameplayEventKind::BombBounced {
                bomb_id: 1,
                x: 0,
                y: 0,
            },
            GameplayEventKind::PoiDiscovered {
                player_id,
                feature,
                poi: PoiKind::Vault,
            },
            GameplayEventKind::VaultOpened { feature },
            GameplayEventKind::VaultLockReset { feature },
            GameplayEventKind::TestModeChanged { player_id },
            GameplayEventKind::QuestAccepted { player_id, quest },
            GameplayEventKind::QuestStageDone { player_id, quest },
            GameplayEventKind::QuestReady { player_id, quest },
            GameplayEventKind::QuestCompleted { player_id, quest },
            GameplayEventKind::QuestFailed {
                player_id,
                quest,
                reason: QuestFailure::OutOfTime,
            },
            GameplayEventKind::BlueprintCompleted {
                player_id,
                blueprint: BlueprintId::HaulerFrame,
            },
            GameplayEventKind::RigCustomized { player_id },
        ];
        events.push(GameplayEventKind::RemoteDetonation { player_id });
        events.push(GameplayEventKind::ClusterSplit {
            bomb_id: 1,
            x: 0,
            y: 0,
        });
        events.push(GameplayEventKind::FireSpread {
            bomb_id: 1,
            x: 0,
            y: 0,
        });
        for kind in ExplosiveKind::ALL {
            events.push(GameplayEventKind::ExplosiveDeployed { player_id, kind });
            events.push(GameplayEventKind::ChargeStuck {
                bomb_id: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::ExplosiveDetonated {
                bomb_id: 1,
                kind,
                x: 0,
                y: 0,
                direction: fatherload_simulation::BlastDirection::Down,
            });
        }
        events.extend(PoiKind::ALL.map(|poi| GameplayEventKind::PoiOpened {
            player_id,
            poi,
            reward: PoiReward::default(),
        }));
        events
    }

    #[test]
    fn every_event_is_either_heard_or_deliberately_silent() {
        let silent: Vec<GameplayEventKind> = every_event()
            .into_iter()
            .filter(|event| event_cue(event).is_none())
            .collect();
        // These are covered by the shell (purchases) or the director's continuous loops.
        assert_eq!(
            silent,
            vec![
                GameplayEventKind::PurchaseCompleted {
                    player_id: PlayerId::new(1)
                },
                GameplayEventKind::EscapeTunnelChanged,
                GameplayEventKind::HazardChanged,
                GameplayEventKind::BouldersChanged,
                GameplayEventKind::TestModeChanged {
                    player_id: PlayerId::new(1)
                },
                GameplayEventKind::QuestAccepted {
                    player_id: PlayerId::new(1),
                    quest: QuestId(101)
                },
            ]
        );
    }

    #[test]
    fn every_explosive_kind_has_its_own_blast_and_throws_whoosh() {
        for kind in ExplosiveKind::ALL {
            let cue = detonation_cue(kind);
            assert!(!cue_patch(cue).looped(), "{kind:?}");
        }
        assert_eq!(detonation_cue(ExplosiveKind::Shaped), SoundCue::ShapedBlast);
        assert_eq!(deploy_cue(ExplosiveKind::Sticky), SoundCue::ExplosiveThrow);
        assert_eq!(deploy_cue(ExplosiveKind::C4), SoundCue::BombPlaced);
    }

    #[test]
    fn every_cue_has_a_distinct_rendered_patch() {
        let cues = [
            SoundCue::Drill,
            SoundCue::Sell,
            SoundCue::Upgrade,
            SoundCue::Damage,
            SoundCue::Milestone,
            SoundCue::Rescue,
            SoundCue::Explosion,
            SoundCue::Ui,
            SoundCue::UiMove,
            SoundCue::UiBack,
            SoundCue::UiError,
            SoundCue::Pickup,
            SoundCue::BombPlaced,
            SoundCue::BombBounce,
            SoundCue::InfrastructurePlaced,
            SoundCue::Scanner,
            SoundCue::PoiDiscovered,
            SoundCue::CrateOpened,
            SoundCue::SalvageOpened,
            SoundCue::VaultOpened,
            SoundCue::VaultReset,
            SoundCue::ExplosiveThrow,
            SoundCue::ChargeStuck,
            SoundCue::Detonator,
            SoundCue::SmallBlast,
            SoundCue::ClusterBurst,
            SoundCue::ShapedBlast,
            SoundCue::DeepBlast,
            SoundCue::FireWhoosh,
            SoundCue::FireCrackle,
            SoundCue::EmpPulse,
            SoundCue::FlareIgnite,
        ];
        let mut patches: Vec<Patch> = cues.iter().map(|cue| cue_patch(*cue)).collect();
        assert!(patches.iter().all(|patch| !patch.looped()));
        patches.sort();
        patches.dedup();
        assert_eq!(patches.len(), cues.len());
    }
}
