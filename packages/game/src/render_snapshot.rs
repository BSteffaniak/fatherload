//! The immutable per-frame view of the game that rendering, HUD and audio consume (plan §10).
//!
//! A [`RenderSnapshot`] is built from the runtime's world after the frame's ticks ran. Each local
//! seat gets a [`ViewSnapshot`] whose camera follows the same player transform the view draws, so
//! the camera can never disagree with the rig. The renderer reads only this snapshot plus the
//! shell state; it never touches the simulation directly.

use std::collections::BTreeMap;

use fatherload_simulation::{
    ActiveQuest, CanonicalProgressionState, DiscoveredTiles, DrillState, FallingBoulder,
    HazardCloud, InfrastructureKind, PlacedBomb, PlacedInfrastructure, Player, PlayerId,
    PlayerSurvivalSnapshot, RigCosmetics, TestModeState, TickRate, WorldEventKind, WorldState,
    economy::SurfaceZone,
    poi::RELAY_WINDOW_MILLIS,
    quest::describe::{QuestMarker, quest_markers},
    terrain::{MineralKind, TILE_SIZE, Terrain, TilePosition},
    worldgen::TileRect,
};

use crate::{
    camera_lead::CameraLeads,
    effects::{Explosion, PickupFlight, SparkParticle},
    runtime::LocalSeat,
    runtime::RuntimeEvent,
};

/// Default (and reference) logical window width, in screen points.
pub const SCREEN_WIDTH: i32 = 1280;
/// Default (and reference) logical window height, in screen points.
pub const SCREEN_HEIGHT: i32 = 720;

/// The screen a snapshot is laid out for: logical size in screen points and the display's
/// physical pixels per point (2.0 on Retina).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenSize {
    pub width: i32,
    pub height: i32,
    pub density: f32,
}

impl From<(i32, i32)> for ScreenSize {
    /// A standard-density screen.
    fn from((width, height): (i32, i32)) -> Self {
        Self {
            width,
            height,
            density: 1.0,
        }
    }
}

/// Screen points per world pixel for `screen`, snapped so one art pixel covers a whole number of
/// physical pixels. Based on the whole screen, not one split-screen view, so every local view
/// draws the world at the same scale.
#[must_use]
pub fn world_scale_for_screen(screen: ScreenSize) -> f32 {
    crate::pixel_scale::world_scale(screen.width, screen.height, screen.density)
}

/// Every mineral, in market display order.
pub const MINERALS: [MineralKind; 10] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
];

/// A screen rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Viewport {
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Lays out `count` views in a near-square grid over the screen. Works for any count: one view
/// fills the screen, two sit side by side, three and four share a 2x2 grid, and so on.
#[must_use]
pub fn split_screen_viewports(
    count: usize,
    screen_width: i32,
    screen_height: i32,
) -> Vec<Viewport> {
    let count = count.max(1);
    let mut columns = 1;
    while columns * columns < count {
        columns += 1;
    }
    let rows = count.div_ceil(columns);
    let columns_i32 = i32::try_from(columns).unwrap_or(i32::MAX);
    let rows_i32 = i32::try_from(rows).unwrap_or(i32::MAX);
    (0..count)
        .map(|index| {
            let column = i32::try_from(index % columns).unwrap_or(0);
            let row = i32::try_from(index / columns).unwrap_or(0);
            let x0 = screen_width * column / columns_i32;
            let x1 = screen_width * (column + 1) / columns_i32;
            let y0 = screen_height * row / rows_i32;
            let y1 = screen_height * (row + 1) / rows_i32;
            Viewport::new(x0, y0, x1 - x0, y1 - y0)
        })
        .collect()
}

/// The grid cells [`split_screen_viewports`] leaves empty for `count` views (three seats use a
/// 2x2 grid with the last cell free).
#[must_use]
pub fn empty_split_cells(count: usize, screen_width: i32, screen_height: i32) -> Vec<Viewport> {
    let count = count.max(1);
    let mut columns = 1;
    while columns * columns < count {
        columns += 1;
    }
    let cells = columns * count.div_ceil(columns);
    split_screen_viewports(cells, screen_width, screen_height)
        .into_iter()
        .skip(count)
        .collect()
}

/// How endangered a rig looks to other players.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DangerLevel {
    Nominal,
    Warning,
    Critical,
}

/// The upgrade tiers a rig shows (drawn by `rendering::rig_art`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RigTiers {
    pub drill: u8,
    pub engine: u8,
    pub cargo: u8,
    pub hull: u8,
    pub radiator: u8,
    /// Drill mount (0 stock, 1 gimbal, 2 crown bore).
    pub mount: u8,
}

