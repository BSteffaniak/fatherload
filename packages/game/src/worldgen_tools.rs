//! World generation tools:
//!
//! - `drillgame --worldgen-report [--seeds N] [--first-seed S] [--size standard|large]
//!   [--richness lean|standard|rich] [--hazards calm|standard|brutal] [--legacy]` prints ore,
//!   cave, hazard, artifact and feature distributions over many seeds.
//! - `drillgame --worldgen-preview <seed> <out.png> [--size standard|large] [--richness X]
//!   [--hazards Y] [--scale N] [--no-features]` writes a whole-world map PNG (no window opens).

use drillgame_simulation::{
    terrain::{MineralKind, TileKind},
    worldgen::{
        self, Band, FeatureKind, GeneratorVersion, HazardIntensity, HazardKind, ResourceRichness,
        WorldSettings, WorldSize, band_at, parse_seed,
        report::{WorldStats, distribution_report},
    },
};
use raylib::prelude::Color;

use crate::rendering::{pixel_art::palette, terrain::tile_art};

/// A parsed worldgen tool invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldgenCommand {
    Report {
        settings: WorldSettings,
        first_seed: u64,
        seeds: u64,
    },
    Preview {
        settings: WorldSettings,
        out: String,
        scale: i32,
        features: bool,
    },
}

/// Parses the worldgen tool flags. `Ok(None)` when neither tool flag is present.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<WorldgenCommand>, String> {
    let report = args.iter().position(|arg| arg == "--worldgen-report");
    let preview = args.iter().position(|arg| arg == "--worldgen-preview");
    let mut settings = WorldSettings::new(0);
    let mut seeds = 200;
    let mut first_seed = 1;
    let mut scale = 4;
    let mut features = true;
    let (mut skip, mut positional) = (Vec::new(), Vec::new());
    match (report, preview) {
        (None, None) => return Ok(None),
        (Some(_), Some(_)) => {
            return Err("pick one of --worldgen-report / --worldgen-preview".into());
        }
        (Some(position), None) => skip.push(position),
        (None, Some(position)) => {
            skip.push(position);
            for offset in 1..=2 {
                let value = args
                    .get(position + offset)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or("--worldgen-preview needs <seed> <out.png>")?;
                skip.push(position + offset);
                positional.push(value.clone());
            }
        }
    }
    let mut iter = args
        .iter()
        .enumerate()
        .filter(|(index, _)| !skip.contains(index));
    while let Some((_, arg)) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .map(|(_, value)| value.clone())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--seeds" => {
                seeds = value("--seeds")?
                    .parse()
                    .ok()
                    .filter(|count| (1..=100_000).contains(count))
                    .ok_or("--seeds needs a count from 1 to 100000")?;
            }
            "--first-seed" => {
                first_seed = parse_seed(&value("--first-seed")?).ok_or("invalid --first-seed")?;
            }
            "--size" => settings.size = parse_size(&value("--size")?)?,
            "--richness" => settings.richness = parse_richness(&value("--richness")?)?,
            "--hazards" => settings.hazards = parse_hazards(&value("--hazards")?)?,
            "--scale" => {
                scale = value("--scale")?
                    .parse()
                    .ok()
                    .filter(|scale| (1..=8).contains(scale))
                    .ok_or("--scale needs 1 to 8")?;
            }
            "--legacy" => settings.generator = GeneratorVersion::Legacy,
            "--no-features" => features = false,
            other => return Err(format!("unknown worldgen argument {other}")),
        }
    }
    if report.is_some() {
        return Ok(Some(WorldgenCommand::Report {
            settings,
            first_seed,
            seeds,
        }));
    }
    settings.seed = parse_seed(&positional[0]).ok_or("invalid seed")?;
    Ok(Some(WorldgenCommand::Preview {
        settings,
        out: positional[1].clone(),
        scale,
        features,
    }))
}

fn parse_size(text: &str) -> Result<WorldSize, String> {
    WorldSize::ALL
        .into_iter()
        .find(|size| size.name().eq_ignore_ascii_case(text))
        .ok_or_else(|| format!("unknown size {text} (standard, large)"))
}

fn parse_richness(text: &str) -> Result<ResourceRichness, String> {
    ResourceRichness::ALL
        .into_iter()
        .find(|richness| richness.name().eq_ignore_ascii_case(text))
        .ok_or_else(|| format!("unknown richness {text} (lean, standard, rich)"))
}

fn parse_hazards(text: &str) -> Result<HazardIntensity, String> {
    HazardIntensity::ALL
        .into_iter()
        .find(|hazards| hazards.name().eq_ignore_ascii_case(text))
        .ok_or_else(|| format!("unknown hazard intensity {text} (calm, standard, brutal)"))
}

