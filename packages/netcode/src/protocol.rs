//! Wire protocol (plan §7) and the versioned postcard codec.
//!
//! Every encoded message is `[PROTOCOL_VERSION, postcard bytes...]`.
//! `ClientMessage::Inputs` is meant for unreliable datagrams; everything else
//! goes over the reliable ordered control stream.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::ids::{
    ActionId, ClientId, InputSeq, PlayerId, ProfileId, ReconnectToken, SessionEpoch, Tick,
};
use crate::params::{NetTuning, SessionParams};
use crate::simulation::{ActionEnvelope, NetSimulation, Pose};

/// Version byte prefixed to every encoded message.
///
/// 8 (expansion wave 1, one bump for every track): entity-store deltas and item containers
/// (`LiveEntities`, cargo bay / block hopper), streamed V5 terrain (chunk diffs, settings-only
/// keyframes, `x_remainder`), rig cosmetics and the garage (`RigBuild` tiers and paint,
/// event 44), test-mode no-clip flight (`TestModeState::no_clip`, `TestModeAction::SetNoClip`)
/// and the explosive kinds (`DeployExplosive`, `RemoteDetonate`, events 33-38).
///
/// 9 (expansion wave 2, one bump for every track):
/// * T7 drops: `DropState` pull / dump / blocked marks, `Player::collect_blocks`
///   (`PlayerInventory`), `DumpBlocks` / `SetCollectBlocks`, `SessionServiceRequest::SellBlocks`
///   and the pickup events 60-63;
/// * T13 caves: V6 cave-ecosystem tiles (`TileKind::Cave`), critter entities, found caves in
///   the POI journal and the cave events 70-72;
/// * T9 villages: generator V6 in the world settings, `FeatureKind::Village`,
///   `SessionServiceRequest::Village`, the village journal in the loadout, events 45-46;
/// * T10 enemies: `EntityState::Enemy` / `EnemyShot`, the `SectionKey::Enemies` section,
///   `SetEnemyDifficulty` and the enemy events 80-87;
/// * T11 weapons: explosive charges as entities (`LiveEntities` keeps only the charge serial),
///   explosive items in the cargo bay, smoke / scare / rocket kinds, weapon controls in
///   `ControlState`, weapon state in `PlayerTickState` / `RigBuild` / `PlayerInventory`,
///   `MountWeapon`, the weapon services and events 90-95;
/// * T12 base building: `PlayerCommand::Build`, built terrain tiles (`TileKind::Built`), the
///   `Bases` world field and section, and base events 100-102;
/// * WP-PERSIST: persistent profiles in `Hello`, `ClaimRig`, parked lobby entries,
///   `Welcome::returning`, `Reassigned` and `ClaimDenied`.
///
/// 10 (expansion wave 2, T8 surface): `GeneratorVersion::V7` biome worlds in the session
/// settings, `TileKind::Surface` materials (codes 0x500) in tile ops and chunk diffs, terrain
/// chunks above row 0 (mountains) and no flight ceiling.
///
/// 11 (expansion wave 3), one bump for all tracks:
///
/// * T14 world clock: the `ClockState` world field and `SectionKey::Clock` section,
///   `PlayerCommand::Sleep`, `ModuleService::Sleep`, `VillageDeal::Room` and the clock events
///   110-115;
/// * T16 sky: `GeneratorVersion::V8` sky worlds in the session settings, `TileKind::Sky` tiles
///   (codes 0x600) in tile ops and chunk diffs, terrain chunks up to the Moon, the altitude
///   intake and de-icer tiers in `HazardGear` (loadout), the sky regional features and the
///   altitude events 116-118;
/// * T15 pilot on foot: `ControlState::jump`, `EntityState::Pilot`,
///   `PlayerCommand::TogglePilot` / `PilotPack`, `SessionServiceRequest::BuyPilotGear`, pilot
///   gear and the personal pack in `PlayerInventory`, `BuiltTile::VentGrate`, `Fixture::Rope`,
///   the parked pilot in the crew roster and the pilot events 120-133;
/// * T17 people in the world: `EntityState::Npc` entities, `PlayerCommand::People`, the
///   `People` section (`PeopleLedger`) and people events 140-145;
/// * T18 upgrade reward moments: the milestone log in the loadout
///   (`PlayerInventory::milestones`), the milestone-reward cosmetics (new `RigCosmetics` option
///   variants in the rig build) and the `UpgradeInstalled` / `MilestoneEarned` events 180-181.
///
/// Version 12 (wave 4):
/// * T20 sky isles: `GeneratorVersion::V9` worlds, the new sky tiles (caches, nests, ruins,
///   springs, vines, keystones, vents, buoy pads), skystone and cloud silver ore and the sky
///   artifacts in tile codes and items, sky towns (`VillageSetting::Sky`, sky cultures and
///   roles, `VillageService` / `VillageDeal::SkyCraft`), `SessionServiceRequest::SkyBuoy`, the
///   aether thruster and pressurized hull tiers in `HazardGear` (loadout), drained buoys in the
///   village journal and the sky isle events 200-203;
/// * T21 sky continents and sky enemies: new
///   `SkyTile` codes (aether core, sky ores, temple stone and fittings, harpy nests and eggs),
///   new `ArtifactKind`s and `StrategicResourceKind`s in tiles and loadouts, the sky
///   `EnemyKind`s and `ShotKind::Cannonball` in entity records, the Act II `WeaponKind`s, the
///   tether line in `RigWeaponState` (per-tick weapon state) and the events 210-219;
/// * T22 the Moon: `GeneratorVersion::V9` worlds in the session settings, `TileKind::Lunar`
///   tiles (codes 0x700) in tile ops and chunk diffs, the Moon resources
///   (`StrategicResourceKind::{Helium3, MoonIce, AlienAlloy}`) and relics
///   (`ArtifactKind::{LunarRelic, WardenEye}`), the pressurized hull and life support tiers in
///   `HazardGear` and `LunarProgress` in the loadout, `PlayerCommand::Moon`, the Moon weapons
///   (`WeaponKind::{LunarCutter, LowGMortar, VoidLance}`), the Moon enemies
///   (`EnemyKind::{RegolithCrawler, CrystalMite, DerelictAutomaton, Warden}`),
///   `ModuleKind::LunarUplink` / `ModuleService::Beam` and the Moon events 220-229;
/// * T23 pilot content: `PlayerCommand::Pilot(PilotAction)`, `SessionServiceRequest::PilotArms`,
///   `VillageDeal::PilotArms`, the personal `WeaponKind`s and `AmmoKind`s, the new `PilotGear`
///   fields, `PilotState`'s arms / rope / recall fields and `PilotMode::Tether`, the pilot
///   arsenal in `PlayerInventory`, ruin `BuiltTile`s (codes 8-69 of the built range), plate
///   timers in `Bases`, `GeneratorVersion::V9` and the events 230-239;
/// * T24 contractor homes: the `Homes` world field and `SectionKey::Homes` section,
///   `PlayerCommand::Home`, `NpcState::site` (site crews), `BedSpot::Home`,
///   `BuildRefusal::OnPlot` and the home events 240-249;
/// * T25 night: the night monster kinds (`EnemyKind::ShadowHound` .. `UmbralAlpha`) and
///   `ShotKind::Wispfire` in entity records, the night materials
///   (`StrategicResourceKind::MothSilk` ..), `BuiltTile::Palisade` in tile ops,
///   `VillageDeal::SellNightGoods`, the `NightLedger` inside the enemies section and the night
///   events 250-253;
/// * T26 story arcs: the arc log in the quest log (loadout), the people ledger's story marks,
///   the `PeopleCommand` story commands (`AskWork`, `AcceptArc`, `TurnInArc`, `AbandonArc`),
///   the new objective primitives and story target places, and the arc events 260-265.
///
/// Version 13 (wave 5; the integrator collapses the wave's bumps into one):
/// * T27 tombs core: `GeneratorVersion::V10` worlds in the session settings, the tomb
///   `BuiltTile`s (codes 72-119 of the built range: masonry, pillars, doors, shrines, stairs,
///   the way out, caches, traps) in tile ops and chunk diffs, the `Tombs` world field and
///   `SectionKey::Tombs` section (runs, site counters, seals, unlocks, summaries),
///   `PlayerCommand::Tomb`, the tomb `Usable`s, `PilotState::{view, boons, air_jumps}`,
///   `ProjectileState::bounces`, the tomb key quest item and the tomb events 270-279;
/// * T28 side-view tombs: the side-view `TrapKind`s (trap tile kinds 1-7), the culture masonry
///   `BuiltTile::{TombWall, TombPillar}` sets 2-4 (built codes 120-125), `TombRun::{crumbling,
///   boss_phase}`, `RelicEffects::trap_pct` and `Relic::Sandwalker`, the tomb creatures
///   (`EnemyKind::Mummy` .. `HollowMatriarch`) and `ShotKind::{Arrow, Rubble, Hex}` in entity
///   records, and the side-view tomb events 280-281;
/// * T29 top-down crypts: `TombRun::crypt` (trap overlays and their state), the pilot's roll
///   and heading fields (`PilotState::{roll, roll_x, roll_y, roll_cooldown, heading_x,
///   heading_y}`), `EnemyState::{phase, pattern, volley}`, the crypt `EnemyKind`s
///   (`CryptSkeleton` .. `CryptLord`) and `ShotKind::{CryptHex, CryptArrow, Boulder}`, and the
///   crypt events 290-296.
///
/// Version 14 (wave 6; the integrator collapses the wave's bumps into one):
/// * Fixes: `SessionServiceRequest::Village` names the deal it buys (`deal: VillageDeal`, its
///   stable id) instead of the offer's row, so a purchase resolved on the tick dusk or dawn
///   restocks a counter can never buy the wrong item.
/// * W6 content: the new `Furniture` pieces and `Blueprint`s in the `Homes` section and home
///   commands, `SiteKey::SkyIsle` deeds, `PersonDeal::Furniture` rows, the Moon colony's
///   NPCs (`LUNAR_COLONY` character ids) and the new story arcs, places and objectives in the
///   arc log (loadout).
///
/// Version 15 (wave 7; the integrator collapses the wave's bumps into one):
/// * F7 world positions: the world's local pixel frame (`Frame`: sector origin and whether it
///   floats) in `LiveEntities` (sent every tick and applied first, so a frame move shifts the
///   replica before the frame's records) and in the terrain snapshot of keyframes, with a far
///   test world's column limit; `Pose::origin` and `Relevance::origin` (positions relative to
///   the frame's origin tile, compared through exact origin deltas).
/// * W7 strata (F8): `GeneratorVersion::V11` (the Deep Descent) in the session settings, and
///   the terrain keyframe's streamed edits as one packed byte stream
///   (`TerrainSnapshot::packed`, runs of equal tiles) instead of nested lists of tile codes.
/// * F9 field effects and craft rotation: `ControlState`'s rotation thrusters and assist
///   bits, `Player::attitude` / `PlayerTickState::attitude` (orientation and spin, per tick),
///   `HazardGear::attitude` (tilt rating, gyro, horizon hold) in loadouts, the `FieldZones`
///   world field and `SectionKey::Fields` section, and `TestModeAction::{PlaceFieldZone,
///   ClearFieldZones}`.
/// * F9 fluids: `TileKind::Fluid(FluidKind)` tiles in terrain ops, chunk refreshes and
///   keyframes (the durability byte is the fluid's level), the fluid events 380-383
///   (`FluidBreached`, `FluidReacted`, `RigSubmerged`, `RigSurfaced`) and
///   `TestModeAction::PourTestPools`.
/// * F10 depth logistics and progression: `PlayerCommand::Logistics(LogisticsCommand)`, the
///   `Logistics` world field and `SectionKey::Logistics` section (lifts, outposts, beacons,
///   rides, dives, dive charges, the progression rules), `HazardGear::rig_systems`, the eight
///   rig-system `UpgradeKind`s (shop offers 18-25), and the logistics events 390-399.
///
/// Version 16 (Wave 8, collapsed into one bump across its tracks). Progression:
/// * `LogisticsCommand::{OutpostUpgrade, Craft}` (the deep shops and workshops),
///   `ItemKind::Layer(Stratum)` (layer resources in bays, drops and events),
///   `UpgradeKind::{PressureRating, Headlamp}` (shop offers 26-27), `HazardGear::
///   {pressure_rating, headlamp}` in loadouts, and `Logistics::{felt_systems, haul_descent}`
///   in the logistics section. No new events.
/// * The authored Crust: `TileKind::Crust(CrustTile)` tiles and `ArtifactKind::ClayPot`,
///   `Player::cart` / `PlayerTickState::cart` (mine-cart rides, per tick), the `CrustState`
///   world field and `SectionKey::Crust` section (failing sets and shelves, the Sinkhole, the
///   Deep Claim, logs read), and the Crust events 400-415.
/// * The Old Deep (Descent II): `TileKind::Deep(DeepTile)` tiles (codes `0xD00`) in terrain ops,
///   chunk refreshes and keyframes, the Old Deep creatures (`EnemyKind::PrismMoth` ..
///   `MagmaSalamander`) in entity records, the `OldDeep` world field and `SectionKey::OldDeep`
///   section (dose meters, mutations, woken machine halls, skeleton digs, the Cathedral Lift,
///   the Star Core fight, the Glowmother, story logs read) and the Old Deep events 420-438.
/// * The Living Dark (Descent III): the seven Living Dark `CaveTile`s
///   (roots, flesh, comb, jelly cells, spore pods, acid glands, charred roots) in terrain ops and
///   keyframes, eighteen appended `EnemyKind`s and three `ShotKind`s, `ScreenEffect::Blur`
///   (`ScreenEffects::levels` sent as a list), `FieldPreset::Spores`, the `LivingDark` world
///   field and `SectionKey::LivingDark` section (root axes, the Hive's standing, errands and
///   Swarm, the Mycelial Mind's fight, the spore clouds' expiries),
///   `LogisticsCommand::LivingDark` (root axes and the Amber Court's counter),
///   `UpgradeLock::Boss` and the Living Dark events 440-459
///   (`GameplayEventKind::LivingDark`).
/// * The Fathers' expedition (story): the `ExpeditionState` world field and `SectionKey::Expedition` section (the
///   Fathers' expedition logs read, sealed canisters, echoes heard, anomaly rooms entered), the
///   story events 490-493 (`ExpeditionLogFound`, `ExpeditionLogSealed`, `ExpeditionEcho`,
///   `AnomalyEntered`), the story `Milestone`s and their `PaintColor` rewards.
///
/// Version 17 (Wave 9: Descents IV-VI; the integration collapses the six tracks' bumps into
/// this one):
/// * The Drowned and Frozen (Descent IV): thirteen appended `CaveTile`s, eleven appended
///   `EnemyKind`s and `ShotKind::PressureBubble`, `Player::water` / `PlayerTickState::water`
///   (ballast and hull frost, per tick), the `DrownedState` world field and
///   `SectionKey::Drowned` section, `Culture::Merrow`, five appended `Furniture` pieces and the
///   Drowned events 520-539 (`GameplayEventKind::Drowned`).
/// * The Hollow World (Descent V): `TileKind::Hollow(HollowTile)` tiles (codes `0x900`) in
///   terrain ops, chunk refreshes and keyframes, thirteen appended `EnemyKind`s (`SkyManta` ..
///   `SerpentCoil`) in entity records, the `Hollow` world field and `SectionKey::Hollow` section
///   (breakthroughs, exposure, Gravity Anchors, rides, isle landings, the Sun Serpent's fight),
///   `LogisticsCommand::Hollow` (the Gravity Anchor), and the Hollow World events 540-559
///   (`GameplayEventKind::Hollow`).
/// * The distortion strata (Descent VI part A): `TileKind::Warped(WarpedTile)` tiles (codes
///   `0x1600`) in terrain ops and keyframes, nine appended `EnemyKind`s (mirror twin to dead
///   pixel) and two `ShotKind`s (`MirrorShard`, `RotPulse`), `ScreenEffect::Glitch`, the
///   `Distortion` world field and `SectionKey::Distortion` section (gazers' holds, rot pulses,
///   the mirror halls' twin cooldowns, landmarks found) and the distortion events 560-579
///   (`GameplayEventKind::Distortion`, `DistortionLandmarkFound`).
/// * The far Strange Layers (Descent VI part B and its gate): six appended `CaveTile`s (codes
///   35-40: echo quartz, the red, green and blue keyed walls, builder alloy, chronoglass) in
///   terrain ops and keyframes, eleven appended `EnemyKind`s (`TickMite` .. `GlitchShard`) and
///   four `ShotKind`s (`PrismRed`, `PrismGreen`, `PrismBlue`, `GlitchBolt`) in entity records,
///   `ExplosiveKind::TimeBubble` (item id 11) in charges, bays and commands, the `FarStrange`
///   world field and `SectionKey::FarStrange` section (lamp filters, sonar pings, amber,
///   landmarks found, The Glitch's fight), `LogisticsCommand::FarStrange` (lamp filters, time
///   bubbles) and the far Strange Layers events 580-599 (`GameplayEventKind::FarStrange`).
/// * The story: the four deeper `AnomalyKind`s (`Sideways`, `Sunlit`, `Weightless`, `Lensing`)
///   in `AnomalyEntered` events, and NPC entities of Cathedral Station's and the Amber Court's
///   residents (their `CharacterId`s name the fixed `people::deep_folk` settlement ids).
/// * Progression: `LogisticsCommand::BuySupply` (the deep supplies counter),
///   `CraftOutput::Supply` recipes (no wire change: recipes are ids), and `Logistics::supplies`
///   (each player's supply kit of `workshop::DeepSupply`) in the logistics section.
/// * The integration: no wire change (the Sonar Suite's one API reads the two sections above).
///
/// Version 18 (Wave 10; the integrator collapses the tracks' bumps). The polish track:
/// `Logistics::supply_effects` (blowing ballast, flare buoys and beast bait in the world,
/// fitted lamp filter glass) in the logistics section, the supply events 720-739
/// (`GameplayEventKind::Supply`), the appended `FarStrangeRefusal::NoFilterGlass`, and the
/// appended `BuildingStyle::{Coral, Jungle, Machine}` in village records.
///
/// Version 18 (wave 10 ancients-a, Descent VII strata 1-3, the builders' buried city):
/// * `TileKind::City` (the `0x1C00` family: city stone, tower glass, tram rails, platforms,
///   lamps, alloy ore, lift rails, archivist cores, engine plating, belts, piston housings,
///   engine core ore, pipes, archive shelving and stone, star glass and chart plinths) in
///   terrain ops and keyframes;
/// * nine appended `EnemyKind`s (`MaintenanceDrone` .. `IndexSpider`) and two `ShotKind`s
///   (`SentinelBolt`, `ReadingBeam`) in entity records;
/// * the `AncientCity` world field and `SectionKey::AncientCity` section (rides on trams, lifts
///   and pistons, the Megacity's alarms, every player's archivist, the star charts read, vaults
///   opened, landmarks found);
/// * the buried city's events 630-649 (`GameplayEventKind::AncientCity`, with a `detail`
///   number);
/// * the Archive Post's residents (a third deep settlement) in NPC entities.
///
/// Wave 10 ancients-b (Descent VII part B: the Sleeping Garrison, the Seal Gate and the
/// Gatekeeper Colossus), one bump for the track (the integrator collapses Wave 10's):
/// * twelve appended `CaveTile`s (codes 160-171: war plate, war bays, hold pylons, barricades,
///   seal stone, the seal door, the Heart Seal, seal gold, collapse rubble, the cracked floor
///   and vault, key altars) in terrain ops and keyframes;
/// * nine appended `EnemyKind`s (`WarSentry` .. `ColossusLegs`) and five `ShotKind`s
///   (`SentryBolt` .. `FallingStone`) in entity records;
/// * `Buildable::Defence` (turrets, barricades, shock traps) in build commands and `BasePlaced`
///   events, two appended `BuildRefusal`s;
/// * the `Garrison` world field and `SectionKey::Garrison` section (forts and waves, defences,
///   the Seal Gate, the Colossus's fight, falling stones, landmarks found);
/// * the Descent VII part B events 650-669 (`GameplayEventKind::Garrison`).
///
/// Version 18 (Wave 10: Descents VII-VIII and the ending; the integration collapses the
/// tracks' bumps into this one):
/// * The Heart (Descent VIII): `TileKind::Core(CoreTile)` tiles (codes `0x1A00`) in terrain
///   ops, chunk refreshes and keyframes, fourteen appended `EnemyKind`s (`NullMoth` ..
///   `HeartSpark`) in entity records, `ScreenEffect` levels unchanged, the `Heart` world field
///   and `SectionKey::Heart` section (floating rigs, wells, landmarks found, the Heart's fight,
///   `space_unlocked`, the Star Drives, the ending's start), and the Heart events 670-699
///   (`GameplayEventKind::Heart`, `HeartLandmarkFound`).
///
/// Version 18 (Wave 10; each track bumps at most once and the integration collapses them):
/// * The story (w10-story): `ExpeditionState::{descents, bosses, cable_lit}` in the shared
///   `Expedition` section, the story events 700-719 (`GameplayEventKind::Story` with
///   `expedition::progress::StoryEvent`), the appended finale `Milestone`s and their
///   `PaintColor` rewards, the appended deep-settlement story places
///   (`StoryPlace::{FathersSite, DeepNeighbour}`), `ArcKind::Deep` and the deep settlements'
///   arcs in the arc log.
pub const PROTOCOL_VERSION: u8 = 18;

