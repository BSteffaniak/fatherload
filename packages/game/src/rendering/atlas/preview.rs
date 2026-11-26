//! The Atlas's previews: tile pictures painted with the real tile art from the deterministic
//! world, the galaxy gathered round a space target for the space renderer, and the maps.
//!
//! A tile preview is a window of [`PREVIEW_TILES`] round the target, painted a few rows a
//! frame from its middle outward ([`ROWS_PER_FRAME`], so opening the Atlas or stepping through
//! a list never stalls a frame) into a texture that is cached ([`CACHE_SIZE`] of them, least
//! recently shown dropped first): stepping back to a destination shows it at once. The home
//! world's tiles come from the world's own terrain (edits included; unloaded chunks read their
//! generated tiles); a planet's from a terrain of its ground built for the preview. Nothing is
//! painted while no Atlas is open.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "texture sizes and tile windows are small whole numbers"
)]

use std::{collections::BTreeMap, sync::Arc};

use raylib::prelude::*;

use fatherload_simulation::{
    WorldState,
    descent::Descent,
    place::SpaceBodyKey,
    space::{Space, SpaceTile},
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{self, descent as gen_descent},
};

use crate::{
    atlas::{AtlasTarget, Destination, index, map::MapKind},
    render_snapshot::RenderSnapshot,
    rendering::{
        pixel_art::{
            palette,
            shade::{PaletteIndex, TRANSPARENT, color_at},
            sprite,
        },
        space::live::LiveSpace,
        terrain::tile_art::{self, TILE_ART},
    },
    shell::{ModalScreen, ShellState, atlas::SeatAtlas},
};

/// Tiles a preview shows (columns, rows).
pub const PREVIEW_TILES: (i32, i32) = (44, 26);
/// Rows of tiles painted per frame per preview being painted.
const ROWS_PER_FRAME: usize = if cfg!(target_os = "emscripten") { 2 } else { 3 };
/// Previews kept.
const CACHE_SIZE: usize = 24;
/// The depth map's texture (texels across, down).
const DEPTH_MAP_TEXELS: (i32, i32) = (96, 256);

thread_local! {
    /// A still (scene capture) paints every preview whole before drawing.
    static PAINT_WHOLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Paint previews whole in one go (scene capture).
#[cfg(not(target_os = "emscripten"))]
pub fn paint_whole(on: bool) {
    PAINT_WHOLE.with(|cell| cell.set(on));
}

/// What a tile preview is painted from.
#[derive(Clone, Debug)]
enum Ground {
    Home,
    Body(Arc<Terrain>),
}

/// One tile preview.
pub struct TilePreview {
    texture: Option<Texture2D>,
    art: Vec<PaletteIndex>,
    /// The window's top-left tile.
    pub origin: TilePosition,
    /// The target tile (the arrival marker).
    pub target: TilePosition,
    ground: Ground,
    /// Rows painted so far (in [`row_order`]).
    rows_done: usize,
    last_used: u64,
    /// Fluid tiles' colours (drawn over the art; the live fluids are drawn over the terrain in
    /// play).
    fluids: Vec<(TilePosition, Color)>,
    /// The sky behind the window: open sky, the deep's dark or space.
    pub backdrop: Backdrop,
}

/// What shows behind a preview's open tiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backdrop {
    Sky,
    Underground,
    Space,
}

impl TilePreview {
    /// Whether every row is painted.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.rows_done >= PREVIEW_TILES.1 as usize
    }

    /// The painted texture.
    #[must_use]
    pub const fn texture(&self) -> Option<&Texture2D> {
        self.texture.as_ref()
    }

    /// Fluid tiles over the art.
    #[must_use]
    pub fn fluids(&self) -> &[(TilePosition, Color)] {
        &self.fluids
    }
}

/// The rows of a preview, middle first, then outward.
fn row_order() -> impl Iterator<Item = i32> {
    let rows = PREVIEW_TILES.1;
    let middle = rows / 2;
    (0..rows).map(move |step| {
        let offset = (step + 1) / 2;
        if step % 2 == 0 {
            middle + offset
        } else {
            middle - offset
        }
    })
}

