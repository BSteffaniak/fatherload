//! Where the expedition left its logs.
//!
//! One site per log, placed by the V11 generator as its own pass ([`site_tile`], called from the streamed assembly after the Descent landmarks and
//! before the builders' cable).
//!
//! The Fathers followed the builders' cable down, so every site lies beside it: on the far side
//! of the ancient lift rail from the town (24 to 120 columns out), in its log's stratum. A
//! stratum with `k` logs splits its inner rows (clear of the ragged boundaries) into `k` slots
//! and log `j` lies in slot `j`, so every log is deeper than the one before it
//! (`logs_lie_deeper_in_order`): a rig digging down meets the trail in order, and the cipher
//! canisters make sure it is read in order anyway ([`super::ExpeditionState`]).
//!
//! Everything here is a pure function of the world settings.

use std::cell::RefCell;

use crate::{
    base::BuiltTile,
    descent::Stratum,
    terrain::{TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        descent::{DescentLayout, landmarks::lift_column},
        noise::hash2,
    },
};

use super::{LOGS, SiteKind};

const SALT_SITE: u64 = 0x5745_0801;

/// Rows kept clear of a stratum's ragged boundaries (above and below its layout rows).
const BOUNDARY_CLEARANCE: i32 = 24;

/// One log's site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpeditionSite {
    /// Index into [`LOGS`] (0-based; log number `log + 1`).
    pub log: u8,
    pub kind: SiteKind,
    pub stratum: Stratum,
    /// Every tile the site carves, walls included.
    pub bounds: TileRect,
}

impl ExpeditionSite {
    /// The open room inside the walls: where a rig or pilot finds the canister.
    #[must_use]
    pub const fn interior(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.bounds.min.x + 1,
                y: self.bounds.min.y + 1,
            },
            max: TilePosition {
                x: self.bounds.max.x - 1,
                y: self.bounds.max.y - 1,
            },
        }
    }

    /// The plank floor row.
    #[must_use]
    pub const fn floor_row(&self) -> i32 {
        self.bounds.max.y
    }

    /// Whether `tile` is inside the room.
    #[must_use]
    pub const fn holds(&self, tile: TilePosition) -> bool {
        self.interior().contains(tile)
    }

    /// The tile the canister rests on the floor at (where the renderer draws it).
    #[must_use]
    pub const fn canister(&self) -> TilePosition {
        let interior = self.interior();
        let x = match self.kind {
            // Camps keep it on the crate by the tent's door; wrecks in the cab; cairns on top.
            SiteKind::Camp => interior.max.x - 2,
            SiteKind::Rig => interior.min.x + 2,
            SiteKind::Marker => i32::midpoint(interior.min.x, interior.max.x),
            // At the grave's foot, beside the headstone.
            SiteKind::Grave => interior.max.x - 1,
        };
        TilePosition {
            x,
            y: interior.max.y,
        }
    }

    /// Rows of rock stamped under the floor (Wave 9): in the Hollow World's open skies (the
    /// Inner Sky, and since the Wave 9 integration the isles' sea and the Sunforge's sky) a site
    /// stands on a little floating stone of its own instead of a plank hanging in the air.
    #[must_use]
    pub const fn footing_rows(&self) -> i32 {
        if matches!(
            self.stratum,
            Stratum::InnerSky | Stratum::UpsideDownIsles | Stratum::TheSunforge
        ) {
            FOOTING_ROWS
        } else {
            0
        }
    }

    /// Everything the site stamps: its bounds and any footing under them.
    #[must_use]
    pub const fn extent(&self) -> TileRect {
        TileRect {
            min: self.bounds.min,
            max: TilePosition {
                x: self.bounds.max.x,
                y: self.bounds.max.y + self.footing_rows(),
            },
        }
    }

    /// Whether `tile` is part of the floating stone under the floor: each row down is a
    /// tile narrower at both ends, so the stone tapers like a keel.
    #[must_use]
    pub const fn in_footing(&self, tile: TilePosition) -> bool {
        let below = tile.y - self.bounds.max.y;
        below >= 1
            && below <= self.footing_rows()
            && tile.x >= self.bounds.min.x + below
            && tile.x <= self.bounds.max.x - below
    }
}