/// One client input frame: the controls and actions for exactly one tick.
///
/// The client predicts exactly the frame it sends (`target_tick` is the tick
/// the client simulated with it).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct InputFrame<S: NetSimulation> {
    /// Strictly increasing per client.
    pub seq: InputSeq,
    /// The tick this input is for (the tick produced by simulating it).
    pub target_tick: Tick,
    /// Held controls.
    pub controls: S::Controls,
    /// One-shot actions issued on this tick.
    pub actions: Vec<ActionEnvelope<S::Action>>,
}

/// An unacknowledged action resent outside its (no longer redundant) frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct TaggedAction<S: NetSimulation> {
    /// Sequence of the frame it was issued in.
    pub seq: InputSeq,
    /// Tick it was issued for.
    pub target_tick: Tick,
    /// The action.
    pub envelope: ActionEnvelope<S::Action>,
}

/// Client to host.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub enum ClientMessage<S: NetSimulation> {
    /// First message on a connection.
    Hello {
        /// Token from a previous `Welcome`, to reclaim the same player.
        token: Option<ReconnectToken>,
        /// Persistent install identity: a host whose world holds a rig for this profile (live,
        /// reserved or parked from an earlier session) hands it back.
        profile: Option<ProfileId>,
        /// Display name.
        name: String,
        /// Must equal [`PROTOCOL_VERSION`].
        version: u8,
    },
    /// Input datagram: newest frame plus redundant unacknowledged frames.
    Inputs {
        /// Epoch the inputs belong to; stale epochs are rejected.
        epoch: SessionEpoch,
        /// Newest frame plus up to `input_redundancy - 1` older unacked frames.
        frames: Vec<InputFrame<S>>,
        /// Unacknowledged actions whose frames are no longer carried.
        actions: Vec<TaggedAction<S>>,
        /// Every action id below this is settled from the client's view.
        action_floor: ActionId,
    },
    /// Lobby ready toggle.
    Ready {
        /// Ready state.
        ready: bool,
    },
    /// Ask for a full keyframe (e.g. after a debug hash mismatch).
    KeyframeRequest {
        /// Current epoch.
        epoch: SessionEpoch,
    },
    /// Leave the session (no reconnect reservation; a player with a profile is parked).
    Leave,
    /// Ask the host to hand over a parked (saved, offline) player's rig (playing from a new
    /// machine or after clearing storage). The host approves or denies.
    ClaimRig {
        /// The parked player to claim.
        player: PlayerId,
    },
}

