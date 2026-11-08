//! The UI's pixel font, authored as glyph grids and built into raylib fonts at startup.
//!
//! Glyphs are up to 5 art pixels wide and 9 tall: rows 0-6 hold capitals and digits (x-height
//! letters use rows 2-6) and rows 7-8 hold descenders. Widths are proportional. A bold face is
//! derived by smearing every glyph one pixel to the right (without closing one-pixel gaps).
//!
//! Draw at a font size of `GLYPH_HEIGHT * n` with spacing `n` (in the units being drawn) for
//! crisp text; under the pixel UI camera that is simply size 9, spacing 1.

use raylib::{ffi, prelude::*};

/// Glyph cell height in art pixels (the font's base size).
pub const GLYPH_HEIGHT: i32 = 9;
/// Capital height in art pixels.
pub const CAP_HEIGHT: i32 = 7;

/// `(codepoint, rows)`: rows separated by spaces, `#` set and `.` clear. Rows may be fewer than
/// [`GLYPH_HEIGHT`]; missing rows are blank.
const GLYPHS: &[(char, &str)] = &[
    (' ', "... ... ... ... ... ... ..."),
    ('!', "# # # # # . #"),
    ('"', "#.# #.# ... ... ... ... ..."),
    ('#', ".#.#. ##### .#.#. .#.#. ##### .#.#. ....."),
    ('$', "..#.. .#### #.#.. .###. ..#.# ####. ..#.."),
    ('%', "##... ##..# ...#. ..#.. .#... #..## ...##"),
    ('&', ".##.. #..#. .##.. .#... #.#.# #..#. .##.#"),
    ('\'', "# # . . . . ."),
    ('(', ".# #. #. #. #. #. .#"),
    (')', "#. .# .# .# .# .# #."),
    ('*', "..... ..#.. #.#.# .###. #.#.# ..#.. ....."),
    ('+', "..... ..#.. ..#.. ##### ..#.. ..#.. ....."),
    (',', ".. .. .. .. .. .# .# #."),
    ('-', ".... .... .... #### .... .... ...."),
    ('.', ". . . . . . #"),
    ('/', "....# ...#. ...#. ..#.. .#... .#... #...."),
    ('0', ".###. #...# #..## #.#.# ##..# #...# .###."),
    ('1', "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('2', ".###. #...# ....# ...#. ..#.. .#... #####"),
    ('3', ".###. #...# ....# ..##. ....# #...# .###."),
    ('4', "...#. ..##. .#.#. #..#. ##### ...#. ...#."),
    ('5', "##### #.... ####. ....# ....# #...# .###."),
    ('6', ".###. #.... #.... ####. #...# #...# .###."),
    ('7', "##### ....# ...#. ..#.. .#... .#... .#..."),
    ('8', ".###. #...# #...# .###. #...# #...# .###."),
    ('9', ".###. #...# #...# .#### ....# ....# .###."),
    (':', ". . # . . # ."),
    (';', ".. .. .# .. .. .# .# #."),
    ('<', "...# ..#. .#.. #... .#.. ..#. ...#"),
    ('=', ".... .... #### .... #### .... ...."),
    ('>', "#... .#.. ..#. ...# ..#. .#.. #..."),
    ('?', ".###. #...# ....# ...#. ..#.. ..... ..#.."),
    ('@', ".###. #...# #.### #.#.# #.### #.... .####"),
    ('A', ".###. #...# #...# ##### #...# #...# #...#"),
    ('B', "####. #...# #...# ####. #...# #...# ####."),
    ('C', ".###. #...# #.... #.... #.... #...# .###."),
    ('D', "####. #...# #...# #...# #...# #...# ####."),
    ('E', "##### #.... #.... ####. #.... #.... #####"),
    ('F', "##### #.... #.... ####. #.... #.... #...."),
    ('G', ".###. #...# #.... #.### #...# #...# .####"),
    ('H', "#...# #...# #...# ##### #...# #...# #...#"),
    ('I', "### .#. .#. .#. .#. .#. ###"),
    ('J', "....# ....# ....# ....# #...# #...# .###."),
    ('K', "#...# #..#. #.#.. ##... #.#.. #..#. #...#"),
    ('L', "#.... #.... #.... #.... #.... #.... #####"),
    ('M', "#...# ##.## #.#.# #.#.# #...# #...# #...#"),
    ('N', "#...# ##..# #.#.# #..## #...# #...# #...#"),
    ('O', ".###. #...# #...# #...# #...# #...# .###."),
    ('P', "####. #...# #...# ####. #.... #.... #...."),
    ('Q', ".###. #...# #...# #...# #.#.# #..#. .##.#"),
    ('R', "####. #...# #...# ####. #.#.. #..#. #...#"),
    ('S', ".###. #...# #.... .###. ....# #...# .###."),
    ('T', "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#.."),
    ('U', "#...# #...# #...# #...# #...# #...# .###."),
    ('V', "#...# #...# #...# #...# #...# .#.#. ..#.."),
    ('W', "#...# #...# #...# #.#.# #.#.# ##.## #...#"),
    ('X', "#...# #...# .#.#. ..#.. .#.#. #...# #...#"),
    ('Y', "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#.."),
    ('Z', "##### ....# ...#. ..#.. .#... #.... #####"),
    ('[', "## #. #. #. #. #. ##"),
    ('\\', "#.... .#... .#... ..#.. ...#. ...#. ....#"),
    (']', "## .# .# .# .# .# ##"),
    ('^', "..#.. .#.#. #...# ..... ..... ..... ....."),
    ('_', "..... ..... ..... ..... ..... ..... #####"),
    ('`', "#. .# .. .. .. .. .."),
    ('a', "..... ..... .###. ....# .#### #...# .####"),
    ('b', "#.... #.... ####. #...# #...# #...# ####."),
    ('c', "..... ..... .#### #.... #.... #.... .####"),
    ('d', "....# ....# .#### #...# #...# #...# .####"),
    ('e', "..... ..... .###. #...# ##### #.... .####"),
    ('f', "..## .#.. ###. .#.. .#.. .#.. .#.."),
    ('g', "..... ..... .#### #...# #...# #...# .#### ....# .###."),
    ('h', "#.... #.... ####. #...# #...# #...# #...#"),
    ('i', "# . # # # # #"),
    ('j', "..# ... ..# ..# ..# ..# ..# #.# .#."),
    ('k', "#... #... #..# #.#. ##.. #.#. #..#"),
    ('l', "#. #. #. #. #. #. .#"),
    ('m', "..... ..... ####. #.#.# #.#.# #.#.# #.#.#"),
    ('n', "..... ..... ####. #...# #...# #...# #...#"),
    ('o', "..... ..... .###. #...# #...# #...# .###."),
    ('p', "..... ..... ####. #...# #...# #...# ####. #.... #...."),
    ('q', "..... ..... .#### #...# #...# #...# .#### ....# ....#"),
    ('r', ".... .... #.## ##.. #... #... #..."),
    ('s', "..... ..... .#### #.... .###. ....# ####."),
    ('t', ".#.. .#.. ###. .#.. .#.. .#.. ..##"),
    ('u', "..... ..... #...# #...# #...# #...# .####"),
    ('v', "..... ..... #...# #...# #...# .#.#. ..#.."),
    ('w', "..... ..... #...# #.#.# #.#.# #.#.# .#.#."),
    ('x', "..... ..... #...# .#.#. ..#.. .#.#. #...#"),
    ('y', "..... ..... #...# #...# #...# #...# .#### ....# .###."),
    ('z', "..... ..... ##### ...#. ..#.. .#... #####"),
    ('{', "..# .#. .#. #.. .#. .#. ..#"),
    ('|', "# # # # # # #"),
    ('}', "#.. .#. .#. ..# .#. .#. #.."),
    ('~', "..... ..... .#... #.#.# ...#. ..... ....."),
    ('\u{2014}', "..... ..... ..... ##### ..... ..... ....."),
    ('\u{2022}', "... ... .#. ### .#. ... ..."),
    ('\u{2190}', "..... ..#.. .#... ##### .#... ..#.. ....."),
    ('\u{2191}', "..#.. .###. #.#.# ..#.. ..#.. ..#.. ..#.."),
    ('\u{2192}', "..... ..#.. ...#. ##### ...#. ..#.. ....."),
    ('\u{2193}', "..#.. ..#.. ..#.. ..#.. #.#.# .###. ..#.."),
    ('\u{00d7}', "..... ..... #...# .#.#. ..#.. .#.#. #...#"),
];

/// One glyph's pixels as rows of booleans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphBitmap {
    pub codepoint: char,
    pub width: i32,
    pub rows: Vec<Vec<bool>>,
}

