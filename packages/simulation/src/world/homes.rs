//! Contractor-built homes in the world (WP-HOME, see [`crate::home`]): buying plots, signing
//! contracts, construction stepped in world time, the site crews, decorating, beds, the garage
//! bay, the basement hatch and the private lift.
//!
//! # Where actions happen
//!
//! - Deeds for listed sites and furniture are bought at a **land office**: the town's
//!   headquarters or a village hall ([`WorldState::at_land_office`]). A surveyed spot is
//!   bought standing on it ([`WorldState::survey_plot`]).
//! - Contracts, rushes, answers and every interior action need the actor (the pilot on foot,
//!   else the rig) on the plot ([`crate::home::Plot::at_site`]): the game shows the interior
//!   while the pilot stands at the front door.
//!
//! # Construction
//!
//! `advance_homes` reads the world time (`WorldClock::world_millis`) once a tick: each
//! contract's stage follows from it, stage changes are announced once each
//! (`ConstructionStage`), the foundation levels the ground (terrain ops), planned story events
//! fire as the work reaches their stage, a materials shortage holds the work until answered or
//! until the contractor's patience runs out, and a finished job hands over the keys
//! (`HomeCompleted`). Because sleeping moves the clock, a night slept through moves the work on.
//!
//! # Site crews
//!
//! During working hours ([`crate::home::WORK_START_MINUTE`]..[`crate::home::WORK_END_MINUTE`])
//! the contractor and a few helpers from their settlement work on site as NPC entities with
//! `NpcState::site` set: the people pass leaves them alone (and keeps their village copies
//! from spawning twice), and talking to them is the ordinary people command, so the contractor
//! talks about the job and remembers you.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "tile coordinates and world pixels stay far inside the f32 grid near plots"
)]

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    base::{BuiltTile, Fixture},
    economy::SurfaceZone,
    entity::{Body, EntityId, EntityKind, EntityState, NpcState},
    event::{GameplayEventKind, TickOutput},
    home::{
        BuildStage, Construction, Deed, Furniture, Home, HomeAccess, HomeCommand, HomeJob,
        HomeQuote, HomeRefusal, MAX_PLOTS, Plot, PlotId, PlotSetting, PlotSite, PlotSize,
        SiteEvent, SiteEventKind, SiteKey,
        blueprint::{Expansion, LIFT_REACH_COLUMNS, MAX_FLOORS},
        construction::{REPEAT_CLIENT_DISCOUNT_PERCENT, plan_events, supply_discount},
        decor::{DecorService, blocks_to_take, displayable},
        plot,
    },
    ids::{PlayerId, SimulationTick},
    item::{ItemCategory, ItemKind},
    people::{
        self, CharacterId, MemoryFact,
        contractor::{ContractQuote, ContractorProfile, ContractorStyle, contractor_profile},
        routine::{Activity, TOWN_NAME, TOWN_VILLAGE},
    },
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    village::{self, BuildingKind},
    worldgen::noise::hash2,
};

use super::WorldState;

/// How far (columns) from a plot contractors come from.
pub const CONTRACTOR_REACH_COLUMNS: i32 = 900;
/// The most contractors the site board lists.
pub const MAX_CONTRACTORS_LISTED: usize = 6;
/// At most this many builders of one style are listed while builders of other styles are
/// within reach (so the choice of contractor is a real one).
pub const MAX_LISTED_PER_STYLE: usize = 2;
/// Columns within which an actor brings a site crew to work (like a settlement's people).
const CREW_NEAR_COLUMNS: i32 = 90;
/// Rows within which an actor brings a site crew to work.
const CREW_NEAR_ROWS: i32 = 30;
/// How often site crews are refreshed (spawned, sent home).
const CREW_REFRESH_MILLIS: u64 = 500;
/// How long a worker stays at one spot before moving on (ticks are derived from this).
const CREW_SHIFT_MILLIS: u64 = 4_500;
const SALT_CREW: u64 = 0x484F_4D45_0000_0020;
/// Credits a shortage's stock costs per block when paid for instead of brought.
const SHORTAGE_PRICE_PER_BLOCK: u32 = 12;
/// The most co-owners a deed may name.
const MAX_CO_OWNERS: usize = 3;
/// The town's middle column (wilds are priced by their distance from it).
const fn town_middle() -> i32 {
    let (first, last) = crate::surface::town_span();
    i32::midpoint(first, last)
}

/// A contractor the site board offers for a job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractorOffer {
    /// The person (`None`: the town builders' guild).
    pub character: Option<CharacterId>,
    pub profile: ContractorProfile,
    pub name: String,
    /// Where they live.
    pub from: String,
    /// Finished jobs for this client (they remember).
    pub past_jobs: u32,
}

/// What signing a contract would cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HireQuote {
    pub quote: ContractQuote,
    /// Credits due after discounts.
    pub price: u32,
    /// Credits off for supplied blocks.
    pub supply_discount: u32,
    /// Blocks handed over.
    pub supplied_blocks: u32,
    /// Credits off as a returning client.
    pub repeat_discount: u32,
    /// World milliseconds the work takes.
    pub millis: u64,
}

impl WorldState {
    /// Every plot, deed, home and furniture stock (WP-HOME).
    #[must_use]
    pub const fn homes(&self) -> &crate::home::Homes {
        &self.homes
    }

    /// World time now in milliseconds (construction runs on it).
    #[must_use]
    pub const fn world_millis(&self) -> u64 {
        self.clock().world_millis()
    }

    /// Whether `player` stands at a land office: the town headquarters or a village hall.
    #[must_use]
    pub fn at_land_office(&self, player: PlayerId) -> bool {
        if self.player_surface_zone(player) == Some(SurfaceZone::Headquarters) {
            return true;
        }
        self.player_village_building(player)
            .is_some_and(|(found, building)| {
                found
                    .buildings
                    .get(building)
                    .is_some_and(|lot| lot.kind == BuildingKind::Hall)
            })
    }

    /// The plot `player`'s actor stands on, if any.
    #[must_use]
    pub fn plot_here(&self, player: PlayerId) -> Option<&Plot> {
        let tile = self.actor_tile(player)?;
        self.homes.plot_at(tile)
    }

    /// Whether `player` may walk into the home on `plot`.
    #[must_use]
    pub fn may_enter_home(&self, player: PlayerId, plot: PlotId) -> bool {
        self.homes
            .plot(plot)
            .is_some_and(|plot| plot.home.is_some() && plot.deed.may_enter(player))
    }

    /// The listed sites around column `column` (the land office's list), west to east.
    #[must_use]
    pub fn land_office_sites(&self, column: i32, reach: i32) -> Vec<PlotSite> {
        plot::listed_sites_between(self.terrain.settings(), column - reach, column + reach)
    }

    /// Surveys the free spot of `size` centred where `player`'s actor stands: the site and
    /// price, or why it cannot be built on.
    ///
    /// # Errors
    ///
    /// The [`HomeRefusal`] when the spot cannot hold a plot.
    pub fn survey_plot(&self, player: PlayerId, size: PlotSize) -> Result<PlotSite, HomeRefusal> {
        let tile = self.actor_tile(player).ok_or(HomeRefusal::NotAtPlot)?;
        let settings = *self.terrain.settings();
        // The ground: the first solid row under the actor whose row above is open.
        let ground_row = (tile.y..=tile.y + 4)
            .find(|row| {
                let here = TilePosition { x: tile.x, y: *row };
                self.terrain.is_solid_at(here)
                    && !self.terrain.is_solid_at(TilePosition {
                        x: tile.x,
                        y: row - 1,
                    })
            })
            .ok_or(HomeRefusal::NotOpenGround)?;
        let width = size.width();
        let left = tile.x - width / 2;
        if !self.terrain.contains_column(left) || !self.terrain.contains_column(left + width - 1) {
            return Err(HomeRefusal::NotOpenGround);
        }
        // Sky lands and the Moon (V8): on their turf, else on the ground's own surface.
        let rect = crate::worldgen::TileRect {
            min: TilePosition {
                x: left,
                y: ground_row - 4,
            },
            max: TilePosition {
                x: left + width - 1,
                y: ground_row + 4,
            },
        };
        let land = crate::worldgen::sky::lands_touching(&settings, &rect)
            .into_iter()
            .find(|land| {
                land.surface_row(tile.x)
                    .is_some_and(|row| (row - ground_row).abs() <= 3)
            });
        let setting = match land.map(|land| land.kind) {
            Some(crate::worldgen::sky::SkyLandKind::Moon) => PlotSetting::Moon,
            Some(_) => PlotSetting::SkyIsland,
            None => {
                if (ground_row - self.terrain.surface_row(tile.x)).abs() > 3 {
                    return Err(HomeRefusal::NotOpenGround);
                }
                PlotSetting::Wilds
            }
        };
        let mut graded = 0u32;
        for x in left..left + width {
            let top = (ground_row - 5..=ground_row + 5).find(|row| {
                self.terrain.is_solid_at(TilePosition { x, y: *row })
                    && !self.terrain.is_solid_at(TilePosition { x, y: row - 1 })
            });
            let Some(top) = top else {
                return Err(HomeRefusal::TooUneven);
            };
            let step = (top - ground_row).unsigned_abs();
            if step > 4 {
                return Err(HomeRefusal::TooUneven);
            }
            graded += step;
            if self.terrain.is_lava_at(TilePosition { x, y: top }) {
                return Err(HomeRefusal::NotOpenGround);
            }
        }
        self.site_is_free(left, left + width - 1, ground_row)?;
        Ok(PlotSite {
            key: SiteKey::Surveyed,
            size,
            setting,
            left,
            ground_row,
            price: plot::survey_price(size, setting, tile.x - town_middle(), graded),
        })
    }