impl RigTiers {
    /// A fresh rig's stock parts.
    #[cfg_attr(
        target_os = "emscripten",
        allow(dead_code, reason = "used by the desktop rig sheets and tests")
    )]
    pub const STOCK: Self = Self {
        drill: 1,
        engine: 1,
        cargo: 1,
        hull: 1,
        radiator: 1,
        mount: 0,
    };

    /// Every line at `tier` (mount at its top tier from tier 2).
    #[cfg_attr(
        target_os = "emscripten",
        allow(dead_code, reason = "used by the desktop rig sheets and tests")
    )]
    #[must_use]
    pub fn all(tier: u8) -> Self {
        Self {
            drill: tier,
            engine: tier,
            cargo: tier,
            hull: tier,
            radiator: tier,
            mount: tier.saturating_sub(1).min(2),
        }
    }

    /// The tiers fitted to `player`.
    #[must_use]
    pub const fn of(player: &Player) -> Self {
        Self {
            drill: player.drill_strength,
            engine: player.engine_level,
            cargo: player.cargo_bay_level,
            hull: player.hull_level,
            radiator: player.radiator_level,
            mount: player.drill_mount_level,
        }
    }
}

/// One rig as drawn in the world.
#[derive(Clone, Debug)]
pub struct PlayerPresentation {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
    pub max_hull: f32,
    pub scanner_level: u8,
    pub active_drill: Option<DrillState>,
    /// Every visible upgrade tier (drill head, thrusters, cargo pods, plating, radiator, mount).
    pub tiers: RigTiers,
    /// The rig's look: the player's garage paint job, or their default.
    pub cosmetics: RigCosmetics,
    /// The weapon on the hardpoint, where it points and whether its trigger is held.
    pub weapon: Option<WeaponPose>,
}

/// A mounted weapon as drawn: which, its tier, its aim (unit vector) and the trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponPose {
    pub mounted: fatherload_simulation::MountedWeapon,
    pub aim: (f32, f32),
    pub firing: bool,
}

/// A projectile in flight (presentation copy of a projectile entity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileView {
    pub weapon: fatherload_simulation::WeaponKind,
    /// Who fired it (a harpoon's line runs back to their rig).
    pub owner: PlayerId,
    pub x: f32,
    pub y: f32,
    pub previous_x: f32,
    pub previous_y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
}

/// A damageable entity (practice target) as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetView {
    pub x: f32,
    pub y: f32,
    /// Health left, 0..=1.
    pub health: f32,
}

/// A plasma cutter's beam: from the muzzle to where it stops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamView {
    pub from: (f32, f32),
    pub to: (f32, f32),
}

impl PlayerPresentation {
    fn from_player(player_id: PlayerId, player: &Player, world: &WorldState) -> Self {
        Self {
            player_id,
            x: player.x,
            y: player.y,
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
            fuel: player.fuel,
            hull: player.hull,
            max_hull: player.max_hull(),
            scanner_level: player.scanner_level,
            active_drill: world.active_drill(player_id).copied(),
            tiers: RigTiers::of(player),
            cosmetics: RigCosmetics::resolve(player.cosmetics, player_id),
            weapon: player.mounted_weapon.map(|mounted| WeaponPose {
                mounted,
                aim: player.weapon.aim(),
                firing: player.weapon.firing,
            }),
        }
    }

    /// Danger shown on the rig for other players.
    #[must_use]
    pub fn danger_level(&self) -> DangerLevel {
        if self.hull <= 0.0 || self.fuel <= 0.0 {
            DangerLevel::Critical
        } else if self.hull < 30.0 || self.fuel < 30.0 {
            DangerLevel::Warning
        } else {
            DangerLevel::Nominal
        }
    }
}

/// HUD numbers for one view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudValues {
    pub hull: f32,
    pub max_hull: f32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub credits: u32,
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    /// Metres (rows) below the ground of the rig's own column (0 at or above it): the surface
    /// heightmap moves the ground, so depth counts from wherever it is.
    pub depth: i32,
    /// Metres above the ground of the rig's column while flying above it (`None` on the
    /// ground and underground). The sky has no ceiling, so this can be very large.
    pub altitude: Option<i32>,
    /// The rig's tile row (absolute: depth rules such as heat read classic nominal rows).
    pub row: i32,
    pub scanner_cooldown_seconds: f32,
}