impl<S: NetSimulation> ClientMessage<S> {
    /// Whether this message belongs on the unreliable datagram channel.
    #[must_use]
    pub const fn is_unreliable(&self) -> bool {
        matches!(self, Self::Inputs { .. })
    }
}

/// Lobby roster entry.
#[allow(clippy::struct_excessive_bools, reason = "independent roster flags")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyEntry {
    /// The player.
    pub player: PlayerId,
    /// Display name.
    pub name: String,
    /// Ready toggle.
    pub ready: bool,
    /// `false` while the player's client is disconnected but reserved.
    pub connected: bool,
    /// Whether the player is a local seat on the host.
    pub local: bool,
    /// A saved player who is not connected: their rig is parked outside the live world, waiting
    /// for them to rejoin (or be claimed).
    pub parked: bool,
}

/// Position-only data for players outside the relevance radius (minimap).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerMarker {
    /// The player.
    pub player: PlayerId,
    /// Its pose.
    pub pose: Pose,
}

/// A gameplay event with its stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetEvent<E> {
    /// Tick that produced it.
    pub tick: Tick,
    /// Owning player, if any.
    pub player: Option<PlayerId>,
    /// Input sequence the host applied for `player` on that tick, if any.
    pub seq: Option<InputSeq>,
    /// Event type discriminant.
    pub kind: u32,
    /// Index among events with the same `(player, kind)` on that tick.
    pub ordinal: u32,
    /// The event.
    pub payload: E,
}

