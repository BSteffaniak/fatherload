//! The look of the time of day (Part II F4): keyframes over the day that the sky
//! (`world/sky.rs`) and the night lighting (`world/lighting.rs`) read.
//!
//! A [`DayLook`] is sampled from the world clock: the low sky's colour bands, glow bands over
//! the horizon, how many stars are out, the tints of clouds and backdrop hills, the rim light on
//! the hills, and the foreground overlay (a dark blue night, a dusky plum, a golden-hour wash).
//! Keyframes name palette colours only; between two keyframes a colour is a [`Mix`]: the earlier
//! colour with the later one laid over it at a fading alpha (the art guide's one allowed
//! variation, `with_alpha`). Presentation only: the simulation never reads any of it.

use raylib::prelude::*;

use fatherload_simulation::clock::WorldClock;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

/// A colour crossfade: `base`, with `over` drawn on top at alpha `t`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mix {
    pub base: Color,
    pub over: Color,
    pub t: u8,
}

impl Mix {
    pub const fn solid(color: Color) -> Self {
        Self {
            base: color,
            over: color,
            t: 0,
        }
    }

    /// Fills `rect` with the mix.
    pub fn fill(self, draw: &mut impl RaylibDraw, rect: Rectangle) {
        draw.draw_rectangle_rec(rect, self.base);
        if self.t > 0 && self.over != self.base {
            draw.draw_rectangle_rec(rect, with_alpha(self.over, self.t));
        }
    }

    /// Draws `texture` (a white mask) from `source` into `dest` in the mix's colours.
    pub fn stamp(
        self,
        draw: &mut impl RaylibDraw,
        texture: &Texture2D,
        source: Rectangle,
        dest: Rectangle,
    ) {
        draw.draw_texture_pro(texture, source, dest, Vector2::zero(), 0.0, self.base);
        if self.t > 0 && self.over != self.base {
            draw.draw_texture_pro(
                texture,
                source,
                dest,
                Vector2::zero(),
                0.0,
                with_alpha(self.over, self.t),
            );
        }
    }
}

/// A translucent colour (a glow band, an overlay, a rim light).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Wash {
    pub color: Color,
    pub alpha: u8,
}

const fn wash(color: Color, alpha: u8) -> Wash {
    Wash { color, alpha }
}

const NONE: Wash = wash(INK, 0);

/// Which side of a slope catches the low sun.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RimSide {
    /// The sun is low in the east (left): dawn.
    Left,
    /// The sun is low in the west (right): golden hour and dusk.
    Right,
}

/// One keyframe of the day.
#[derive(Clone, Copy, Debug)]
struct Key {
    /// Day slice (0..96, from midnight) the keyframe sits at.
    slice: f32,
    /// The five low sky bands, top to bottom (they replace `DEEP_BLUE`, `OCEAN`, `SKY`,
    /// `SKY_LIGHT` and `ICE`).
    bands: [Color; 5],
    /// Glow bands over the horizon, bottom to top.
    glow: [Wash; 3],
    stars: f32,
    clouds: Color,
    hills: Color,
    rim: Wash,
    rim_side: RimSide,
    /// The foreground overlay: night darkness or a coloured wash.
    overlay: Wash,
}

const DAY_BANDS: [Color; 5] = [DEEP_BLUE, OCEAN, SKY, SKY_LIGHT, ICE];
const NIGHT_BANDS: [Color; 5] = [INK, NIGHT, NIGHT, NIGHT, DEEP_BLUE];
const WHITE: Color = Color::WHITE;

const NIGHT_KEY: Key = Key {
    slice: 0.0,
    bands: NIGHT_BANDS,
    glow: [wash(OCEAN, 70), wash(DEEP_BLUE, 60), NONE],
    stars: 1.0,
    clouds: SLATE_LIGHT,
    hills: SLATE_LIGHT,
    rim: NONE,
    rim_side: RimSide::Left,
    overlay: wash(DEEP_BLUE, 186),
};

const DAY_KEY: Key = Key {
    slice: 30.0,
    bands: DAY_BANDS,
    glow: [NONE, NONE, NONE],
    stars: 0.0,
    clouds: WHITE,
    hills: WHITE,
    rim: NONE,
    rim_side: RimSide::Left,
    overlay: NONE,
};

