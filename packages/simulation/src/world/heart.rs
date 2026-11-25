//! Descent VIII, the Heart, in the world (Wave 10; the rules live in [`crate::heart`]).
//!
//! [`WorldState::advance_heart`] runs once per tick after the Hollow World's pass. On worlds
//! without Descents it returns at once. In order:
//!
//! 1. **Dark matter**: each rig's weight from its bay ([`WorldState::cargo_mass_of`]); a rig
//!    whose dark matter outweighs it starts falling up ([`HeartEvent::RigFloating`]) and settles
//!    when it is heavy again. The weight itself acts inside the movement step
//!    (`world/movement.rs` scales the rig's gravity by it).
//! 2. **Wells**: a rig in a well's strong pull is caught (the cue); one inside a horizon is
//!    crushed. The pull itself is a field ([`crate::heart::fields`]), so rigs, pilots,
//!    creatures and drops all fall in without anything here.
//! 3. **Landmarks**: the Upfall, the Lens, the Ferry Terminal and the Heart's chamber tell
//!    their story the first time a rig or pilot reaches them.
//! 4. **The Star Drive** fits every rig of the crew once the Heart woke (later joiners too);
//!    a rig over the Moon after the ending sees that the stars are open.
//! 5. The Heart's fight (`world/enemies/heart.rs`).
//!
//! The magnetic rails ride through the Crust's cart code (`world/crust.rs`, with
//! [`crate::heart::MAG_RAIL`]'s tuning and the magnetic catch).

use crate::{
    descent::{Descent, Stratum},
    event::{GameplayEventKind, TickOutput},
    heart::{
        CHAMBER_RADIUS, CoreTile, HORIZON_DAMAGE_PER_SECOND, HORIZON_TILES, Heart, HeartEvent,
        HeartLandmark, SpaceAccess, WELL_CAUGHT_SHARE, cargo_mass_permille,
    },
    ids::{PlayerId, SimulationTick},
    item::ItemKind,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::descent::heart::{self as places, Well},
};

use super::WorldState;