/// Runs a worldgen tool, writing text output to `out`.
///
/// # Errors
///
/// Returns a message if the preview image cannot be written.
pub fn run(command: &WorldgenCommand, out: &mut impl std::io::Write) -> Result<(), String> {
    match command {
        WorldgenCommand::Report {
            settings,
            first_seed,
            seeds,
        } => {
            let report = distribution_report(*settings, *first_seed, *seeds);
            write!(out, "{report}").map_err(|error| error.to_string())
        }
        WorldgenCommand::Preview {
            settings,
            out: path,
            scale,
            features,
        } => {
            let world = worldgen::generate(settings);
            let stats = WorldStats::measure(settings, &world);
            let mut image = preview_image(settings, &world, *scale, *features);
            image.export_png(path)?;
            writeln!(
                out,
                "wrote {path}: seed {} ({}), {}x{} tiles, {} features, caves {} per mille, starter field {}",
                settings.seed_text(),
                settings.size.name(),
                world.width,
                world.height,
                world.features.len(),
                stats.cave_per_mille(),
                stats.starter_field_value,
            )
            .map_err(|error| error.to_string())
        }
    }
}

/// Width in tiles of the band legend strip on the preview's left edge.
const LEGEND_TILES: i32 = 3;

/// An RGBA pixel buffer written out through raylib's PNG encoder (no window or GPU needed).
struct Canvas {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: i32, height: i32, fill: Color) -> Self {
        let count = usize::try_from(width * height).unwrap_or_default();
        Self {
            width,
            height,
            pixels: [fill.r, fill.g, fill.b, fill.a].repeat(count),
        }
    }

    fn draw_rectangle(&mut self, x: i32, y: i32, width: i32, height: i32, color: Color) {
        for py in y.max(0)..(y + height).min(self.height) {
            for px in x.max(0)..(x + width).min(self.width) {
                let index = usize::try_from((py * self.width + px) * 4).unwrap_or_default();
                self.pixels[index..index + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
            }
        }
    }

    fn draw_rectangle_lines(&mut self, x: i32, y: i32, width: i32, height: i32, color: Color) {
        self.draw_rectangle(x, y, width, 1, color);
        self.draw_rectangle(x, y + height - 1, width, 1, color);
        self.draw_rectangle(x, y, 1, height, color);
        self.draw_rectangle(x + width - 1, y, 1, height, color);
    }

    fn export_png(&mut self, path: &str) -> Result<(), String> {
        let c_path = std::ffi::CString::new(path).map_err(|error| error.to_string())?;
        let image = raylib::ffi::Image {
            data: self.pixels.as_mut_ptr().cast(),
            width: self.width,
            height: self.height,
            mipmaps: 1,
            format: raylib::ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
        };
        // SAFETY: `image` borrows `self.pixels` (width * height RGBA bytes) for the duration of
        // the call; ExportImage only reads it and never frees it.
        let written = unsafe { raylib::ffi::ExportImage(image, c_path.as_ptr()) };
        if written {
            Ok(())
        } else {
            Err(format!("failed to write {path}"))
        }
    }
}

/// Renders a whole-world map: one `scale` x `scale` block per tile.
fn preview_image(
    settings: &WorldSettings,
    world: &worldgen::GeneratedWorld,
    scale: i32,
    features: bool,
) -> Canvas {
    let legend = LEGEND_TILES * scale;
    let mut image = Canvas::new(
        world.width * scale + legend,
        world.height * scale,
        palette::INK,
    );
    let kind_at = |x: i32, y: i32| -> Option<TileKind> {
        (x >= 0 && y >= 0 && x < world.width && y < world.height)
            .then(|| world.tiles[usize::try_from(y * world.width + x).unwrap_or(0)].kind)
    };
    for y in 0..world.height {
        // Band legend: each row's band at the world's middle column.
        let band = band_at(settings, world.width / 2, y);
        image.draw_rectangle(0, y * scale, legend - 1, scale, legend_color(band, y));
        for x in 0..world.width {
            let Some(kind) = kind_at(x, y) else {
                continue;
            };
            let band = band_at(settings, x, y);
            let mut color = tile_color(kind, band, y);
            // Tell-tale stain: rock near a hazard pocket takes on the pocket's colour, as the
            // game's tile art stains it, so the preview shows what a player can read.
            if kind != TileKind::Air
                && HazardKind::of_tile(kind).is_none()
                && let Some(hazard) = nearby_hazard(&kind_at, x, y)
            {
                color = mix(color, tile_color(hazard, band, y), 35);
            }
            // Light the floors and darken the ceilings of open spaces so caves read as volumes.
            if kind != TileKind::Air && y > 4 {
                if kind_at(x, y - 1) == Some(TileKind::Air) {
                    color = mix(color, Color::new(255, 255, 255, 255), 30);
                } else if kind_at(x, y + 1) == Some(TileKind::Air) {
                    color = mix(color, Color::new(0, 0, 0, 255), 35);
                }
            }
            image.draw_rectangle(legend + x * scale, y * scale, scale, scale, color);
            // Hazard tiles get a dark rim so pockets never read as ore of a similar colour.
            if HazardKind::of_tile(kind).is_some() && scale >= 3 {
                image.draw_rectangle_lines(
                    legend + x * scale,
                    y * scale,
                    scale,
                    scale,
                    palette::INK,
                );
            }
        }
    }
    if features {
        for feature in world.features.all() {
            let color = match feature.kind {
                FeatureKind::Motherlode => palette::GOLD,
                FeatureKind::ArtifactBed => palette::PINK,
                FeatureKind::Chamber => palette::ICE,
                FeatureKind::PrefabSlot
                | FeatureKind::LargeCavern
                | FeatureKind::HazardCluster
                | FeatureKind::StratumTransition => continue,
            };
            let bounds = feature.bounds;
            image.draw_rectangle_lines(
                legend + (bounds.min.x - 1) * scale,
                (bounds.min.y - 1) * scale,
                (bounds.width() + 2) * scale,
                (bounds.height() + 2) * scale,
                color,
            );
        }
    }
    image
}

