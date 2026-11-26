//! The endgame in the world (Wave 13 endgame, see [`crate::endgame`]): the Works counter at a
//! docked station, alien estates and their neighbours, the hunt list, completion, the space
//! achievements' facts, and the ship's effects with every late upgrade.

use std::collections::BTreeSet;

use crate::{
    alien::{AlienSpecies, language::Fluency, town},
    endgame::{
        Career, EndgameEvent, EndgameRefusal, EndgameState, WorkOrder,
        completion::{Completion, CompletionFacts},
        hunt::{Quarry, THEME_BOSSES, hunt_list},
        materials::{SPACE_MATERIALS, belt_material, is_space_material, station_stock},
        works::{
            Bill, ESTATE_FULL, ESTATE_WINGS, GATE_ARRIVAL_TILES, GATE_JUMP_MILLIS, GATE_PARTS,
            HEARTLINE_BILL, HEARTLINE_PRICE, HEARTLINE_THEME_BOSSES, MARK_MAX, OUTFITTER,
            ShipEffects, StarwrightPart, builds_starwright, estate_bill, estate_price,
            gate_part_bill, gate_part_price, has_outfitter, line_bill, mark_bill, mark_fluency,
            mark_price, ship_effects,
        },
    },
    event::{GameplayEventKind, TickOutput},
    flight::{
        FlightPlace, StarDrive, SystemRef, TransitRun, WarpRun,
        space::{SpaceQuery, advance, nearest_surface, tiles},
        systems::SystemsEvent,
    },
    hostiles::{Faction, HostileEvent},
    ids::PlayerId,
    item::ItemKind,
    milestone::{Milestone, MilestoneGroup, SpaceFacts},
    people::VillageId,
    place::StoreKey,
    station::{AlienTech, ShipLine, StationOwner, StationProfile, shipyard::LINE_MAX},
    summary::PlayerScopedCommandOutcome,
    terrain::MineralKind,
};

use super::WorldState;

/// Understanding of the town's tongue a neighbours' visit teaches.
pub const NEIGHBOUR_UNDERSTANDING: u16 = 12;
/// Goodwill a visit leaves with each neighbour.
pub const NEIGHBOUR_GOODWILL: i16 = 2;

const SALT_NEIGHBOURS: u64 = 0xE5_7A7E_0013;

const fn endgame_event(what: EndgameEvent, player_id: PlayerId, detail: u32) -> GameplayEventKind {
    GameplayEventKind::Endgame {
        what,
        player_id: Some(player_id),
        detail,
    }
}