/// Rows of floating stone under a site in the open sky ([`ExpeditionSite::footing_rows`]).
pub const FOOTING_ROWS: i32 = 3;

/// `(width, height)` of a site of `kind`, walls and floor included.
#[must_use]
pub const fn site_size(kind: SiteKind) -> (i32, i32) {
    match kind {
        SiteKind::Camp => (15, 7),
        SiteKind::Rig => (12, 6),
        SiteKind::Marker => (8, 6),
        SiteKind::Grave => (10, 6),
    }
}

/// The side of the cable the sites lie on: away from the town (`1` east, `-1` west).
#[must_use]
pub const fn trail_side(settings: &WorldSettings) -> i32 {
    if lift_column(settings) >= SPAWN_TILE_X {
        1
    } else {
        -1
    }
}

/// The placement hook of one stratum: the logs left in `stratum`, as `(log index, slot,
/// slots)`.
pub fn placements_for(stratum: Stratum) -> impl Iterator<Item = (u8, usize, usize)> {
    let slots = LOGS.iter().filter(|log| log.stratum == stratum).count();
    LOGS.iter()
        .enumerate()
        .filter(move |(_, log)| log.stratum == stratum)
        .enumerate()
        .map(move |(slot, (index, _))| (u8::try_from(index).unwrap_or(u8::MAX), slot, slots))
}

/// The site of log `log` (index into [`LOGS`]) in a world with `settings` and `layout`.
#[must_use]
pub fn site(settings: &WorldSettings, layout: &DescentLayout, log: u8) -> Option<ExpeditionSite> {
    let entry = LOGS.get(usize::from(log))?;
    let (slot, slots) = placements_for(entry.stratum)
        .find(|(index, _, _)| *index == log)
        .map(|(_, slot, slots)| (slot, slots))?;
    let (width, height) = site_size(entry.site);
    let (top, bottom) = layout.rows(entry.stratum);
    let first = top + BOUNDARY_CLEARANCE;
    let last = bottom - BOUNDARY_CLEARANCE - height;
    let slots = i32::try_from(slots).unwrap_or(1).max(1);
    let span = (last - first).max(slots);
    let slot = i32::try_from(slot).unwrap_or(0);
    let slot_top = first + span * slot / slots;
    let slot_rows = (span / slots - height).max(1);
    let roll = hash2(settings.seed, SALT_SITE, i32::from(log), 0);
    let y = slot_top + i32::try_from(roll % u64::try_from(slot_rows).unwrap_or(1)).unwrap_or(0);
    let side = trail_side(settings);
    let at = |out: i32, y: i32| {
        let column = lift_column(settings) + side * out;
        let x = if side > 0 { column } else { column - width + 1 };
        ExpeditionSite {
            log,
            kind: entry.site,
            stratum: entry.stratum,
            bounds: TileRect {
                min: TilePosition { x, y },
                max: TilePosition {
                    x: x + width - 1,
                    y: y + height - 1,
                },
            },
        }
    };
    // The rolled distance out from the cable, or the next of a few further along the row where
    // the site would cut a pilot-only ruin, sit under a tomb entrance or cut one of the strata's
    // set pieces (Wave 8 integration); then the same tries on two other rows of its slot; then
    // (Wave 9 integration: the Strange Layers' halls fill whole rows) three more rows and further
    // out (kept as rolled if every try is taken, which the placement tests rule out for their
    // seeds). The later tries only follow the earlier ones, so a site the first tries placed
    // stays where it was.
    let rolled = i32::try_from((roll >> 20) % 97).unwrap_or(0);
    let first_try = at(24 + rolled, y);
    let rows = [
        y,
        slot_top + (y - slot_top + slot_rows / 3) % slot_rows,
        slot_top + (y - slot_top + 2 * slot_rows / 3) % slot_rows,
    ];
    let more_rows =
        [1, 3, 5].map(|sixth| slot_top + (y - slot_top + sixth * slot_rows / 6) % slot_rows);
    rows.into_iter()
        .flat_map(|row| (0..8).map(move |attempt| (row, attempt)))
        .map(|(row, attempt)| at(24 + (rolled + attempt * 29) % 97, row))
        .chain(
            rows.into_iter()
                .chain(more_rows)
                .flat_map(|row| (0..12).map(move |attempt| (row, attempt)))
                .map(|(row, attempt)| at(24 + (rolled + attempt * 37) % 180, row)),
        )
        .find(|site| {
            let around = site.extent().expanded(3);
            !crosses_older_spaces(settings, &around)
                && !crosses_strata_set_pieces(settings, &around)
        })
        .or(Some(first_try))
}