    /// Whether columns `first..=last` at `ground_row` are clear of the town, villages, claims,
    /// plots and listed sites.
    fn site_is_free(&self, first: i32, last: i32, ground_row: i32) -> Result<(), HomeRefusal> {
        let settings = self.terrain.settings();
        let (town_first, town_last) = crate::surface::town_span();
        let near_ground = (ground_row - self.terrain.surface_row(first)).abs() < 30;
        if near_ground && first <= town_last + 4 && last >= town_first - 4 {
            return Err(HomeRefusal::Occupied);
        }
        for x in [first, i32::midpoint(first, last), last] {
            let at = TilePosition { x, y: ground_row };
            if crate::worldgen::streamed::in_village_area(settings, at, 4) {
                return Err(HomeRefusal::Occupied);
            }
        }
        let claimed = self.bases.claims().any(|claim| {
            let (min, max) = claim.bounds();
            min.x <= last + 1
                && max.x >= first - 1
                && min.y <= ground_row + 3
                && max.y >= ground_row - 14
        });
        if claimed {
            return Err(HomeRefusal::Occupied);
        }
        let plotted = self
            .homes
            .plots_between(first - 1, last + 1)
            .any(|plot| (plot.site.ground_row - ground_row).abs() < 24);
        if plotted {
            return Err(HomeRefusal::Occupied);
        }
        if near_ground
            && plot::listed_sites_between(settings, first - 1, last + 1)
                .iter()
                .any(|site| (site.ground_row - ground_row).abs() < 24)
        {
            return Err(HomeRefusal::Occupied);
        }
        Ok(())
    }

    /// Contractors who would take a job on `plot`: builders from the town and the villages
    /// within [`CONTRACTOR_REACH_COLUMNS`], nearest first (the builders' guild when there are
    /// none, as on worlds without people).
    #[must_use]
    pub fn contractors_for(&self, plot: PlotId, client: PlayerId) -> Vec<ContractorOffer> {
        let Some(plot) = self.homes.plot(plot) else {
            return Vec::new();
        };
        let settings = *self.terrain.settings();
        let column = plot.site.middle();
        let mut found: Vec<(i32, ContractorOffer)> = Vec::new();
        if settings.generator.has_villages() {
            let mut ids: Vec<(i32, village::VillageId, String)> = village::villages_between(
                &settings,
                column - CONTRACTOR_REACH_COLUMNS,
                column + CONTRACTOR_REACH_COLUMNS,
            )
            .iter()
            .map(|found| {
                (
                    (found.center_column() - column).abs(),
                    found.id,
                    found.name.clone(),
                )
            })
            .collect();
            ids.push((
                (town_middle() - column).abs(),
                TOWN_VILLAGE,
                TOWN_NAME.to_owned(),
            ));
            for (distance, id, name) in ids {
                if distance > CONTRACTOR_REACH_COLUMNS {
                    continue;
                }
                let Some(settlement) = people::routine::settlement(&settings, id) else {
                    continue;
                };
                for character in &settlement.population.characters {
                    let Some(profile) = contractor_profile(character) else {
                        continue;
                    };
                    found.push((
                        distance,
                        ContractorOffer {
                            character: Some(character.id),
                            profile,
                            name: character.name.full(),
                            from: name.clone(),
                            past_jobs: self.homes.jobs_with(character.id, client),
                        },
                    ));
                }
            }
        }
        found.sort_by_key(|(distance, offer)| (*distance, offer.character));
        let mut offers = pick_contractors(found.into_iter().map(|(_, offer)| offer).collect());
        if offers.is_empty() {
            offers.push(guild_offer());
        }
        offers
    }

    /// What signing `job` on `plot` with `contractor` would cost `player` (or why not).
    ///
    /// # Errors
    ///
    /// The [`HomeRefusal`] when the contract cannot be signed.
    pub fn hire_quote(
        &self,
        player: PlayerId,
        plot_id: PlotId,
        job: HomeJob,
        contractor: Option<CharacterId>,
        supply: bool,
    ) -> Result<(HireQuote, ContractorOffer), HomeRefusal> {
        let plot = self.homes.plot(plot_id).ok_or(HomeRefusal::NoSuchPlot)?;
        if !plot.deed.owns(player) {
            return Err(HomeRefusal::NotOwner);
        }
        if plot.construction.is_some() {
            return Err(HomeRefusal::Busy);
        }
        self.job_fits(player, plot, job)?;
        let offer = self
            .contractors_for(plot_id, player)
            .into_iter()
            .find(|offer| offer.character == contractor)
            .ok_or(HomeRefusal::NoContractor)?;
        let quote = offer.profile.quote(job.job(plot.site.size));
        let repeat_discount = if offer.past_jobs > 0 {
            quote.price * REPEAT_CLIENT_DISCOUNT_PERCENT / 100
        } else {
            0
        };
        let held = if supply {
            self.players.get(&player).map_or(0, |rig| {
                rig.block_hopper.blocks().map(|(_, count)| count).sum()
            })
        } else {
            0
        };
        let (supply_off, supplied_blocks) = supply_discount(held, job.blocks(), quote.price);
        let price = quote
            .price
            .saturating_sub(repeat_discount)
            .saturating_sub(supply_off);
        let millis = u64::from(quote.tenth_days) * self.clock.day_length.millis() / 10;
        Ok((
            HireQuote {
                quote,
                price,
                supply_discount: supply_off,
                supplied_blocks,
                repeat_discount,
                millis,
            },
            offer,
        ))
    }

    /// Whether `job` can go on `plot` (size, setting, what already stands).
    fn job_fits(&self, player: PlayerId, plot: &Plot, job: HomeJob) -> Result<(), HomeRefusal> {
        match job {
            HomeJob::Build(blueprint) => {
                if plot.home.is_some() {
                    return Err(HomeRefusal::AlreadyBuilt);
                }
                if blueprint.min_plot() > plot.site.size
                    || blueprint.footprint().0 > plot.site.width()
                {
                    return Err(HomeRefusal::PlotTooSmall);
                }
                if !blueprint.allowed_on(plot.site.setting) {
                    return Err(HomeRefusal::WrongSetting);
                }
            }
            HomeJob::Expand(expansion) => {
                let home = plot.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                let fits = match expansion {
                    Expansion::Room(_) => home.width + expansion.extra_width() <= plot.site.width(),
                    Expansion::Garage => {
                        !home.has_garage()
                            && home.width + expansion.extra_width() <= plot.site.width()
                    }
                    Expansion::Floor => home.floors < MAX_FLOORS,
                    Expansion::Basement => !home.basement,
                    Expansion::Lift => {
                        home.basement && !home.lift && self.lift_claim(plot, player).is_some()
                    }
                };
                if !fits {
                    return Err(HomeRefusal::CannotExpand);
                }
            }
        }
        Ok(())
    }

    /// The claim a private lift from `plot` runs to: one of `player`'s (or the deed owner's)
    /// claims below the plot within [`LIFT_REACH_COLUMNS`]; the nearest beacon.
    #[must_use]
    pub fn lift_claim(&self, plot: &Plot, player: PlayerId) -> Option<crate::base::Claim> {
        let middle = plot.site.middle();
        self.bases
            .claims()
            .filter(|claim| claim.owner == player || claim.owner == plot.deed.owner)
            .filter(|claim| {
                (claim.center.x - middle).abs() <= LIFT_REACH_COLUMNS
                    && claim.center.y > plot.site.ground_row + 4
            })
            .min_by_key(|claim| ((claim.center.x - middle).abs(), claim.id))
            .copied()
    }