/// Depth below and altitude above column ground at `surface_row` for a rig whose centre is
/// in tile row `row`: a rig resting on the ground (centre in the row above it) reads 0 m
/// either way.
#[must_use]
pub const fn depth_and_altitude(row: i32, surface_row: i32) -> (i32, Option<i32>) {
    let resting_row = surface_row - 1;
    let offset = row - resting_row;
    if offset > 0 {
        (offset, None)
    } else if offset < 0 {
        (0, Some(-offset))
    } else {
        (0, None)
    }
}

/// Camera position (top-left of the view, world pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPosition {
    pub x: f32,
    pub y: f32,
}

/// One local seat's view.
#[derive(Clone, Debug)]
pub struct ViewSnapshot<'w> {
    /// Seat order index (0 = primary).
    pub seat_index: usize,
    pub player_id: PlayerId,
    /// Where the view sits on screen, in logical screen points.
    pub viewport: Viewport,
    /// Screen points per world pixel (see [`world_scale_for_screen`]).
    pub world_scale: f32,
    /// Top-left of the visible world (world pixels); centres `player` in the viewport.
    pub camera: CameraPosition,
    /// The seat's rig (inventory, upgrades, loan, kits...).
    pub player: &'w Player,
    pub hud: HudValues,
    pub active_drill: Option<DrillState>,
    /// Tiles this player has discovered.
    pub explored: Option<&'w DiscoveredTiles>,
    pub lost_cargo: Option<(f32, f32)>,
    /// `Some` while the rig is disabled or stranded (rescue prompt).
    pub failure: Option<PlayerSurvivalSnapshot>,
    pub rescue_count: u32,
    pub deep_instability: f32,
    /// The surface building the rig stands at (its door prompt).
    pub surface_zone: Option<SurfaceZone>,
    /// The village building whose door the rig stands at, as its door prompt ("Enter the
    /// Trading Post").
    pub village_door: Option<String>,
    /// The quests this player sees: their own solo quests, then the crew's.
    pub quests: Vec<&'w ActiveQuest>,
    /// Markers of those quests' current objectives (map, minimap and world).
    pub quest_markers: Vec<QuestMarker>,
    /// The quest desk of the keeper whose door the rig stands at (empty elsewhere).
    pub quest_desk: Vec<crate::shell::DeskRow>,
    /// The seat's throw aim preview (set by the game loop from the seat's selection).
    pub aim_preview: Option<AimPreview>,
    /// The seat's weapon aim reticle (set by the game loop from the seat's aim).
    pub reticle: Option<crate::weapon_aim::Reticle>,
    /// The seat's build-mode preview (set by the game loop while build mode is on).
    pub build_preview: Option<crate::base_view::BuildPreview>,
    /// The module menu the seat has open (set by the game loop).
    pub base_module: Option<crate::base_view::BaseModuleView>,
    /// The base module the rig stands at (its use prompt).
    pub nearby_module: Option<fatherload_simulation::base::ModuleKind>,
}

impl ViewSnapshot<'_> {
    /// Size of the world area this view shows, in world pixels.
    #[must_use]
    pub fn visible_world_size(&self) -> (f32, f32) {
        visible_world_size(self.viewport, self.world_scale)
    }

    /// Whether a tile is discovered for this view.
    #[must_use]
    pub fn is_explored(&self, position: TilePosition) -> bool {
        self.explored.is_some_and(|tiles| tiles.contains(position))
    }
}

/// One mineral's market quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarketQuote {
    pub mineral: MineralKind,
    pub value: u32,
    pub factor: u32,
    pub previous_factor: Option<u32>,
}

/// A twin-relay vault lock with one relay lit, counting down to its reset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelayCountdown {
    /// The vault's bounds (tiles).
    pub bounds: TileRect,
    /// Share of the relay window left, `1.0` just lit down to `0.0` at the reset.
    pub remaining: f32,
}

/// The twin-relay locks of `world` waiting for their second relay.
#[allow(
    clippy::cast_precision_loss,
    reason = "tick counts of a few hundred map exactly to f32"
)]
fn relay_countdowns(world: &WorldState) -> Vec<RelayCountdown> {
    let window = world.timing().ticks_for_millis(RELAY_WINDOW_MILLIS).max(1);
    let now = world.simulation_tick().get();
    world
        .progression()
        .vault_relay_deadlines
        .iter()
        .filter_map(|(feature, deadline)| {
            let vault = world.world_feature(*feature)?;
            let left = deadline.get().saturating_sub(now).min(window);
            Some(RelayCountdown {
                bounds: vault.bounds,
                remaining: left as f32 / window as f32,
            })
        })
        .collect()
}

/// An active world event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldEventView {
    pub kind: WorldEventKind,
    pub severity: u32,
    pub days_remaining: u32,
}

