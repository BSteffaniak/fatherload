//! Anomaly rooms (expansion plan §14, cross-Descent systems).
//!
//! Rare sealed rooms at any depth of a Deep Descent world where the rules of a deeper chapter
//! leak through early. Gravity flips,
//! the picture rots into pixels, time all but stops, or the world turns into its mirror image.
//! Each is a hint of what is below and holds a small reward (treasure caches placed where the
//! room's rule makes them a little puzzle).
//!
//! * **Placement** is a pure function of the settings: the world is cut into
//!   [`ANOMALY_REGION`] regions, and [`ANOMALY_CHANCE`] per mille of the regions below the
//!   first [`FIRST_ROWS`] rows of the ground hold one room ([`room_in_region`]), never over a
//!   Descent landmark, the builders' cable or an expedition site. The V11 generator stamps it
//!   ([`room_tile`]) after the landmarks.
//! * **The rule** is a Wave 7 field: [`anomaly_field_source`] is registered first in
//!   [`crate::field::STRATUM_FIELD_SOURCES`]. Inside a room (feathered over its walls) it
//!   answers the stratum's profile blended into the room's ([`AnomalyKind::profile`]), and
//!   `None` everywhere else, so rigs, pilots, creatures, drops and the screen-effect stack all
//!   feel it with no other change. Screen effects stay steady (no flashing) and go through the
//!   game's effects-intensity setting like every field.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::{
    base::BuiltTile,
    cave::CaveTile,
    descent::{Descent, Stratum},
    field::{FieldPreset, FieldProfile, FieldZone, ScreenEffect, ScreenEffects},
    position::Frame,
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        descent::{DescentLayout, landmarks},
        noise::hash2,
    },
};

/// What leaks through in a room.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AnomalyKind {
    /// Gravity pulls up: the room's floor is its ceiling (a taste of the Upside-Down Isles and
    /// the Gravity Shear).
    GravityFlip,
    /// The picture rots into big pixels and few colours (the Pixel Rot).
    PixelRot,
    /// Time all but stops (the Time Sludge).
    TimeFrozen,
    /// Screen and controls mirrored (the Mirror Strata).
    Mirror,
}

impl AnomalyKind {
    pub const ALL: [Self; 4] = [
        Self::GravityFlip,
        Self::PixelRot,
        Self::TimeFrozen,
        Self::Mirror,
    ];

    /// Bit in [`super::ExpeditionState::anomaly_kinds`].
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GravityFlip => "The Upturned Room",
            Self::PixelRot => "The Rotten Room",
            Self::TimeFrozen => "The Stopped Room",
            Self::Mirror => "The Mirror Room",
        }
    }

    /// What the card says on entering: a hint of what lies far below.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::GravityFlip => {
                "Gravity lets go and falls the other way. Somewhere far below, whole regions \
                 must turn like this."
            }
            Self::PixelRot => {
                "The world comes apart into blocks and a handful of colours. Something deep \
                 down is wearing reality thin."
            }
            Self::TimeFrozen => {
                "Dust hangs in the air. Your engine's roar drags like syrup. Time runs slow \
                 near whatever sleeps at the bottom."
            }
            Self::Mirror => {
                "Left is right and right is left. The rock remembers a deeper place where \
                 everything has a twin."
            }
        }
    }

    /// The room's field.
    #[must_use]
    pub const fn profile(self) -> FieldProfile {
        let none = ScreenEffects::NONE;
        match self {
            Self::GravityFlip => {
                let mut profile = FieldPreset::GravityFlip.profile();
                profile.free_rotation = true;
                profile.screen = none
                    .with(ScreenEffect::Lensing, 25)
                    .with(ScreenEffect::Vignette, 30)
                    .with(ScreenEffect::ColourGrade, 25)
                    .graded([200, 170, 255]);
                profile
            }
            Self::PixelRot => FieldPreset::PixelRot.profile(),
            Self::TimeFrozen => {
                let mut profile = FieldProfile::NEUTRAL.with_time_scale(0.15).with_screen(
                    none.with(ScreenEffect::Desaturate, 70)
                        .with(ScreenEffect::ColourGrade, 45)
                        .with(ScreenEffect::Vignette, 35)
                        .graded([170, 205, 255]),
                );
                profile.sound_occlusion = 0.5;
                profile
            }
            Self::Mirror => FieldPreset::Mirror.profile(),
        }
    }
}