/// Whether `area` crosses one of the authored strata's set pieces (Wave 8 integration).
///
/// They are the Crust's cabin, Gravel Hollow, the Sinkhole and its mine works; the Old Deep's
/// Cathedral, Star Core arena, Glowmother's den and Fathers' camps; the Living Dark's stamps.
/// The log sites keep off them (the story pass is stamped after them and would cut them open).
#[must_use]
pub fn crosses_strata_set_pieces(settings: &WorldSettings, area: &TileRect) -> bool {
    use crate::{
        crust,
        old_deep::sites as deep,
        worldgen::descent::{crust as crust_rules, living},
    };
    if !settings.generator.has_descents() {
        return false;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let rect = |min: (i32, i32), max: (i32, i32)| TileRect {
        min: TilePosition { x: min.0, y: min.1 },
        max: TilePosition { x: max.0, y: max.1 },
    };
    let mut pieces: Vec<TileRect> = Vec::new();
    pieces.extend(crust::cabin(settings));
    if let Some((center, top)) = crust::gravel_hollow(settings) {
        pieces.push(rect((center - 40, top - 6), (center + 40, top + 14)));
    }
    // Parallax Station's settlement (Wave 9).
    if let Some((center, top)) = crate::distortion::parallax_station(settings) {
        pieces.push(rect((center - 40, top - 6), (center + 40, top + 14)));
    }
    pieces.extend(crust::sinkhole_site(settings).map(|site| site.zone));
    pieces.extend(deep::cathedral(settings, &layout).map(|site| site.bounds));
    pieces.extend(deep::star_core_arena(settings, &layout).map(|site| site.bounds));
    if let Some((centre, (w, h))) = deep::glowmother_den(settings, &layout) {
        pieces.push(rect(
            (centre.x - w, centre.y - h),
            (centre.x + w, centre.y + h),
        ));
    }
    for stratum in crate::descent::Descent::OldDeep.strata() {
        if let Some(camp) = deep::expedition_camp(settings, &layout, stratum) {
            pieces.push(rect((camp.x - 3, camp.y - 3), (camp.x + 3, camp.y + 1)));
        }
    }
    // The distortion strata (Wave 9): the Looking-Glass Hall, the Shear Spire, the Dead
    // Screen, and every mirror and shear hall.
    {
        use crate::distortion::{mirror, rot, shear};
        pieces.extend(
            mirror::looking_glass_hall(settings, &layout).map(|hall| hall.bounds.expanded(2)),
        );
        pieces.extend(shear::spire(settings, &layout).map(|spire| spire.expanded(2)));
        pieces.extend(rot::dead_screen(settings, &layout).map(|screen| screen.expanded(2)));
        pieces.extend(
            mirror::zones_touching(settings, &layout, area)
                .iter()
                .map(|zone| zone.bounds.expanded(2)),
        );
        pieces.extend(
            shear::regions_touching(settings, &layout, area)
                .iter()
                .map(|region| region.bounds.expanded(2)),
        );
    }
    pieces.iter().any(|piece| piece.intersects(area))
        || crust_rules::works_touching(settings, &layout, area)
            .iter()
            .any(|works| works.bounds().intersects(area))
        || living::stamps_with(settings, &layout, area)
            .iter()
            .any(|stamp| stamp.bounds.intersects(area))
        // The Hollow World's caverns and set pieces (Wave 9): a room would float in the air.
        || crate::worldgen::descent::hollow::crosses_set_pieces(settings, &layout, area)
        // The far Strange Layers' set pieces (Wave 9 strange-b), with a margin round them.
        || crate::worldgen::descent::far_strange::stamps_with(settings, &layout, &area.expanded(2))
            .iter()
            .any(|stamp| stamp.bounds.expanded(2).intersects(area))
        // ... and the alloy seal over Descent VII: a room carved through it would be a way
        // past The Glitch without the Phase Drill.
        || crate::worldgen::descent::far_strange::crosses_alloy_seal(settings, &layout, area)
        || DEEPER_SET_PIECES
            .iter()
            .any(|crosses| crosses(settings, area))
}

/// A set-piece query of a deeper Descent's track: whether `area` crosses one of its stamps.
pub type SetPieceQuery = fn(&WorldSettings, &TileRect) -> bool;

/// The deeper Descents' set pieces the story's rooms keep off (Wave 9 seam).
///
/// Every Descent's own landmarks (outposts, gate chambers, the Descent IV-VIII settlements
/// the registry places) are here from the start. A Descent track that stamps a settlement,
/// arena or set piece of its own appends its query (Merrow City's bubble, the jungle tribes'
/// village, the Sun Serpent's orbit, the Prism Relay ...); the log sites then move along their
/// rows and the anomaly rooms are dropped, exactly as for Descents I-III.
pub const DEEPER_SET_PIECES: &[SetPieceQuery] = &[crosses_descent_landmarks, crosses_drowned];

/// Whether `area` crosses one of the Drowned and Frozen's set pieces (Wave 9 integration):
/// Merrow City's bubble, the eel-dragon's grotto, the drowned builder sub, Old Faithful, the
/// Leviathan's arena, and the basins, galleries and chimneys its regions stamp.
fn crosses_drowned(settings: &WorldSettings, area: &TileRect) -> bool {
    use crate::worldgen::descent::drowned;
    let near = area.expanded(2);
    if drowned::set_piece_bounds(settings)
        .iter()
        .any(|bounds| bounds.intersects(&near))
    {
        return true;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    drowned::stamps_with(settings, &layout, &near)
        .iter()
        .any(|stamp| stamp.bounds.intersects(&near))
}

/// Whether `area` crosses one of the registry's Descent landmarks (in Descents IV-VIII only,
/// so the placement of Descents I-III's logs stays as Wave 8 left it).
fn crosses_descent_landmarks(settings: &WorldSettings, area: &TileRect) -> bool {
    use crate::{descent::Descent, worldgen::descent::landmarks};
    let layout = crate::descent::fields::cached_layout(settings);
    landmarks::landmarks_with(settings, &layout, area)
        .iter()
        .any(|landmark| landmark.descent >= Descent::DrownedAndFrozen)
}

/// Whether `area` crosses a pilot-only space (V9+, stamped before the story pass) or a tomb
/// entrance (V10+, stamped after it): the story's rooms keep off both.
#[must_use]
pub fn crosses_older_spaces(settings: &WorldSettings, area: &TileRect) -> bool {
    use crate::worldgen::streamed::{pilot_spaces, tomb_sites};
    pilot_spaces::spaces_touching(settings, area)
        .iter()
        .any(|space| space.bounds().intersects(area))
        || tomb_sites::sites_touching(settings, area)
            .iter()
            .any(|tomb| {
                tomb_sites::entrance_cells(tomb)
                    .iter()
                    .any(|(tile, _)| area.contains(*tile))
            })
}

thread_local! {
    static SITES: RefCell<Option<(WorldSettings, Vec<ExpeditionSite>)>> =
        const { RefCell::new(None) };
}

/// Every log site of a world (empty for worlds without Descents), in log order. Cached per
/// thread (a pure function, so the cache never changes an answer).
#[must_use]
pub fn sites(settings: &WorldSettings) -> Vec<ExpeditionSite> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    SITES.with(|cache| {
        if let Some((cached, sites)) = cache.borrow().as_ref()
            && cached == settings
        {
            return sites.clone();
        }
        let layout = crate::descent::fields::cached_layout(settings);
        let sites: Vec<ExpeditionSite> = (0..LOGS.len())
            .filter_map(|log| site(settings, &layout, u8::try_from(log).ok()?))
            .collect();
        *cache.borrow_mut() = Some((*settings, sites.clone()));
        sites
    })
}

/// The sites whose stamps ([`ExpeditionSite::extent`]) overlap `area`.
#[must_use]
pub fn sites_in(settings: &WorldSettings, area: &TileRect) -> Vec<ExpeditionSite> {
    let mut found = sites(settings);
    found.retain(|site| site.extent().intersects(area));
    found
}

/// The site whose room holds `tile`, if any.
#[must_use]
pub fn site_holding(settings: &WorldSettings, tile: TilePosition) -> Option<ExpeditionSite> {
    sites(settings).into_iter().find(|site| site.holds(tile))
}

/// The tile `site` carves at `(x, y)`, or `None` to leave the generated tile.
///
/// The room is open air on a plank floor (the crew always laid boards); its walls stay the
/// stratum's own rock. A wreck rests on a floor of its own scrap plating. In the open sky the
/// floor rests on a tapering stone of the stratum's hard rock ([`ExpeditionSite::in_footing`]).
#[must_use]
pub const fn site_tile(site: &ExpeditionSite, x: i32, y: i32) -> Option<TileKind> {
    let position = TilePosition { x, y };
    if site.in_footing(position) {
        return Some(site.stratum.def().hard);
    }
    if !site.bounds.contains(position) {
        return None;
    }
    if site.interior().contains(position) {
        return Some(TileKind::Air);
    }
    if y == site.bounds.max.y {
        return Some(match site.kind {
            SiteKind::Rig => TileKind::Built(BuiltTile::Plating),
            SiteKind::Camp | SiteKind::Marker | SiteKind::Grave => {
                TileKind::Built(BuiltTile::Plank)
            }
        });
    }
    None
}

/// The expedition's stamps touching one chunk: the log sites and anomaly rooms the V11
/// generator's story pass carves ([`story_stamps`]).
#[derive(Clone, Debug, Default)]
pub struct StoryStamps {
    pub sites: Vec<ExpeditionSite>,
    pub rooms: Vec<super::anomaly::AnomalyRoom>,
}

impl StoryStamps {
    /// The tile the story pass writes at `position`, if any (rooms over sites; they never
    /// overlap).
    #[must_use]
    pub fn tile_at(&self, position: TilePosition) -> Option<TileKind> {
        self.rooms
            .iter()
            .find_map(|room| super::anomaly::room_tile(room, position.x, position.y))
            .or_else(|| {
                self.sites
                    .iter()
                    .find_map(|site| site_tile(site, position.x, position.y))
            })
    }

    /// Whether `position` is inside a site or room (generated fluid stays out).
    #[must_use]
    pub fn covers(&self, position: TilePosition) -> bool {
        self.rooms.iter().any(|room| room.bounds.contains(position))
            || self
                .sites
                .iter()
                .any(|site| site.extent().contains(position))
    }
}

/// The log sites and anomaly rooms whose bounds overlap `area` (empty for worlds without
/// Descents): the V11 generator's story pass.
#[must_use]
pub fn story_stamps(settings: &WorldSettings, area: &TileRect) -> StoryStamps {
    if !settings.generator.has_descents() {
        return StoryStamps::default();
    }
    StoryStamps {
        sites: sites_in(settings, area),
        rooms: super::anomaly::rooms_in(settings, area),
    }
}