/// A seat's throw aim preview: the path its selected charge would fly, one point per tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AimPreview {
    pub path: Vec<(f32, f32)>,
    /// The charge sticks where the path ends (sticky bombs, C4, shaped charges).
    pub sticks: bool,
}

impl AimPreview {
    /// The preview for `player_id` deploying `selection` in `world`, if the selected kind is
    /// thrown or placed and the rig carries one.
    #[must_use]
    pub fn for_selection(
        world: &WorldState,
        player_id: PlayerId,
        selection: crate::input_mapping::ExplosiveSelection,
    ) -> Option<Self> {
        let kind = selection.selected;
        if kind.deployment() == fatherload_simulation::explosive::Deployment::Drop
            || world.player(player_id)?.explosive_count(kind) == 0
        {
            return None;
        }
        Some(Self {
            path: world.preview_explosive_path(player_id, kind, selection.last_aim),
            sticks: kind.sticks(),
        })
    }
}

/// Projectiles in flight, for drawing.
fn projectile_views(world: &WorldState) -> Vec<ProjectileView> {
    use fatherload_simulation::entity::{EntityKind, EntityState};
    world
        .entities()
        .of_kind(EntityKind::Projectile)
        .filter_map(|entity| match entity.state {
            EntityState::Projectile(state) => Some(ProjectileView {
                weapon: state.weapon,
                owner: state.owner,
                x: entity.body.x,
                y: entity.body.y,
                previous_x: entity.body.previous_x,
                previous_y: entity.body.previous_y,
                velocity_x: entity.body.velocity_x,
                velocity_y: entity.body.velocity_y,
            }),
            _ => None,
        })
        .collect()
}

/// Damageable entities with their health, for drawing.
fn target_views(world: &WorldState) -> Vec<TargetView> {
    use fatherload_simulation::entity::EntityKind;
    world
        .entities()
        .of_kind(EntityKind::Target)
        .filter_map(|entity| {
            let health = entity.state.health()?;
            Some(TargetView {
                x: entity.body.x,
                y: entity.body.y,
                health: (health.current / health.max.max(1.0)).clamp(0.0, 1.0),
            })
        })
        .collect()
}

/// Everything rendering needs for one frame.
#[derive(Clone, Debug)]
pub struct RenderSnapshot<'w> {
    pub tick: u64,
    /// The session tick rate (quest timers in seconds).
    pub tick_rate: TickRate,
    pub terrain: &'w Terrain,
    /// Gas clouds (entities, copied out of the world's entity store).
    pub hazards: Vec<HazardCloud>,
    /// Live explosive charges (entities, copied out of the world's entity store).
    pub bombs: Vec<PlacedBomb>,
    /// Presentation fraction between a bomb's previous and current fixed-tick poses.
    pub bomb_alpha: f32,
    pub infrastructure: &'w [PlacedInfrastructure],
    /// Falling boulders (entities, copied out of the world's entity store).
    pub boulders: Vec<FallingBoulder>,
    /// The world's entity store: item drops are drawn straight from it, looked up per view
    /// through its spatial index (WP-DROPS).
    pub entities: &'w fatherload_simulation::entity::EntityStore,
    /// Collected drops flying into their rigs (set by the game after [`Self::build`]).
    pub pickups: &'w [PickupFlight],
    pub sparks: &'w [SparkParticle],
    /// Bomb blasts in progress.
    pub explosions: &'w [Explosion],
    /// Muzzle flashes and tracers (set by the game loop from its effects).
    pub shots: &'w [crate::effects::ShotFlash],
    /// Projectiles in flight.
    pub projectiles: Vec<ProjectileView>,
    /// Practice targets and other damageable entities.
    pub targets: Vec<TargetView>,
    /// Plasma cutter beams being fired.
    pub beams: Vec<BeamView>,

    /// Twin-relay vault locks counting down.
    pub relay_countdowns: Vec<RelayCountdown>,
    /// Every rig in the world.
    pub players: Vec<PlayerPresentation>,
    /// One view per local seat, in seat order.
    pub views: Vec<ViewSnapshot<'w>>,
    pub progression: &'w CanonicalProgressionState,
    pub market: Vec<MarketQuote>,
    pub world_events: Vec<WorldEventView>,
    pub signal_relay_count: usize,
    pub won_game: bool,
    /// The world's test-mode toggles (`None` outside test mode).
    pub test_mode: Option<TestModeState>,
    /// Gameplay events presented for the first time this frame (audio/particles).
    pub events: &'w [RuntimeEvent],
    /// Cave critters (entities, copied out of the world's entity store).
    pub critters: Vec<CritterPresentation>,
    /// Enemies and enemy shots (entities copied out of the world; drawn interpolated with
    /// `bomb_alpha`).
    pub enemies: Vec<fatherload_simulation::entity::Entity>,
    /// Rigs inside their post-hit invulnerability window and the seconds left (hit flash).
    pub rig_guards: Vec<(PlayerId, f32)>,
    /// The world's enemy setting (pause menu).
    pub enemy_difficulty: fatherload_simulation::EnemyDifficulty,
    /// Claims, base modules and fixtures (WP-BASE).
    pub bases: &'w fatherload_simulation::base::Bases,
    /// The base modules that have power this frame.
    pub powered_modules: std::collections::BTreeSet<fatherload_simulation::base::StructureId>,
}

