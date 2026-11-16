//! The sky gauge (expansion plan F6): a small HUD panel that appears under the wallet while a
//! rig flies through a V8 sky, reading its altitude environment at a glance: the band it is
//! in, how thin the air is and how much thrust its intakes keep, the cold against its de-icer,
//! gravity when it is not the ground's, and which way the wind blows.

use raylib::prelude::*;

use fatherload_simulation::{
    environment::{self, Environment},
    sky::SkyBand,
    terrain::TILE_SIZE,
};

#[allow(
    clippy::wildcard_imports,
    reason = "HUD code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::sprites::icons,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// What the gauge shows for one rig.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyGauge {
    pub band: SkyBand,
    /// Air density, percent of the ground's.
    pub air_percent: u32,
    /// Thrust the rig keeps with its intakes, percent.
    pub thrust_percent: u32,
    /// Cold level and how much of it the de-icer covers.
    pub cold: u8,
    pub cold_cover: u8,
    /// Gravity relative to the ground's.
    pub gravity: f32,
    /// The wind (px/s, positive to the right).
    pub wind: f32,
}

impl SkyGauge {
    /// The gauge of a rig at world pixel `(x, y)` with intakes `intake` and de-icer `de_icer`,
    /// or `None` where the air is standard (the ground, old worlds).
    #[must_use]
    pub fn read(
        settings: &fatherload_simulation::worldgen::WorldSettings,
        (x, y): (f32, f32),
        intake: u8,
        de_icer: u8,
    ) -> Option<Self> {
        let air: Environment = environment::environment_at(settings, x, y);
        if air.is_standard() {
            return None;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "percentages of values in 0..=1"
        )]
        let percent = |value: f32| (value * 100.0).round().clamp(0.0, 100.0) as u32;
        #[allow(clippy::cast_possible_truncation, reason = "pixel to row")]
        let row = (y / TILE_SIZE).floor() as i32;
        Some(Self {
            band: SkyBand::at_row(row),
            air_percent: percent(air.air_density),
            thrust_percent: percent(air.thrust_efficiency(intake)),
            cold: air.cold,
            cold_cover: environment::cold_cover(de_icer),
            gravity: air.gravity,
            wind: air.wind_x,
        })
    }

    /// The wind as arrows: up to three, pointing downwind (empty in calm air).
    #[must_use]
    pub fn wind_arrows(&self) -> &'static str {
        let strength = self.wind.abs();
        match (self.wind > 0.0, strength) {
            (_, s) if s < 12.0 => "",
            (true, s) if s < 60.0 => ">",
            (true, s) if s < 130.0 => ">>",
            (true, _) => ">>>",
            (false, s) if s < 60.0 => "<",
            (false, s) if s < 130.0 => "<<",
            (false, _) => "<<<",
        }
    }
}