/// One anomaly room.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnomalyRoom {
    pub kind: AnomalyKind,
    /// The region it was placed in (its stable key).
    pub region: (i32, i32),
    pub stratum: Stratum,
    /// Every tile it stamps, walls included.
    pub bounds: TileRect,
}

impl AnomalyRoom {
    #[must_use]
    pub const fn descent(&self) -> Descent {
        self.stratum.descent()
    }

    /// The open room inside the walls.
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

    /// The tiles holding the room's reward caches.
    #[must_use]
    pub const fn caches(&self) -> [Option<TilePosition>; 2] {
        let inner = self.interior();
        let mid = i32::midpoint(inner.min.x, inner.max.x);
        match self.kind {
            // On the ceiling: the floor, once gravity has flipped.
            AnomalyKind::GravityFlip => [
                Some(TilePosition {
                    x: mid,
                    y: inner.min.y,
                }),
                None,
            ],
            // A twin pair, mirrored about the middle.
            AnomalyKind::Mirror => [
                Some(TilePosition {
                    x: inner.min.x + 3,
                    y: inner.max.y,
                }),
                Some(TilePosition {
                    x: inner.max.x - 3,
                    y: inner.max.y,
                }),
            ],
            // Hanging in mid-air: a stopped moment.
            AnomalyKind::TimeFrozen => [
                Some(TilePosition {
                    x: mid,
                    y: i32::midpoint(inner.min.y, inner.max.y),
                }),
                None,
            ],
            AnomalyKind::PixelRot => [
                Some(TilePosition {
                    x: mid,
                    y: inner.max.y,
                }),
                None,
            ],
        }
    }

    /// Whether `position` is one of the room's two doorways: three rows tall at the floor, in
    /// both side walls. The linings (brick, crystal, obsidian, building glass) can be harder
    /// than the rock around them, or not diggable at all, so a rig always walks in through a
    /// door it reaches by digging the ordinary rock beside the room.
    #[must_use]
    pub const fn is_door(&self, position: TilePosition) -> bool {
        (position.x == self.bounds.min.x || position.x == self.bounds.max.x)
            && position.y >= self.bounds.max.y - 3
            && position.y < self.bounds.max.y
    }

    /// The cache tile the room's reward is: a relic cache only where the stratum's own drill
    /// opens one, else a treasure cache.
    #[must_use]
    pub const fn cache_tile(&self) -> CaveTile {
        if matches!(self.kind, AnomalyKind::TimeFrozen)
            && self.stratum.host_band().required_drill_strength() >= CaveTile::RelicCache.hardness()
        {
            CaveTile::RelicCache
        } else {
            CaveTile::TreasureCache
        }
    }

    /// The field zone the room's rule fills (its walls and one tile beyond, feathered over two
    /// tiles, so the interior is fully inside).
    #[must_use]
    pub const fn zone(&self) -> FieldZone {
        FieldZone::new(
            (self.bounds.min.x - 1, self.bounds.min.y - 1),
            (self.bounds.max.x + 1, self.bounds.max.y + 1),
            2,
            self.kind.profile(),
        )
    }
}

/// The regions rooms are placed per, `(columns, rows)`.
pub const ANOMALY_REGION: (i32, i32) = (112, 96);
/// `(width, height)` of a room, walls included.
pub const ROOM_SIZE: (i32, i32) = (18, 10);
/// Chance (per mille) that a region holds a room.
pub const ANOMALY_CHANCE: u64 = 220;
/// Rows under the ground line kept free of rooms (the first dig stays ordinary).
pub const FIRST_ROWS: i32 = 40;

const SALT_ANOMALY: u64 = 0x5745_0802;