    /// The plot whose private lift lands in the claim `player`'s actor stands in, if any.
    #[must_use]
    pub fn lift_home_from_here(&self, player: PlayerId) -> Option<PlotId> {
        let tile = self.actor_tile(player)?;
        let claim = self.bases.claim_at(tile)?;
        self.homes
            .plots()
            .filter(|plot| plot.home.as_ref().is_some_and(|home| home.lift))
            .filter(|plot| plot.deed.may_enter(player))
            .find(|plot| {
                self.lift_claim(plot, plot.deed.owner).map(|found| found.id) == Some(claim.id)
            })
            .map(|plot| plot.id)
    }

    /// Whether `player` stands in the cellar under the hatch of `plot`'s basement (pushing up
    /// there climbs back into the house).
    #[must_use]
    pub fn in_cellar_of(&self, player: PlayerId) -> Option<PlotId> {
        let tile = self.actor_tile(player)?;
        self.homes
            .plots()
            .filter(|plot| plot.home.as_ref().is_some_and(|home| home.basement))
            .find(|plot| {
                let (first, last) = cellar_columns(plot);
                tile.x >= first
                    && tile.x <= last
                    && tile.y > plot.site.ground_row
                    && tile.y <= plot.site.ground_row + CELLAR_ROWS
            })
            .map(|plot| plot.id)
    }