/// Why the host refused a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// Protocol versions differ.
    VersionMismatch {
        /// Host version.
        expected: u8,
        /// Client version.
        found: u8,
    },
    /// The reconnect token is currently owned by another live connection.
    TokenInUse,
    /// The session is no longer accepting players.
    NotAccepting,
}

/// Why the session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// The host closed the session.
    HostClosed,
    /// The host removed this client.
    Kicked,
    /// This client left on its own (local only; never sent by the host).
    Left,
}

/// Initial state for a newly joined client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Welcome<S: NetSimulation> {
    /// Transport connection id.
    pub client: ClientId,
    /// Assigned (or reclaimed) player.
    pub player: PlayerId,
    /// Current epoch.
    pub epoch: SessionEpoch,
    /// Token to reclaim `player` after a disconnect.
    pub token: ReconnectToken,
    /// Host tick at the time of the keyframe.
    pub host_tick: Tick,
    /// `false` while in the lobby (wait for `StartGame`).
    pub started: bool,
    /// Fixed session parameters.
    pub session_params: SessionParams,
    /// This client's network tuning.
    pub tuning: NetTuning,
    /// Full world at `host_tick`.
    pub keyframe: S::Keyframe,
    /// The player is a returning one whose saved rig the host restored.
    pub returning: bool,
}