/// The room of region `(rx, ry)`, if it holds one.
#[must_use]
pub fn room_in_region(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
) -> Option<AnomalyRoom> {
    if !settings.generator.has_descents() {
        return None;
    }
    let roll = hash2(settings.seed, SALT_ANOMALY, rx, ry);
    if roll % 1_000 >= ANOMALY_CHANCE {
        return None;
    }
    let (width, height) = ANOMALY_REGION;
    let margin = 8;
    let x = rx * width
        + margin
        + i32::try_from((roll >> 12) % u64::try_from(width - ROOM_SIZE.0 - 2 * margin).ok()?)
            .ok()?;
    let y = ry * height
        + margin
        + i32::try_from((roll >> 28) % u64::try_from(height - ROOM_SIZE.1 - 2 * margin).ok()?)
            .ok()?;
    let bounds = TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + ROOM_SIZE.0 - 1,
            y: y + ROOM_SIZE.1 - 1,
        },
    };
    if bounds.min.y < layout.tops[0] + FIRST_ROWS || bounds.max.y > layout.floor - FIRST_ROWS {
        return None;
    }
    let stratum = layout.stratum_of_row(bounds.center().y);
    // Not in the Hollow World's open sky, where a room would float, nor where the stratum's
    // own drill could not open the room's cache (the topsoil: the first dig stays ordinary).
    if stratum == Stratum::InnerSky
        || stratum.host_band().required_drill_strength() < CaveTile::TreasureCache.hardness()
    {
        return None;
    }
    // Clear of the cable, the landmarks and the expedition's sites.
    let lift = landmarks::lift_column(settings);
    let around = bounds.expanded(3);
    if around.min.x <= lift + 2 && around.max.x >= lift - 2 {
        return None;
    }
    if !landmarks::landmarks_with(settings, layout, &around).is_empty() {
        return None;
    }
    if !super::sites::sites_in(settings, &around).is_empty() {
        return None;
    }
    // Nor over a pilot-only ruin (stamped before the story pass, so a room would cut it open)
    // or under a tomb entrance (stamped after it, so the entrance would cut into the room).
    if super::sites::crosses_older_spaces(settings, &around) {
        return None;
    }
    let kind = AnomalyKind::ALL[usize::try_from((roll >> 40) % 4).unwrap_or(0)];
    Some(AnomalyRoom {
        kind,
        region: (rx, ry),
        stratum,
        bounds,
    })
}

/// One remembered region: the world, the region and its room.
type CachedRoom = (WorldSettings, (i32, i32), Option<AnomalyRoom>);

thread_local! {
    /// Recently asked regions (a pure function's answers, so the cache never changes one).
    static ROOMS: RefCell<Vec<CachedRoom>> =
        const { RefCell::new(Vec::new()) };
}

/// How many regions [`ROOMS`] remembers.
const ROOM_CACHE: usize = 24;

/// [`room_in_region`] through a small per-thread cache (field samples ask it every tick).
fn cached_room(settings: &WorldSettings, rx: i32, ry: i32) -> Option<AnomalyRoom> {
    ROOMS.with(|cache| {
        if let Some((_, _, room)) = cache
            .borrow()
            .iter()
            .find(|(cached, region, _)| cached == settings && *region == (rx, ry))
        {
            return *room;
        }
        let layout = crate::descent::fields::cached_layout(settings);
        let room = room_in_region(settings, &layout, rx, ry);
        let mut cache = cache.borrow_mut();
        if cache.len() >= ROOM_CACHE {
            cache.remove(0);
        }
        cache.push((*settings, (rx, ry), room));
        room
    })
}

/// The room whose bounds (or field zone) reach tile `tile`, if any. Rooms sit at least eight
/// tiles inside their region, so only the tile's own region can hold one.
#[must_use]
pub fn room_near(settings: &WorldSettings, tile: TilePosition) -> Option<AnomalyRoom> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (width, height) = ANOMALY_REGION;
    cached_room(
        settings,
        tile.x.div_euclid(width),
        tile.y.div_euclid(height),
    )
    .filter(|room| room.bounds.expanded(1).contains(tile))
}

/// The room whose interior holds `tile`, if any.
#[must_use]
pub fn room_holding(settings: &WorldSettings, tile: TilePosition) -> Option<AnomalyRoom> {
    room_near(settings, tile).filter(|room| room.interior().contains(tile))
}

/// Every room whose bounds overlap `area`.
#[must_use]
pub fn rooms_in(settings: &WorldSettings, area: &TileRect) -> Vec<AnomalyRoom> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let layout = crate::descent::fields::cached_layout(settings);
    rooms_with(settings, &layout, area)
}

/// [`rooms_in`] with the layout already known.
#[must_use]
pub fn rooms_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<AnomalyRoom> {
    let (width, height) = ANOMALY_REGION;
    let mut found = Vec::new();
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(room) = room_in_region(settings, layout, rx, ry)
                && room.bounds.intersects(area)
            {
                found.push(room);
            }
        }
    }
    found
}

