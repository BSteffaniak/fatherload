//! Crystal refraction (Crystal Faults): a prism the headlamp reaches splits its light in three.
//!
//! The beams run straight on and 45 degrees to each side through open ground and crystal until
//! rock stops them. Every tile a beam passes (and its neighbours) is lit, and a latent gem in lit
//! rock shows itself.
//!
//! A pure function of the terrain around the eye, so the renderer draws exactly the beams and
//! gems the simulation (and the tests) see.

use std::collections::BTreeSet;

use crate::terrain::{TileKind, TilePosition};

use super::DeepTile;

/// How far (tiles) the headlamp reaches a prism.
pub const LAMP_REACH_TILES: i32 = 7;
/// How far a refracted beam runs.
pub const BEAM_LENGTH_TILES: i32 = 14;
/// The most prisms one eye lights at once.
pub const MAX_PRISMS: usize = 6;

/// The eight directions, clockwise from east.
const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// One refracted beam: from a prism's centre to where it ends (tile coordinates of centres),
/// coloured by its place in the fan (0 left, 1 straight, 2 right).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Beam {
    pub from: TilePosition,
    pub to: TilePosition,
    pub hue: u8,
}

/// What the headlamp lights through the prisms near an eye.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Refraction {
    /// The prisms lit (each splits the lamp).
    pub prisms: Vec<TilePosition>,
    pub beams: Vec<Beam>,
    /// Every tile a beam lights.
    pub lit: BTreeSet<TilePosition>,
    /// The latent gems revealed.
    pub gems: Vec<TilePosition>,
}

const fn passes(kind: Option<TileKind>) -> bool {
    match kind {
        Some(TileKind::Deep(tile)) => tile.translucent(),
        Some(kind) => kind.is_open(),
        None => false,
    }
}

/// The nearest of the eight directions to `(dx, dy)`.
fn octant(dx: i32, dy: i32) -> usize {
    let (ax, ay) = (dx.abs(), dy.abs());
    // Within about 22.5 degrees of an axis (tan 22.5 ~ 0.414 ~ 5/12) counts as straight.
    let sx = dx.signum();
    let sy = dy.signum();
    let dir = if ay * 12 <= ax * 5 {
        (sx, 0)
    } else if ax * 12 <= ay * 5 {
        (0, sy)
    } else {
        (sx, sy)
    };
    DIRECTIONS.iter().position(|&d| d == dir).unwrap_or(0)
}

/// Whether the straight line from `a` to `b` passes only open or translucent tiles (the end
/// tiles do not count).
fn clear_line(
    tile_at: &impl Fn(TilePosition) -> Option<TileKind>,
    a: TilePosition,
    b: TilePosition,
) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let steps = dx.abs().max(dy.abs());
    for step in 1..steps {
        // Rounded to the nearest tile along the line (integer, symmetric).
        let x = a.x + (2 * dx * step + steps * dx.signum()) / (2 * steps);
        let y = a.y + (2 * dy * step + steps * dy.signum()) / (2 * steps);
        let at = TilePosition { x, y };
        if at != a && at != b && !passes(tile_at(at)) {
            return false;
        }
    }
    true
}

/// The beams the headlamp at tile `eye` raises from the prisms within `reach` tiles, and what
/// they light. `tile_at` reads the terrain (`None` outside it).
#[must_use]
pub fn refract(
    tile_at: impl Fn(TilePosition) -> Option<TileKind>,
    eye: TilePosition,
    reach: i32,
) -> Refraction {
    let mut out = Refraction::default();
    let mut prisms = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            if dx * dx + dy * dy > reach * reach || (dx == 0 && dy == 0) {
                continue;
            }
            let at = TilePosition {
                x: eye.x + dx,
                y: eye.y + dy,
            };
            if tile_at(at) == Some(TileKind::Deep(DeepTile::Prism)) {
                prisms.push((dx * dx + dy * dy, at));
            }
        }
    }
    prisms.sort_by_key(|&(distance, at)| (distance, at.y, at.x));
    for (_, prism) in prisms {
        if out.prisms.len() >= MAX_PRISMS {
            break;
        }
        if !clear_line(&tile_at, eye, prism) {
            continue;
        }
        out.prisms.push(prism);
        let incoming = octant(prism.x - eye.x, prism.y - eye.y);
        for (hue, turn) in [7usize, 0, 1].into_iter().enumerate() {
            let (sx, sy) = DIRECTIONS[(incoming + turn) % 8];
            let mut end = prism;
            for step in 1..=BEAM_LENGTH_TILES {
                let at = TilePosition {
                    x: prism.x + sx * step,
                    y: prism.y + sy * step,
                };
                // The beam lights the tiles beside its path too (the glow spills into rock).
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        out.lit.insert(TilePosition {
                            x: at.x + ox,
                            y: at.y + oy,
                        });
                    }
                }
                end = at;
                if !passes(tile_at(at)) {
                    break;
                }
            }
            out.beams.push(Beam {
                from: prism,
                to: end,
                hue: u8::try_from(hue).unwrap_or(1),
            });
        }
    }
    let mut gems: Vec<TilePosition> = out
        .lit
        .iter()
        .copied()
        .filter(|&at| tile_at(at) == Some(TileKind::Deep(DeepTile::LatentGem)))
        .collect();
    gems.sort_by_key(|at| (at.y, at.x));
    out.gems = gems;
    out
}

