//! World props: falling boulders, bombs, the lost-cargo crate and placed infrastructure.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

pub const BOULDER: SpriteDef = SpriteDef {
    name: "world_boulder",
    legend: &[
        ('o', INK),
        ('l', STONE_LIGHT),
        ('m', STONE),
        ('d', STONE_DARK),
    ],
    frames: &[&[
        "...oooooo...", //
        "..ollmmmdo..", //
        ".olmmmmmmdo.", //
        "olmmmmmdmmdo", //
        "olmmdmmmmmdo", //
        "ommmmmmmmddo", //
        "ommmmmdmmddo", //
        "omdmmmmmdddo", //
        ".odmmmmdddo.", //
        "..oddddddo..", //
        "...oooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// A bomb with a fizzing fuse (two spark frames).
pub const BOMB: SpriteDef = SpriteDef {
    name: "world_bomb",
    legend: &[
        ('o', INK),
        ('n', SLATE_DARK),
        ('w', SLATE_LIGHT),
        ('S', STEEL),
        ('R', RED),
        ('g', GOLD),
        ('c', CREAM),
        ('r', ORANGE),
    ],
    frames: &[
        &[
            "......g...", //
            ".....c.r..", //
            ".....o....", //
            "....oSo...", //
            "..oooSooo.", //
            ".onwnnnnno", //
            "onwnnnnnno", //
            "onnnnnnnno", //
            "oRRRRRRRRo", //
            "onnnnnnnno", //
            ".onnnnnno.", //
            "..oooooo..", //
        ],
        &[
            "....r.....", //
            "......c...", //
            ".....o....", //
            "....oSo...", //
            "..oooSooo.", //
            ".onwnnnnno", //
            "onwnnnnnno", //
            "onnnnnnnno", //
            "oRRRRRRRRo", //
            "onnnnnnnno", //
            ".onnnnnno.", //
            "..oooooo..", //
        ],
    ],
    frame_seconds: 0.08,
};

/// The crate of cargo a stranded rig left behind.
pub const CARGO_CRATE: SpriteDef = SpriteDef {
    name: "world_cargo_crate",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', CLAY),
        ('d', LOAM),
        ('x', SOIL),
        ('S', STEEL),
    ],
    frames: &[&[
        "oooooooooooo", //
        "oSllllllllSo", //
        "olxmmmmmmxdo", //
        "olmxmmmmxmdo", //
        "olmmxmmxmmdo", //
        "olmmmxxmmmdo", //
        "olmmxmmxmmdo", //
        "olmxmmmmxmdo", //
        "olxmmmmmmxdo", //
        "oSddddddddSo", //
        "oooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A downward arrow bobbing over the lost cargo.
pub const MARKER_ARROW: SpriteDef = SpriteDef {
    name: "world_marker_arrow",
    legend: &[('o', INK), ('g', GOLD), ('c', CREAM), ('a', AMBER)],
    frames: &[&[
        "..ooooo..", //
        "..ocgao..", //
        "..ogaao..", //
        "oooggaooo", //
        "ocgggaaao", //
        ".ogggaao.", //
        "..ogaao..", //
        "...oao...", //
        "....o....", //
    ]],
    frame_seconds: 0.0,
};

pub const SIGNAL_RELAY: SpriteDef = SpriteDef {
    name: "infra_signal_relay",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('L', STEEL_LIGHT),
        ('S', STEEL),
        ('d', STEEL_DARK),
        ('a', SKY_LIGHT),
    ],
    frames: &[
        &[
            ".....rr.....", //
            ".....oo.....", //
            "..a..LL..a..", //
            ".a...LS...a.", //
            ".a..oLSo..a.", //
            "..a.oLSo.a..", //
            "....oLSo....", //
            "....oLSo....", //
            "...ooLSoo...", //
            "...oLLSSo...", //
            "..ooLLSSoo..", //
            "..oLLLSSSo..", //
            ".ooLLLSSSoo.", //
            ".oLLLLSSSSo.", //
            "oooooooooooo", //
            "oddddddddddo", //
        ],
        &[
            ".....oo.....", //
            ".....oo.....", //
            ".....LL.....", //
            ".....LS.....", //
            "....oLSo....", //
            "....oLSo....", //
            "....oLSo....", //
            "....oLSo....", //
            "...ooLSoo...", //
            "...oLLSSo...", //
            "..ooLLSSoo..", //
            "..oLLLSSSo..", //
            ".ooLLLSSSoo.", //
            ".oLLLLSSSSo.", //
            "oooooooooooo", //
            "oddddddddddo", //
        ],
    ],
    frame_seconds: 0.5,
};

pub const SURVEY_DRONE: SpriteDef = SpriteDef {
    name: "infra_survey_drone",
    legend: &[
        ('o', INK),
        ('l', LEAF),
        ('d', MOSS),
        ('G', MINT),
        ('w', BONE),
    ],
    frames: &[
        &[
            "oooo....oooo", //
            "..o......o..", //
            "..oooooooo..", //
            ".ollllllldo.", //
            ".olGwlllldo.", //
            ".odddddddo..", //
            "..oooooooo..", //
            "....o..o....", //
        ],
        &[
            ".oo......oo.", //
            "..o......o..", //
            "..oooooooo..", //
            ".ollllllldo.", //
            ".olGwlllldo.", //
            ".odddddddo..", //
            "..oooooooo..", //
            "....o..o....", //
        ],
    ],
    frame_seconds: 0.06,
};

pub const CARGO_LIFT: SpriteDef = SpriteDef {
    name: "infra_cargo_lift",
    legend: &[
        ('o', INK),
        ('a', GOLD),
        ('S', STEEL),
        ('c', STEEL_LIGHT),
        ('l', SAND),
        ('m', CLAY),
        ('d', LOAM),
    ],
    frames: &[&[
        "oooooooooooo", //
        "oaaaaaaaaaao", //
        "oooooooooooo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSo..cc..oSo", //
        "oSooooooooSo", //
        "oSolmmmmdoSo", //
        "oSommmmmdoSo", //
        "oSooooooooSo", //
        "oSo......oSo", //
        "oooo....oooo", //
    ]],
    frame_seconds: 0.0,
};

pub const TUNNEL_SUPPORT: SpriteDef = SpriteDef {
    name: "infra_tunnel_support",
    legend: &[('o', INK), ('l', SAND), ('m', LOAM), ('d', SOIL)],
    frames: &[&[
        "oooooooooooooooo", //
        "ollllllllllllldo", //
        "oddddddddddddddo", //
        "oooooooooooooooo", //
        "olmoo......oolmo", //
        "olmo.o....o.olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "olmo........olmo", //
        "oldo........oldo", //
        "oooo........oooo", //
    ]],
    frame_seconds: 0.0,
};

pub const PUMP_STATION: SpriteDef = SpriteDef {
    name: "infra_pump_station",
    legend: &[
        ('o', INK),
        ('B', OCEAN),
        ('w', SKY_LIGHT),
        ('G', GOLD),
        ('d', DEEP_BLUE),
        ('S', STEEL),
    ],
    frames: &[&[
        "....oooo......", //
        "....oSSo......", //
        "..oooSSooo....", //
        "..oBBBBBBo....", //
        "..oBwBBBBooooo", //
        "..oBBBBBBSSSSo", //
        "..oBBGBBBooooo", //
        "..oBBBBBBo....", //
        "..oddddddo....", //
        ".oooooooooo...", //
        ".oSSSSSSSSo...", //
        ".oooooooooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const ORE_PROCESSOR: SpriteDef = SpriteDef {
    name: "infra_ore_processor",
    legend: &[
        ('o', INK),
        ('P', PLUM),
        ('V', VIOLET),
        ('L', LAVENDER),
        ('g', GOLD),
        ('a', ORANGE),
        ('S', STEEL),
    ],
    frames: &[
        &[
            "oooooooo......", //
            "oPPPPPPo......", //
            ".oPPPPo.......", //
            "..oPPo........", //
            "oooooooooooooo", //
            "oVVVVVVVVVVVVo", //
            "oVLLVVVVooooVo", //
            "oVLVVVVVoggoVo", //
            "oVVVVVVVoaaoVo", //
            "oVVVVVVVooooVo", //
            "oPPPPPPPPPPPPo", //
            "oooooooooooooo", //
            ".oSo......oSo.", //
            ".ooo......ooo.", //
        ],
        &[
            "oooooooo......", //
            "oPPPPPPo......", //
            ".oPPPPo.......", //
            "..oPPo........", //
            "oooooooooooooo", //
            "oVVVVVVVVVVVVo", //
            "oVLLVVVVooooVo", //
            "oVLVVVVVoaaoVo", //
            "oVVVVVVVoggoVo", //
            "oVVVVVVVooooVo", //
            "oPPPPPPPPPPPPo", //
            "oooooooooooooo", //
            ".oSo......oSo.", //
            ".ooo......ooo.", //
        ],
    ],
    frame_seconds: 0.2,
};

/// Bedrock beyond the edges and below the floor of the world (tiled): the same obsidian slabs,
/// seams and violet facets as the bedrock floor's tile art.
pub const BEDROCK: SpriteDef = SpriteDef {
    name: "world_bedrock",
    legend: &[
        ('n', NIGHT),
        ('o', INK),
        ('s', SLATE_DARK),
        ('p', PLUM),
        ('l', LAVENDER),
    ],
    // Generated from the bedrock tile art's faceted-slab recipe (`tile_art::bedrock_pixel`) on a
    // 4x4 grid of 11-pixel cells that wraps, so the fill tiles without visible repeats or seams.
    frames: &[&[
        "oossnnllnnonnnnnopppssssnnnnoosssnnnnnnosnnn", //
        "oosnnnnnnnoooooooppssssnnnnnoossnnnnnnnonnnn", //
        "oonnnnnnnoppssssnossssnnnnnnoosnnnnnnnnonnnn", //
        "oonnnnnnoppssssnnnossnnnnnnnoonnnnnnnnnonnnn", //
        "oonnnnloppssssnnnnnonnnnnnnnoonnnnnnnnnonnnn", //
        "oonnnnoplssssnlnnnnnonnnnnnnoonnnnnnnnnonnln", //
        "oonnnoppssssnnnnnnnnnonnnnnnoonnlnnnnnnonnnn", //
        "oonnnopssssnnnnnnnnnnonnnnnnooonnnnnnnnonnnn", //
        "oonnnossssnnnnoooooooonnnnnoppoonnnooooonnnn", //
        "oonnnoooooonnosssnnnnnonnnoppppoonooooooonnn", //
        "ooooopppppsoosssnnnnnnnonoppppppooopppppoonn", //
        "pppppppppssosssnnnnnnnnnoppppppssopppppsssop", //
        "ppppppppsssossnnnnnnnnnnopppppsssoppppssssop", //
        "pppppppssssnonnnnnnnnnnopppppssssopppssssnop", //
        "ppppppssssnnnonnnnnnnnopppppssssnoppsslsnnop", //
        "pppppssssnnnnonnnnnnnopppppssssnnopssssnnnno", //
        "opppssssnnnnnonnnnnnopppppssssnnnossssnnnnnn", //
        "nopssssnnnnnnonnnnnopppppssssnnnnosssnnnnnnn", //
        "nlosssnnnnnnnonnnnnoppppssssnnnnosssnnnnnnnn", //
        "nnnooooooonnnonnnnnopppssssnnnnosssnnnnnnnnn", //
        "nnopppppppoooonnnnopooosssnnnnnossnnnnnnnnnn", //
        "oopppppppssssnooooppopposnnnnnnosnnnnnoooooo", //
        "ooppppppssssnnnnnoppoppponnnnnnonnnnnooooooo", //
        "oopppppssssnnnnnnoppopppponnnnnonnnnoossssnn", //
        "ooppppssssnnnnnnnoppopppssonnnnoooooossssnnn", //
        "oopppssssnnnnnnnnoppoppssssonnoooooossssnnnn", //
        "noopssssnnnnnnnnopppopssssnooooppppssssnnnnn", //
        "nnoooosnnnnnnnnopppsossssnnoooppppssssnnnnnn", //
        "nnooooolnnnnnlopppssosssnnnooooppssssnnnnnnn", //
        "noppppoonnnnnopppsssossnnnnooooossssnnnnnnnn", //
        "oppppssoonnnopppssssosnnnnnoooooossnnnnnnnnn", //
        "ppppssssoooopppssssnnonnnnossssnoonnnnnnnnno", //
        "pppssssnnoooopssssnnnonlnossssnnnoonnnnnnnop", //
        "ppssssnnnnnooosssnnnnonnossssnnnnnoonnnnnopp", //
        "pssssnnnnnnoooosnnnnnonossssnnnnnnnoonnnoppp", //
        "ssssnnnnnnoopssonnnnnoossssnnnnnnnnnoonnoooo", //
        "ossnnnnnnoopssssonnnnoosssnnnnnnnooooonopppp", //
        "sonnnnnnoopssssnnonnnoossnnnnnnnoooooooppppp", //
        "ssooonnoopssssnnnnonnoooonnnnnnooppssoppppps", //
        "ssoooooopssssnnnnnnoopppponnnnooppsssoppppss", //
        "snoosoopssssnnnnnnnnopppssonnooppssssopppsss", //
        "nnoosssosssnnnnnnnnoppplsssoooppssssnoppssss", //
        "nnoossnnosnnnnnnnnopppssssnnoopssssnnopssssn", //
        "noossnnnnonnnnnnnopppssssnnnoossssnnnnosssnn", //
    ]],
    frame_seconds: 0.0,
};
