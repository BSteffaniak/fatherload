# drillgame_netcode

Sans-IO multiplayer netcode for Drillgame (see `multiplayer-architecture-plan.md`,
sections 5-8, 11.2, 15 and 16).

- `NetSimulation`: the trait a gameplay simulation implements to be hosted and predicted.
- `HostSession`: authoritative fixed-tick host with local seats and remote clients.
- `ClientSession`: prediction, reconciliation, clock sync and remote interpolation.
- `protocol`: `ClientMessage` / `ServerMessage` and the versioned postcard codec.
- `LinkSim`: seeded deterministic lossy link used by the end-to-end harness.

No sockets, threads, async runtimes or wall clock: time is always passed in as a `Duration`.

The `harness` feature exposes a toy simulation, scripted bots and the harness.