    /// The verdict on a home action: what it costs, or why it cannot happen (the tick applies
    /// the same rules; the game previews with it).
    ///
    /// # Errors
    ///
    /// The [`HomeRefusal`] when the action cannot happen.
    #[allow(clippy::too_many_lines, reason = "one flat match over the commands")]
    pub fn check_home(
        &self,
        player_id: PlayerId,
        command: &HomeCommand,
    ) -> Result<HomeQuote, HomeRefusal> {
        let player = self.players.get(&player_id).ok_or(HomeRefusal::NotAtPlot)?;
        let at_plot = |plot: PlotId| -> Result<&Plot, HomeRefusal> {
            let found = self.homes.plot(plot).ok_or(HomeRefusal::NoSuchPlot)?;
            let tile = self.actor_tile(player_id).ok_or(HomeRefusal::NotAtPlot)?;
            if found.at_site(tile) {
                Ok(found)
            } else {
                Err(HomeRefusal::NotAtPlot)
            }
        };
        let home_room =
            |plot: PlotId, room: u8| -> Result<(&Plot, &crate::home::Room), HomeRefusal> {
                let found = at_plot(plot)?;
                let home = found.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                if !found.deed.may_enter(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let room = home
                    .rooms
                    .get(usize::from(room))
                    .ok_or(HomeRefusal::NoSpace)?;
                Ok((found, room))
            };
        match *command {
            HomeCommand::BuyPlot { site, size } => {
                if self.homes.deeds_held(player_id) >= MAX_PLOTS {
                    return Err(HomeRefusal::TooManyPlots);
                }
                let found = if site == SiteKey::Surveyed {
                    self.survey_plot(player_id, size)?
                } else {
                    if !self.at_land_office(player_id) {
                        return Err(HomeRefusal::NotAtLandOffice);
                    }
                    let found = plot::listed_site(self.terrain.settings(), site)
                        .ok_or(HomeRefusal::NoSuchPlot)?;
                    if self.homes.plot_of_site(site).is_some() {
                        return Err(HomeRefusal::SiteTaken);
                    }
                    // A claim or a surveyed plot staked over it since.
                    self.site_clear_of_claims(&found)?;
                    found
                };
                if player.credits < found.price {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote {
                    credits: found.price,
                    blocks: 0,
                })
            }
            HomeCommand::Hire {
                plot,
                job,
                contractor,
                supply,
            } => {
                at_plot(plot)?;
                let (quote, _) = self.hire_quote(player_id, plot, job, contractor, supply)?;
                if player.credits < quote.price {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote {
                    credits: quote.price,
                    blocks: quote.supplied_blocks,
                })
            }
            HomeCommand::Rush { plot } => {
                let found = at_plot(plot)?;
                if !found.deed.owns(player_id) {
                    return Err(HomeRefusal::NotOwner);
                }
                let construction = found
                    .construction
                    .as_ref()
                    .ok_or(HomeRefusal::NothingToDo)?;
                let (price, _) = construction
                    .rush_quote(self.world_millis())
                    .ok_or(HomeRefusal::NothingToDo)?;
                if player.credits < price {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote {
                    credits: price,
                    blocks: 0,
                })
            }
            HomeCommand::AnswerEvent { plot, pay } => {
                let found = at_plot(plot)?;
                if !found.deed.owns(player_id) {
                    return Err(HomeRefusal::NotOwner);
                }
                let construction = found
                    .construction
                    .as_ref()
                    .ok_or(HomeRefusal::NothingToDo)?;
                let (_, event) = construction.open_event().ok_or(HomeRefusal::NothingToDo)?;
                match event.kind {
                    SiteEventKind::Shortage { block, count } => {
                        if pay {
                            let price = count * SHORTAGE_PRICE_PER_BLOCK;
                            if player.credits < price {
                                return Err(HomeRefusal::CannotAfford);
                            }
                            Ok(HomeQuote {
                                credits: price,
                                blocks: 0,
                            })
                        } else if player.block_hopper.count(ItemKind::Block(block)) >= count {
                            Ok(HomeQuote {
                                credits: 0,
                                blocks: count,
                            })
                        } else {
                            Err(HomeRefusal::MissingItems)
                        }
                    }
                    SiteEventKind::Request { item } => {
                        if !pay && player.cargo_bay.count(item) == 0 {
                            return Err(HomeRefusal::MissingItems);
                        }
                        Ok(HomeQuote::default())
                    }
                    SiteEventKind::Anecdote { .. } => Ok(HomeQuote::default()),
                }
            }
            HomeCommand::SetAccess { plot, access } => {
                let found = self.homes.plot(plot).ok_or(HomeRefusal::NoSuchPlot)?;
                if found.deed.owner != player_id {
                    return Err(HomeRefusal::NotOwner);
                }
                if found.deed.access == access {
                    return Err(HomeRefusal::NothingToDo);
                }
                Ok(HomeQuote::default())
            }
            HomeCommand::CoOwner { plot, player, add } => {
                let found = self.homes.plot(plot).ok_or(HomeRefusal::NoSuchPlot)?;
                if found.deed.owner != player_id {
                    return Err(HomeRefusal::NotOwner);
                }
                let listed = found.deed.co_owners.contains(&player);
                let can = if add {
                    player != player_id
                        && !listed
                        && found.deed.co_owners.len() < MAX_CO_OWNERS
                        && self.players.contains_key(&player)
                } else {
                    listed
                };
                if can {
                    Ok(HomeQuote::default())
                } else {
                    Err(HomeRefusal::NothingToDo)
                }
            }
            HomeCommand::BuyFurniture { furniture } => {
                if !self.at_land_office(player_id) {
                    return Err(HomeRefusal::NotAtLandOffice);
                }
                if !Furniture::CATALOGUE.contains(&furniture) {
                    return Err(HomeRefusal::NoFurniture);
                }
                let price = furniture.catalogue_price();
                if player.credits < price {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote {
                    credits: price,
                    blocks: 0,
                })
            }
            HomeCommand::CraftFurniture { plot, furniture } => {
                let found = at_plot(plot)?;
                let home = found.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let bench = home.rooms.iter().any(|room| {
                    room.decor
                        .pieces
                        .values()
                        .any(|piece| piece.furniture == Furniture::Workbench)
                });
                if !bench {
                    return Err(HomeRefusal::NothingToDo);
                }
                let (blocks, credits) = furniture.craft_cost().ok_or(HomeRefusal::NoFurniture)?;
                if blocks_to_take(&player.block_hopper, blocks).is_none() {
                    return Err(HomeRefusal::MissingItems);
                }
                if player.credits < credits {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote { credits, blocks })
            }
            HomeCommand::Place {
                plot,
                room,
                cell,
                furniture,
            } => {
                let (found, room) = home_room(plot, room)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                if self.homes.furniture_count(player_id, furniture) == 0 {
                    return Err(HomeRefusal::NoFurniture);
                }
                if !room.decor.fits(furniture, cell) {
                    return Err(HomeRefusal::NoSpace);
                }
                Ok(HomeQuote::default())
            }
            HomeCommand::PickUp { plot, room, cell } => {
                let (found, room) = home_room(plot, room)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let piece = room
                    .decor
                    .piece_at(cell)
                    .or_else(|| room.decor.rug_at(cell))
                    .ok_or(HomeRefusal::NothingToDo)?;
                if let Some(item) = piece.1.display
                    && player.room_for(item, 1) == 0
                {
                    return Err(HomeRefusal::Full);
                }
                Ok(HomeQuote::default())
            }
            HomeCommand::Restyle { plot, room, .. } => {
                let (found, _) = home_room(plot, room)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let price = crate::home::decor::RESTYLE_PRICE;
                if player.credits < price {
                    return Err(HomeRefusal::CannotAfford);
                }
                Ok(HomeQuote {
                    credits: price,
                    blocks: 0,
                })
            }
            HomeCommand::Display {
                plot,
                room,
                cell,
                item,
            } => {
                let (found, room) = home_room(plot, room)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let (_, piece) = room.decor.piece_at(cell).ok_or(HomeRefusal::NothingToDo)?;
                if piece.furniture != Furniture::TrophyCase || !displayable(item) {
                    return Err(HomeRefusal::NothingToDo);
                }
                if player.cargo_bay.count(item) == 0 {
                    return Err(HomeRefusal::MissingItems);
                }
                if piece.display == Some(item) {
                    return Err(HomeRefusal::NothingToDo);
                }
                Ok(HomeQuote::default())
            }
            HomeCommand::UsePiece { plot, room, cell } => {
                let (found, room) = home_room(plot, room)?;
                let (_, piece) = room.decor.piece_at(cell).ok_or(HomeRefusal::NothingToDo)?;
                match piece.furniture.service() {
                    Some(DecorService::Sleep) => {
                        let tired = self.clock().phase().sleepable();
                        if !tired && !found.deed.owns(player_id) {
                            return Err(HomeRefusal::NotTired);
                        }
                        if tired && self.is_in_bed(player_id) {
                            return Err(HomeRefusal::NothingToDo);
                        }
                        Ok(HomeQuote::default())
                    }
                    Some(DecorService::Cook | DecorService::Stargaze) => Ok(HomeQuote::default()),
                    Some(DecorService::Store) => self.check_home(
                        player_id,
                        &HomeCommand::Stash {
                            plot: found.id,
                            deposit: true,
                        },
                    ),
                    Some(DecorService::Display | DecorService::Craft) | None => {
                        Err(HomeRefusal::NothingToDo)
                    }
                }
            }
            HomeCommand::Stash { plot, deposit } => {
                let found = at_plot(plot)?;
                let home = found.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                if !found.deed.may_decorate(player_id) {
                    return Err(HomeRefusal::NotPermitted);
                }
                let capacity = home.storage_capacity();
                let any = if deposit {
                    player.cargo_bay.iter().any(|(kind, _)| {
                        is_cargo(kind) && home.storage.accepts(kind, 1, capacity) > 0
                    })
                } else {
                    home.storage
                        .iter()
                        .any(|(kind, _)| player.room_for(kind, 1) > 0)
                };
                if any {
                    Ok(HomeQuote::default())
                } else {
                    Err(HomeRefusal::NothingToDo)
                }
            }
            HomeCommand::RideLift { plot, down } => {
                if !self.is_on_foot(player_id) {
                    return Err(HomeRefusal::NeedsPilot);
                }
                let found = self.homes.plot(plot).ok_or(HomeRefusal::NoSuchPlot)?;
                let home = found.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                if !home.lift || !found.deed.may_enter(player_id) {
                    return Err(HomeRefusal::NothingToDo);
                }
                if down {
                    at_plot(plot)?;
                    self.lift_claim(found, found.deed.owner)
                        .ok_or(HomeRefusal::NothingToDo)?;
                } else if self.lift_home_from_here(player_id) != Some(plot) {
                    return Err(HomeRefusal::NotAtPlot);
                }
                Ok(HomeQuote::default())
            }
            HomeCommand::Hatch { plot } => {
                if !self.is_on_foot(player_id) {
                    return Err(HomeRefusal::NeedsPilot);
                }
                let found = at_plot(plot)?;
                let home = found.home.as_ref().ok_or(HomeRefusal::NoHome)?;
                if !home.basement || !found.deed.may_enter(player_id) {
                    return Err(HomeRefusal::NothingToDo);
                }
                Ok(HomeQuote::default())
            }
        }
    }

    /// Whether a listed site is still clear of claims and surveyed plots.
    fn site_clear_of_claims(&self, site: &PlotSite) -> Result<(), HomeRefusal> {
        let (first, last) = (site.left, site.right());
        let claimed = self.bases.claims().any(|claim| {
            let (min, max) = claim.bounds();
            min.x <= last
                && max.x >= first
                && min.y <= site.ground_row + 3
                && max.y >= site.ground_row - 14
        });
        let plotted = self
            .homes
            .plots_between(first, last)
            .any(|plot| (plot.site.ground_row - site.ground_row).abs() < 24);
        if claimed || plotted {
            Err(HomeRefusal::SiteTaken)
        } else {
            Ok(())
        }
    }

    /// Applies a home command, or `None` if `command` is not one.
    pub(super) fn home_command(
        &mut self,
        player_id: PlayerId,
        command: &crate::command::PlayerCommand,
    ) -> Option<(PlayerScopedCommandOutcome, Vec<GameplayEventKind>)> {
        let crate::command::PlayerCommand::Home(command) = command else {
            return None;
        };
        let mut events = Vec::new();
        let outcome = self.check_home(player_id, command).map_or(
            PlayerScopedCommandOutcome::IgnoredUnavailable,
            |quote| {
                self.apply_home(player_id, *command, quote, &mut events);
                PlayerScopedCommandOutcome::Applied
            },
        );
        Some((outcome, events))
    }

    #[allow(clippy::too_many_lines, reason = "one flat match over the commands")]
    fn apply_home(
        &mut self,
        player_id: PlayerId,
        command: HomeCommand,
        quote: HomeQuote,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let now = self.world_millis();
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_sub(quote.credits);
        }
        match command {
            HomeCommand::BuyPlot { site, size } => {
                let found = if site == SiteKey::Surveyed {
                    self.survey_plot(player_id, size).ok()
                } else {
                    plot::listed_site(self.terrain.settings(), site)
                };
                let Some(found) = found else {
                    return;
                };
                let id = self.homes.insert_plot(
                    found,
                    Deed {
                        owner: player_id,
                        co_owners: BTreeSet::new(),
                        access: HomeAccess::default(),
                        bought_millis: now,
                        paid: quote.credits,
                    },
                );
                events.push(GameplayEventKind::PlotBought {
                    player_id,
                    plot: id,
                    size: found.size,
                    setting: found.setting,
                });
            }
            HomeCommand::Hire {
                plot,
                job,
                contractor,
                supply,
            } => {
                let Ok((hire, offer)) = self.hire_quote(player_id, plot, job, contractor, supply)
                else {
                    return;
                };
                if hire.supplied_blocks > 0
                    && let Some(player) = self.players.get_mut(&player_id)
                    && let Some(taken) = blocks_to_take(&player.block_hopper, hire.supplied_blocks)
                {
                    for (kind, count) in taken {
                        player.block_hopper.remove(ItemKind::Block(kind), count);
                    }
                }
                let seed = self.terrain.seed();
                let Some(found) = self.homes.plot_mut(plot) else {
                    return;
                };
                let serial = found.contracts;
                found.contracts = found.contracts.saturating_add(1);
                found.construction = Some(Construction {
                    job,
                    contractor: offer.profile,
                    guild: offer.character.is_none(),
                    quote: hire.quote,
                    price_paid: hire.price,
                    supplied_blocks: hire.supplied_blocks,
                    started_millis: now,
                    total_millis: hire.millis.max(1),
                    rushed_millis: 0,
                    held_millis: 0,
                    hold_since: None,
                    planned: plan_events(seed, plot.0, serial, job, hire.quote.tenth_days),
                    events: Vec::new(),
                    announced: BuildStage::Survey,
                    bonus_quality: 0,
                });
                if let Some(character) = offer.character {
                    let day = self.people_clock().day;
                    self.people
                        .characters
                        .entry(character)
                        .or_default()
                        .remember(player_id, day, MemoryFact::Traded);
                }
                events.push(GameplayEventKind::ContractSigned {
                    player_id,
                    plot,
                    job,
                });
            }
            HomeCommand::Rush { plot } => {
                if let Some(construction) = self
                    .homes
                    .plot_mut(plot)
                    .and_then(|found| found.construction.as_mut())
                    && let Some((_, skip)) = construction.rush_quote(now)
                {
                    construction.rushed_millis = construction.rushed_millis.saturating_add(skip);
                    events.push(GameplayEventKind::HomeRushed { player_id, plot });
                }
            }
            HomeCommand::AnswerEvent { plot, pay } => {
                self.answer_site_event(player_id, plot, pay, now, events);
            }
            HomeCommand::SetAccess { plot, access } => {
                if let Some(found) = self.homes.plot_mut(plot) {
                    found.deed.access = access;
                }
            }
            HomeCommand::CoOwner { plot, player, add } => {
                if let Some(found) = self.homes.plot_mut(plot) {
                    if add {
                        found.deed.co_owners.insert(player);
                    } else {
                        found.deed.co_owners.remove(&player);
                    }
                }
            }
            HomeCommand::BuyFurniture { furniture } => {
                self.homes.add_furniture(player_id, furniture, 1);
            }
            HomeCommand::CraftFurniture { furniture, .. } => {
                if let Some(player) = self.players.get_mut(&player_id)
                    && let Some(taken) = blocks_to_take(&player.block_hopper, quote.blocks)
                {
                    for (kind, count) in taken {
                        player.block_hopper.remove(ItemKind::Block(kind), count);
                    }
                }
                self.homes.add_furniture(player_id, furniture, 1);
            }
            HomeCommand::Place {
                plot,
                room,
                cell,
                furniture,
            } => {
                if !self.homes.take_furniture(player_id, furniture) {
                    return;
                }
                if let Some(room) = self.room_mut(plot, room) {
                    let piece = crate::home::decor::PlacedDecor {
                        furniture,
                        display: None,
                    };
                    if furniture.underlay() {
                        room.decor.rugs.insert(cell, piece);
                    } else {
                        room.decor.pieces.insert(cell, piece);
                    }
                }
                events.push(GameplayEventKind::FurniturePlaced {
                    player_id,
                    plot,
                    furniture,
                });
            }
            HomeCommand::PickUp { plot, room, cell } => {
                let Some(room) = self.room_mut(plot, room) else {
                    return;
                };
                let taken = if let Some((start, _)) = room.decor.piece_at(cell) {
                    room.decor.pieces.remove(&start)
                } else if let Some((start, _)) = room.decor.rug_at(cell) {
                    room.decor.rugs.remove(&start)
                } else {
                    None
                };
                let Some(piece) = taken else {
                    return;
                };
                if let Some(item) = piece.display
                    && let Some(player) = self.players.get_mut(&player_id)
                {
                    let _ = player.stow(item, 1);
                }
                // Owners take the piece back into their stock; a crew member decorating an
                // open house moves it, but it stays the house's: back to the deed owner.
                let keeper = self
                    .homes
                    .plot(plot)
                    .filter(|found| !found.deed.owns(player_id))
                    .map_or(player_id, |found| found.deed.owner);
                self.homes.add_furniture(keeper, piece.furniture, 1);
                events.push(GameplayEventKind::FurnitureRemoved {
                    player_id,
                    plot,
                    furniture: piece.furniture,
                });
            }
            HomeCommand::Restyle {
                plot,
                room,
                wallpaper,
            } => {
                if let Some(room) = self.room_mut(plot, room) {
                    if wallpaper {
                        room.decor.wallpaper = room.decor.wallpaper.next();
                    } else {
                        room.decor.flooring = room.decor.flooring.next();
                    }
                }
            }
            HomeCommand::Display {
                plot,
                room,
                cell,
                item,
            } => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.cargo_bay.remove(item, 1);
                }
                let returned = self.room_mut(plot, room).and_then(|room| {
                    let (start, _) = room.decor.piece_at(cell)?;
                    room.decor.pieces.get_mut(&start)?.display.replace(item)
                });
                if let Some(old) = returned
                    && let Some(player) = self.players.get_mut(&player_id)
                {
                    let _ = player.stow(old, 1);
                }
                events.push(GameplayEventKind::FurniturePlaced {
                    player_id,
                    plot,
                    furniture: Furniture::TrophyCase,
                });
            }
            HomeCommand::UsePiece { plot, room, cell } => {
                self.use_piece(player_id, plot, room, cell);
            }
            HomeCommand::Stash { plot, deposit } => self.stash(player_id, plot, deposit),
            HomeCommand::RideLift { plot, down } => self.ride_lift(player_id, plot, down),
            HomeCommand::Hatch { plot } => {
                if let Some(found) = self.homes.plot(plot) {
                    let (first, _) = cellar_columns(found);
                    let x = (first as f32 + 1.5) * TILE_SIZE;
                    let y = (found.site.ground_row + CELLAR_ROWS + 1) as f32 * TILE_SIZE;
                    self.move_pilot(player_id, x, y);
                }
            }
        }
    }

    fn room_mut(&mut self, plot: PlotId, room: u8) -> Option<&mut crate::home::Room> {
        self.homes
            .plot_mut(plot)?
            .home
            .as_mut()?
            .rooms
            .get_mut(usize::from(room))
    }

    fn answer_site_event(
        &mut self,
        player_id: PlayerId,
        plot: PlotId,
        pay: bool,
        now: u64,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let Some((index, event, character, guild)) = self.homes.plot(plot).and_then(|found| {
            let construction = found.construction.as_ref()?;
            let (index, event) = construction.open_event()?;
            Some((index, event, construction.character(), construction.guild))
        }) else {
            return;
        };
        let mut goodwill = 0;
        match event.kind {
            SiteEventKind::Shortage { block, count } => {
                if !pay && let Some(player) = self.players.get_mut(&player_id) {
                    player.block_hopper.remove(ItemKind::Block(block), count);
                    goodwill = 4;
                }
            }
            SiteEventKind::Request { item } => {
                if !pay {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.cargo_bay.remove(item, 1);
                    }
                    goodwill = 8;
                }
            }
            SiteEventKind::Anecdote { .. } => goodwill = 1,
        }
        if let Some(construction) = self
            .homes
            .plot_mut(plot)
            .and_then(|found| found.construction.as_mut())
        {
            if let Some(since) = construction.hold_since.take() {
                construction.held_millis = construction
                    .held_millis
                    .saturating_add(now.saturating_sub(since));
            }
            if matches!(event.kind, SiteEventKind::Request { .. }) && !pay {
                construction.bonus_quality = construction.bonus_quality.saturating_add(1);
            }
            if let Some(record) = construction.events.get_mut(index) {
                record.answered = true;
            }
        }
        if goodwill > 0 && !guild {
            self.people
                .characters
                .entry(character)
                .or_default()
                .hear_about(player_id, goodwill);
        }
        events.push(GameplayEventKind::SiteEventAnswered { player_id, plot });
    }

    fn use_piece(&mut self, player_id: PlayerId, plot: PlotId, room: u8, cell: u8) {
        let Some((service, owns)) = self.homes.plot(plot).and_then(|found| {
            let room = found.home.as_ref()?.rooms.get(usize::from(room))?;
            let (_, piece) = room.decor.piece_at(cell)?;
            Some((piece.furniture.service()?, found.deed.owns(player_id)))
        }) else {
            return;
        };
        match service {
            DecorService::Sleep => {
                if owns {
                    self.homes.set_home_point(player_id, plot);
                }
                if self.clock().phase().sleepable() {
                    let _ = self.lie_down(player_id, crate::clock::BedSpot::Home(plot));
                }
            }
            DecorService::Cook => {
                // A hot meal: the pilot is patched up and catches its breath.
                if let Some((id, _, _)) = self.pilot_of(player_id) {
                    self.entities.update(id, |entity| {
                        if let EntityState::Pilot(state) = &mut entity.state {
                            state.health = state.max_health;
                            state.oxygen = state.max_oxygen;
                        }
                    });
                }
            }
            DecorService::Store => self.stash(player_id, plot, true),
            DecorService::Display | DecorService::Craft | DecorService::Stargaze => {}
        }
    }

    fn stash(&mut self, player_id: PlayerId, plot: PlotId, deposit: bool) {
        let Some(home) = self
            .homes
            .plot_mut(plot)
            .and_then(|found| found.home.as_mut())
        else {
            return;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let capacity = home.storage_capacity();
        if deposit {
            player.cargo_bay.update_counts(|kind, count| {
                if !is_cargo(kind) {
                    return count;
                }
                let stored = home.storage.accepts(kind, count, capacity);
                home.storage.add(kind, stored);
                count - stored
            });
        } else {
            home.storage
                .update_counts(|kind, count| count - player.stow(kind, count));
        }
    }

    fn ride_lift(&mut self, player_id: PlayerId, plot: PlotId, down: bool) {
        let Some(found) = self.homes.plot(plot).cloned() else {
            return;
        };
        if down {
            let Some(claim) = self.lift_claim(&found, found.deed.owner) else {
                return;
            };
            let x = (claim.center.x as f32 + 0.5) * TILE_SIZE;
            let y = (claim.center.y + 1) as f32 * TILE_SIZE;
            self.move_pilot(player_id, x, y);
        } else {
            let x = (found.door_column() as f32 + 0.5) * TILE_SIZE;
            let y = found.site.ground_row as f32 * TILE_SIZE;
            self.move_pilot(player_id, x, y);
        }
    }

    /// Moves `player_id`'s pilot to feet position `(x, y)`, at rest.
    fn move_pilot(&mut self, player_id: PlayerId, x: f32, y: f32) {
        let Some((id, _, _)) = self.pilot_of(player_id) else {
            return;
        };
        let (x, y) = (
            crate::quantize::quantize_simulation_scalar(x),
            crate::quantize::quantize_simulation_scalar(y),
        );
        self.entities.update(id, |entity| {
            entity.body.x = x;
            entity.body.y = y;
            entity.body.previous_x = x;
            entity.body.previous_y = y;
            entity.body.velocity_x = 0.0;
            entity.body.velocity_y = 0.0;
        });
    }

    /// Gives `player` `count` pieces of `furniture` (rewards, quest payouts, drops, scenes).
    pub fn grant_furniture(&mut self, player: PlayerId, furniture: Furniture, count: u32) {
        self.homes.add_furniture(player, furniture, count);
    }

    /// The homes' part of a tick (to `next_tick`): construction stepped in world time, site
    /// crews, and rigs parked in garage bays.
    pub(super) fn advance_homes(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if self.homes.is_empty() {
            return;
        }
        let now = self.clock_at(next_tick).world_millis();
        for plot in self.homes.plot_ids() {
            self.advance_construction(plot, now, output);
        }
        let period = self.timing.ticks_for_millis(CREW_REFRESH_MILLIS).max(1);
        if next_tick.get().is_multiple_of(period) {
            self.refresh_site_crews(next_tick);
        }
        self.walk_site_crews(next_tick);
        self.repair_garaged_rigs();
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one contract's step reads best in one place"
    )]
    fn advance_construction(&mut self, plot_id: PlotId, now: u64, output: &mut TickOutput) {
        let Some(plot) = self.homes.plot(plot_id) else {
            return;
        };
        let Some(construction) = plot.construction.clone() else {
            return;
        };
        let owner = plot.deed.owner;
        let mut next = construction.clone();
        // A shortage nobody answered: the contractor sources the stock after a while.
        if let Some(since) = next.hold_since
            && now.saturating_sub(since) >= next.shortage_patience()
        {
            next.held_millis = next.held_millis.saturating_add(now.saturating_sub(since));
            next.hold_since = None;
            if let Some((index, _)) = next.open_event()
                && let Some(record) = next.events.get_mut(index)
            {
                record.answered = true;
            }
        }
        let stage = next.stage(now);
        // Story beats planned for stages the work has reached.
        let fired: Vec<crate::home::construction::PlannedEvent> = next
            .planned
            .iter()
            .filter(|planned| planned.stage <= stage && stage != BuildStage::Done)
            .copied()
            .collect();
        if !fired.is_empty() {
            next.planned.retain(|planned| planned.stage > stage);
            for planned in fired {
                // Let open events go when a new one comes (only the latest waits).
                for old in &mut next.events {
                    if !old.kind.holds_work() {
                        old.answered = true;
                    }
                }
                next.events.push(SiteEvent {
                    kind: planned.kind,
                    stage: planned.stage,
                    at_millis: now,
                    answered: false,
                });
                if planned.kind.holds_work() && next.hold_since.is_none() {
                    next.hold_since = Some(now);
                }
                output.push_event(GameplayEventKind::SiteEventRaised {
                    plot: plot_id,
                    owner,
                    event: planned.kind,
                });
            }
        }
        let stage = next.stage(now);
        let mut graded = false;
        if stage > next.announced {
            // Each stage reached is announced once, in order.
            for reached in BuildStage::ALL {
                if reached > next.announced && reached <= stage {
                    if reached == BuildStage::Foundation {
                        graded = true;
                    }
                    output.push_event(GameplayEventKind::ConstructionStage {
                        plot: plot_id,
                        owner,
                        stage: reached,
                    });
                }
            }
            next.announced = stage;
        }
        if graded && matches!(next.job, HomeJob::Build(_)) {
            self.lay_foundation(plot_id);
        }
        if stage == BuildStage::Done {
            self.hand_over(plot_id, &next, now, output);
            return;
        }
        if next != construction
            && let Some(found) = self.homes.plot_mut(plot_id)
        {
            found.construction = Some(next);
        }
    }

    /// Levels the house's ground: clears rock out of the house's space and lays a masonry
    /// foundation on the ground row (filling dips under it).
    fn lay_foundation(&mut self, plot_id: PlotId) {
        let Some(plot) = self.homes.plot(plot_id) else {
            return;
        };
        let Some((width, height)) = plot.planned_footprint() else {
            return;
        };
        let left = plot.house_left(width);
        let ground = plot.site.ground_row;
        for x in left - 1..=left + width {
            for y in ground - height - 1..ground {
                let at = TilePosition { x, y };
                if self.terrain.is_solid_at(at)
                    && !matches!(
                        self.terrain.tile(at).map(|tile| tile.kind),
                        Some(TileKind::Built(_))
                    )
                {
                    let _ = self.terrain.set_kind(at, TileKind::Air);
                }
            }
            if x < left || x >= left + width {
                continue;
            }
            let _ = self.terrain.set_kind(
                TilePosition { x, y: ground },
                TileKind::Built(BuiltTile::Wall(crate::item::BlockKind::Stone)),
            );
            for y in ground + 1..=ground + 3 {
                let at = TilePosition { x, y };
                if !self.terrain.is_solid_at(at) {
                    let _ = self.terrain.set_kind(at, TileKind::Dirt);
                }
            }
        }
    }

    /// Carves the cellar under a house with a basement: a room-sized pocket under the
    /// foundation with a vent-grate hatch at the door (pilots climb through, rigs cannot).
    fn dig_cellar(&mut self, plot_id: PlotId) {
        let Some(plot) = self.homes.plot(plot_id) else {
            return;
        };
        let (first, last) = cellar_columns(plot);
        let ground = plot.site.ground_row;
        let hatch = first + 1;
        for x in first - 1..=last + 1 {
            for y in ground + 1..=ground + CELLAR_ROWS + 1 {
                let at = TilePosition { x, y };
                let wall = x < first || x > last || y > ground + CELLAR_ROWS;
                let kind = if wall {
                    TileKind::Built(BuiltTile::Wall(crate::item::BlockKind::Stone))
                } else {
                    TileKind::Air
                };
                let _ = self.terrain.set_kind(at, kind);
            }
        }
        let _ = self.terrain.set_kind(
            TilePosition {
                x: hatch,
                y: ground,
            },
            TileKind::Built(BuiltTile::VentGrate),
        );
        // A lamp keeps the cellar from pitch dark.
        self.bases.update_fixtures(
            TilePosition {
                x: last,
                y: ground + 1,
            },
            |fixtures| fixtures.front = Some((Fixture::Light, crate::base::Facing::Left)),
        );
    }

    /// The work is done: the keys are handed over (a new home, or the expansion applied).
    fn hand_over(
        &mut self,
        plot_id: PlotId,
        construction: &Construction,
        now: u64,
        output: &mut TickOutput,
    ) {
        let Some(plot) = self.homes.plot(plot_id) else {
            return;
        };
        let owner = plot.deed.owner;
        let builder = (!construction.guild).then(|| construction.character());
        let quality = construction.quality();
        let job = construction.job;
        let mut dig = false;
        if let Some(found) = self.homes.plot_mut(plot_id) {
            found.construction = None;
            match job {
                HomeJob::Build(blueprint) => {
                    found.home = Some(Home::built(blueprint, quality, builder, now));
                }
                HomeJob::Expand(expansion) => {
                    if let Some(home) = found.home.as_mut() {
                        home.expand(expansion);
                        dig = expansion == Expansion::Basement;
                    }
                }
            }
        }
        if dig {
            self.dig_cellar(plot_id);
        }
        // A housewarming gift from a contractor proud of the work.
        let gift = match (job, quality) {
            (HomeJob::Build(_), 5) => Some(Furniture::Piano),
            (HomeJob::Build(_), 4) => Some(Furniture::Clock),
            (HomeJob::Build(_), _) => Some(Furniture::Plant),
            (HomeJob::Expand(_), _) => None,
        };
        if let Some(gift) = gift {
            self.homes.add_furniture(owner, gift, 1);
        }
        if let Some(character) = builder {
            self.homes.record_job(character, owner);
            self.people
                .characters
                .entry(character)
                .or_default()
                .hear_about(owner, crate::home::HANDOVER_GOODWILL);
        }
        output.push_event(GameplayEventKind::HomeCompleted {
            plot: plot_id,
            owner,
            job,
        });
        self.send_crew_home(Some(plot_id));
    }

    // ----- site crews -----------------------------------------------------------------------

    /// Who works on `plot` (the contractor first, then helpers from their settlement), during
    /// working hours.
    fn site_crew(&self, plot: &Plot) -> Vec<CharacterId> {
        let Some(construction) = plot.construction.as_ref() else {
            return Vec::new();
        };
        if construction.guild || !self.terrain.settings().generator.has_villages() {
            return Vec::new();
        }
        let time = self.people_clock();
        if time.minute < crate::home::WORK_START_MINUTE
            || time.minute >= crate::home::WORK_END_MINUTE
        {
            return Vec::new();
        }
        let boss = construction.character();
        let mut crew = vec![boss];
        // A held job sends the helpers home; the boss frets on site.
        if construction.hold_since.is_some() {
            return crew;
        }
        let helpers = match construction.job {
            HomeJob::Build(blueprint) => match blueprint.job().tenth_days {
                0..=15 => 1,
                16..=40 => 2,
                _ => 3,
            },
            HomeJob::Expand(_) => 1,
        };
        let Some(settlement) = people::routine::settlement(self.terrain.settings(), boss.village)
        else {
            return crew;
        };
        let count = settlement.population.characters.len();
        for step in 1..count {
            if crew.len() > helpers {
                break;
            }
            let index = (usize::from(boss.index) + step) % count;
            let character = &settlement.population.characters[index];
            if character.age >= 16 {
                crew.push(character.id);
            }
        }
        crew
    }

    /// Characters working on a site right now (their NPC entities carry `NpcState::site`).
    pub(super) fn crew_characters(&self) -> BTreeSet<CharacterId> {
        self.entities
            .of_kind(EntityKind::Npc)
            .filter_map(|entity| match entity.state {
                EntityState::Npc(state) if state.site.is_some() => Some(state.character),
                _ => None,
            })
            .collect()
    }

    fn refresh_site_crews(&mut self, next_tick: SimulationTick) {
        let actors: Vec<TilePosition> = self
            .players
            .keys()
            .filter_map(|player| self.actor_tile(*player))
            .collect();
        let mut wanted: Vec<(PlotId, usize, CharacterId)> = Vec::new();
        for plot in self.homes.plots() {
            let near = actors.iter().any(|tile| {
                tile.x >= plot.site.left - CREW_NEAR_COLUMNS
                    && tile.x <= plot.site.right() + CREW_NEAR_COLUMNS
                    && (tile.y - plot.site.ground_row).abs() <= CREW_NEAR_ROWS
            });
            if !near {
                continue;
            }
            for (slot, character) in self.site_crew(plot).into_iter().enumerate() {
                wanted.push((plot.id, slot, character));
            }
        }
        // Send home anyone no longer wanted.
        let mut leaving = BTreeSet::new();
        let mut present = BTreeSet::new();
        for entity in self.entities.of_kind(EntityKind::Npc) {
            let EntityState::Npc(state) = entity.state else {
                continue;
            };
            let Some(site) = state.site else {
                continue;
            };
            let keep = wanted
                .iter()
                .any(|(plot, _, character)| plot.0 == site && *character == state.character);
            if keep && present.insert(state.character) {
                continue;
            }
            leaving.insert(entity.id);
        }
        // Their village copies step aside while they work.
        for (_, _, character) in &wanted {
            for entity in self.entities.of_kind(EntityKind::Npc) {
                if let EntityState::Npc(state) = entity.state
                    && state.site.is_none()
                    && state.character == *character
                {
                    leaving.insert(entity.id);
                }
            }
        }
        if !leaving.is_empty() {
            self.entities.despawn_many(&leaving);
        }
        for (plot_id, slot, character) in wanted {
            if present.contains(&character) {
                continue;
            }
            let Some(plot) = self.homes.plot(plot_id) else {
                continue;
            };
            let x = crew_station_px(plot, slot, 0);
            let state = NpcState {
                character,
                activity: Activity::Working,
                inside: None,
                going_inside: None,
                from_x: x,
                target_x: x,
                depart: next_tick,
                arrive: next_tick,
                settled: true,
                facing: if slot % 2 == 0 { 1 } else { -1 },
                talking_to: None,
                idle_until: next_tick,
                site: Some(plot_id.0),
            };
            #[allow(clippy::cast_precision_loss, reason = "rows are small")]
            let y = plot.site.ground_row as f32 * TILE_SIZE;
            let _ = self.spawn_entity(Body::at(x, y), EntityState::Npc(state), None);
            present.insert(character);
        }
    }

    /// Despawns the crews of `plot` (every site when `None`).
    fn send_crew_home(&mut self, plot: Option<PlotId>) {
        let leaving: BTreeSet<EntityId> = self
            .entities
            .of_kind(EntityKind::Npc)
            .filter(|entity| match entity.state {
                EntityState::Npc(state) => {
                    state.site.is_some() && plot.is_none_or(|plot| state.site == Some(plot.0))
                }
                _ => false,
            })
            .map(|entity| entity.id)
            .collect();
        if !leaving.is_empty() {
            self.entities.despawn_many(&leaving);
        }
    }

    /// Moves workers between stations along the house every so often.
    fn walk_site_crews(&mut self, next_tick: SimulationTick) {
        let shift = self.timing.ticks_for_millis(CREW_SHIFT_MILLIS).max(1);
        let ids: Vec<EntityId> = self
            .entities
            .of_kind(EntityKind::Npc)
            .filter(
                |entity| matches!(entity.state, EntityState::Npc(state) if state.site.is_some()),
            )
            .map(|entity| entity.id)
            .collect();
        for id in ids {
            let Some((entity, state)) = self.npc(id) else {
                continue;
            };
            let body = entity.body;
            let Some(plot) = state.site.and_then(|site| self.homes.plot(PlotId(site))) else {
                continue;
            };
            if state.talking_to.is_some() {
                // Talking: the people pass's conversation rules release them; stand still.
                let listening = state.talking_to.is_some_and(|player| {
                    self.people
                        .conversation(player)
                        .is_some_and(|conversation| conversation.npc == id)
                });
                if listening {
                    continue;
                }
                let mut next = state;
                next.talking_to = None;
                self.commit_crew(id, body, next);
                continue;
            }
            let phase = (next_tick.get() + u64::from(state.character.index) * 97) % shift;
            if phase != 0 || next_tick.get() < state.arrive.get() {
                continue;
            }
            let roll = hash2(
                self.terrain.seed() ^ SALT_CREW,
                u64::from(plot.id.0) << 16 | u64::from(state.character.index),
                i32::try_from(next_tick.get() / shift % 0x7FFF_FFFF).unwrap_or(0),
                0,
            );
            let station = usize::try_from(roll % 5).unwrap_or(0);
            let to = crew_station_px(plot, station, roll);
            let mut next = state;
            let x = state.x_at(next_tick, 0.0);
            next.walk(x, to, TILE_SIZE * 1.6, next_tick, self.timing.rate.hz());
            next.activity = Activity::Working;
            self.commit_crew(id, body, next);
        }
    }

    fn commit_crew(&mut self, id: EntityId, body: Body, after: NpcState) {
        let mut next_body = body;
        next_body.x = after.from_x;
        next_body.previous_x = after.from_x;
        next_body.velocity_x = 0.0;
        self.entities.update(id, |entity| {
            entity.body = next_body;
            entity.state = EntityState::Npc(after);
        });
    }

    /// Rigs parked in a garage bay they may use get their hull patched up.
    fn repair_garaged_rigs(&mut self) {
        let dt = self.timing.delta_seconds;
        let parked: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(id, rig)| {
                let speed = rig.velocity_x.abs() + rig.velocity_y.abs();
                if speed > 2.0 || rig.hull <= 0.0 {
                    return false;
                }
                let tile = TilePosition {
                    x: (rig.x / TILE_SIZE).floor() as i32,
                    y: (rig.y / TILE_SIZE).floor() as i32,
                };
                self.homes.plot_at(tile).is_some_and(|plot| {
                    plot.deed.may_enter(**id)
                        && plot
                            .garage_columns()
                            .is_some_and(|(first, last)| tile.x >= first && tile.x < last)
                        && tile.y < plot.site.ground_row
                        && tile.y >= plot.site.ground_row - 2
                })
            })
            .map(|(id, _)| *id)
            .collect();
        for id in parked {
            if let Some(rig) = self.players.get_mut(&id) {
                let max = rig.max_hull();
                if rig.hull < max {
                    rig.hull = crate::quantize::quantize_simulation_scalar(
                        crate::home::GARAGE_REPAIR_PER_SECOND
                            .mul_add(dt, rig.hull)
                            .min(max),
                    );
                }
            }
        }
    }

    /// What `player`'s hired contractor says about the work on their plot (the talk panel's
    /// building-work topic), if `character` is building for them.
    pub(super) fn site_talk(&self, character: CharacterId, player: PlayerId) -> Option<String> {
        let now = self.world_millis();
        let plot = self.homes.plots().find(|plot| {
            plot.deed.owns(player)
                && plot
                    .construction
                    .as_ref()
                    .is_some_and(|work| !work.guild && work.character() == character)
        })?;
        let work = plot.construction.as_ref()?;
        let stage = work.stage(now);
        let left = work.remaining_millis(now);
        let day = self.clock.day_length.millis().max(1);
        let when = if work.hold_since.is_some() {
            "as soon as the stock turns up".to_owned()
        } else if left < day / 4 {
            "by this evening".to_owned()
        } else if left < day {
            "by this time tomorrow".to_owned()
        } else {
            format!("in about {} days", left.div_ceil(day))
        };
        let line = match work.contractor.style {
            ContractorStyle::FastAndLoose => format!(
                "{} going great. Done {when}, give or take a wall.",
                stage.name()
            ),
            ContractorStyle::SlowAndPerfect => format!(
                "{}: every joint true so far. Finished {when}, and not a minute sooner.",
                stage.name()
            ),
            ContractorStyle::Chatty => format!(
                "Oh, we're on the {} now! Lovely work, if I say so myself. Finished {when}, I'd think.",
                stage.name().to_lowercase()
            ),
            ContractorStyle::Grumpy => {
                format!("{}. It'll be done {when}. Stop asking.", stage.name())
            }
            ContractorStyle::Steady => {
                format!("{} on schedule. Keys in your hand {when}.", stage.name())
            }
        };
        Some(line)
    }

    /// Scenario helper (tests and screenshot scenes): stakes `site` for `player` and, with a
    /// `blueprint`, hands over a finished home straight away. Returns the plot.
    pub fn scenario_home(
        &mut self,
        player: PlayerId,
        site: PlotSite,
        blueprint: Option<crate::home::Blueprint>,
    ) -> PlotId {
        let now = self.world_millis();
        let id = self.homes.insert_plot(
            site,
            Deed {
                owner: player,
                co_owners: BTreeSet::new(),
                access: HomeAccess::default(),
                bought_millis: now,
                paid: site.price,
            },
        );
        if let Some(blueprint) = blueprint {
            if let Some(found) = self.homes.plot_mut(id) {
                found.construction = Some(Construction {
                    job: HomeJob::Build(blueprint),
                    contractor: guild_offer().profile,
                    guild: true,
                    quote: guild_offer().profile.quote(blueprint.job()),
                    price_paid: 0,
                    supplied_blocks: 0,
                    started_millis: now,
                    total_millis: 1,
                    rushed_millis: 0,
                    held_millis: 0,
                    hold_since: None,
                    planned: Vec::new(),
                    events: Vec::new(),
                    announced: BuildStage::Survey,
                    bonus_quality: 0,
                });
            }
            self.lay_foundation(id);
            if let Some(found) = self.homes.plot_mut(id) {
                found.construction = None;
                found.home = Some(Home::built(blueprint, 4, None, now));
            }
        }
        id
    }

    /// Scenario helper (tests and screenshot scenes): edits `plot` in place.
    pub fn scenario_edit_plot(&mut self, plot: PlotId, change: impl FnOnce(&mut Plot)) {
        if let Some(found) = self.homes.plot_mut(plot) {
            change(found);
        }
    }

    /// Scenario helper: applies an expansion to a finished home at once (cellar dug).
    pub fn scenario_expand(&mut self, plot: PlotId, expansion: Expansion) {
        if let Some(home) = self
            .homes
            .plot_mut(plot)
            .and_then(|found| found.home.as_mut())
        {
            home.expand(expansion);
        }
        if expansion == Expansion::Basement {
            self.dig_cellar(plot);
        }
    }
}