const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The crew's endgame state.
    #[must_use]
    pub const fn endgame(&self) -> &EndgameState {
        &self.endgame
    }

    /// Replaces the crew's endgame state (scenes, tests, tools).
    pub fn set_endgame(&mut self, endgame: EndgameState) {
        self.endgame = endgame;
    }

    /// `player_id`'s endgame record.
    #[must_use]
    pub fn career(&self, player_id: PlayerId) -> Career {
        self.endgame.career(player_id)
    }

    /// What `player_id`'s ship does with its lines, alien tech marks, the Starwright ship and
    /// the Heartline Drive.
    #[must_use]
    pub fn ship_effects(&self, player_id: PlayerId) -> ShipEffects {
        let gear = self.ship_gear(player_id);
        ship_effects(&gear, self.endgame.careers.get(&player_id))
    }

    /// Records a theme boss beaten (`theme` is its planet theme's id; the outland and exotic
    /// worlds' boss fights call this when theirs falls). Returns whether it was new.
    pub fn record_theme_boss(
        &mut self,
        theme: u16,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) -> bool {
        if !self.endgame.theme_bosses.insert(theme) {
            return false;
        }
        output.push_event(GameplayEventKind::Endgame {
            what: EndgameEvent::QuarryDown,
            player_id: by,
            detail: Quarry::Theme(theme).code(),
        });
        true
    }

    /// Whether `quarry` on the hunt list is beaten (by the crew).
    #[must_use]
    pub fn quarry_beaten(&self, quarry: Quarry) -> bool {
        match quarry {
            Quarry::Faction(faction) => self.hostiles.bosses_beaten.contains(&faction),
            Quarry::Theme(theme) => self.endgame.theme_bosses.contains(&theme),
        }
    }

    /// The hunt list with each quarry's state: beaten, and the system to head for (a faction
    /// boss's lair; a theme boss's nearest world of its kind round home, Wave 13 integration).
    #[must_use]
    pub fn hunt(&self) -> Vec<(Quarry, bool, Option<SystemRef>)> {
        let seed = self.world_seed();
        hunt_list()
            .into_iter()
            .map(|quarry| {
                let lair = match quarry {
                    Quarry::Faction(faction) => self.boss_lair(faction).map(|lair| lair.system),
                    Quarry::Theme(theme) => THEME_BOSSES
                        .iter()
                        .find(|boss| boss.theme == theme)
                        .and_then(|boss| {
                            crate::endgame::hunt::nearest_theme_world(seed, boss.world)
                        })
                        .and_then(|id| {
                            let space = crate::space::Space::shared(seed);
                            let sector = space.sector(id.sector);
                            let system = sector
                                .systems
                                .iter()
                                .find(|found| found.index == id.system)?;
                            Some(crate::flight::space::GeneratedSpace::reference(
                                id.sector, system,
                            ))
                        }),
                };
                (quarry, self.quarry_beaten(quarry), lair)
            })
            .collect()
    }

    /// Theme bosses the Heartline Drive asks for (as many as are registered, up to three).
    #[must_use]
    pub fn heartline_theme_bosses(&self) -> usize {
        THEME_BOSSES.len().min(HEARTLINE_THEME_BOSSES)
    }

    /// Whether `player_id` holds a finished home on an alien world.
    #[must_use]
    pub fn owns_alien_home(&self, player_id: PlayerId) -> bool {
        self.aliens
            .homes_of(player_id)
            .any(|home| home.home.is_some())
    }

    /// What the completion screen counts for `player_id`.
    #[must_use]
    pub fn completion_facts(&self, player_id: PlayerId) -> CompletionFacts {
        let career = self.endgame.careers.get(&player_id);
        let fluent = |species: AlienSpecies| {
            Fluency::of(self.aliens.understanding(player_id, species)) >= Fluency::Fluent
        };
        let met = |species: AlienSpecies| self.aliens.log(player_id, species).met_millis.is_some();
        let count = |iter: usize| u8::try_from(iter).unwrap_or(u8::MAX);
        let gear = self.ship_gear(player_id);
        let earned = self.players.get(&player_id).map_or(0, |player| {
            Milestone::ALL
                .into_iter()
                .filter(|milestone| *milestone != Milestone::Completionist)
                .filter(|milestone| player.milestones.has(*milestone))
                .count()
        });
        CompletionFacts {
            worlds: career.map_or(0, |career| {
                u16::try_from(career.landed.len()).unwrap_or(u16::MAX)
            }),
            themes: career.map_or(0, |career| {
                count(
                    career
                        .themes
                        .iter()
                        .filter(|theme| crate::space::Theme::PLANET.contains(theme))
                        .count(),
                )
            }),
            species_met: count(AlienSpecies::ALL.into_iter().filter(|s| met(*s)).count()),
            species_total: count(AlienSpecies::ALL.len()),
            tongues: count(AlienSpecies::ALL.into_iter().filter(|s| fluent(*s)).count()),
            descent_bosses: count(self.expedition.bosses.count_ones() as usize),
            faction_bosses: count(self.hostiles.bosses_beaten.len()),
            theme_bosses: count(
                THEME_BOSSES
                    .iter()
                    .filter(|boss| self.endgame.theme_bosses.contains(&boss.theme))
                    .count(),
            ),
            theme_bosses_total: count(THEME_BOSSES.len()),
            logs: self.expedition.found,
            logs_total: count(crate::expedition::LOGS.len()),
            cable_lit: self.expedition.cable_lit,
            // The galaxy story's chapters done and in all (Wave 13 integration).
            story_done: self.galaxy_story.chapters_done(),
            story_total: crate::galaxy_story::CHAPTER_COUNT,
            catalogued: career.map_or(0, |career| count(career.catalogue.count_ones() as usize)),
            lines_maxed: count(
                ShipLine::ALL
                    .into_iter()
                    .filter(|line| gear.line(*line) >= LINE_MAX)
                    .count(),
            ),
            starwright_parts: career
                .map_or(0, |career| count(career.starwright.count_ones() as usize)),
            heartline: career.is_some_and(|career| career.heartline),
            gate_parts: self.endgame.gate_parts_done(),
            estate_full: self.estate_full(player_id),
            achievements: u16::try_from(earned).unwrap_or(u16::MAX),
            achievements_total: u16::try_from(Milestone::ALL.len() - 1).unwrap_or(u16::MAX),
        }
    }

    /// `player_id`'s completion.
    #[must_use]
    pub fn completion(&self, player_id: PlayerId) -> Completion {
        Completion::of(&self.completion_facts(player_id))
    }

    /// Whether `player_id` raised every wing of an estate.
    fn estate_full(&self, player_id: PlayerId) -> bool {
        self.aliens
            .homes_of(player_id)
            .any(|home| self.endgame.estate(home.town, home.plot).expansions.len() >= ESTATE_FULL)
    }

    /// The space achievements' facts for `player_id` (default on a world without space).
    #[must_use]
    pub fn space_facts(&self, player_id: PlayerId) -> SpaceFacts {
        if !self.space_enabled() {
            return SpaceFacts::default();
        }
        let career = self.endgame.careers.get(&player_id);
        let facts = self.completion_facts(player_id);
        let gear = self.ship_gear(player_id);
        let faction_bosses = Faction::ALL
            .into_iter()
            .filter(|faction| self.hostiles.bosses_beaten.contains(faction))
            .fold(0_u8, |bits, faction| bits | 1 << faction.code());
        SpaceFacts {
            mined: career.map_or(0, |career| career.mined),
            comet_dust: career.map_or(0, |career| career.comet_dust),
            worlds: facts.worlds,
            themes: facts.themes,
            species_met: facts.species_met,
            tongues: facts.tongues,
            species_total: facts.species_total,
            faction_bosses,
            hunt_done: hunt_list()
                .into_iter()
                .all(|quarry| self.quarry_beaten(quarry)),
            line_maxed: ShipLine::ALL
                .into_iter()
                .any(|line| gear.line(line) >= LINE_MAX),
            starwright: career.is_some_and(Career::starwright_ship),
            heartline: career.is_some_and(|career| career.heartline),
            gate_open: self.endgame.gate_open(),
            alien_home: self.owns_alien_home(player_id),
            estate_full: facts.estate_full,
            complete: Completion::of(&facts).complete(),
        }
    }

    // --- The Works counter -------------------------------------------------------------------

    /// What `order` costs `player_id` at `station` and whether it may be placed: the bill, or
    /// why not (the counter shows both).
    ///
    /// # Errors
    ///
    /// The [`EndgameRefusal`] the counter would answer with: not offered here, locked behind
    /// something that must come first, or already done.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per kind of order reads best together"
    )]
    pub fn works_quote(
        &self,
        player_id: PlayerId,
        station: &StationProfile,
        order: WorkOrder,
    ) -> Result<Bill, EndgameRefusal> {
        let career = self.career(player_id);
        let gear = self.ship_gear(player_id);
        match order {
            WorkOrder::BuyMaterial(mineral, units) => {
                let (_, price) = station_stock(station)
                    .into_iter()
                    .find(|(stock, _)| *stock == mineral)
                    .ok_or(EndgameRefusal::NotHere)?;
                Ok(Bill {
                    credits: price.saturating_mul(u32::from(units.max(1))),
                    materials: &[],
                })
            }
            WorkOrder::AlienMark(tech) => {
                let StationOwner::Alien(kin) = station.owner else {
                    return Err(EndgameRefusal::NotHere);
                };
                if !station.alien_tech {
                    return Err(EndgameRefusal::NotHere);
                }
                if !gear.has(tech) {
                    return Err(EndgameRefusal::Locked);
                }
                let mark = career.mark(tech);
                if mark >= MARK_MAX {
                    return Err(EndgameRefusal::Done);
                }
                let next = mark + 1;
                if Fluency::of(self.aliens.understanding(player_id, kin.species()))
                    < mark_fluency(next)
                {
                    return Err(EndgameRefusal::Locked);
                }
                Ok(Bill {
                    credits: mark_price(tech, next),
                    materials: mark_bill(tech, next),
                })
            }
            WorkOrder::Starwright(part) => {
                if !builds_starwright(station) {
                    return Err(EndgameRefusal::NotHere);
                }
                if career.has_starwright(part) {
                    return Err(EndgameRefusal::Done);
                }
                if gear.line(part.line()) < LINE_MAX {
                    return Err(EndgameRefusal::Locked);
                }
                Ok(Bill {
                    credits: part.price(),
                    materials: part.bill(),
                })
            }
            WorkOrder::Heartline => {
                if !builds_starwright(station) {
                    return Err(EndgameRefusal::NotHere);
                }
                if career.heartline {
                    return Err(EndgameRefusal::Done);
                }
                let theme_bosses = THEME_BOSSES
                    .iter()
                    .filter(|boss| self.endgame.theme_bosses.contains(&boss.theme))
                    .count();
                let factions = Faction::ALL
                    .into_iter()
                    .all(|faction| self.hostiles.bosses_beaten.contains(&faction));
                if !career.starwright_ship()
                    || !factions
                    || theme_bosses < self.heartline_theme_bosses()
                {
                    return Err(EndgameRefusal::Locked);
                }
                Ok(Bill {
                    credits: HEARTLINE_PRICE,
                    materials: HEARTLINE_BILL,
                })
            }
            WorkOrder::GatePart(index) => {
                if !station.halcyon || index >= GATE_PARTS {
                    return Err(EndgameRefusal::NotHere);
                }
                if self.endgame.gate_part(index) {
                    return Err(EndgameRefusal::Done);
                }
                // The keystone goes in last.
                if index + 1 == GATE_PARTS && self.endgame.gate_parts_done() + 1 < GATE_PARTS {
                    return Err(EndgameRefusal::Locked);
                }
                Ok(Bill {
                    credits: gate_part_price(index),
                    materials: gate_part_bill(index),
                })
            }
            WorkOrder::GateJump(system) => {
                if !station.halcyon {
                    return Err(EndgameRefusal::NotHere);
                }
                if !self.endgame.gate_open()
                    || system == crate::flight::space::HOME_SYSTEM
                    || !self.known_systems().contains(&system)
                {
                    return Err(EndgameRefusal::NoRoute);
                }
                Ok(Bill {
                    credits: 0,
                    materials: &[],
                })
            }
            WorkOrder::Cosmetic(item) => {
                if !has_outfitter(station) {
                    return Err(EndgameRefusal::NotHere);
                }
                let (_, price) = OUTFITTER
                    .iter()
                    .find(|(stock, _)| *stock == item)
                    .ok_or(EndgameRefusal::NotHere)?;
                if self
                    .players
                    .get(&player_id)
                    .is_some_and(|player| player.cosmetic_unlocks.has(item))
                {
                    return Err(EndgameRefusal::Done);
                }
                Ok(Bill {
                    credits: *price,
                    materials: &[],
                })
            }
        }
    }

    fn endgame_refuse(
        player_id: PlayerId,
        why: EndgameRefusal,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        output.push_event(endgame_event(EndgameEvent::Refused, player_id, why.code()));
        PlayerScopedCommandOutcome::IgnoredUnavailable
    }

    /// Pays `bill` from `player_id`'s purse and cargo bay, if it can.
    fn pay_bill(&mut self, player_id: PlayerId, bill: &Bill) -> Result<(), EndgameRefusal> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(EndgameRefusal::NotHere)?;
        if player.credits < bill.credits {
            return Err(EndgameRefusal::Credits);
        }
        if !bill.materials_in(&player.cargo_bay) {
            return Err(EndgameRefusal::Materials);
        }
        player.credits -= bill.credits;
        bill.take_from(&mut player.cargo_bay);
        Ok(())
    }

    /// A Works order at the station `player_id` is docked at.
    pub(super) fn station_works(
        &mut self,
        player_id: PlayerId,
        station: &StationProfile,
        order: WorkOrder,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let bill = match self.works_quote(player_id, station, order) {
            Ok(bill) => bill,
            Err(why) => return Self::endgame_refuse(player_id, why, output),
        };
        // Materials bought need room in the bay first.
        if let WorkOrder::BuyMaterial(mineral, units) = order {
            let room = self.players.get(&player_id).map_or(0, |player| {
                player.room_for(ItemKind::Mineral(mineral), u32::from(units.max(1)))
            });
            if room == 0 {
                return Self::endgame_refuse(player_id, EndgameRefusal::BayFull, output);
            }
            let price = bill.credits / u32::from(units.max(1));
            let bill = Bill {
                credits: price.saturating_mul(room),
                materials: &[],
            };
            if let Err(why) = self.pay_bill(player_id, &bill) {
                return Self::endgame_refuse(player_id, why, output);
            }
            if let Some(player) = self.players.get_mut(&player_id) {
                player.cargo_bay.add(mineral, room);
            }
            output.push_event(endgame_event(
                EndgameEvent::MaterialsBought,
                player_id,
                bill.credits,
            ));
            output.push_event(GameplayEventKind::CargoChanged { player_id });
            return PlayerScopedCommandOutcome::Applied;
        }
        if let Err(why) = self.pay_bill(player_id, &bill) {
            return Self::endgame_refuse(player_id, why, output);
        }
        match order {
            WorkOrder::BuyMaterial(..) => {}
            WorkOrder::AlienMark(tech) => {
                let career = self.endgame.career_mut(player_id);
                career.marks[tech as usize] = career.marks[tech as usize].saturating_add(1);
                let mark = career.mark(tech);
                output.push_event(endgame_event(
                    EndgameEvent::MarkRaised,
                    player_id,
                    u32::from(mark),
                ));
            }
            WorkOrder::Starwright(part) => {
                self.endgame.career_mut(player_id).starwright |= part.bit();
                output.push_event(endgame_event(
                    EndgameEvent::StarwrightFitted,
                    player_id,
                    part.code(),
                ));
            }
            WorkOrder::Heartline => {
                self.endgame.career_mut(player_id).heartline = true;
                output.push_event(endgame_event(EndgameEvent::HeartlineFitted, player_id, 0));
            }
            WorkOrder::GatePart(index) => {
                self.endgame.gate_parts |= 1 << index;
                self.endgame.gate_builders.insert(index, player_id);
                output.push_event(endgame_event(
                    EndgameEvent::GatePartDelivered,
                    player_id,
                    u32::from(index),
                ));
                if self.endgame.gate_open() {
                    output.push_event(GameplayEventKind::Endgame {
                        what: EndgameEvent::GateFinished,
                        player_id: Some(player_id),
                        detail: u32::from(GATE_PARTS),
                    });
                }
            }
            WorkOrder::GateJump(system) => self.gate_jump(player_id, system, output),
            WorkOrder::Cosmetic(item) => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.cosmetic_unlocks.unlock(item);
                }
                let career = self.endgame.career_mut(player_id);
                career.outfitted = career.outfitted.saturating_add(1);
                output.push_event(endgame_event(
                    EndgameEvent::CosmeticBought,
                    player_id,
                    bill.credits,
                ));
            }
        }
        if !matches!(order, WorkOrder::GateJump(_)) {
            output.push_event(GameplayEventKind::PurchaseCompleted { player_id });
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Where the gate puts a rig riding to `system`: [`GATE_ARRIVAL_TILES`] from its star on
    /// the side facing `from`, turned a quarter at a time until it is clear of every body.
    fn gate_arrival(
        &self,
        from: crate::position::WorldPos,
        system: SystemRef,
    ) -> Option<crate::position::WorldPos> {
        let space = self.space();
        let star = space.system(system)?.star;
        let (dx, dy) = star.delta_f64(from);
        let length = crate::polar::trig::hypot(dx, dy).max(1.0);
        let (ux, uy) = (dx / length, dy / length);
        // Clear of the star's own approach (its heat and light) by half again.
        let star_approach = self
            .space_bodies_near(star)
            .iter()
            .filter(|body| body.centre.span(star) < tiles(1.0))
            .map(|body| f64::from(body.approach))
            .fold(0.0, f64::max);
        let reach = (f64::from(tiles(1.0)) * GATE_ARRIVAL_TILES).max(star_approach * 1.5);
        let turns = [(ux, uy), (-uy, ux), (uy, -ux), (-ux, -uy)];
        turns
            .into_iter()
            .map(|(x, y)| advance(star, x * reach, y * reach))
            .find(|at| {
                let bodies = self.space_bodies_near(*at);
                let (gap, _) = nearest_surface(&bodies, *at);
                bodies
                    .iter()
                    .all(|body| at.span(body.centre) > body.approach)
                    && gap > tiles(2_000.0)
            })
            .or_else(|| Some(advance(star, ux * reach, uy * reach)))
    }

    /// The ride through the Halcyon Gate: a streak like a warp's, out into free flight.
    fn gate_jump(&mut self, player_id: PlayerId, system: SystemRef, output: &mut TickOutput) {
        let Some(origin) = self.players.get(&player_id).map(|player| player.flight.pos) else {
            return;
        };
        let Some(arrival) = self.gate_arrival(origin, system) else {
            return;
        };
        let now = self.millis_at(self.simulation_tick.next());
        let (dx, dy) = origin.delta_f64(arrival);
        let length = crate::polar::trig::hypot(dx, dy).max(1.0);
        #[allow(clippy::cast_possible_truncation, reason = "a unit heading")]
        let (hx, hy) = ((dx / length) as f32, (dy / length) as f32);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.flight.place = FlightPlace::Space;
            player.flight.velocity_x = 0.0;
            player.flight.velocity_y = 0.0;
            player.flight.drive = StarDrive::Transit(TransitRun {
                run: WarpRun {
                    origin,
                    arrival,
                    target: system,
                    start: now,
                    duration_millis: GATE_JUMP_MILLIS,
                },
                heading_x: hx,
                heading_y: hy,
            });
            player.attitude.angular_velocity = 0.0;
        }
        let _ = self.flight.found(system);
        output.push_event(endgame_event(
            EndgameEvent::GateJumped,
            player_id,
            u32::try_from(system.id & 0xFFFF_FFFF).unwrap_or(0),
        ));
    }

    // --- Alien estates -----------------------------------------------------------------------

    /// Whether `player_id` stands in the town of their finished home (`town`, `plot`): the
    /// refusal if not.
    fn at_own_alien_home(
        &self,
        player_id: PlayerId,
        town_id: VillageId,
        plot: u8,
    ) -> Result<(std::sync::Arc<town::AlienTown>, AlienSpecies), EndgameRefusal> {
        let found = town::town_by_id(town_id).ok_or(EndgameRefusal::NoHome)?;
        let home = self
            .aliens
            .home_at(town_id, plot)
            .filter(|home| home.owner == player_id && home.home.is_some())
            .ok_or(EndgameRefusal::NoHome)?;
        if found.buildings.get(usize::from(plot)).is_none() {
            return Err(EndgameRefusal::NoHome);
        }
        // Anywhere in the town: the orders go through the town's builder (the rows sit on the
        // builder's Homes page, and the builder walks a schedule, rarely at the house's door);
        // an estate's wings stand either side of the house at the town's edge, so the reach
        // grows with them.
        let wings = i64::try_from(self.endgame.estate(town_id, plot).expansions.len()).unwrap_or(0);
        let at_home = self.store_of(player_id) == StoreKey::Body(home.body)
            && self.actor_tile_anywhere(player_id).is_some_and(|tile| {
                town::wrapped_distance(i64::from(tile.x), found.site.centre, found.strip_width())
                    <= i64::from(found.site.half_width) + super::aliens::HOME_REACH.0 + wings * 4
                    && (tile.y - found.ground_row()).abs() <= super::aliens::HOME_REACH.1
            });
        if !at_home {
            return Err(EndgameRefusal::NotHere);
        }
        let species = found.species;
        Ok((found, species))
    }

    /// The next wing of `player_id`'s estate at (`town`, `plot`) and its bill (`None`: the
    /// estate has every wing).
    #[must_use]
    pub fn estate_quote(
        &self,
        town_id: VillageId,
        plot: u8,
    ) -> Option<(crate::home::blueprint::Expansion, Bill)> {
        let done = self.endgame.estate(town_id, plot).expansions.len();
        let wing = *ESTATE_WINGS.get(done)?;
        Some((
            wing,
            Bill {
                credits: estate_price(wing, done),
                materials: estate_bill(wing),
            },
        ))
    }

    /// Raises the next wing of `player_id`'s home (`town`, `plot`) on an alien world.
    pub(super) fn expand_estate(
        &mut self,
        player_id: PlayerId,
        town_id: VillageId,
        plot: u8,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let refuse = |why: EndgameRefusal, events: &mut Vec<GameplayEventKind>| {
            events.push(endgame_event(EndgameEvent::Refused, player_id, why.code()));
            PlayerScopedCommandOutcome::IgnoredUnavailable
        };
        let (found, _) = match self.at_own_alien_home(player_id, town_id, plot) {
            Ok(found) => found,
            Err(why) => return refuse(why, events),
        };
        let Some((wing, bill)) = self.estate_quote(town_id, plot) else {
            return refuse(EndgameRefusal::Done, events);
        };
        if let Err(why) = self.pay_bill(player_id, &bill) {
            return refuse(why, events);
        }
        if let Some(home) = self
            .aliens
            .homes
            .iter_mut()
            .find(|home| home.town == town_id && home.plot == plot)
            .and_then(|home| home.home.as_mut())
        {
            home.expand(wing);
        }
        let estate = self.endgame.estates.entry((town_id, plot)).or_default();
        estate.expansions.push(wing);
        let wings = u32::try_from(estate.expansions.len()).unwrap_or(u32::MAX);
        // The town's builder raised it: goodwill.
        if let Some(builder) = found.keeper(town::BUILDER) {
            self.add_goodwill(player_id, builder.id, 3);
        }
        events.push(endgame_event(
            EndgameEvent::EstateExpanded,
            player_id,
            wings,
        ));
        events.push(GameplayEventKind::PurchaseCompleted { player_id });
        PlayerScopedCommandOutcome::Applied
    }

    /// The neighbours call at `player_id`'s home (`town`, `plot`): once a day, a gift on the
    /// doorstep (common space materials, now and then comet dust), a few words of their
    /// tongue, and goodwill both ways.
    pub(super) fn call_neighbours(
        &mut self,
        player_id: PlayerId,
        town_id: VillageId,
        plot: u8,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let refuse = |why: EndgameRefusal, events: &mut Vec<GameplayEventKind>| {
            events.push(endgame_event(EndgameEvent::Refused, player_id, why.code()));
            PlayerScopedCommandOutcome::IgnoredUnavailable
        };
        let (found, species) = match self.at_own_alien_home(player_id, town_id, plot) {
            Ok(found) => found,
            Err(why) => return refuse(why, events),
        };
        let day = self.people_clock().day;
        if self.endgame.estate(town_id, plot).visited_day == Some(day) {
            return refuse(EndgameRefusal::AlreadyToday, events);
        }
        let roll = mix(self.world_seed()
            ^ SALT_NEIGHBOURS
            ^ (u64::from(town_id.0) << 20)
            ^ (u64::from(plot) << 12)
            ^ u64::from(day).wrapping_mul(0x9E37_79B9));
        // Bigger estates have more neighbours dropping by.
        let wings = self.endgame.estate(town_id, plot).expansions.len();
        let gift = if roll.is_multiple_of(7) {
            MineralKind::CometDust
        } else {
            SPACE_MATERIALS[usize::try_from((roll >> 8) % 4).unwrap_or(0)]
        };
        let units = if gift == MineralKind::CometDust {
            1
        } else {
            3 + u32::try_from((roll >> 16) % 4).unwrap_or(0) + u32::try_from(wings).unwrap_or(0)
        };
        let given = self
            .players
            .get_mut(&player_id)
            .map_or(0, |player| player.stow(ItemKind::Mineral(gift), units));
        let estate = self.endgame.estates.entry((town_id, plot)).or_default();
        estate.visited_day = Some(day);
        estate.gifts = estate.gifts.saturating_add(given);
        self.learn_tongue(player_id, species, NEIGHBOUR_UNDERSTANDING, events);
        // The neighbours: the residents of the two nearest buildings (the town's keepers).
        for slot in [town::TRADER, town::BUILDER] {
            if let Some(neighbour) = found.keeper(slot) {
                self.add_goodwill(player_id, neighbour.id, NEIGHBOUR_GOODWILL);
            }
        }
        events.push(endgame_event(
            EndgameEvent::NeighboursCalled,
            player_id,
            given,
        ));
        if given > 0 {
            events.push(GameplayEventKind::CargoChanged { player_id });
        }
        PlayerScopedCommandOutcome::Applied
    }

    // --- The tick ------------------------------------------------------------------------------

    /// The endgame's pass, once a tick with every rig back from its store: worlds landed,
    /// space materials mined, quarries down, and the space achievements (the home pass only
    /// sees the players at home).
    pub(super) fn advance_endgame(&mut self, output: &mut TickOutput) {
        if !self.space_enabled() {
            return;
        }
        // What this tick's events say.
        let mut mined: Vec<(PlayerId, MineralKind)> = Vec::new();
        let mut quarries: Vec<(Option<PlayerId>, Faction)> = Vec::new();
        for event in &output.events {
            match event.kind {
                GameplayEventKind::SpaceSystems {
                    what: SystemsEvent::OreMined,
                    player_id,
                    detail,
                } => {
                    if let Some(ore) = crate::space::AsteroidOre::ALL
                        .get(usize::try_from(detail).unwrap_or(usize::MAX))
                    {
                        mined.push((player_id, belt_material(*ore)));
                    }
                }
                GameplayEventKind::SpaceSystems {
                    what: SystemsEvent::CometDust,
                    player_id,
                    ..
                } => mined.push((player_id, crate::endgame::materials::COMET_MATERIAL)),
                GameplayEventKind::Hostile {
                    what: HostileEvent::BossDefeated,
                    player_id,
                    detail,
                } => {
                    if let Some(faction) = Faction::from_code(detail) {
                        quarries.push((player_id, faction));
                    }
                }
                _ => {}
            }
        }
        for (player_id, mineral) in mined {
            if !is_space_material(mineral) {
                continue;
            }
            let career = self.endgame.career_mut(player_id);
            career.mined = career.mined.saturating_add(1);
            if mineral == MineralKind::CometDust {
                career.comet_dust = career.comet_dust.saturating_add(1);
            }
            if let Some(index) = SPACE_MATERIALS.iter().position(|each| *each == mineral) {
                career.catalogue |= 1 << index;
            }
        }
        for (player_id, faction) in quarries {
            output.push_event(GameplayEventKind::Endgame {
                what: EndgameEvent::QuarryDown,
                player_id,
                detail: Quarry::Faction(faction).code(),
            });
        }
        self.note_landings(output);
        self.advance_space_milestones(output);
    }

    /// Records every world a rig stands on for the first time.
    fn note_landings(&mut self, output: &mut TickOutput) {
        let landed: Vec<(PlayerId, crate::flight::BodyRef)> = self
            .players
            .iter()
            .filter_map(|(id, player)| match player.flight.place {
                FlightPlace::Landed(body) => Some((*id, body)),
                _ => None,
            })
            .collect();
        if landed.is_empty() {
            return;
        }
        let galaxy = self.space();
        for (player_id, body) in landed {
            let Some(key) = galaxy.space_body(body) else {
                continue;
            };
            if self
                .endgame
                .careers
                .get(&player_id)
                .is_some_and(|career| career.landed.contains(&key))
            {
                continue;
            }
            let theme = galaxy.space().body(key).map(|found| found.theme);
            let career = self.endgame.career_mut(player_id);
            career.landed.insert(key);
            if let Some(theme) = theme {
                career.themes.insert(theme);
            }
            let worlds = u32::try_from(career.landed.len()).unwrap_or(u32::MAX);
            output.push_event(endgame_event(EndgameEvent::WorldLanded, player_id, worlds));
        }
    }

    /// The space achievements for every player wherever they are (the home pass's milestone
    /// check sees only the players at home). Primed players only, and never in test mode.
    fn advance_space_milestones(&mut self, output: &mut TickOutput) {
        if self.is_test_mode() {
            return;
        }
        let tick = self.simulation_tick.get();
        let players: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| player.milestones.primed)
            .map(|(id, _)| *id)
            .collect();
        for player_id in players {
            let space = self.space_facts(player_id);
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            let facts = crate::milestone::MilestoneFacts {
                space,
                ..crate::milestone::MilestoneFacts::default()
            };
            let met: Vec<Milestone> = Milestone::ALL
                .into_iter()
                .filter(|milestone| milestone.group() == MilestoneGroup::Space)
                .filter(|milestone| !player.milestones.has(*milestone))
                .filter(|milestone| milestone.met(player, &facts))
                .collect();
            for milestone in met {
                if player.earn_milestone(milestone, tick) {
                    output.push_event(GameplayEventKind::MilestoneEarned {
                        player_id,
                        milestone,
                    });
                }
            }
        }
    }

    /// The landings, materials and quarries every peer agrees on (tests and tools): the set of
    /// planet themes `player_id` has stood on.
    #[must_use]
    pub fn themes_landed(&self, player_id: PlayerId) -> BTreeSet<crate::space::Theme> {
        self.endgame
            .careers
            .get(&player_id)
            .map_or_default(|career| career.themes.clone())
    }

    /// Whether `player_id` speaks `species`' tongue well enough for alien tech `mark`.
    #[must_use]
    pub fn speaks_for_mark(&self, player_id: PlayerId, species: AlienSpecies, mark: u8) -> bool {
        Fluency::of(self.aliens.understanding(player_id, species)) >= mark_fluency(mark)
    }

    /// The ship line material bill of `line`'s next tier for `player_id` (the shipyard shows
    /// it beside the price).
    #[must_use]
    pub fn line_bill_for(
        &self,
        player_id: PlayerId,
        line: ShipLine,
    ) -> crate::endgame::works::Materials {
        line_bill(line, self.ship_gear(player_id).line(line).saturating_add(1))
    }

    /// The tech's mark for `player_id` (0: not fitted).
    #[must_use]
    pub fn alien_tech_mark(&self, player_id: PlayerId, tech: AlienTech) -> u8 {
        if self.ship_gear(player_id).has(tech) {
            self.career(player_id).mark(tech)
        } else {
            0
        }
    }

    /// Starwright parts `player_id` has (for the HUD and the renderer).
    #[must_use]
    pub fn starwright_parts(&self, player_id: PlayerId) -> Vec<StarwrightPart> {
        let career = self.career(player_id);
        StarwrightPart::ALL
            .into_iter()
            .filter(|part| career.has_starwright(*part))
            .collect()
    }
}