/// The hazard tile within two tiles of `(x, y)`, if any (the reach of the in-game stain).
fn nearby_hazard(
    kind_at: &impl Fn(i32, i32) -> Option<TileKind>,
    x: i32,
    y: i32,
) -> Option<TileKind> {
    (-2..=2)
        .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| kind_at(x + dx, y + dy))
        .find(|kind| HazardKind::of_tile(*kind).is_some())
}

fn mix(a: Color, b: Color, percent: u16) -> Color {
    let blend = |x: u8, y: u8| {
        let value = (u16::from(x) * (100 - percent) + u16::from(y) * percent) / 100;
        u8::try_from(value).unwrap_or(u8::MAX)
    };
    Color::new(blend(a.r, b.r), blend(a.g, b.g), blend(a.b, b.b), 255)
}

/// Alternating light/dark swatches per band so boundaries stand out in the legend.
fn legend_color(band: Band, y: i32) -> Color {
    if y < 5 {
        return palette::SKY_LIGHT;
    }
    let base = tile_art::material_color(band.soft_material(), band);
    if band.index().is_multiple_of(2) {
        mix(base, Color::new(255, 255, 255, 255), 25)
    } else {
        base
    }
}

fn tile_color(kind: TileKind, band: Band, y: i32) -> Color {
    match kind {
        TileKind::Air if y < 5 => mix(
            palette::SKY,
            palette::SKY_LIGHT,
            20 * u16::try_from(y.max(0)).unwrap_or(0),
        ),
        TileKind::Air => mix(
            tile_art::material_color(TileKind::Air, band),
            palette::INK,
            55,
        ),
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::Foundation => {
            tile_art::material_color(kind, band)
        }
        TileKind::HardRock => mix(tile_art::material_color(kind, band), palette::INK, 20),
        TileKind::Ore(mineral) => mineral_color(mineral),
        TileKind::Artifact(_) => palette::PINK,
        TileKind::Gas => palette::LIME,
        TileKind::Lava => palette::ORANGE,
        TileKind::MagmaVent => palette::RED,
        TileKind::PressurePocket => palette::SKY,
        TileKind::ExplosivePocket => palette::AMBER,
    }
}

const fn mineral_color(mineral: MineralKind) -> Color {
    match mineral {
        MineralKind::Copper => palette::COPPER,
        MineralKind::Iron => palette::SAND,
        MineralKind::Silver => palette::STEEL_LIGHT,
        MineralKind::Gold => palette::GOLD,
        MineralKind::Emerald => palette::MINT,
        MineralKind::Ruby => palette::ROSE,
        MineralKind::Diamond => palette::ICE,
        MineralKind::Platinum => palette::CREAM,
        MineralKind::Uranium => palette::LEAF,
        MineralKind::Mythril => palette::LAVENDER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_report_and_preview_flags() {
        assert_eq!(parse_args(&args("--fullscreen")), Ok(None));
        let Ok(Some(WorldgenCommand::Report {
            settings, seeds, ..
        })) = parse_args(&args(
            "--worldgen-report --seeds 12 --richness rich --hazards calm",
        ))
        else {
            panic!("report flags");
        };
        assert_eq!(seeds, 12);
        assert_eq!(settings.richness, ResourceRichness::Rich);
        assert_eq!(settings.hazards, HazardIntensity::Calm);
        let Ok(Some(WorldgenCommand::Preview { settings, out, .. })) =
            parse_args(&args("--worldgen-preview D111-6A4E map.png --size large"))
        else {
            panic!("preview flags");
        };
        assert_eq!(settings.seed, 0xD111_6A4E);
        assert_eq!(settings.size, WorldSize::Large);
        assert_eq!(out, "map.png");
        assert!(parse_args(&args("--worldgen-preview 12")).is_err());
        assert!(parse_args(&args("--worldgen-report --richness huge")).is_err());
    }
}