/// A Heart event at `tile`.
pub(super) const fn heart_event(
    what: HeartEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Heart {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

/// The builders' cable from the Heart to the Moon, for drawing its light (tile coordinates).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CablePath {
    /// The cable's column underground.
    pub column: i32,
    /// The Heart chamber's middle (the beam's root).
    pub heart: TilePosition,
    /// The ground line the cable leaves the ground at (row 0 of the town).
    pub surface_row: i32,
    /// Where it meets the Moon's underside (`None` on worlds without a Moon).
    pub moon: Option<TilePosition>,
    /// The Moon's top row (above it, the stars).
    pub moon_top: Option<i32>,
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// Descent VIII's shared state.
    #[must_use]
    pub const fn heart(&self) -> &Heart {
        &self.heart
    }

    /// Replaces Descent VIII's shared state (scenes, tests, tools).
    pub fn set_heart(&mut self, heart: Heart) {
        self.heart = heart;
    }

    /// Whether the Heart woke and space past the Moon is open: **the world flag Wave 11's
    /// space flight reads**. Saved and replicated (the shared `Heart` section).
    #[must_use]
    pub const fn space_unlocked(&self) -> bool {
        self.heart.space_unlocked
    }

    /// Whether `player_id`'s rig carries the Star Drive (per player; every rig of the crew
    /// once the Heart woke).
    #[must_use]
    pub fn has_star_drive(&self, player_id: PlayerId) -> bool {
        self.heart.has_star_drive(player_id)
    }

    /// What `player_id` may do past the Moon (Wave 11's space flight reads this).
    #[must_use]
    pub fn space_access(&self, player_id: PlayerId) -> SpaceAccess {
        self.heart.space_access(player_id)
    }

    /// The Heart chamber's middle (Deep Descent worlds).
    #[must_use]
    pub fn heart_centre(&self) -> Option<TilePosition> {
        crate::heart::heart_centre(self.terrain.settings())
    }

    /// Whether `tile` is inside the Heart's chamber (within its shell).
    #[must_use]
    pub fn in_heart_chamber(&self, tile: TilePosition) -> bool {
        self.heart_centre().is_some_and(|centre| {
            let (dx, dy) = (tile.x - centre.x, tile.y - centre.y);
            dx * dx + dy * dy <= CHAMBER_RADIUS * CHAMBER_RADIUS
        })
    }

    /// The builders' cable's path to the Moon (Deep Descent worlds).
    #[must_use]
    pub fn cable_path(&self) -> Option<CablePath> {
        let settings = self.terrain.settings();
        let heart = self.heart_centre()?;
        let moon = crate::worldgen::sky::moon(settings);
        Some(CablePath {
            column: heart.x,
            heart,
            surface_row: 0,
            moon: moon.map(|land| TilePosition {
                x: land.center_x,
                y: land.top_row + land.depth,
            }),
            moon_top: moon.map(|land| land.top_row),
        })
    }

    /// Milliseconds since the cable lit (the ending's clock), `None` before the Heart woke.
    #[must_use]
    pub fn ending_elapsed(&self) -> Option<u64> {
        let lit = self.heart.cable_lit_millis?;
        Some(self.millis_at(self.simulation_tick).saturating_sub(lit))
    }

    /// `player_id`'s weight from its bay, per mille of its own (1000 with no dark matter
    /// aboard; negative: it falls up).
    #[must_use]
    pub fn cargo_mass_of(&self, player_id: PlayerId) -> i32 {
        let Some(player) = self.players.get(&player_id) else {
            return 1_000;
        };
        let dark = player
            .cargo_bay
            .count(ItemKind::Layer(Stratum::DarkMatterVeins));
        if dark == 0 {
            return 1_000;
        }
        let other = player.cargo_used().saturating_sub(dark);
        cargo_mass_permille(dark, other)
    }

    /// The well pulling hardest at local pixel `(x, y)` and how hard (`0..=1`), if any.
    #[must_use]
    pub fn well_at(&self, x: f32, y: f32) -> Option<(Well, f32)> {
        let layout = self.descent_strata_layout()?;
        let tile = self.frame().tile_at(x, y);
        let (top, bottom) = layout.rows(Stratum::SingularityFields);
        if tile.y < top - 30 || tile.y > bottom + 30 {
            return None;
        }
        let here = crate::worldgen::TileRect {
            min: tile,
            max: tile,
        };
        let wells = places::wells_touching(self.terrain.settings(), &layout, &here);
        #[allow(
            clippy::cast_precision_loss,
            reason = "frame origins of a 13,000-row world are small"
        )]
        let (column, row) = (
            self.frame().origin_col() as f32 + x / TILE_SIZE,
            self.frame().origin_row() as f32 + y / TILE_SIZE,
        );
        crate::heart::fields::strongest(&wells, column, row)
    }

    /// The wells whose pull touches `rect` (presentation: the renderer draws them).
    #[must_use]
    pub fn wells_in(&self, rect: &crate::worldgen::TileRect) -> Vec<Well> {
        self.descent_strata_layout()
            .map_or_else(Vec::new, |layout| {
                places::wells_touching(self.terrain.settings(), &layout, rect)
            })
    }

    /// Whether every rig is shielded now (the ending plays: nobody dies during the credits).
    #[must_use]
    pub fn heart_shields_rigs(&self) -> bool {
        self.heart
            .ending_shields(self.millis_at(self.simulation_tick))
    }

    // --- Hooks --------------------------------------------------------------------------------

    /// A Heart tile was mined by `player_id` at `tile` (the dark matter cue).
    pub(super) fn mined_core_tile(
        player_id: PlayerId,
        tile: TilePosition,
        kind: CoreTile,
        output: &mut TickOutput,
    ) {
        if kind == CoreTile::DarkMatter {
            output.push_event(heart_event(
                HeartEvent::DarkMatterMined,
                Some(player_id),
                tile,
            ));
        }
    }

    /// The Star Drive's sky boost for `player_id`'s rig at local `y`: [`STAR_DRIVE_SKY_BOOST`]
    /// over the ground line once it carries the drive, else 1.
    ///
    /// [`STAR_DRIVE_SKY_BOOST`]: crate::heart::STAR_DRIVE_SKY_BOOST
    #[must_use]
    pub(super) fn star_drive_boost(&self, player_id: PlayerId, y: f32) -> f32 {
        if !self.heart.has_star_drive(player_id) {
            return 1.0;
        }
        if self.frame().legacy_y(y) < 0.0 {
            crate::heart::STAR_DRIVE_SKY_BOOST
        } else {
            1.0
        }
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of Descent VIII (see the module docs).
    pub(super) fn advance_heart(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if self.descent_strata_layout().is_none() {
            return;
        }
        self.advance_dark_matter(output);
        self.advance_wells(next_tick, output);
        self.advance_heart_landmarks(output);
        self.advance_star_drive(output);
        self.advance_heart_fight(next_tick, output);
    }

    fn advance_dark_matter(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let players: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .collect();
        for (player_id, tile) in players {
            let floating = self.cargo_mass_of(player_id) < 0;
            let was = self.heart.floating.contains(&player_id);
            if floating && !was {
                self.heart.floating.insert(player_id);
                output.push_event(heart_event(HeartEvent::RigFloating, Some(player_id), tile));
            } else if !floating && was {
                self.heart.floating.remove(&player_id);
                output.push_event(heart_event(HeartEvent::RigSettled, Some(player_id), tile));
            }
        }
    }

    fn advance_wells(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let frame = self.frame();
        let players: Vec<(PlayerId, f32, f32)> = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.x, player.y))
            .collect();
        let seconds = self.timing.delta_seconds;
        let now_second = self.millis_at(next_tick) / 1_000;
        let last_second = self.millis_at(self.simulation_tick) / 1_000;
        for (player_id, x, y) in players {
            let tile = frame.tile_at(x, y);
            let pull = self.well_at(x, y);
            let caught = pull.is_some_and(|(_, weight)| weight >= WELL_CAUGHT_SHARE);
            if caught && self.heart.in_well.insert(player_id) {
                output.push_event(heart_event(HeartEvent::WellCaught, Some(player_id), tile));
            } else if !caught {
                self.heart.in_well.remove(&player_id);
            }
            let Some((well, _)) = pull else {
                continue;
            };
            let (cx, cy) = frame.tile_center(well.centre);
            if (x - cx).hypot(y - cy) <= HORIZON_TILES * TILE_SIZE {
                let scale = crate::enemy::tier_scale(self.tier_at(tile));
                if self.damage_player(player_id, HORIZON_DAMAGE_PER_SECOND * scale * seconds) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    if now_second != last_second {
                        output.push_event(heart_event(
                            HeartEvent::HorizonCrush,
                            Some(player_id),
                            tile,
                        ));
                    }
                }
            }
        }
    }

    fn advance_heart_landmarks(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        if self.heart.found == 0b1111 {
            return;
        }
        let settings = *self.terrain.settings();
        let (heart_top, _) = layout.descent_rows(Descent::Heart);
        let frame = self.frame();
        let actors: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .filter_map(|id| {
                self.actor_position(*id)
                    .map(|(x, y)| (*id, frame.tile_at(x, y)))
            })
            .filter(|(_, tile)| tile.y >= heart_top - 30)
            .collect();
        for (player_id, tile) in actors {
            let lens = places::lens(&settings, &layout);
            let (dx, dy) = (tile.x - lens.centre.x, tile.y - lens.centre.y);
            let at_lens = dx * dx + dy * dy <= (lens.radius + 3) * (lens.radius + 3);
            let found = if places::upfall(&settings, &layout).contains(tile) {
                Some(HeartLandmark::Upfall)
            } else if at_lens {
                Some(HeartLandmark::Lens)
            } else if places::ferry_terminal(&settings, &layout)
                .expanded(2)
                .contains(tile)
            {
                Some(HeartLandmark::FerryTerminal)
            } else if self.in_heart_chamber(tile) {
                Some(HeartLandmark::HeartChamber)
            } else {
                None
            };
            if let Some(landmark) = found
                && self.heart.find(landmark)
            {
                output.push_event(GameplayEventKind::HeartLandmarkFound {
                    player_id,
                    landmark,
                });
                output.push_event(heart_event(
                    HeartEvent::LandmarkFound,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    fn advance_star_drive(&mut self, output: &mut TickOutput) {
        if !self.heart.fight.beaten {
            return;
        }
        let frame = self.frame();
        let players: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .collect();
        // Every rig of the crew carries it (later joiners are fitted as they arrive).
        for (player_id, tile) in &players {
            if self.heart.star_drive.insert(*player_id) {
                output.push_event(heart_event(
                    HeartEvent::StarDriveFitted,
                    Some(*player_id),
                    *tile,
                ));
            }
        }
        if !self.heart.space_unlocked {
            return;
        }
        // Over the Moon after the ending: the stars are open.
        let Some(moon_top) = self.cable_path().and_then(|path| path.moon_top) else {
            return;
        };
        for (player_id, tile) in players {
            if tile.y < moon_top - 20 && self.heart.stars_seen.insert(player_id) {
                output.push_event(heart_event(HeartEvent::StarsOpen, Some(player_id), tile));
            }
        }
    }
}