impl<'w> RenderSnapshot<'w> {
    /// Builds the snapshot for `seats` laid out over `screen`.
    #[must_use]
    pub fn build(
        world: &'w WorldState,
        seats: &[LocalSeat],
        screen: impl Into<ScreenSize>,
        effects: (&'w [SparkParticle], &'w [Explosion]),
        events: &'w [RuntimeEvent],
    ) -> Self {
        let (sparks, explosions) = effects;
        let screen = screen.into();
        let viewports = split_screen_viewports(seats.len(), screen.width, screen.height);
        let world_scale = world_scale_for_screen(screen);
        let views = seats
            .iter()
            .zip(viewports)
            .enumerate()
            .filter_map(|(seat_index, (seat, viewport))| {
                view_snapshot(world, seat_index, seat.player, viewport, world_scale)
            })
            .collect();
        Self {
            tick: world.simulation_tick().get(),
            tick_rate: world.timing().rate,
            terrain: world.terrain(),
            hazards: world.hazards(),
            bombs: world.bombs(),
            bomb_alpha: 1.0,
            infrastructure: world.infrastructure(),
            boulders: world.falling_boulders(),
            entities: world.entities(),
            pickups: &[],
            critters: critters_of(world),
            sparks,
            explosions,
            shots: &[],
            projectiles: projectile_views(world),
            targets: target_views(world),
            beams: world
                .player_ids()
                .filter_map(|id| world.active_beam(id))
                .map(|(from, to)| BeamView { from, to })
                .collect(),

            relay_countdowns: relay_countdowns(world),
            players: world
                .players()
                .iter()
                .map(|(id, player)| PlayerPresentation::from_player(*id, player, world))
                .collect(),
            views,
            progression: world.progression(),
            market: MINERALS
                .into_iter()
                .map(|mineral| MarketQuote {
                    mineral,
                    value: world.mineral_market_value(mineral),
                    factor: world.mineral_market_factor(mineral),
                    previous_factor: world.previous_mineral_market_factor(mineral),
                })
                .collect(),
            world_events: world
                .active_world_events()
                .into_iter()
                .map(|(kind, severity, days_remaining)| WorldEventView {
                    kind,
                    severity,
                    days_remaining,
                })
                .collect(),
            signal_relay_count: world
                .infrastructure()
                .iter()
                .filter(|item| item.kind == InfrastructureKind::SignalRelay)
                .count(),
            won_game: world.won_game(),
            test_mode: world.test_mode(),
            events,
            enemies: world
                .entities()
                .of_kind(fatherload_simulation::entity::EntityKind::Enemy)
                .chain(
                    world
                        .entities()
                        .of_kind(fatherload_simulation::entity::EntityKind::EnemyShot),
                )
                .cloned()
                .collect(),
            rig_guards: world
                .player_ids()
                .map(|id| (id, world.rig_guard_seconds(id)))
                .filter(|(_, seconds)| *seconds > 0.0)
                .collect(),
            enemy_difficulty: world.enemy_difficulty(),
            bases: world.bases(),
            powered_modules: world
                .bases()
                .modules()
                .filter(|module| world.base_module_powered(module.id))
                .map(|module| module.id)
                .collect(),
        }
    }

    /// Moves rigs to presentation poses (world pixels) and re-centres the cameras of views that
    /// follow a moved rig, so a camera always follows exactly the transform its rig is drawn at.
    /// A joined client passes its predicted + smoothed local rig and interpolated remote rigs.
    pub fn apply_pose_overrides(&mut self, poses: &BTreeMap<PlayerId, (f32, f32)>) {
        if poses.is_empty() {
            return;
        }
        for player in &mut self.players {
            if let Some(&(x, y)) = poses.get(&player.player_id) {
                player.x = x;
                player.y = y;
            }
        }
        for view in &mut self.views {
            if let Some(&(x, y)) = poses.get(&view.player_id) {
                view.camera = camera_for(view.viewport, view.world_scale, x, y);
            }
        }
    }