/// The host moved this client to another player (an approved rig claim, or a loaded save that
/// holds this client's saved rig under another id). Replaces the client's world like a keyframe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Reassigned<S: NetSimulation> {
    /// The client's player from now on.
    pub player: PlayerId,
    /// Token to reclaim `player` after a disconnect.
    pub token: ReconnectToken,
    /// Epoch of `keyframe` (the current one, or a new one after a load).
    pub epoch: SessionEpoch,
    /// Tick of `keyframe`.
    pub tick: Tick,
    /// Full world as `player` sees it.
    pub keyframe: S::Keyframe,
    /// The player got a saved rig back.
    pub returning: bool,
}

/// Host to client, per frame (plan §7).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct ServerFrame<S: NetSimulation> {
    /// Epoch.
    pub epoch: SessionEpoch,
    /// Host tick this frame describes.
    pub tick: Tick,
    /// Highest input sequence consumed by the simulation for this client.
    pub ack_seq: Option<InputSeq>,
    /// Every action id at or below this is applied (or settled).
    pub ack_action: ActionId,
    /// Minimum `(target tick of newest input) - (host tick at arrival)` since
    /// the previous frame.
    pub input_margin: Option<i32>,
    /// Sequence of the newest input the margin was measured from.
    pub margin_seq: InputSeq,
    /// Full player roster, only when it changed for this client.
    pub roster: Option<Vec<PlayerId>>,
    /// Per-tick state of relevant players (always includes the viewer).
    pub players: Vec<(PlayerId, S::PlayerState)>,
    /// Low-rate markers for non-relevant players, when due.
    pub far_players: Option<Vec<PlayerMarker>>,
    /// Relevance-filtered per-tick entities.
    pub entities: S::Entities,
    /// Incrementally replicated entities that are new to this client or changed since its
    /// previous frame (relevance-filtered).
    pub entity_updates: Vec<S::EntityRecord>,
    /// Incrementally replicated entities this client must drop (gone, or out of relevance).
    pub entity_despawns: Vec<S::EntityKey>,
    /// Op-log entries since the previous frame, in order (only those in
    /// chunks relevant to this client).
    pub ops: Vec<(Tick, S::Op)>,
    /// Current contents of chunks whose ops this client skipped while they
    /// were out of relevance and that are relevant again. Applied after `ops`.
    pub chunk_refreshes: Vec<(S::Chunk, S::ChunkSnapshot)>,
    /// On-change sections whose hash changed.
    pub sections: Vec<(S::SectionKey, S::Section)>,
    /// On-change sections that no longer exist.
    pub removed_sections: Vec<S::SectionKey>,
    /// Gameplay events since the previous frame.
    pub events: Vec<NetEvent<S::Event>>,
    /// The host world's hash as visible to this client after this tick
    /// ([`crate::NetSimulation::visible_state_hash`]; debug builds / harness).
    pub debug_hash: Option<u64>,
}