/// Rows of the cellar under a house with a basement.
pub const CELLAR_ROWS: i32 = 3;

/// The cellar's columns under `plot`'s house.
fn cellar_columns(plot: &Plot) -> (i32, i32) {
    let width = plot.home.as_ref().map_or(7, |home| home.width);
    let left = plot.house_left(width);
    (left + 1, left + width.clamp(4, 9) - 2)
}

/// A worker's station `slot` along the house (world pixel column), jittered by `roll`.
fn crew_station_px(plot: &Plot, slot: usize, roll: u64) -> f32 {
    let width = plot.planned_footprint().map_or(7, |(width, _)| width);
    let left = plot.house_left(width);
    let stations = [-1, width / 4, width / 2, width * 3 / 4, width];
    let column = left + stations[slot % stations.len()];
    let jitter = i32::try_from(roll % 3).unwrap_or(0) - 1;
    #[allow(clippy::cast_precision_loss, reason = "columns are small")]
    let x = ((column + jitter) as f32 + 0.5) * TILE_SIZE;
    x.round()
}

/// The town builders' guild: hired where no person takes the work.
/// Builders listed out of `nearest` (nearest first): everyone who built for this client
/// before (they remember you), then a varied pick (at most [`MAX_LISTED_PER_STYLE`] of one
/// style while others are about), then the nearest of the rest; listed nearest first.
fn pick_contractors(nearest: Vec<ContractorOffer>) -> Vec<ContractorOffer> {
    let mut picked = vec![false; nearest.len()];
    let mut styles: BTreeMap<ContractorStyle, usize> = BTreeMap::new();
    // Pass 0: past clients; pass 1: a varied pick; pass 2: the nearest of the rest.
    for pass in 0..3 {
        for (index, offer) in nearest.iter().enumerate() {
            let count = picked.iter().filter(|kept| **kept).count();
            if count >= MAX_CONTRACTORS_LISTED || picked[index] {
                continue;
            }
            let style_count = styles.get(&offer.profile.style).copied().unwrap_or(0);
            let wanted = match pass {
                0 => offer.past_jobs > 0,
                1 => style_count < MAX_LISTED_PER_STYLE,
                _ => true,
            };
            if wanted {
                picked[index] = true;
                *styles.entry(offer.profile.style).or_default() += 1;
            }
        }
    }
    nearest
        .into_iter()
        .zip(picked)
        .filter_map(|(offer, keep)| keep.then_some(offer))
        .collect()
}