/// The day's keyframes in slice order; the first and last are both night so the day wraps.
const KEYS: [Key; 14] = [
    NIGHT_KEY,
    Key {
        slice: 16.0,
        ..NIGHT_KEY
    },
    // First light: the horizon turns plum and rose.
    Key {
        slice: 18.0,
        bands: [INK, NIGHT, NIGHT, PLUM, VIOLET],
        glow: [wash(ROSE, 120), wash(VIOLET, 90), wash(PLUM, 50)],
        stars: 0.6,
        clouds: LAVENDER,
        hills: FOG,
        rim: wash(PINK, 90),
        rim_side: RimSide::Left,
        overlay: wash(PLUM, 120),
    },
    // Dawn: gold at the horizon through pinks into blue.
    Key {
        slice: 20.5,
        bands: [DEEP_BLUE, OCEAN, SKY, LAVENDER, PINK],
        glow: [wash(GOLD, 210), wash(AMBER, 150), wash(PINK, 100)],
        stars: 0.1,
        clouds: PINK,
        hills: PINK,
        rim: wash(GOLD, 220),
        rim_side: RimSide::Left,
        overlay: wash(ROSE, 40),
    },
    // Morning: the last of the dawn haze.
    Key {
        slice: 23.5,
        bands: DAY_BANDS,
        glow: [wash(CREAM, 110), wash(PINK, 50), NONE],
        stars: 0.0,
        clouds: BONE,
        hills: BONE,
        rim: wash(CREAM, 90),
        rim_side: RimSide::Left,
        overlay: wash(CREAM, 12),
    },
    DAY_KEY,
    // Noon: deep blue overhead, a pale haze at the horizon.
    Key {
        slice: 48.0,
        bands: [DEEP_BLUE, OCEAN, OCEAN, SKY, SKY_LIGHT],
        glow: [wash(ICE, 120), wash(SKY_LIGHT, 60), NONE],
        ..DAY_KEY
    },
    Key {
        slice: 63.0,
        ..DAY_KEY
    },
    // Golden hour: warm light and a gold horizon.
    Key {
        slice: 69.0,
        bands: [DEEP_BLUE, OCEAN, SKY, SKY_LIGHT, CREAM],
        glow: [wash(AMBER, 210), wash(GOLD, 140), wash(CREAM, 70)],
        stars: 0.0,
        clouds: CREAM,
        hills: CREAM,
        rim: wash(GOLD, 170),
        rim_side: RimSide::Right,
        overlay: wash(AMBER, 58),
    },
    // Sunset: orange at the horizon, rose and lavender above.
    Key {
        slice: 73.0,
        bands: [DEEP_BLUE, OCEAN, LAVENDER, LAVENDER, PINK],
        glow: [wash(ORANGE, 225), wash(ROSE, 165), wash(PINK, 95)],
        stars: 0.0,
        clouds: PINK,
        hills: PINK,
        rim: wash(AMBER, 230),
        rim_side: RimSide::Right,
        overlay: wash(ORANGE, 46),
    },
    // Dusk: purples.
    Key {
        slice: 75.5,
        bands: [NIGHT, DEEP_BLUE, PLUM, PLUM, VIOLET],
        glow: [wash(ROSE, 215), wash(VIOLET, 160), wash(LAVENDER, 60)],
        stars: 0.35,
        clouds: LAVENDER,
        hills: LAVENDER,
        rim: wash(ROSE, 140),
        rim_side: RimSide::Right,
        overlay: wash(PLUM, 95),
    },
    // Last light.
    Key {
        slice: 78.5,
        bands: [INK, NIGHT, NIGHT, PLUM, PLUM],
        glow: [wash(VIOLET, 100), wash(PLUM, 60), NONE],
        stars: 0.85,
        clouds: FOG,
        hills: FOG,
        rim: NONE,
        rim_side: RimSide::Right,
        overlay: wash(DEEP_BLUE, 150),
    },
    Key {
        slice: 81.0,
        ..NIGHT_KEY
    },
    Key {
        slice: 96.0,
        ..NIGHT_KEY
    },
];

/// Everything the time of day changes in the picture, at one moment.
#[derive(Clone, Copy, Debug)]
pub struct DayLook {
    /// The five low sky bands, top to bottom.
    pub bands: [Mix; 5],
    /// Glow bands over the horizon, bottom to top (each a crossfade of two washes).
    pub glow: [(Wash, Wash); 3],
    /// Share of the stars out from the time of day (0..1).
    pub stars: f32,
    pub clouds: Mix,
    pub hills: Mix,
    pub rim: Wash,
    pub rim_side: RimSide,
    /// The foreground overlay as a crossfade of two washes.
    pub overlay: (Wash, Wash),
    /// How dark the foreground is (0 by day, about 0.6 at night): lights cut this much.
    pub darkness: f32,
}

