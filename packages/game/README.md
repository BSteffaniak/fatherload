# drillgame_game

Desktop raylib frontend for Drillgame.

Single-player and local split-screen run on `drillgame_netcode::HostSession<WorldState>` with
one local seat per player and no remote clients; that host is the only gameplay authority.

Frame loop (`local_game::LocalGame::frame`, driven by `app::run`):

1. Sample input per local seat (`app`).
2. Shell input (menus, modals, interiors, settings) updates `shell::ShellState`; menu choices
   become `ShellRequest`s (service/rescue actions, saves, loads, new game, extra seats).
3. Each seat's held controls and one-shot actions go to `runtime::Runtime`, which advances
   0..n fixed ticks from real time. Pause, title and interiors stop the host from ticking.
4. `render_snapshot::RenderSnapshot` is built from the world: one view per seat (camera follows
   that seat's rig), terrain, hazards, bombs, infrastructure, HUD values and this frame's
   de-duplicated gameplay events.
5. `rendering::GameRenderer` draws the snapshot plus shell state; audio plays the frame's cues.

Modules:

- `runtime` — `Runtime { Host, Client }`, `LocalHost`, `MAX_LOCAL_SEATS` (the only seat cap).
- `shell` — shell state and menu logic; reads the world, never mutates it.
- `render_snapshot` — immutable per-frame view and the N-view split-screen layout.
- `effects` — exactly-once event delivery, sound cues and presentation particles.
- `save` — format v4 (`WorldState` + play time); `save::legacy` migrates v1-v3.
- `input`, `input_mapping` — device sampling and seat input to controls/actions.

Online hosting/joining is temporarily hidden; it returns on the same runtime in the next step.