/// Draws the sky gauge right-aligned under the wallet panel, if the rig is in a V8 sky.
#[allow(clippy::too_many_lines, reason = "one gauge line after another")]
pub(super) fn draw_sky_gauge(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    let player = view.player;
    let Some(mut gauge) = SkyGauge::read(
        snapshot.terrain.settings(),
        (player.x, player.y),
        player.hazard_gear.altitude_intake,
        player.hazard_gear.de_icer,
    ) else {
        return;
    };
    // A pressurized hull (WP-SKY) keeps out cold on top of the de-icer.
    gauge.cold_cover = player.hazard_gear.cold_cover();
    // Drained buoys are not worth a detour.
    let nearest = super::world::sky_isles::nearest_sky_place(
        snapshot.terrain.settings(),
        (player.x, player.y),
        &player.villages.buoys,
    );
    let band = gauge.band.name().to_uppercase();
    let air = format!(
        "AIR {}%  THRUST {}%",
        gauge.air_percent, gauge.thrust_percent
    );
    let cold = (gauge.cold > 0).then(|| format!("COLD {} / {}", gauge.cold, gauge.cold_cover));
    let gravity = ((gauge.gravity - 1.0).abs() > 0.005).then(|| format!("G {:.2}", gauge.gravity));
    let arrows = gauge.wind_arrows();
    let place = nearest
        .as_ref()
        .map(super::world::sky_isles::NearestSkyPlace::line);
    let lines = 2 + i32::from(cold.is_some() || gravity.is_some()) + i32::from(place.is_some());
    let text_width = [
        ui_kit::text_width(&band, TextStyle::Bold)
            + ui_kit::text_width(arrows, TextStyle::Bold)
            + 8,
        ui_kit::text_width(&air, TextStyle::Body) + icons::ICON_SIZE + 4,
        cold.as_ref()
            .map_or(0, |line| ui_kit::text_width(line, TextStyle::Body))
            + gravity
                .as_ref()
                .map_or(0, |line| ui_kit::text_width(line, TextStyle::Body) + 8)
            + icons::ICON_SIZE
            + 4,
        place.as_ref().map_or(0, |line| {
            ui_kit::text_width(line, TextStyle::Body) + icons::ICON_SIZE + 4
        }),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let width = text_width + ui_kit::PANEL_PADDING * 2;
    let height = 6 + lines * 12;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    let inner = x + ui_kit::PANEL_PADDING;
    let right = x + width - ui_kit::PANEL_PADDING;
    ui_kit::text(draw, &band, inner, top + 5, TextStyle::Bold, MINT);
    ui_kit::text_right(draw, arrows, right, top + 5, TextStyle::Bold, ICE);
    ui_kit::icon(draw, &icons::AIR, inner, top + 16);
    let thrust_color = match gauge.thrust_percent {
        0..=24 => RED,
        25..=49 => AMBER,
        _ => SKY_LIGHT,
    };
    ui_kit::text(
        draw,
        &air,
        inner + icons::ICON_SIZE + 3,
        top + 17,
        TextStyle::Body,
        thrust_color,
    );
    if lines > 2 {
        let y = top + 29;
        ui_kit::icon(draw, &icons::DE_ICER, inner, y - 1);
        let mut text_x = inner + icons::ICON_SIZE + 3;
        if let Some(line) = &cold {
            let color = if gauge.cold > gauge.cold_cover {
                ROSE
            } else {
                MIST
            };
            ui_kit::text(draw, line, text_x, y, TextStyle::Body, color);
            text_x += ui_kit::text_width(line, TextStyle::Body) + 8;
        }
        if let Some(line) = &gravity {
            ui_kit::text(draw, line, text_x, y, TextStyle::Body, LAVENDER);
        }
    }
    // The nearest sky town or unused fuel buoy (WP-SKY navigation).
    if let (Some(line), Some(found)) = (&place, &nearest) {
        let y = top + 5 + (lines - 1) * 12;
        let icon = if found.is_town() {
            &icons::CARGO
        } else {
            &icons::FUEL
        };
        ui_kit::icon(draw, icon, inner, y - 1);
        ui_kit::text(
            draw,
            line,
            inner + icons::ICON_SIZE + 3,
            y,
            TextStyle::Body,
            if found.is_town() { GOLD } else { AMBER },
        );
    }
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::worldgen::{GeneratorVersion, WorldSettings, sky};

    use super::*;

    #[test]
    fn the_gauge_reads_the_sky_and_stays_away_on_the_ground() {
        let settings = WorldSettings::new(9);
        assert!(settings.generator.has_sky());
        assert_eq!(settings.generator, GeneratorVersion::CURRENT);
        assert!(SkyGauge::read(&settings, (100.0, 50.0), 0, 0).is_none());
        #[allow(clippy::cast_precision_loss, reason = "band rows")]
        let y = -((sky::DEAD_AIR_BOTTOM_ROWS + 300) as f32) * TILE_SIZE;
        let gauge = SkyGauge::read(&settings, (-60_000.0, y), 6, 0).expect("in the gap");
        assert_eq!(gauge.band, SkyBand::DeadAir);
        assert!(gauge.air_percent < 20);
        assert!(gauge.thrust_percent > gauge.air_percent);
        assert!(gauge.cold > gauge.cold_cover);
        let old = WorldSettings {
            generator: GeneratorVersion::V7,
            ..settings
        };
        assert!(SkyGauge::read(&old, (-60_000.0, y), 6, 0).is_none());
    }
}