/// A seat's depth map picture.
pub struct DepthMapPicture {
    key: (u64, i32, i32),
    pub texture: Option<Texture2D>,
    /// Descents' first rows on the map (v) with their numerals.
    pub descents: Vec<(f32, &'static str, &'static str)>,
    /// Gate chambers and outposts ((u, v), is a gate).
    pub marks: Vec<((f32, f32), bool)>,
}

/// A seat's galaxy map: the systems on it.
pub struct GalaxyPicture {
    key: (i64, i64, i64),
    /// Each system: (u, v), its star's colour, its name, its planets.
    pub systems: Vec<((f32, f32), Color, String, usize)>,
}

/// Every preview the Atlas shows (the renderer owns one).
#[derive(Default)]
pub struct AtlasPreviews {
    tiles: BTreeMap<String, TilePreview>,
    frame: u64,
    /// Each seat's galaxy gathered round its space target.
    spaces: Vec<LiveSpace>,
    planets: BTreeMap<SpaceBodyKey, Arc<Terrain>>,
    depth_maps: BTreeMap<usize, DepthMapPicture>,
    galaxies: BTreeMap<usize, GalaxyPicture>,
}

/// Where a destination's tile preview looks, and from what.
fn tile_view(world: &WorldState, target: &AtlasTarget) -> Option<(String, TilePosition, bool)> {
    match *target {
        AtlasTarget::Home(tile) => Some((target.key(), tile, false)),
        AtlasTarget::Body { body, column } => {
            let params = fatherload_simulation::place::planet_params_for(world.world_seed(), body)?;
            let _ = params;
            Some((
                target.key(),
                TilePosition {
                    x: column as i32,
                    y: 0,
                },
                true,
            ))
        }
        AtlasTarget::Player(id) => {
            let resolved = index::resolve_player(world, id)?;
            tile_view(world, &resolved)
        }
        AtlasTarget::Space { .. } | AtlasTarget::Story => None,
    }
}

