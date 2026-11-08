use crate::economy::SurfaceZone;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceBuilding {
    pub zone: SurfaceZone,
    pub label: &'static str,
    pub tile_x: i32,
    pub tile_width: i32,
}

pub const SURFACE_BUILDING_WIDTH: i32 = 8;
pub const SURFACE_BUILDINGS: [SurfaceBuilding; 8] = [
    SurfaceBuilding {
        zone: SurfaceZone::Fuel,
        label: "FUEL",
        tile_x: 48,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Repair,
        label: "REPAIR",
        tile_x: 66,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Depot,
        label: "DEPOT",
        tile_x: 84,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Headquarters,
        label: "HQ",
        tile_x: 102,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Shop,
        label: "SHOP",
        tile_x: 120,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Bank,
        label: "BANK",
        tile_x: 138,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Explosives,
        label: "BOOM",
        tile_x: 156,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
    SurfaceBuilding {
        zone: SurfaceZone::Salvage,
        label: "SALVAGE",
        tile_x: 174,
        tile_width: SURFACE_BUILDING_WIDTH,
    },
];

#[must_use]
pub fn surface_building_at_tile(tile_x: i32) -> Option<SurfaceBuilding> {
    SURFACE_BUILDINGS.iter().copied().find(|building| {
        tile_x >= building.tile_x && tile_x < building.tile_x + building.tile_width
    })
}

#[must_use]
pub fn building_foundation_at(tile_x: i32, tile_y: i32) -> bool {
    let Some(building) = surface_building_at_tile(tile_x) else {
        return false;
    };

    match tile_y {
        5 | 6 => true,
        7 => staggered_foundation_tile(building, tile_x),
        _ => false,
    }
}

const fn staggered_foundation_tile(building: SurfaceBuilding, tile_x: i32) -> bool {
    let local_x = tile_x - building.tile_x;
    let stagger = match building.zone {
        SurfaceZone::Fuel => 0,
        SurfaceZone::Repair => 3,
        SurfaceZone::Depot => 6,
        SurfaceZone::Headquarters => 1,
        SurfaceZone::Shop => 4,
        SurfaceZone::Bank => 7,
        SurfaceZone::Explosives => 2,
        SurfaceZone::Salvage => 5,
    };
    let shifted_x = (local_x + stagger).rem_euclid(building.tile_width);

    matches!(shifted_x, 0 | 1 | 4 | 5 | 6)
}