fn guild_offer() -> ContractorOffer {
    ContractorOffer {
        character: None,
        profile: ContractorProfile {
            character: CharacterId::default(),
            style: ContractorStyle::Steady,
            speed_percent: 110,
            quality: 3,
            price_percent: 110,
        },
        name: "Builders' Guild".to_owned(),
        from: TOWN_NAME.to_owned(),
        past_jobs: 0,
    }
}

const fn is_cargo(kind: ItemKind) -> bool {
    matches!(
        kind.category(),
        ItemCategory::Mineral | ItemCategory::Artifact | ItemCategory::Material
    )
}

#[cfg(test)]
impl WorldState {
    /// Test helper: stakes a claim for `owner` with its beacon at `center` (no checks).
    pub(crate) fn scenario_claim(&mut self, owner: PlayerId, center: TilePosition) {
        let beacon = crate::base::PlacedModule {
            id: crate::base::StructureId::default(),
            kind: crate::base::ModuleKind::Beacon,
            origin: center,
            facing: crate::base::Facing::Right,
            claim: crate::base::ClaimId::default(),
            owner,
            paid_credits: 0,
            paid_blocks: crate::item::ItemContainer::new(),
            fuel: 0,
            storage: crate::item::ItemContainer::new(),
            refined: 0,
        };
        let _ = self.bases.stake_claim(owner, center, false, beacon);
    }
}

