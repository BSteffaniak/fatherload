# drillgame_simulation

Canonical, deterministic Drillgame gameplay simulation: `WorldState`, `simulate_tick`, terrain,
rigs, drilling, hazards, economy, contracts, expeditions, rescue, deterministic RNG, and the
simulation state hash.

This crate performs no IO and has no rendering, async runtime, or networking dependencies.

The tick rate is a session parameter (`SimulationParams`, default 60 Hz). Gameplay constants are
authored per second and derived per tick once at world creation (`TickTiming`). World state is
organized into replication sections (per-tick rig state and live entities, an op log of terrain
tile changes, and on-change per-player and shared sections) with serde payloads and stable
hashes, so a replica built from a keyframe plus section updates reaches the identical state hash.