/// Host to client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub enum ServerMessage<S: NetSimulation> {
    /// Join accepted.
    Welcome(Box<Welcome<S>>),
    /// Per-frame replication.
    Frame(Box<ServerFrame<S>>),
    /// Lobby roster and ready states.
    Lobby {
        /// Roster.
        roster: Vec<LobbyEntry>,
    },
    /// The game started at `tick`.
    StartGame {
        /// Epoch.
        epoch: SessionEpoch,
        /// Tick the host starts simulating from.
        tick: Tick,
    },
    /// Full world (on request or after an epoch change).
    Keyframe {
        /// Epoch.
        epoch: SessionEpoch,
        /// Tick of `world`.
        tick: Tick,
        /// Full world.
        world: Box<S::Keyframe>,
    },
    /// This client's tuning changed.
    TuningChanged {
        /// New tuning.
        tuning: NetTuning,
    },
    /// Connection refused.
    Rejected {
        /// Why.
        reason: RejectReason,
    },
    /// The session is over.
    SessionEnded {
        /// Why.
        reason: EndReason,
    },
    /// This client now plays another player (see [`Reassigned`]).
    Reassigned(Box<Reassigned<S>>),
    /// The host turned down this client's [`ClientMessage::ClaimRig`].
    ClaimDenied {
        /// The rig that was asked for.
        player: PlayerId,
    },
}