#[cfg(test)]
mod pick_tests {
    use super::*;

    fn offer(index: u16, style: ContractorStyle, past_jobs: u32) -> ContractorOffer {
        let mut offer = guild_offer();
        let id = CharacterId {
            village: TOWN_VILLAGE,
            index,
        };
        offer.character = Some(id);
        offer.profile.character = id;
        offer.profile.style = style;
        offer.past_jobs = past_jobs;
        offer
    }

    #[test]
    fn the_listing_is_varied_and_keeps_past_builders() {
        use ContractorStyle::{Chatty, FastAndLoose, Grumpy, SlowAndPerfect};
        // Nearest first: five perfectionists, then other styles; the farthest built for us.
        let nearest = vec![
            offer(0, SlowAndPerfect, 0),
            offer(1, SlowAndPerfect, 0),
            offer(2, SlowAndPerfect, 0),
            offer(3, SlowAndPerfect, 0),
            offer(4, SlowAndPerfect, 0),
            offer(5, FastAndLoose, 0),
            offer(6, Grumpy, 0),
            offer(7, Chatty, 0),
            offer(8, SlowAndPerfect, 1),
        ];
        let listed: Vec<u16> = pick_contractors(nearest)
            .iter()
            .filter_map(|offer| offer.character.map(|id| id.index))
            .collect();
        // The past builder, one more perfectionist, then the other styles, nearest first;
        // the last place goes to the nearest of the rest.
        assert_eq!(listed, vec![0, 1, 5, 6, 7, 8]);
    }
}
