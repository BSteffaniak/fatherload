//! The garage: repainting and refitting a rig's cosmetics (the Repair Garage's paint bay).
//!
//! Submitted as [`crate::SessionServiceRequest::CustomizeRig`] while the rig stands at the
//! Repair Garage. The paint job is free; it is refused only if it uses a cosmetic item the
//! player has not unlocked (test-mode worlds unlock everything). The new look lands in the
//! public rig build section, so every client redraws the rig, and in saves with the player.

use crate::{cosmetics::RigCosmetics, ids::PlayerId, summary::PlayerScopedCommandOutcome};

use super::WorldState;

impl WorldState {
    /// Applies a garage paint job (`None`: back to the player's default look).
    pub(crate) fn apply_rig_customization(
        &mut self,
        player_id: PlayerId,
        cosmetics: Option<RigCosmetics>,
    ) -> PlayerScopedCommandOutcome {
        let everything_unlocked = self.is_test_mode();
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if let Some(look) = &cosmetics
            && !everything_unlocked
            && !player.cosmetic_unlocks.allows(look)
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.cosmetics = cosmetics;
        PlayerScopedCommandOutcome::Applied
    }

    /// The look `player_id`'s rig wears now: their garage paint job or their default.
    #[must_use]
    pub fn rig_cosmetics(&self, player_id: PlayerId) -> Option<RigCosmetics> {
        let player = self.players.get(&player_id)?;
        Some(RigCosmetics::resolve(player.cosmetics, player_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CosmeticItem, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
        RigCosmetics, SectionKey, SessionServiceRequest, TickInputs, WorldState,
        cosmetics::{Decal, FlameColor, PaintColor, RoofAttachment},
        economy::SurfaceZone,
        surface::SURFACE_BUILDINGS,
        terrain::TILE_SIZE,
    };

    const PLAYER: PlayerId = PlayerId::new(1);
    const OTHER: PlayerId = PlayerId::new(2);

    fn look() -> RigCosmetics {
        RigCosmetics {
            primary: PaintColor::Cobalt,
            secondary: PaintColor::Pearl,
            trim: PaintColor::Crimson,
            flame: FlameColor::Blue,
            decal: Decal::Bolt,
            roof: RoofAttachment::Dish,
            ..RigCosmetics::default()
        }
    }

    fn stand_at(world: &mut WorldState, player: PlayerId, zone: SurfaceZone) {
        let building = SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == zone)
            .expect("building exists");
        let rig = world.player_mut(player).expect("player");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let x = (building.tile_x as f32 + 2.5) * TILE_SIZE;
        rig.x = x;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }

    fn customize(
        world: &mut WorldState,
        cosmetics: Option<RigCosmetics>,
    ) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
        let tick = world.simulation_tick();
        let inputs = TickInputs::default().act(
            PLAYER,
            PlayerCommand::Service(SessionServiceRequest::CustomizeRig { cosmetics }),
        );
        let output = world.simulate_tick(tick, &inputs);
        (
            output.action_outcomes[0].outcome,
            output.events.iter().map(|event| event.kind).collect(),
        )
    }

    #[test]
    fn a_paint_job_at_the_garage_is_free_and_announced() {
        let mut world = WorldState::new_game(PLAYER);
        stand_at(&mut world, PLAYER, SurfaceZone::Repair);
        let credits = world.player(PLAYER).expect("player").credits;
        let (outcome, events) = customize(&mut world, Some(look()));
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
        assert!(events.contains(&GameplayEventKind::RigCustomized { player_id: PLAYER }));
        let player = world.player(PLAYER).expect("player");
        assert_eq!(player.cosmetics, Some(look()));
        assert_eq!(player.credits, credits);
        assert_eq!(world.rig_cosmetics(PLAYER), Some(look()));
    }

    #[test]
    fn resetting_restores_the_per_player_default() {
        let mut world = WorldState::new_game(PLAYER);
        stand_at(&mut world, PLAYER, SurfaceZone::Repair);
        customize(&mut world, Some(look()));
        let (outcome, _) = customize(&mut world, None);
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
        assert_eq!(world.player(PLAYER).expect("player").cosmetics, None);
        assert_eq!(
            world.rig_cosmetics(PLAYER),
            Some(RigCosmetics::default_for(PLAYER))
        );
    }

