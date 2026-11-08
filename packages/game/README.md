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

- `online` — online play on the same runtime:
  - `bridge` maps netcode `ClientMessage`/`ServerMessage` to `quic_transport` bytes (client
    inputs as datagrams, everything else on the reliable control stream);
  - `host::HostNet` feeds a `HostSession` from the network thread (non-blocking drain each frame)
    and publishes the session on the LAN;
  - `client::OnlineClient` wraps a `ClientSession` (`Runtime::Client`): predicted world for
    terrain/HUD, predicted + smoothed local rig, interpolated remote rigs;
  - `lan` runs mDNS browsing and descriptor fetches on helper threads;
  - `bots` + `headless`: `drillgame --headless-host [addr] [--lan] [--duration S]` and
    `drillgame --bot-client <addr> [--count N] [--duration S]` (no window, periodic stats lines).

Online flow: Title > Host Online Game (lobby with address, descriptor copy, roster, Start) or
Join Online Game (address, pasted descriptor, or LAN list), lobby ready toggle, Leave/End,
reconnect with the token after a dropped connection, and notices when the host ends the session,
rejects the join, or the connection is lost. A running single-player game can be opened from the
pause menu. Online pause is a local overlay (the world keeps running). Only the host saves (manual
and a periodic autosave); joined clients cannot save or load. F3 toggles the network overlay; in
dev builds F4 / Page Up / Page Down tune live `NetTuning` values on the host. The
`DRILLGAME_NET_SIM` link conditioner applies to every online session.