/// Codec failure.
#[derive(Debug)]
pub enum CodecError {
    /// Zero-length message.
    Empty,
    /// The version byte did not match [`PROTOCOL_VERSION`].
    Version {
        /// Expected version.
        expected: u8,
        /// Received version.
        found: u8,
    },
    /// Postcard serialization or deserialization error.
    Postcard(postcard::Error),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty message"),
            Self::Version { expected, found } => {
                write!(
                    f,
                    "protocol version mismatch: expected {expected}, found {found}"
                )
            }
            Self::Postcard(error) => write!(f, "postcard: {error}"),
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Postcard(error) => Some(error),
            _ => None,
        }
    }
}

impl From<postcard::Error> for CodecError {
    fn from(error: postcard::Error) -> Self {
        Self::Postcard(error)
    }
}

/// Encodes a message as `[PROTOCOL_VERSION, postcard...]`.
///
/// # Errors
///
/// Returns [`CodecError::Postcard`] if serialization fails.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, CodecError> {
    Ok(postcard::to_extend(message, vec![PROTOCOL_VERSION])?)
}

/// Decodes a message produced by [`encode`].
///
/// # Errors
///
/// Returns an error for empty input, a version mismatch or malformed bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    let (&version, body) = bytes.split_first().ok_or(CodecError::Empty)?;
    if version != PROTOCOL_VERSION {
        return Err(CodecError::Version {
            expected: PROTOCOL_VERSION,
            found: version,
        });
    }
    Ok(postcard::from_bytes(body)?)
}
