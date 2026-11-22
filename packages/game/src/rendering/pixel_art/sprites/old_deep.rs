//! Descent II, the Old Deep's creatures (Wave 8): one animated sprite per new
//! `fatherload_simulation::EnemyKind`. Asymmetric creatures face right (the renderer mirrors
//! them). Generated grids, hand-tuned to the shared palette.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A prism moth: glassy blue wings that throw a rainbow edge, a dark body and gold eyes
/// (wing beat).
pub const PRISM_MOTH: SpriteDef = SpriteDef {
    name: "enemy_prism_moth",
    legend: &[
        ('o', INK),
        ('b', DEEP_BLUE),
        ('s', SKY),
        ('i', ICE),
        ('w', BONE),
        ('r', ROSE),
        ('g', GOLD),
        ('d', NIGHT),
    ],
    frames: &[
        &[
            ".oo........oo..", //
            "oiso......osio.", //
            "oisso....ossio.", //
            "oiissooooossiio", //
            ".oissodddosiio.", //
            "..obsdgdgdsbo..", //
            "..orsodddosro..", //
            ".obsio.d.oisbo.", //
            ".oboo..o..oobo.", //
            "..o.........o..", //
        ],
        &[
            "...............", //
            "...............", //
            "..oo..ooo..oo..", //
            ".oisoodddoosio.", //
            "oiissdgdgdssiio", //
            "oisssodddosssio", //
            ".orsbo.d.obsro.", //
            "..oobo.o.oboo..", //
            "....oo...oo....", //
            "...............", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A fossil eel: a long bony eel, ivory ribs over a dark body, a toothed jaw (undulating).
pub const FOSSIL_EEL: SpriteDef = SpriteDef {
    name: "enemy_fossil_eel",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('k', SOIL),
        ('c', CREAM),
        ('w', BONE),
        ('e', AMBER),
    ],
    frames: &[
        &[
            "...............oooo..", //
            "..oo.......oooocwwoo.", //
            ".okko...ooodkcwcwewo.", //
            "okcdkooodkcdkcwcwwwwo", //
            "okcdkcdkcdkcdkdkowowo", //
            ".okkoodkcdkooodkoo.o.", //
            "..oo..ooooo...ooo....", //
            ".....................", //
        ],
        &[
            ".....................", //
            "..........ooo.oooo...", //
            ".oo....oodkcooocwwoo.", //
            "okkooodkcdkcdkcwcewo.", //
            "okcdkcdkcooodkcwwwwwo", //
            ".okcdkooo...ookowowo.", //
            "..ooo.........oo.o...", //
            ".....................", //
        ],
    ],
    frame_seconds: 0.2,
};

/// A jet ray: a broad violet ray with pale wingtips, a lavender belly line and a whip tail
/// (wing ripple).
pub const JET_RAY: SpriteDef = SpriteDef {
    name: "enemy_jet_ray",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('w', BONE),
        ('e', MINT),
    ],
    frames: &[
        &[
            "........ooooooo........", //
            "oo...oopvvvvvvvpoo...oo", //
            "owoopvvvvlvlvlvvvvpoowo", //
            ".olvvvvvvvvevvvvvvvvlo.", //
            "..oolvvvplllllpvvvloo..", //
            "....oooopppppppoooo....", //
            "..........ooooo........", //
            "............oooooo.....", //
            "..................oo...", //
        ],
        &[
            ".......................", //
            "........ooooooo........", //
            "...oooopvvvvvvvpoooo...", //
            "oolvvvvvvlvlvlvvvvvvloo", //
            "owvvvvvvvvvevvvvvvvvvwo", //
            ".oooolvpllllllpvloooo..", //
            "......oopppppppoo......", //
            "..........oooooooo.....", //
            "..................ooo..", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A glow grub: a fat segmented grub glowing lime, dark bands and tiny legs (crawl).
pub const GLOW_GRUB: SpriteDef = SpriteDef {
    name: "enemy_glow_grub",
    legend: &[
        ('o', INK),
        ('p', PINE),
        ('m', MOSS),
        ('l', LIME),
        ('t', MINT),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "...ooooooo...", //
            "..olltlltloo.", //
            ".olmlmlmlmkto", //
            "olmpmpmpmlkko", //
            ".ompmpmpmpmo.", //
            "..okokokoko..", //
            ".............", //
        ],
        &[
            ".............", //
            "...ooooooo...", //
            "..olltlltloo.", //
            ".olmlmlmlmkto", //
            "olmpmpmpmpkko", //
            ".okokokokoo..", //
            ".............", //
        ],
    ],
    frame_seconds: 0.22,
};

/// The Glowmother: a vast brood mother, a bloated lime abdomen veined dark, a carapace head
/// with mint eyes and mandibles, egg sacs glowing along her flank (breathing).
pub const GLOWMOTHER: SpriteDef = SpriteDef {
    name: "enemy_glowmother",
    legend: &[
        ('o', INK),
        ('k', NIGHT),
        ('p', PINE),
        ('m', MOSS),
        ('l', LIME),
        ('t', MINT),
        ('w', BONE),
        ('r', ROSE),
    ],
    frames: &[
        &[
            "............oooooooooo............", //
            ".........ooolllllllllloo..........", //
            ".......oollllltllltlllllloo.......", //
            "......olllmlllllmllllllmllloo.....", //
            ".....ollmlllmlllllllmlllllllo.....", //
            "....ollllllllpllllllllllmllllooo..", //
            "...ollmllltllllllmlllllllllokkkko.", //
            "...olllllllllmllllltlllllloktkktko", //
            "..ollllmllllllllllllllmlllokkkkko.", //
            "..olllllllpllllmlllllllllllokwkwko", //
            "..ollmllllllllllllllllllllooowowo.", //
            "..ollllltlllllllmlllllmllo..o.o.o.", //
            "...olllllllmllllllllllllo.........", //
            "...ollmllllllllllltllllo..........", //
            "....olllllllllllllllloo...........", //
            ".....oollmlllllllmlloo............", //
            ".......ooollllllloo...............", //
            "........okoko.okoko...............", //
            ".......ok.ok..ok.ok...............", //
            "......ok..ok.ok..ok...............", //
            "..................................", //
        ],
        &[
            ".............oooooooo.............", //
            "..........oooolllllllloo..........", //
            "........oolllllltlllltllloo.......", //
            ".......olllmllllllmlllllmlloo.....", //
            "......ollmlllmlllllllmllllllo.....", //
            ".....olllllllpllllllllllmllllooo..", //
            "....ollmllltllllllmllllllllokkkko.", //
            "....olllllllllmllllltllllloktkktko", //
            "...ollllmllllllllllllllmlllokkkkko", //
            "...olllllllpllllmllllllllllokwkwko", //
            "...ollmlllllllllllllllllllooowowo.", //
            "...ollllltlllllllmlllllmllo.o.o...", //
            "....olllllllmllllllllllllo........", //
            "....ollmllllllllllltllllo.........", //
            ".....olllllllllllllllloo..........", //
            "......oollmllllllllmloo...........", //
            "........ooollllllloo..............", //
            "........okoko.okoko...............", //
            "........ok.ok.ok.ok...............", //
            ".......ok..ok.ok..ok..............", //
            "..................................", //
        ],
    ],
    frame_seconds: 0.5,
};

/// A piston warden: a squat builder automaton, bronze plates, a piston ram for an arm and an
/// amber eye slot (idle, wind-up with the ram drawn back).
pub const PISTON_WARDEN: SpriteDef = SpriteDef {
    name: "enemy_piston_warden",
    legend: &[
        ('o', INK),
        ('k', SOIL_DARK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('s', STEEL),
        ('l', STEEL_LIGHT),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "....oooooooo........", //
            "...orccccccro.......", //
            "..orcaaaaaacro......", //
            "..orccccccccro......", //
            "..oorrrrrrrroo......", //
            ".orcccrccrcccrooo...", //
            ".orcgcrccrcgcrslo...", //
            ".orcccrccrcccrslo...", //
            ".orrrrrrrrrrrrslo...", //
            "..orcccccccccrooo...", //
            "..orkkkkkkkkkro.....", //
            "..ooskso.oskso......", //
            "...osso...osso......", //
            "..ooooo..ooooo......", //
        ],
        &[
            "....oooooooo........", //
            "...orccccccro.......", //
            "..orcaaaaaacro......", //
            "..orccccccccro......", //
            "..oorrrrrrrroo......", //
            ".orcccrccrcccro.....", //
            ".orcgcrccrcgcrooo...", //
            ".orcccrccrcccrsllllo", //
            ".orrrrrrrrrrrrooo...", //
            "..orcccccccccro.....", //
            "..orkkkkkkkkkro.....", //
            "..ooskso.oskso......", //
            "...osso...osso......", //
            "..ooooo..ooooo......", //
        ],
    ],
    frame_seconds: 0.3,
};

/// A void jelly: a translucent violet bell with a pink core and trailing tendrils (pulse).
pub const VOID_JELLY: SpriteDef = SpriteDef {
    name: "enemy_void_jelly",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('k', PINK),
        ('w', BONE),
    ],
    frames: &[
        &[
            "....ooooo....", //
            "..oolllllo...", //
            ".ollvvvvvllo.", //
            "olvvvkkkvvvlo", //
            "olvvkwwkkvvlo", //
            "olvvvkkkvvvlo", //
            ".oopvvvvvpoo.", //
            "..opopopopo..", //
            "..o.p.p.p.o..", //
            "...p.p.p.p...", //
            "..p.p...p.p..", //
            "...p.....p...", //
            ".............", //
            ".............", //
            ".............", //
        ],
        &[
            ".............", //
            "....ooooo....", //
            "..oolllllloo.", //
            ".ollvvvvvvllo", //
            "olvvvkkkvvvlo", //
            "olvvkwwkkvvlo", //
            "oopvvkkkvvpoo", //
            "..ooppppppo..", //
            "...p.p.p.p...", //
            "..p.p.p.p.p..", //
            "...p..p..p...", //
            "..p...p...p..", //
            "...p.....p...", //
            ".............", //
            ".............", //
        ],
    ],
    frame_seconds: 0.35,
};

/// A magma salamander: a low-slung black-and-ember lizard, glowing cracks down its back and a
/// fiery throat (walk, wind-up with the throat aglow).
pub const MAGMA_SALAMANDER: SpriteDef = SpriteDef {
    name: "enemy_magma_salamander",
    legend: &[
        ('o', INK),
        ('k', NIGHT),
        ('m', MAROON),
        ('r', RUST),
        ('n', ORANGE),
        ('g', GOLD),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "......................", //
            "..........ooooooo.....", //
            "oo....ooookmkmkmkoooo.", //
            "okoooknkmnkmnkmnkkwko.", //
            ".okkmkkkkkkkkkkkkkkko.", //
            "..ookrkrrkrrkrrkkoooo.", //
            "....okko.okko.okko....", //
            "...ok.o..ok....ok.o...", //
            "......................", //
        ],
        &[
            "......................", //
            "..........ooooooo.....", //
            "oo....ooookmkmkmkoooo.", //
            "okoooknkmnkmnkmnkkwko.", //
            ".okkmkkkkkkkkkkkkknngo", //
            "..ookrkrrkrrkrrkknnno.", //
            "....okko.okko.okkooo..", //
            ".....ok.o.okko.o.ok...", //
            "......................", //
        ],
    ],
    frame_seconds: 0.24,
};