    /// Shifts each view's camera by its rig's speed-scaled lead (see [`crate::camera_lead`]):
    /// zero for slow rigs, ahead of the travel direction for fast ones. Call after
    /// [`Self::apply_pose_overrides`].
    pub fn apply_camera_leads(&mut self, leads: &CameraLeads) {
        for view in &mut self.views {
            let visible = visible_world_size(view.viewport, view.world_scale);
            let (x, y) = leads.offset(view.player_id, visible);
            view.camera.x += x;
            view.camera.y += y;
        }
    }

    /// The primary seat's view, if any.
    #[must_use]
    pub fn primary_view(&self) -> Option<&ViewSnapshot<'w>> {
        self.views.first()
    }

    /// Sale value of one unit of `mineral` today.
    #[must_use]
    pub fn market_value(&self, mineral: MineralKind) -> u32 {
        self.market
            .iter()
            .find(|quote| quote.mineral == mineral)
            .map_or(0, |quote| quote.value)
    }

    /// One-line summary of active world events.
    #[must_use]
    pub fn world_event_summary(&self) -> String {
        if self.world_events.is_empty() {
            return "No active world events.".to_owned();
        }
        self.world_events
            .iter()
            .map(|event| {
                format!(
                    "{} S{} ({}d)",
                    event.kind.label(),
                    event.severity,
                    event.days_remaining
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Warning line for a view's rig.
    #[must_use]
    pub fn warning_summary(&self, view: &ViewSnapshot<'_>) -> String {
        let player = view.player;
        // Warning depths are classic nominal rows (deep worlds stretch them with their bands).
        let depth = self.terrain.nominal_row(player.tile_position(TILE_SIZE).y);
        let mut warnings = Vec::new();
        if player.fuel <= player.fuel_capacity * 0.2 {
            warnings.push("fuel");
        }
        if player.hull <= player.max_hull() * 0.35 {
            warnings.push("hull");
        }
        if depth >= 60 {
            warnings.push("heat");
        }
        if view.deep_instability >= 60.0 {
            warnings.push("instability");
        }
        if depth >= 80 && self.signal_relay_count == 0 {
            warnings.push("signal");
        }
        if warnings.is_empty() {
            "Warnings: nominal".to_owned()
        } else {
            format!("Warnings: {}", warnings.join(", "))
        }
    }
}

/// World pixels a `viewport` (screen points) shows at `world_scale` screen points per world pixel.
#[must_use]
pub fn visible_world_size(viewport: Viewport, world_scale: f32) -> (f32, f32) {
    #[allow(
        clippy::cast_precision_loss,
        reason = "bounded viewport dimensions convert to world pixels"
    )]
    let (width, height) = (viewport.width as f32, viewport.height as f32);
    let scale = world_scale.max(f32::EPSILON);
    (width / scale, height / scale)
}

/// The camera (top-left, world pixels) that centres world pixel `(x, y)` in `viewport`.
fn camera_for(viewport: Viewport, world_scale: f32, x: f32, y: f32) -> CameraPosition {
    let (width, height) = visible_world_size(viewport, world_scale);
    CameraPosition {
        x: width.mul_add(-0.5, x),
        y: height.mul_add(-0.5, y),
    }
}

fn view_snapshot(
    world: &WorldState,
    seat_index: usize,
    player_id: PlayerId,
    viewport: Viewport,
    world_scale: f32,
) -> Option<ViewSnapshot<'_>> {
    let player = world.player(player_id)?;
    let camera = camera_for(viewport, world_scale, player.x, player.y);
    let tile = player.tile_position(TILE_SIZE);
    let (depth, altitude) = depth_and_altitude(tile.y, world.terrain().surface_row(tile.x));
    Some(ViewSnapshot {
        aim_preview: None,
        reticle: None,
        build_preview: None,
        base_module: None,
        nearby_module: crate::shell::module_at_rig(world, Some(player_id))
            .and_then(|id| world.bases().module(id))
            .map(|module| module.kind),
        seat_index,
        player_id,
        viewport,
        world_scale,
        camera,
        player,
        hud: HudValues {
            hull: player.hull,
            max_hull: player.max_hull(),
            fuel: player.fuel,
            fuel_capacity: player.fuel_capacity,
            credits: player.credits,
            cargo_used: player.cargo_used(),
            cargo_capacity: player.cargo_capacity,
            depth,
            altitude,
            row: tile.y,
            scanner_cooldown_seconds: world.scanner_cooldown_seconds(player_id).unwrap_or(0.0),
        },
        active_drill: world.active_drill(player_id).copied(),
        explored: world.discovered_tiles(player_id),
        lost_cargo: world
            .lost_cargo_sites()
            .get(&player_id)
            .map(|site| (site.x, site.y)),
        failure: world.failure_state(player_id),
        rescue_count: world.rescue_count(player_id),
        deep_instability: world
            .deep_run_state(player_id)
            .map_or(0.0, |state| state.instability),
        surface_zone: world.player_surface_zone(player_id),
        village_door: world
            .player_village_building(player_id)
            .map(|(found, building)| {
                let lot = &found.buildings[building];
                if lot.kind == fatherload_simulation::village::BuildingKind::House {
                    let family = lot
                        .residents
                        .first()
                        .and_then(|index| found.population.characters.get(usize::from(*index)))
                        .map_or("a", |who| who.name.family.as_str());
                    format!("Visit the {family} home")
                } else {
                    format!("Enter the {}", lot.kind.label())
                }
            }),
        quests: world.visible_quests(player_id),
        quest_markers: world
            .visible_quests(player_id)
            .into_iter()
            .flat_map(|quest| quest_markers(quest, world.simulation_tick().get()))
            .collect(),
        quest_desk: world
            .player_surface_zone(player_id)
            .and_then(fatherload_simulation::quest::QuestGiver::at)
            .map(|giver| crate::shell::desk_rows(world, player_id, giver))
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_overrides_move_the_rig_and_its_camera_together() {
        let rig = PlayerId::new(1);
        let world = WorldState::new_game(rig);
        let seats = [LocalSeat {
            seat: fatherload_netcode::SeatId(0),
            player: rig,
        }];
        let mut snapshot = RenderSnapshot::build(
            &world,
            &seats,
            (SCREEN_WIDTH, SCREEN_HEIGHT),
            (&[], &[]),
            &[],
        );
        let (x, y) = (1_000.0, 450.0);
        snapshot.apply_pose_overrides(&BTreeMap::from([(rig, (x, y))]));
        let drawn = snapshot
            .players
            .iter()
            .find(|player| player.player_id == rig)
            .expect("rig drawn");
        assert!((drawn.x - x).abs() < f32::EPSILON && (drawn.y - y).abs() < f32::EPSILON);
        let view = snapshot.primary_view().expect("view");
        assert!((view.camera.x - (x - 640.0)).abs() < f32::EPSILON);
        assert!((view.camera.y - (y - 360.0)).abs() < f32::EPSILON);
    }

    const SCREEN_SIZES: [(i32, i32); 6] = [
        (960, 540),
        (1280, 720),
        (1920, 1080),
        (2560, 1080),
        (2560, 1440),
        (1100, 900),
    ];

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
    }

    fn scale_at(height: i32) -> f32 {
        // A widescreen desktop window (phones scale by their own rules, see `pixel_scale`).
        world_scale_for_screen(ScreenSize::from((height * 16 / 9, height)))
    }

    #[test]
    fn world_scale_keeps_the_vertical_world_span_near_constant() {
        assert_near(scale_at(720), 1.0);
        assert_near(scale_at(1080), 1.5);
        assert_near(scale_at(540), 1.0);
        assert_near(scale_at(1440), 2.0);
        assert_near(scale_at(0), 0.5);
        let retina = ScreenSize {
            width: 1280,
            height: 720,
            density: 2.0,
        };
        assert_near(world_scale_for_screen(retina), 1.0);
        for (width, height) in SCREEN_SIZES {
            let scale = scale_at(height);
            let (_, visible_height) = visible_world_size(Viewport::new(0, 0, width, height), scale);
            assert!(
                (crate::pixel_scale::WORLD_VIEW_HEIGHT * 0.66
                    ..=crate::pixel_scale::WORLD_VIEW_HEIGHT * 1.34)
                    .contains(&visible_height),
                "{height}: {visible_height}"
            );
        }
    }

    #[test]
    fn wider_screens_see_more_world_horizontally() {
        let (ultrawide, _) = visible_world_size(Viewport::new(0, 0, 2560, 1080), scale_at(1080));
        let (standard, _) = visible_world_size(Viewport::new(0, 0, 1920, 1080), scale_at(1080));
        assert_near(standard, 1280.0);
        assert_near(ultrawide, 2560.0 / 1.5);
    }

    #[test]
    fn viewports_tile_the_screen_at_any_size() {
        for (width, height) in SCREEN_SIZES {
            for count in 1..=4 {
                let views = split_screen_viewports(count, width, height);
                let area: i32 = views.iter().map(|view| view.width * view.height).sum();
                if count == 3 {
                    // Three views leave the fourth grid cell empty.
                    assert!(area < width * height);
                } else {
                    assert_eq!(area, width * height, "{count} views at {width}x{height}");
                }
                for view in &views {
                    assert!(view.x >= 0 && view.y >= 0);
                    assert!(view.x + view.width <= width && view.y + view.height <= height);
                }
            }
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss, reason = "test viewports are small")]
    fn each_split_view_centres_its_own_rig_at_any_size() {
        let rigs = [(1_000.0, 450.0), (3_000.0, 2_000.0)];
        for (width, height) in SCREEN_SIZES {
            let scale = scale_at(height);
            for (viewport, (x, y)) in split_screen_viewports(2, width, height)
                .into_iter()
                .zip(rigs)
            {
                let camera = camera_for(viewport, scale, x, y);
                let (visible_width, visible_height) = visible_world_size(viewport, scale);
                assert_near(visible_width.mul_add(0.5, camera.x), x);
                assert_near(visible_height.mul_add(0.5, camera.y), y);
                // The rig lands at the viewport's centre in screen points.
                assert_near((x - camera.x) * scale, viewport.width as f32 * 0.5);
                assert_near((y - camera.y) * scale, viewport.height as f32 * 0.5);
            }
        }
    }

    #[test]
    fn snapshot_cameras_use_the_screen_world_scale() {
        let rig = PlayerId::new(1);
        let world = WorldState::new_game(rig);
        let seats = [LocalSeat {
            seat: fatherload_netcode::SeatId(0),
            player: rig,
        }];
        let mut snapshot = RenderSnapshot::build(&world, &seats, (1920, 1080), (&[], &[]), &[]);
        let (x, y) = (1_000.0, 450.0);
        snapshot.apply_pose_overrides(&BTreeMap::from([(rig, (x, y))]));
        let view = snapshot.primary_view().expect("view");
        assert_near(view.world_scale, 1.5);
        assert_near(view.camera.x, x - 640.0);
        assert_near(view.camera.y, y - 360.0);
    }

    #[test]
    fn empty_split_cells_fill_the_rest_of_the_grid() {
        for count in [1, 2, 4] {
            assert!(empty_split_cells(count, SCREEN_WIDTH, SCREEN_HEIGHT).is_empty());
        }
        let half_width = SCREEN_WIDTH / 2;
        let half_height = SCREEN_HEIGHT / 2;
        assert_eq!(
            empty_split_cells(3, SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![Viewport::new(
                half_width,
                half_height,
                SCREEN_WIDTH - half_width,
                SCREEN_HEIGHT - half_height
            )]
        );
    }

    #[test]
    fn layout_handles_any_view_count_without_overlap() {
        for count in 1..=8 {
            let views = split_screen_viewports(count, SCREEN_WIDTH, SCREEN_HEIGHT);
            assert_eq!(views.len(), count);
            let area: i32 = views.iter().map(|view| view.width * view.height).sum();
            assert!(area <= SCREEN_WIDTH * SCREEN_HEIGHT);
            for (index, a) in views.iter().enumerate() {
                assert!(a.width > 0 && a.height > 0);
                assert!(a.x + a.width <= SCREEN_WIDTH && a.y + a.height <= SCREEN_HEIGHT);
                for b in &views[index + 1..] {
                    let disjoint = a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y;
                    assert!(disjoint, "{a:?} overlaps {b:?}");
                }
            }
        }
        assert_eq!(
            split_screen_viewports(1, SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![Viewport::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)]
        );
        assert_eq!(
            split_screen_viewports(2, SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![
                Viewport::new(0, 0, SCREEN_WIDTH / 2, SCREEN_HEIGHT),
                Viewport::new(SCREEN_WIDTH / 2, 0, SCREEN_WIDTH / 2, SCREEN_HEIGHT),
            ]
        );
    }
}

/// The world's cave critters (WP-CAVES), in id order.
fn critters_of(world: &WorldState) -> Vec<CritterPresentation> {
    world
        .entities()
        .of_kind(fatherload_simulation::entity::EntityKind::Critter)
        .filter_map(|entity| match entity.state {
            fatherload_simulation::entity::EntityState::Critter(state) => {
                Some(CritterPresentation {
                    id: entity.id.get(),
                    home_x: entity.body.x,
                    home_y: entity.body.y,
                    state,
                })
            }
            _ => None,
        })
        .collect()
}

/// A cave critter as presentation sees it: its home and state (its position now is
/// `state.offset_at(seconds)` from home).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CritterPresentation {
    pub id: u64,
    pub home_x: f32,
    pub home_y: f32,
    pub state: fatherload_simulation::entity::CritterState,
}