/// The tile `room` stamps at `(x, y)`, or `None` outside it.
///
/// Each kind has its own lining: builder brick for the upturned room, crystal for the rotten
/// one, volcanic glass for the stopped one and glass panes for the mirror room. A glyph marks
/// the middle of the ceiling, a doorway opens in each side wall ([`AnomalyRoom::is_door`]),
/// and the caches sit where the rule puts them.
#[must_use]
pub fn room_tile(room: &AnomalyRoom, x: i32, y: i32) -> Option<TileKind> {
    let position = TilePosition { x, y };
    if !room.bounds.contains(position) {
        return None;
    }
    if room.caches().contains(&Some(position)) {
        return Some(TileKind::Cave(room.cache_tile()));
    }
    if room.interior().contains(position) || room.is_door(position) {
        return Some(TileKind::Air);
    }
    let middle = i32::midpoint(room.bounds.min.x, room.bounds.max.x);
    if y == room.bounds.min.y && (x == middle || x == middle + 1) {
        return Some(TileKind::Cave(CaveTile::RuinGlyph));
    }
    Some(match room.kind {
        AnomalyKind::GravityFlip => TileKind::Cave(CaveTile::RuinBrick),
        AnomalyKind::PixelRot => TileKind::Cave(CaveTile::Crystal),
        AnomalyKind::TimeFrozen => TileKind::Cave(CaveTile::Obsidian),
        AnomalyKind::Mirror => TileKind::Built(BuiltTile::Glass),
    })
}

/// The anomaly rooms' field source (see the module docs).
#[must_use]
pub fn anomaly_field_source(
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> Option<FieldProfile> {
    if !settings.generator.has_descents() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let tile = frame.tile_at(x, y);
    let (width, height) = ANOMALY_REGION;
    let room = cached_room(
        settings,
        tile.x.div_euclid(width),
        tile.y.div_euclid(height),
    )?;
    let weight = room.zone().weight_in(frame, x, y);
    if weight <= 0.0 {
        return None;
    }
    let base = crate::descent::fields::stratum_field_source(settings, frame, x, y)
        .unwrap_or(FieldProfile::NEUTRAL);
    Some(over(&base, &room.kind.profile(), weight))
}

/// `room`'s departures from the ordinary world laid over `base` with `weight` (`0..=1`).
///
/// What the room changes moves toward the room's value, what it leaves alone keeps the stratum's
/// (a pixel-rot room in the Gravity Shear still turns gravity), switches turn on past half
/// way, and screen effects keep the stronger of the two.
#[must_use]
#[allow(clippy::float_cmp, reason = "the neutral values are exact constants")]
pub fn over(base: &FieldProfile, room: &FieldProfile, weight: f32) -> FieldProfile {
    let t = weight.clamp(0.0, 1.0);
    let neutral = FieldProfile::NEUTRAL;
    let mix = |base: f32, room: f32, plain: f32| {
        if room == plain {
            base
        } else {
            (room - base).mul_add(t, base)
        }
    };
    let gravity_departs =
        room.gravity_x != neutral.gravity_x || room.gravity_y != neutral.gravity_y;
    let mut result = *base;
    if gravity_departs {
        result.gravity_x = (room.gravity_x - base.gravity_x).mul_add(t, base.gravity_x);
        result.gravity_y = (room.gravity_y - base.gravity_y).mul_add(t, base.gravity_y);
    }
    result.time_scale = mix(base.time_scale, room.time_scale, neutral.time_scale);
    result.friction = mix(base.friction, room.friction, neutral.friction);
    result.drag = mix(base.drag, room.drag, neutral.drag);
    result.light = mix(base.light, room.light, neutral.light);
    result.sound_occlusion = mix(
        base.sound_occlusion,
        room.sound_occlusion,
        neutral.sound_occlusion,
    );
    result.mirror_controls = base.mirror_controls || (room.mirror_controls && t > 0.5);
    result.free_rotation = base.free_rotation || (room.free_rotation && t > 0.5);
    for (index, level) in result.screen.levels.iter_mut().enumerate() {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a weighted percent is a percent"
        )]
        let wanted = (f32::from(room.screen.levels[index]) * t).round() as u8;
        *level = (*level).max(wanted);
    }
    let blend = |a: [u8; 3], b: [u8; 3]| -> [u8; 3] {
        std::array::from_fn(|channel| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a mix of two bytes is a byte"
            )]
            let value = (f32::from(b[channel]) - f32::from(a[channel]))
                .mul_add(t, f32::from(a[channel]))
                .round() as u8;
            value
        })
    };
    if room.screen.level(ScreenEffect::ColourGrade) > 0 {
        result.screen.grade_rgb = blend(base.screen.grade_rgb, room.screen.grade_rgb);
    }
    if room.screen.level(ScreenEffect::Fog) > 0 {
        result.screen.fog_rgb = blend(base.screen.fog_rgb, room.screen.fog_rgb);
    }
    result
}