/// Parses the glyph table; `bold` smears each glyph one pixel right (one pixel wider).
#[must_use]
pub fn glyph_bitmaps(bold: bool) -> Vec<GlyphBitmap> {
    GLYPHS
        .iter()
        .map(|(codepoint, source)| {
            let mut rows: Vec<Vec<bool>> = source
                .split(' ')
                .map(|row| row.chars().map(|c| c == '#').collect())
                .collect();
            let base_width = rows.first().map_or(0, Vec::len);
            rows.resize(
                usize::try_from(GLYPH_HEIGHT).unwrap_or(9),
                vec![false; base_width],
            );
            if bold && *codepoint != ' ' {
                for row in &mut rows {
                    let mut smeared = vec![false; base_width + 1];
                    for (index, set) in row.iter().enumerate() {
                        if *set {
                            smeared[index] = true;
                            // Never fill a one-pixel gap ("#.#" stays open in m, w, M, W...).
                            let closes_gap = !row.get(index + 1).copied().unwrap_or(false)
                                && row.get(index + 2).copied().unwrap_or(false);
                            if !closes_gap {
                                smeared[index + 1] = true;
                            }
                        }
                    }
                    *row = smeared;
                }
            }
            let width = rows.first().map_or(0, Vec::len);
            GlyphBitmap {
                codepoint: *codepoint,
                width: i32::try_from(width).unwrap_or(0),
                rows,
            }
        })
        .collect()
}

