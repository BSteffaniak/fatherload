//! Wave 8 progression art: the Pressure Rating and Headlamp lines, the outposts' workshop, and
//! the layer resources (one shape per kind of material, coloured for the strata they come
//! from).

use fatherload_simulation::descent::Stratum;

#[allow(
    clippy::wildcard_imports,
    reason = "sprite legends name colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// Pressure Rating: a dial with its needle in the red, on a riveted seal ring.
pub const PRESSURE_RATING: SpriteDef = SpriteDef {
    name: "icon_pressure_rating_line",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('c', CREAM),
        ('r', RED),
        ('a', AMBER),
    ],
    frames: &[&[
        "..ooooo..",
        ".ossssso.",
        "osccccaso",
        "osccc.rso",
        "oscccrcso",
        "oscc.ccso",
        "osdcccdso",
        ".osdddso.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

/// Headlamp: a lamp housing throwing a cone of light.
pub const HEADLAMP: SpriteDef = SpriteDef {
    name: "icon_headlamp_line",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('l', STEEL_LIGHT),
        ('y', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        ".......cc",
        "......ccy",
        "oooo.ccy.",
        "olssoccy.",
        "olsyyyyyy",
        "olssoccy.",
        "oooo.ccy.",
        "......ccy",
        ".......cc",
    ]],
    frame_seconds: 0.0,
};

/// The outposts' workshop: an anvil with a hammer over it.
pub const WORKSHOP: SpriteDef = SpriteDef {
    name: "icon_workshop_anvil",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('k', STEEL_DARK),
        ('l', LOAM),
        ('r', ORANGE),
    ],
    frames: &[&[
        "...ooo...",
        "...ods.r.",
        "....l..r.",
        "....l....",
        "osssssso.",
        ".okdddko.",
        "...odo...",
        "..oddko..",
        ".ooooooo.",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource of fibres (roots, basalt wool, spore silk, sinew): a tied bundle.
pub const LAYER_FIBRE: SpriteDef = SpriteDef {
    name: "layer_fibre",
    legend: &[('o', INK), ('a', MOSS), ('b', LEAF), ('t', SAND)],
    frames: &[&[
        "..o.o.o..",
        ".oab.ba..",
        ".obabab..",
        "..tttt...",
        "..babao..",
        ".obabab..",
        ".oabbaab.",
        "obab.bab.",
        ".o.o..o..",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource worked like stone or wood (kiln clay, timber, shale, heartwood): a block.
pub const LAYER_LUMP: SpriteDef = SpriteDef {
    name: "layer_lump",
    legend: &[('o', INK), ('h', SAND), ('m', CLAY), ('d', SOIL)],
    frames: &[&[
        ".........",
        "..ooooo..",
        ".ohhhhmo.",
        "ohhmhmmdo",
        "ohmmmmddo",
        "ommdmddo.",
        "omddddo..",
        ".ooooo...",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource that is a gem or glass (crystal lens, void glass, pearls, lure light).
pub const LAYER_GEM: SpriteDef = SpriteDef {
    name: "layer_gem",
    legend: &[
        ('o', INK),
        ('w', CREAM),
        ('l', SKY_LIGHT),
        ('m', SKY),
        ('d', OCEAN),
    ],
    frames: &[&[
        "....o....",
        "...olo...",
        "..owlmo..",
        ".owllmmo.",
        "olllmmmdo",
        ".olmmmdo.",
        "..ommdo..",
        "...odo...",
        "....o....",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource that is a resin or jelly (fossil amber, royal jelly): a glossy drop.
pub const LAYER_RESIN: SpriteDef = SpriteDef {
    name: "layer_resin",
    legend: &[
        ('o', INK),
        ('w', CREAM),
        ('g', GOLD),
        ('a', AMBER),
        ('r', RUST),
    ],
    frames: &[&[
        "....o....",
        "...oao...",
        "..ogaao..",
        ".ogwgaao.",
        ".owgaaro.",
        "ogggaarro",
        "oaaaarrro",
        ".oaarrro.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource that is a hot shard (isotopes, mantle core, living tissue).
pub const LAYER_SHARD: SpriteDef = SpriteDef {
    name: "layer_shard",
    legend: &[
        ('o', INK),
        ('w', CREAM),
        ('l', LIME),
        ('m', MOSS),
        ('p', PINE),
    ],
    frames: &[&[
        ".o.......",
        ".olo.....",
        "..olo..o.",
        "..owlo.lo",
        "..olllolo",
        ".olmllmo.",
        ".omlmmpo.",
        "..ompppo.",
        "...oooo..",
    ]],
    frame_seconds: 0.0,
};

/// A layer resource that is builder machinery: a toothed relic cog.
pub const LAYER_RELIC: SpriteDef = SpriteDef {
    name: "layer_relic",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('c', COPPER),
        ('t', TEAL),
    ],
    frames: &[&[
        "...o.o...",
        ".oosdsoo.",
        ".osdddso.",
        "osddtddso",
        ".dctttcd.",
        "osddtddso",
        ".osdcdso.",
        ".oosdsoo.",
        "...o.o...",
    ]],
    frame_seconds: 0.0,
};

/// The sprite of `stratum`'s layer resource.
#[must_use]
pub const fn layer_resource(stratum: Stratum) -> &'static SpriteDef {
    use Stratum as S;
    match stratum {
        S::Topsoil | S::BasaltColumns | S::FungalForest | S::FleshCaverns | S::MegafaunaJungle => {
            &LAYER_FIBRE
        }
        S::ClayBeds | S::GreyStone | S::SlateShelves | S::RootLabyrinth | S::GeyserFields => {
            &LAYER_LUMP
        }
        S::FossilOceans | S::TheHive | S::InnerSky | S::TimeSludge => &LAYER_RESIN,
        S::RadioactiveHollow | S::MantleStormZone | S::TheSunforge | S::CoreOcean | S::TheHeart => {
            &LAYER_SHARD
        }
        S::AncientMachineLayer
        | S::PixelRot
        | S::BuriedMegacity
        | S::EngineHalls
        | S::ArchiveOfStars
        | S::SleepingGarrison
        | S::TheSealGate => &LAYER_RELIC,
        S::CrystalFaults
        | S::PressureCathedrals
        | S::VoidGeodeFields
        | S::GlowSea
        | S::UndergroundOcean
        | S::BrineGlaciers
        | S::MerrowCity
        | S::PearlAbyss
        | S::UpsideDownIsles
        | S::MirrorStrata
        | S::GravityShear
        | S::EchoCaves
        | S::PrismDesert
        | S::DarkMatterVeins
        | S::SingularityFields => &LAYER_GEM,
    }
}