    #[test]
    fn the_garage_is_only_at_the_repair_garage() {
        let mut world = WorldState::new_game(PLAYER);
        stand_at(&mut world, PLAYER, SurfaceZone::Shop);
        let (outcome, _) = customize(&mut world, Some(look()));
        assert_ne!(outcome, PlayerScopedCommandOutcome::Applied);
        assert_eq!(world.player(PLAYER).expect("player").cosmetics, None);
    }

    #[test]
    fn locked_items_are_refused_until_unlocked() {
        let mut world = WorldState::new_game(PLAYER);
        stand_at(&mut world, PLAYER, SurfaceZone::Repair);
        let stealth = RigCosmetics {
            primary: PaintColor::Stealth,
            ..look()
        };
        let (outcome, events) = customize(&mut world, Some(stealth));
        assert_eq!(outcome, PlayerScopedCommandOutcome::IgnoredUnavailable);
        assert!(events.is_empty());
        assert!(
            world
                .player_mut(PLAYER)
                .expect("player")
                .unlock_cosmetic(CosmeticItem::Paint(PaintColor::Stealth))
        );
        let (outcome, _) = customize(&mut world, Some(stealth));
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    }

    #[test]
    fn test_mode_unlocks_every_cosmetic() {
        let mut world = WorldState::new_game(PLAYER);
        world.enable_test_mode();
        stand_at(&mut world, PLAYER, SurfaceZone::Repair);
        let (outcome, _) = customize(
            &mut world,
            Some(RigCosmetics {
                primary: PaintColor::Stealth,
                flame: FlameColor::Prism,
                ..look()
            }),
        );
        assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    }

    #[test]
    fn cosmetics_replicate_publicly_and_unlocks_stay_private() {
        let mut authority = WorldState::new_game(PLAYER);
        authority.add_player(OTHER);
        stand_at(&mut authority, PLAYER, SurfaceZone::Repair);
        customize(&mut authority, Some(look()));
        authority
            .player_mut(PLAYER)
            .expect("player")
            .unlock_cosmetic(CosmeticItem::Decal(Decal::Crown));
        assert!(SectionKey::RigBuild(PLAYER).visible_to(Some(OTHER)));

        let mut replica = WorldState::new_game(OTHER);
        let key = SectionKey::RigBuild(PLAYER);
        replica.apply_section(key, authority.section(key).expect("rig build"));
        let seen = replica.player(PLAYER).expect("replicated player");
        assert_eq!(seen.cosmetics, Some(look()));
        assert!(seen.cosmetic_unlocks.items.is_empty());

        let keyframe = authority.redacted_for(Some(OTHER));
        let redacted = keyframe.player(PLAYER).expect("player");
        assert_eq!(redacted.cosmetics, Some(look()), "the look is public");
        assert!(
            redacted.cosmetic_unlocks.items.is_empty(),
            "unlocks are not"
        );
    }

    #[test]
    fn visible_tiers_are_public() {
        let mut authority = WorldState::new_game(PLAYER);
        let rig = authority.player_mut(PLAYER).expect("player");
        rig.drill_strength = 7;
        rig.engine_level = 9;
        rig.cargo_bay_level = 12;
        let keyframe = authority.redacted_for(Some(OTHER));
        let seen = keyframe.player(PLAYER).expect("player");
        assert_eq!(
            (seen.drill_strength, seen.engine_level, seen.cargo_bay_level),
            (7, 9, 12)
        );
    }

    #[test]
    fn saves_from_before_cosmetics_load_with_the_default_look() {
        let world = WorldState::new_game(PLAYER);
        let mut json = serde_json::to_value(world.player(PLAYER).expect("player")).unwrap();
        let object = json.as_object_mut().unwrap();
        object.remove("cosmetics");
        object.remove("cosmetic_unlocks");
        let player: crate::Player = serde_json::from_value(json).unwrap();
        assert_eq!(player.cosmetics, None);
        assert!(player.cosmetic_unlocks.items.is_empty());
    }
}