/// Builds the pixel font (`bold` for the heavy face) as a raylib font with base size
/// [`GLYPH_HEIGHT`] and a point-filtered texture.
pub fn load_pixel_font(
    raylib: &mut RaylibHandle,
    thread: &RaylibThread,
    bold: bool,
) -> Option<Font> {
    let glyphs = glyph_bitmaps(bold);
    let atlas_width: i32 = glyphs.iter().map(|glyph| glyph.width + 1).sum::<i32>() + 1;
    let mut image = super::sprite::blank_image(atlas_width, GLYPH_HEIGHT);
    let mut x = 1;
    let mut placements = Vec::with_capacity(glyphs.len());
    for glyph in &glyphs {
        for (row_index, row) in glyph.rows.iter().enumerate() {
            for (column, set) in row.iter().enumerate() {
                if *set {
                    image.draw_pixel(
                        x + i32::try_from(column).unwrap_or(0),
                        i32::try_from(row_index).unwrap_or(0),
                        Color::WHITE,
                    );
                }
            }
        }
        placements.push((glyph.codepoint, x, glyph.width));
        x += glyph.width + 1;
    }
    let texture = match raylib.load_texture_from_image(thread, &image) {
        Ok(texture) => texture,
        Err(error) => {
            eprintln!("Failed to build the pixel font: {error}");
            return None;
        }
    };
    texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
    let count = placements.len();
    // SAFETY: the glyph and rectangle arrays come from raylib's allocator (MemAlloc returns
    // zeroed memory), so UnloadFont frees them when the returned `Font` drops; the texture's
    // ownership moves into the font (`to_raw` releases it without unloading).
    unsafe {
        let recs = ffi::MemAlloc(u32::try_from(count * size_of::<ffi::Rectangle>()).ok()?)
            .cast::<ffi::Rectangle>();
        let infos = ffi::MemAlloc(u32::try_from(count * size_of::<ffi::GlyphInfo>()).ok()?)
            .cast::<ffi::GlyphInfo>();
        if recs.is_null() || infos.is_null() {
            return None;
        }
        for (index, (codepoint, x, width)) in placements.into_iter().enumerate() {
            *recs.add(index) = ffi::Rectangle {
                x: x as f32,
                y: 0.0,
                width: width as f32,
                height: GLYPH_HEIGHT as f32,
            };
            let info = &mut *infos.add(index);
            info.value = i32::try_from(u32::from(codepoint)).unwrap_or(63);
            info.offsetX = 0;
            info.offsetY = 0;
            info.advanceX = 0;
        }
        Some(Font::from_raw(ffi::Font {
            baseSize: GLYPH_HEIGHT,
            glyphCount: i32::try_from(count).unwrap_or(0),
            glyphPadding: 0,
            texture: texture.to_raw(),
            recs,
            glyphs: infos,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for (codepoint, source) in GLYPHS {
            assert!(seen.insert(*codepoint), "duplicate glyph {codepoint:?}");
            let rows: Vec<&str> = source.split(' ').collect();
            assert!(
                (CAP_HEIGHT as usize..=GLYPH_HEIGHT as usize).contains(&rows.len()),
                "{codepoint:?} has {} rows",
                rows.len()
            );
            let width = rows[0].len();
            assert!((1..=5).contains(&width), "{codepoint:?} is {width} wide");
            for row in rows {
                assert_eq!(row.len(), width, "{codepoint:?} has a ragged row");
                assert!(row.chars().all(|c| c == '#' || c == '.'), "{codepoint:?}");
            }
        }
        for c in ' '..='~' {
            assert!(seen.contains(&c), "printable ASCII {c:?} is missing");
        }
    }

    #[test]
    fn bold_glyphs_are_one_pixel_wider() {
        let regular = glyph_bitmaps(false);
        let bold = glyph_bitmaps(true);
        for (regular, bold) in regular.iter().zip(&bold) {
            if regular.codepoint == ' ' {
                assert_eq!(regular.width, bold.width);
            } else {
                assert_eq!(regular.width + 1, bold.width, "{:?}", regular.codepoint);
            }
            assert_eq!(bold.rows.len(), GLYPH_HEIGHT as usize);
        }
    }
}
