//! The streaming budget of a Deep Descent world (Wave 7 F8): the heap a V11 world holds once a
//! crew has explored it to the bottom, measured with a counting allocator.
//!
//! The browser build runs the same simulation in a 32-bit WASM heap (emscripten, memory growth
//! on, 2 GiB maximum); the simulation's share must stay inside
//! [`fatherload_simulation::terrain::WASM_TERRAIN_BUDGET_BYTES`]. Native pointers are twice as
//! wide, so the native measurement is an upper bound for the browser.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use fatherload_simulation::{
    PlayerId, TILE_SIZE, WorldState,
    descent::Descent,
    terrain::{TileKind, TilePosition, WASM_TERRAIN_BUDGET_BYTES},
    worldgen::{
        GeneratorVersion, WorldSettings,
        descent::{floor_row, layout},
    },
};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call forwards to the system allocator unchanged; the counters only observe.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded as is.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded as is.
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const PLAYER: PlayerId = PlayerId::new(1);

fn mib(bytes: usize) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a report")]
    let bytes = bytes as f64;
    bytes / f64::from(1_u32 << 20)
}

#[test]
fn an_explored_deep_descent_world_stays_inside_the_wasm_budget() {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(0xBEEF)
    };
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &settings,
        fatherload_simulation::SimulationParams::default(),
    );
    // A shaft from the surface to the floor and a gallery in every Descent.
    let floor = floor_row(&settings);
    for y in 6..floor - 2 {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: 60, y }, TileKind::Air);
    }
    let layout = layout(&settings);
    for descent in Descent::ALL {
        let (top, bottom) = layout.descent_rows(descent);
        let row = i32::midpoint(top, bottom);
        for x in -60..180 {
            for y in row..row + 3 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
    }
    // Ride the shaft down, streaming as the rig goes (every chunk on the way is generated).
    let mut peak_loaded = 0;
    for y in (6..floor - 2).step_by(16) {
        if let Some(rig) = world.player_mut(PLAYER) {
            rig.x = 60.5 * TILE_SIZE;
            #[allow(clippy::cast_precision_loss, reason = "rows are small")]
            let row = y as f32;
            rig.y = (row + 0.5) * TILE_SIZE;
        }
        world.stream_terrain_near_players();
        peak_loaded = peak_loaded.max(world.terrain().loaded_chunk_count());
    }
    let settled = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    let save = serde_json::to_string(world.terrain()).expect("json").len();
    println!(
        "explored V11 world ({} rows, {} edited tiles in {} chunks, peak {peak_loaded} chunks \
         loaded): heap {:.1} MiB settled, {:.1} MiB peak; terrain save {:.1} KiB; budget \
         {:.0} MiB",
        settings.height_tiles(),
        world.terrain().stored_tile_count(),
        world.terrain().edited_chunk_count(),
        mib(settled),
        mib(peak),
        f64::from(u32::try_from(save).unwrap_or(u32::MAX)) / 1024.0,
        mib(WASM_TERRAIN_BUDGET_BYTES),
    );
    assert!(peak_loaded <= 160, "{peak_loaded} chunks loaded at once");
    assert!(
        peak <= WASM_TERRAIN_BUDGET_BYTES,
        "peak heap {:.1} MiB",
        mib(peak)
    );
}