fn mix(a: Color, b: Color, t: f32) -> Mix {
    Mix {
        base: a,
        over: b,
        t: (t.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

fn fade(a: Wash, t: f32) -> Wash {
    wash(
        a.color,
        (f32::from(a.alpha) * t.clamp(0.0, 1.0)).round() as u8,
    )
}

impl DayLook {
    /// The look at the clock's time of day.
    #[must_use]
    pub fn at(clock: WorldClock) -> Self {
        Self::at_slice(clock.day_fraction() * fatherload_simulation::clock::DAY_SLICES as f32)
    }

    /// The look at day slice `slice` (`0..96`, from midnight).
    #[must_use]
    pub fn at_slice(slice: f32) -> Self {
        let slice = slice.rem_euclid(96.0);
        let next = KEYS
            .iter()
            .position(|key| key.slice > slice)
            .unwrap_or(KEYS.len() - 1);
        let (a, b) = (KEYS[next.saturating_sub(1)], KEYS[next]);
        let span = (b.slice - a.slice).max(f32::EPSILON);
        let t = ((slice - a.slice) / span).clamp(0.0, 1.0);
        let bands = std::array::from_fn(|index| mix(a.bands[index], b.bands[index], t));
        let glow =
            std::array::from_fn(|index| (fade(a.glow[index], 1.0 - t), fade(b.glow[index], t)));
        let (rim, rim_side) = {
            let (ra, rb) = (fade(a.rim, 1.0 - t), fade(b.rim, t));
            if ra.alpha >= rb.alpha {
                (
                    wash(
                        a.rim.color,
                        ra.alpha.saturating_add(if a.rim_side == b.rim_side {
                            rb.alpha
                        } else {
                            0
                        }),
                    ),
                    a.rim_side,
                )
            } else {
                (
                    wash(
                        b.rim.color,
                        rb.alpha.saturating_add(if a.rim_side == b.rim_side {
                            ra.alpha
                        } else {
                            0
                        }),
                    ),
                    b.rim_side,
                )
            }
        };
        let overlay = (fade(a.overlay, 1.0 - t), fade(b.overlay, t));
        let dark_of = |key: &Key| {
            let color = key.overlay.color;
            if color == DEEP_BLUE || color == PLUM || color == NIGHT {
                f32::from(key.overlay.alpha) / 255.0
            } else {
                0.0
            }
        };
        Self {
            bands,
            glow,
            stars: (b.stars - a.stars).mul_add(t, a.stars),
            clouds: mix(a.clouds, b.clouds, t),
            hills: mix(a.hills, b.hills, t),
            rim,
            rim_side,
            overlay,
            darkness: (dark_of(&b) - dark_of(&a)).mul_add(t, dark_of(&a)),
        }
    }

    /// Whether the foreground needs the lighting pass at all.
    #[must_use]
    pub const fn has_overlay(&self) -> bool {
        self.overlay.0.alpha > 0 || self.overlay.1.alpha > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_are_in_order_and_wrap_at_night() {
        assert!(KEYS.windows(2).all(|pair| pair[0].slice < pair[1].slice));
        assert!(KEYS[0].slice.abs() < f32::EPSILON);
        assert!((KEYS[KEYS.len() - 1].slice - 96.0).abs() < f32::EPSILON);
        assert_eq!(KEYS[0].bands, KEYS[KEYS.len() - 1].bands);
    }

    #[test]
    fn noon_is_clear_and_night_is_dark() {
        let noon = DayLook::at_slice(48.0);
        assert!(!noon.has_overlay());
        assert!(noon.stars <= 0.0);
        assert!(noon.darkness <= 0.0);
        let night = DayLook::at_slice(90.0);
        assert!(night.has_overlay());
        assert!(night.darkness > 0.5);
        assert!(night.stars >= 1.0);
        let dawn = DayLook::at_slice(20.5);
        assert_eq!(dawn.rim_side, RimSide::Left);
        assert!(dawn.rim.alpha > 150);
        let dusk = DayLook::at_slice(73.0);
        assert_eq!(dusk.rim_side, RimSide::Right);
    }

    #[test]
    fn every_keyframe_colour_is_from_the_palette() {
        for key in KEYS {
            for color in key
                .bands
                .into_iter()
                .chain(key.glow.map(|glow| glow.color))
                .chain([key.rim.color, key.overlay.color])
                .chain(
                    [key.clouds, key.hills]
                        .into_iter()
                        .filter(|color| *color != WHITE),
                )
            {
                assert!(
                    crate::rendering::pixel_art::palette::is_palette_color(color),
                    "{color:?}"
                );
            }
        }
    }
}