impl AtlasPreviews {
    /// Paints the previews the open Atlases show this frame (and the neighbours they would
    /// show next), within the frame's budget.
    pub fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        shell: &ShellState,
        snapshot: &RenderSnapshot<'_>,
    ) {
        self.frame += 1;
        let world = snapshot.space.world;
        let whole = PAINT_WHOLE.with(std::cell::Cell::get);
        let mut budget = if whole {
            usize::MAX
        } else {
            ROWS_PER_FRAME * 2
        };
        for view in &snapshot.views {
            let seat = shell.seat(view.seat_index);
            if seat.modal != Some(ModalScreen::Atlas) {
                continue;
            }
            let atlas = &seat.atlas;
            let wanted: Vec<&Destination> = atlas
                .current()
                .into_iter()
                .chain(atlas.neighbours())
                .collect();
            // Neighbours wait until the selection is painted: a cold preview may generate
            // its chunks, and one at a time keeps the browser's frames even.
            let mut selection_done = true;
            for (rank, destination) in wanted.iter().enumerate() {
                if rank > 0 && !selection_done && !whole {
                    break;
                }
                let Some((key, tile, body)) = tile_view(world, &destination.target) else {
                    continue;
                };
                if !self.tiles.contains_key(&key) {
                    let ground = if body {
                        let AtlasTarget::Body { body: id, .. } =
                            resolve(world, &destination.target)
                        else {
                            continue;
                        };
                        match self.planet(world, id) {
                            Some(terrain) => Ground::Body(terrain),
                            None => continue,
                        }
                    } else {
                        Ground::Home
                    };
                    let preview = new_preview(raylib, thread, snapshot.terrain, ground, tile);
                    self.tiles.insert(key.clone(), preview);
                }
                let frame = self.frame;
                if let Some(preview) = self.tiles.get_mut(&key) {
                    preview.last_used = frame;
                    // The selection paints first; neighbours only with what is left.
                    let rows = if rank == 0 {
                        if whole { usize::MAX } else { ROWS_PER_FRAME }
                    } else {
                        budget.min(ROWS_PER_FRAME)
                    };
                    let painted = paint_rows(preview, snapshot.terrain, rows);
                    budget = budget.saturating_sub(painted);
                    if rank == 0 {
                        selection_done = preview.done();
                    }
                }
            }
            self.sync_space(view.seat_index, atlas, world, snapshot);
            self.sync_maps(raylib, thread, view.seat_index, atlas, world);
        }
        self.evict();
    }

    /// The galaxy gathered round a space target (or a planet's own place).
    fn sync_space(
        &mut self,
        seat: usize,
        atlas: &SeatAtlas,
        world: &WorldState,
        snapshot: &RenderSnapshot<'_>,
    ) {
        let Some(destination) = atlas.current() else {
            return;
        };
        let Some((centre, _)) = space_focus(world, &destination.target) else {
            return;
        };
        if self.spaces.len() <= seat {
            self.spaces.resize_with(seat + 1, LiveSpace::default);
        }
        self.spaces[seat].update(snapshot.space.seed, centre);
        self.spaces[seat].turn_bodies(snapshot.space.millis);
    }

    fn sync_maps(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        seat: usize,
        atlas: &SeatAtlas,
        world: &WorldState,
    ) {
        let Some(map) = &atlas.map else {
            return;
        };
        match map.kind {
            MapKind::Depth => {
                let key = (
                    world.world_seed().seed,
                    map.depth.first_column,
                    map.depth.columns,
                );
                if self
                    .depth_maps
                    .get(&seat)
                    .is_none_or(|picture| picture.key != key)
                {
                    let picture = depth_picture(raylib, thread, world, map, key);
                    self.depth_maps.insert(seat, picture);
                }
            }
            MapKind::Galaxy => {
                let key = (map.galaxy.centre.x, map.galaxy.centre.y, map.galaxy.radius);
                if self
                    .galaxies
                    .get(&seat)
                    .is_none_or(|picture| picture.key != key)
                {
                    self.galaxies
                        .insert(seat, galaxy_picture(world, &map.galaxy, key));
                }
            }
        }
    }

    /// The ground of body `id` for previews (kept for the few bodies looked at).
    fn planet(&mut self, world: &WorldState, id: SpaceBodyKey) -> Option<Arc<Terrain>> {
        if let Some(found) = self.planets.get(&id) {
            return Some(Arc::clone(found));
        }
        let params = fatherload_simulation::place::planet_params_for(world.world_seed(), id)?;
        let terrain = Arc::new(Terrain::planet(params));
        if self.planets.len() >= 8 {
            let first = *self.planets.keys().next()?;
            self.planets.remove(&first);
        }
        self.planets.insert(id, Arc::clone(&terrain));
        Some(terrain)
    }

    /// Drops the least recently shown previews past the cache's size.
    fn evict(&mut self) {
        while self.tiles.len() > CACHE_SIZE {
            let Some(oldest) = self
                .tiles
                .iter()
                .min_by_key(|(_, preview)| preview.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.tiles.remove(&oldest);
        }
    }

    /// The tile preview of `target`, if one is (being) painted.
    #[must_use]
    pub fn tile(&self, world: &WorldState, target: &AtlasTarget) -> Option<&TilePreview> {
        let (key, _, _) = tile_view(world, target)?;
        self.tiles.get(&key)
    }

    /// Seat `seat`'s galaxy gathering.
    #[must_use]
    pub fn space(&self, seat: usize) -> Option<&LiveSpace> {
        self.spaces.get(seat)
    }

    #[must_use]
    pub fn depth_map(&self, seat: usize) -> Option<&DepthMapPicture> {
        self.depth_maps.get(&seat)
    }

    #[must_use]
    pub fn galaxy(&self, seat: usize) -> Option<&GalaxyPicture> {
        self.galaxies.get(&seat)
    }

    /// Forgets everything (a new world).
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.planets.clear();
        self.depth_maps.clear();
        self.galaxies.clear();
        self.spaces.clear();
    }

    /// Whether every preview the open Atlases show is painted (scene capture waits for it).
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub fn settled(&self) -> bool {
        self.tiles.values().all(TilePreview::done)
    }
}

/// `target` with a player resolved to where they are.
fn resolve(world: &WorldState, target: &AtlasTarget) -> AtlasTarget {
    match *target {
        AtlasTarget::Player(id) => index::resolve_player(world, id).unwrap_or(*target),
        other => other,
    }
}

/// Where the space preview of `target` looks and how many tiles round it it shows, when the
/// target is seen from space.
#[must_use]
pub fn space_focus(
    world: &WorldState,
    target: &AtlasTarget,
) -> Option<(fatherload_simulation::position::WorldPos, f32)> {
    match resolve(world, target) {
        AtlasTarget::Space { pos, reach } => Some((pos, reach)),
        AtlasTarget::Body { body, .. } => {
            let found = Space::shared(world.world_seed()).body(body)?;
            Some((found.centre.pos(), found.radius as f32 * 1.7))
        }
        AtlasTarget::Home(_) | AtlasTarget::Player(_) | AtlasTarget::Story => None,
    }
}