/// Latent gems a Lumen-mutated rig lights by itself within `radius` tiles of `eye`.
#[must_use]
pub fn glow_reveals(
    tile_at: impl Fn(TilePosition) -> Option<TileKind>,
    eye: TilePosition,
    radius: i32,
) -> Vec<TilePosition> {
    let mut gems = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let at = TilePosition {
                x: eye.x + dx,
                y: eye.y + dy,
            };
            if dx * dx + dy * dy <= radius * radius
                && tile_at(at) == Some(TileKind::Deep(DeepTile::LatentGem))
            {
                gems.push(at);
            }
        }
    }
    gems
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn world(cells: &[(i32, i32, TileKind)]) -> impl Fn(TilePosition) -> Option<TileKind> + use<> {
        let map: BTreeMap<(i32, i32), TileKind> =
            cells.iter().map(|&(x, y, kind)| ((x, y), kind)).collect();
        move |at: TilePosition| {
            Some(map.get(&(at.x, at.y)).copied().unwrap_or(if at.y >= 1 {
                TileKind::Stone
            } else {
                TileKind::Air
            }))
        }
    }

    #[test]
    fn a_prism_splits_the_lamp_into_three_beams_that_reveal_hidden_gems() {
        // Open air above row 0, a prism on the surface 4 tiles east of the eye, a latent gem in
        // the rock where the right beam (east-south-east fan) runs.
        let gem = (8, 3);
        let tile_at = world(&[
            (4, 0, TileKind::Deep(DeepTile::Prism)),
            (5, 1, TileKind::Air),
            (6, 2, TileKind::Air),
            (7, 3, TileKind::Air),
            (gem.0, gem.1, TileKind::Deep(DeepTile::LatentGem)),
        ]);
        let eye = TilePosition { x: 0, y: -1 };
        let refraction = refract(&tile_at, eye, LAMP_REACH_TILES);
        assert_eq!(refraction.prisms, vec![TilePosition { x: 4, y: 0 }]);
        assert_eq!(refraction.beams.len(), 3);
        assert!(
            refraction
                .gems
                .contains(&TilePosition { x: gem.0, y: gem.1 })
        );
        // Without the prism the gem stays hidden.
        let plain = world(&[(gem.0, gem.1, TileKind::Deep(DeepTile::LatentGem))]);
        assert_eq!(refract(&plain, eye, LAMP_REACH_TILES).gems, []);
    }

    #[test]
    fn rock_between_the_lamp_and_a_prism_blocks_it() {
        let tile_at = world(&[
            (4, -1, TileKind::Deep(DeepTile::Prism)),
            (2, -1, TileKind::Stone),
        ]);
        let refraction = refract(&tile_at, TilePosition { x: 0, y: -1 }, LAMP_REACH_TILES);
        assert_eq!(refraction.prisms, []);
        // Crystal lets the lamp through.
        let tile_at = world(&[
            (4, -1, TileKind::Deep(DeepTile::Prism)),
            (2, -1, TileKind::Deep(DeepTile::ResonantCrystal)),
        ]);
        let refraction = refract(&tile_at, TilePosition { x: 0, y: -1 }, LAMP_REACH_TILES);
        assert_eq!(refraction.prisms.len(), 1);
    }
}