fn new_preview(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    home: &Terrain,
    ground: Ground,
    target: TilePosition,
) -> TilePreview {
    let mut preview = blank_preview(home, ground, target);
    let (width, height) = (PREVIEW_TILES.0 * TILE_ART, PREVIEW_TILES.1 * TILE_ART);
    preview.texture = raylib
        .load_texture_from_image(thread, &sprite::blank_image(width, height))
        .ok();
    if let Some(texture) = &preview.texture {
        texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
    }
    preview
}

/// A preview of `target` with nothing painted and no texture yet.
fn blank_preview(home: &Terrain, ground: Ground, target: TilePosition) -> TilePreview {
    let (columns, rows) = PREVIEW_TILES;
    let terrain: &Terrain = match &ground {
        Ground::Home => home,
        Ground::Body(terrain) => terrain,
    };
    // A planet target names a column: stand on its surface.
    let target = match ground {
        Ground::Body(_) => TilePosition {
            x: target.x,
            y: terrain.surface_row(target.x) - 2,
        },
        Ground::Home => target,
    };
    // The arrival a little above the middle: more of the ground under it shows.
    let origin = TilePosition {
        x: target.x - columns / 2,
        y: target.y - rows / 2 + 2,
    };
    let surface = terrain.surface_row(target.x);
    let backdrop = match ground {
        Ground::Body(_) => Backdrop::Space,
        Ground::Home if target.y < -200 => Backdrop::Space,
        Ground::Home if origin.y + rows / 2 < surface + 8 => Backdrop::Sky,
        Ground::Home => Backdrop::Underground,
    };
    let (width, height) = (columns * TILE_ART, rows * TILE_ART);
    TilePreview {
        texture: None,
        art: vec![TRANSPARENT; (width * height) as usize],
        origin,
        target,
        ground,
        rows_done: 0,
        last_used: 0,
        fluids: Vec::new(),
        backdrop,
    }
}

/// Paints up to `rows` more rows of `preview` and uploads them; returns how many it painted.
fn paint_rows(preview: &mut TilePreview, home: &Terrain, rows: usize) -> usize {
    let (columns, total) = PREVIEW_TILES;
    let stride = (columns * TILE_ART) as usize;
    let mut painted = 0;
    let ground = preview.ground.clone();
    let terrain: &Terrain = match &ground {
        Ground::Home => home,
        Ground::Body(terrain) => terrain,
    };
    while painted < rows && preview.rows_done < total as usize {
        let Some(row) = row_order().nth(preview.rows_done) else {
            break;
        };
        let y = preview.origin.y + row;
        for column in 0..columns {
            let tile = TilePosition {
                x: preview.origin.x + column,
                y,
            };
            tile_art::paint_tile(
                terrain,
                tile,
                &mut preview.art,
                stride,
                ((column * TILE_ART) as usize, (row * TILE_ART) as usize),
            );
            if let Some(found) = terrain.tile(tile)
                && let TileKind::Fluid(kind) = found.kind
            {
                preview
                    .fluids
                    .push((tile, palette::with_alpha(tile_art::fluid_color(kind), 170)));
            }
        }
        upload_row(preview, row);
        preview.rows_done += 1;
        painted += 1;
    }
    painted
}

/// Sends row `row`'s art to the texture.
fn upload_row(preview: &mut TilePreview, row: i32) {
    let width = PREVIEW_TILES.0 * TILE_ART;
    let start = (row * TILE_ART * width) as usize;
    let end = start + (TILE_ART * width) as usize;
    let mut pixels = vec![0_u8; (end - start) * 4];
    for (offset, index) in preview.art[start..end].iter().enumerate() {
        if *index == TRANSPARENT {
            continue;
        }
        let color = color_at(*index);
        pixels[offset * 4..offset * 4 + 4].copy_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    if let Some(texture) = &mut preview.texture {
        let _ = texture.update_texture_rec(
            Rectangle::new(0.0, (row * TILE_ART) as f32, width as f32, TILE_ART as f32),
            &pixels,
        );
    }
}

/// The depth map's picture: every texel the rock colour of the stratum there, the sky above
/// the ground, with the Descents' first rows, gates and outposts.
fn depth_picture(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    world: &WorldState,
    map: &crate::shell::atlas::AtlasMap,
    key: (u64, i32, i32),
) -> DepthMapPicture {
    let settings = *world.world_settings();
    let (width, height) = DEPTH_MAP_TEXELS;
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    let descents = settings.generator.has_descents();
    let surfaces: Vec<i32> = (0..width)
        .map(|x| {
            let tile = map.depth.tile_at((x as f32 / (width - 1) as f32, 0.0));
            world.terrain().surface_row(tile.x)
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            let uv = (
                x as f32 / (width - 1) as f32,
                y as f32 / (height - 1) as f32,
            );
            let tile = map.depth.tile_at(uv);
            let color = if tile.y < surfaces[x as usize] {
                // Sky: lighter toward the ground.
                let t = (y as f32 / height as f32 * 12.0).clamp(0.0, 1.0);
                lerp(palette::NIGHT, palette::SKY, t)
            } else if descents {
                let stratum = gen_descent::stratum_at(&settings, tile.x, tile.y);
                let base = tile_art::stratum_rock_color(stratum);
                // Alternate Descents a touch darker so their bands read.
                if stratum.descent().index() % 2 == 1 {
                    lerp(base, palette::INK, 0.18)
                } else {
                    base
                }
            } else {
                tile_art::material_color(TileKind::Stone, world.terrain().band_at(tile))
            };
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
        }
    }
    let mut texture = raylib
        .load_texture_from_image(thread, &sprite::blank_image(width, height))
        .ok();
    if let Some(texture) = &mut texture {
        texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
        let _ = texture.update_texture(&pixels);
    }
    let mut picture = DepthMapPicture {
        key,
        texture,
        descents: Vec::new(),
        marks: Vec::new(),
    };
    if descents {
        let layout = gen_descent::layout(&settings);
        let cached = fatherload_simulation::descent::fields::cached_layout(&settings);
        for descent in Descent::ALL {
            let (top, _) = layout.descent_rows(descent);
            let (_, v) = map.depth.uv_of(TilePosition {
                x: worldgen::SPAWN_TILE_X,
                y: top,
            });
            picture
                .descents
                .push((v, descent.numeral(), descent.name()));
            let gate = gen_descent::landmarks::gate_chamber(&settings, &cached, descent);
            picture.marks.push((
                map.depth.uv_of(TilePosition {
                    x: i32::midpoint(gate.bounds.min.x, gate.bounds.max.x),
                    y: gate.bounds.max.y,
                }),
                true,
            ));
            if let Some(hall) = gen_descent::landmarks::outpost(&settings, &cached, descent) {
                picture.marks.push((
                    map.depth.uv_of(TilePosition {
                        x: i32::midpoint(hall.bounds.min.x, hall.bounds.max.x),
                        y: hall.bounds.max.y,
                    }),
                    false,
                ));
            }
        }
    }
    picture
}

fn galaxy_picture(
    world: &WorldState,
    map: &crate::atlas::map::GalaxyMap,
    key: (i64, i64, i64),
) -> GalaxyPicture {
    let galaxy = Space::shared(world.world_seed());
    let mut systems = Vec::new();
    for sector in galaxy.sectors_near(map.centre, map.radius * 3 / 2) {
        for system in &sector.systems {
            let uv = map.uv_of(system.centre);
            if !(0.0..=1.0).contains(&uv.0) || !(0.0..=1.0).contains(&uv.1) {
                continue;
            }
            let [r, g, b] = system.class.colour();
            systems.push((
                uv,
                Color::new(r, g, b, 255),
                system.name.clone(),
                system.planet_count(),
            ));
        }
    }
    let _ = SpaceTile::new(0, 0);
    GalaxyPicture { key, systems }
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::{AtlasCategory, DestinationKind, index};
    use fatherload_simulation::{
        PlayerId, SimulationParams,
        worldgen::{GeneratorVersion, WorldSettings},
    };

    /// A test-mode Deep Descent world with two rigs (so the Players list has one).
    fn world() -> WorldState {
        let settings = WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(0x0A71_A514)
        };
        let mut world = WorldState::new_game_with_settings(
            PlayerId::new(1),
            &settings,
            SimulationParams::new(30),
        );
        world.add_player(PlayerId::new(2));
        world.enable_test_mode();
        world
    }

    /// Every kind of destination the Atlas lists gets a preview: a tile picture painted from
    /// the world (home or a planet's ground, its art filled in), or the space renderer's view
    /// of it, or (the story's goal) its words.
    #[test]
    fn a_preview_is_made_for_every_kind_of_destination() {
        let world = world();
        let me = Some(PlayerId::new(1));
        let mut every: Vec<crate::atlas::Destination> = AtlasCategory::ALL
            .iter()
            .flat_map(|category| index::destinations(&world, me, *category))
            .collect();
        every.push(index::depth_spot(&world, TilePosition { x: 300, y: 4_000 }));
        every.push(index::space_spot(
            &world,
            fatherload_simulation::position::WorldPos::from_tile(9_000_000, -2_000_000),
        ));
        let kinds: std::collections::BTreeSet<DestinationKind> =
            every.iter().map(|destination| destination.kind).collect();
        for wanted in [
            DestinationKind::Surface,
            DestinationKind::Descent,
            DestinationKind::Stratum,
            DestinationKind::DepthSpot,
            DestinationKind::HomeTown,
            DestinationKind::Village,
            DestinationKind::Moon,
            DestinationKind::Outpost,
            DestinationKind::StorySite,
            DestinationKind::GateBoss,
            DestinationKind::ThemeBoss,
            DestinationKind::FactionBoss,
            DestinationKind::DeepSettlement,
            DestinationKind::AlienTown,
            DestinationKind::StarSystem,
            DestinationKind::Planet,
            DestinationKind::Station,
            DestinationKind::Belt,
            DestinationKind::Player,
            DestinationKind::SpaceSpot,
        ] {
            assert!(kinds.contains(&wanted), "the Atlas lists a {wanted:?}");
        }
        let mut painted_kinds = std::collections::BTreeSet::new();
        let mut planets: BTreeMap<SpaceBodyKey, Arc<Terrain>> = BTreeMap::new();
        for destination in &every {
            let tile = tile_view(&world, &destination.target);
            let space = space_focus(&world, &destination.target);
            assert!(
                tile.is_some() || space.is_some() || destination.target == AtlasTarget::Story,
                "{} ({:?}) has a preview",
                destination.name,
                destination.kind
            );
            // Paint one of each kind's tile pictures on the CPU (the texture comes from the
            // renderer): real art fills it.
            let Some((_, at, body)) = tile else {
                continue;
            };
            if !painted_kinds.insert(destination.kind) {
                continue;
            }
            let ground = if body {
                let AtlasTarget::Body { body: id, .. } = resolve(&world, &destination.target)
                else {
                    continue;
                };
                let terrain = planets.entry(id).or_insert_with(|| {
                    let params =
                        fatherload_simulation::place::planet_params_for(world.world_seed(), id)
                            .expect("a landable body");
                    Arc::new(Terrain::planet(params))
                });
                Ground::Body(Arc::clone(terrain))
            } else {
                Ground::Home
            };
            let mut preview = blank_preview(world.terrain(), ground, at);
            let rows = paint_rows(&mut preview, world.terrain(), usize::MAX);
            assert_eq!(rows, PREVIEW_TILES.1 as usize);
            assert!(preview.done());
            let filled = preview
                .art
                .iter()
                .filter(|index| **index != TRANSPARENT)
                .count();
            assert!(
                filled > preview.art.len() / 8,
                "{} ({:?}): {filled} of {} art pixels painted",
                destination.name,
                destination.kind,
                preview.art.len()
            );
        }
        assert!(painted_kinds.contains(&DestinationKind::GateBoss));
        assert!(painted_kinds.contains(&DestinationKind::Planet));
    }

    #[test]
    fn rows_paint_from_the_middle_out_and_cover_the_window() {
        let rows: Vec<i32> = row_order().collect();
        assert_eq!(rows.len(), PREVIEW_TILES.1 as usize);
        assert_eq!(rows[0], PREVIEW_TILES.1 / 2);
        assert_eq!(rows[1], PREVIEW_TILES.1 / 2 - 1);
        let mut sorted = rows;
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, (0..PREVIEW_TILES.1).collect::<Vec<_>>());
    }
}
