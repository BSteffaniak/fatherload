# Fatherload

A Rust-native 2D mining game mechanically inspired by classic drill/mining games, with original names, procedural programmer art, and a desktop-first initial target.

## Development

```bash
cargo fmt
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo run --bin fatherload
```

With Nix:

```bash
nix develop -c cargo run --bin fatherload
```

### Web (raylib / Emscripten)

The browser build keeps the Rust simulation, raylib renderer, audio and local split-screen.
It currently supports **offline single-player and local multiplayer**, not online/LAN play.
It plays on phones and tablets with touch controls (see Controls), drawn at the device's
pixel density. No replacement engine or separate JavaScript gameplay is used.

Build using the locked flake's web shell (Emscripten and the Rust target are provided by Nix):

```bash
nix develop .#web --command bash scripts/build-web.sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory target/web
# Open http://localhost:8080; the original game menu loads automatically.
```

Deploy the contents of `target/web/` together to a static HTTPS host. Serve `.wasm` as
`application/wasm`. Do not open the HTML directly as a `file://` URL. The web shell is separate
from the desktop shell to avoid native SDK/linker flags leaking into cross-compilation;
Emscripten's writable cache stays under `target/`.

The browser runs frames through `requestAnimationFrame` (no timer sleeps or Asyncify);
simulation uses bounded animation timestamps and resets elapsed time after tab suspension.
The first click or keypress unlocks browser audio. WASD and arrows work as on desktop; in two-player mode,
WASD controls player 1 and arrows control player 2. Controller support uses raylib's browser
backend; physical controller mappings still need device/browser testing. Fullscreen is offered
by the web page, and the canvas render buffer follows the available window size.

Saves/settings live in this site's IndexedDB, not in your desktop save folder. Save through
the pause menu and wait for **Saved in this browser** before closing. A periodic autosave and
a best-effort checkpoint on tab suspension help, but abrupt browser/process termination can
still lose unsaved progress. Hidden tabs stop advancing gameplay. Clearing site data removes
browser saves; saves are scoped to the site's origin and do not sync between devices. Storage
restore errors block startup rather than silently starting over; persistence errors remain
visible and retryable. Private browsing/storage restrictions can prevent persistence.

Browser acceptance checks (Node comes from the same web shell):

```bash
nix develop .#web
npm ci --prefix web
(cd web && npx playwright install chromium)
npm --prefix web test
# Or use an installed Chrome:
# PLAYWRIGHT_CHROMIUM_EXECUTABLE=/path/to/chrome npm --prefix web test
```

Frame timing in staged states (night, sky chunks, the Moon, tombs and crypts):
`--screenshot-scenes <dir> --export-saves --only ...` writes those states as saves and
`node scripts/perf-web.cjs <dir>` times every animation frame of the WASM build in each (see
`packages/game/README.md`, "Browser frame timing").

The test serves the built bundle on a temporary loopback port, checks gameplay, exact save
restoration, resume, suspension checkpoint, two-seat split-screen, resizing and runtime errors.
Screenshots go to `target/web-test/`. Storage failure/transaction-ordering checks also run.
Headless audio initialization is checked; audible quality and Safari/Firefox/mobile acceptance
still require hands-on testing.

### Production hosting

The CI-built browser release targets `https://fatherload.hyperchad.dev` on Cloudflare
Pages. See [PRODUCTION.md](PRODUCTION.md) for protected deployment, secret provisioning,
live smoke tests and rollback. Hosting requires Cloudflare credentials and approval
of the first deployment.

### Windows

Builds natively with the MSVC toolchain (`x86_64-pc-windows-msvc`). raylib is compiled from C
source, so it needs:

- Visual Studio 2022 Build Tools with the C++ workload (provides `cl` and a bundled CMake under
  `Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin`; put that or any CMake on `PATH`).
- `libclang.dll` for raylib's bindgen step: install LLVM, or unpack the `libclang` wheel from
  PyPI, and set `LIBCLANG_PATH` to the folder containing the DLL.

```powershell
$env:LIBCLANG_PATH = "C:\path\to\folder\with\libclang"
cargo build --release -p fatherload_game   # target\release\fatherload.exe
```

Saves and settings go to `%LOCALAPPDATA%\fatherload`.

## Current Gameplay

Drill below the surface, gather minerals and artifacts, manage fuel/hull/cargo/debt, complete HQ contracts, buy upgrades, use scanners and bombs, and push deeper toward the Star Core. Deeper layers add harder rock, richer ore, artifacts, gas, lava, magma vents, pressure pockets, explosive pockets, falling boulders, cave-ins, and stronger heat pressure.

The dark caves below the topsoil are home to enemies, creatures and machines that grow tougher with depth: bat swarms, burrowers lying in wait in dens in the rock, acid spitters, rock mimics posing as boulders, armoured shellbacks, magma wisps in the mantle, brood nests and their brood lords, rogue mining drones, sentry turrets, excavators that now and then chew a block out of their way and shielded ancient guardians. Each has a tell before it strikes (a screech, a wind-up, a charge glow) and a counter: bombs crack armour, EMP charges knock machines out and drop shields, flares lure bats away. A hit knocks the rig back and leaves it a moment of invulnerability; the surface, the town and your outposts stay safe, kills drop ore and materials, and cleared lairs stay cleared. The New World dialog (and the host's pause menu) sets enemies to Peaceful, Normal or Hard; god mode makes rigs immune.

The upgrade shop sells long upgrade lines (up to 15 tiers: endgame bays of ~600 slots, tanks of ~15,000 fuel, hulls of ~2,500, drills that plow through the hardest rock) and a hazard counter for every danger: Heat Shielding (each layer runs hotter; radiator and shielding together decide how deep you stay cool), Lava Plating (lava is drillable but burns; the top tier is lava-proof), Gas Scrubber, Pressure Bracing (pressure pockets, explosive pockets, magma vents) and Boulder Deflector.

### Day and night

The world has a clock. A full day lasts 48 real minutes by default (24 or 72 in the New World
dialog): about 30 minutes of daylight and 18 of night, with a dawn and a dusk between. The sky
follows it, from dawn pinks and oranges through a deep noon blue and golden hour to dusk
purples and a starry night with the stars wheeling overhead; the sun arcs across the sky and
the mountains catch its low light. The Moon never moves: it hangs huge above one spot in the
world, a few thousand tiles from town, lit by the sun in its phases. At night the open surface
goes dark and your rig's headlights matter; village and town windows, street lamps, base lamps,
flares, lava and glowing crystals light the dark. The underground is as dark as ever whatever
the time. A rooster crows at dawn, crickets come out at dusk and a far bell tolls at midnight.

Sleep through the night in a bed: your base's bed or homestead (**Sleep until dawn**), or a room
at a village inn. In split-screen or online everyone has to turn in before the night passes (the
host can switch the pause menu's **Sleeping** row to let anyone sleep). The HUD shows a sun dial
with the day count and the time.

**The night is dangerous.** As dusk deepens, night monsters rise on the open surface (a
"NIGHT FALLS" banner and a low sting warn you) and they burrow and flee at dawn ("DAWN", with
the night's tally, for whoever stayed up: not after a night slept away). Which ones depends on the land: packs of **shadow hounds** on the plains,
tundra, badlands and mountains (they will not step into lamp light and run from flares),
**dune stalkers** that hear footsteps around corners (never through solid ground), **will-o'-wisps** drifting
over marsh and forest throwing cold wispfire (bullets pass through them; blasts do not), and
**moth swarms** drawn to every light (a lamp out past your camp lures them off). The Moon
matters: a full moon brings more of them and brighter nights; on a new moon the **umbral alpha**
walks once, a huge hound heard howling across the land, with an Umbral Heart to take from it.
Towns, villages, wanderers' camps, homes (a claim with a bed or homestead) and the pools of
light round lamps are safe; so is a claim fenced with **palisades** on both sides (a cheap
sharpened-log wall in the build menu). Down a shaft or high in the sky you are out of their
reach. A **Peaceful** world turns all of it off.

Night has its own economy. After dark a village's trading post becomes the **night market**
(lanterns and a pennant out front) while the smith, tailor, workshop, assayer and map room shut
until morning. It buys the night's drops (moth silk, shadow pelts, stalker fangs, wisp essence,
umbral hearts) that no one else wants, and sells flares, scare charges, a ward and a curio that
changes every night. Shallow silver, emerald, diamond and platinum glow through the turf at
night and show up on your map as you pass, and villagers keep the sky's hours.

### Rigs

Every rig is drawn from layered parts that show its upgrades: a drill head per drill tier (steel,
carbide, tempered, diamond, plasma ... star, each in its own metal, longer and wider at higher
tiers), thruster nacelles and nozzles that grow with the engine, cargo crates and containers
on the rear deck that grow with the cargo bay, radiator fins, armour plating, the drill mount and
an empty weapon hardpoint on the dome. A player's garage paint job (see the Repair Garage below)
replaces their default crew colour everywhere: the rig outdoors and inside buildings, their name
tag, their split-screen HUD title and their minimap marker. Players who never visit the garage
keep distinct default colours (player 1 yellow, 2 teal, 3 rose, 4 lime, ...). Paint jobs are
saved with the player and every co-op or online crewmate sees them.

### Movement feel

Every engine tier raises the rig's top speed (the first upgrades by about two thirds of a tile
a second each, later ones by about 9 %, and no tier is slower than before the retune): the top
engine is about five times faster than stock, and test mode more than doubles that again. Fast rigs ease into their
speed instead of snapping to it (acceleration builds as the rig spools up and tapers off near
top speed), brake harder the faster they go, and brake harder still when you steer against the
motion, so stopping and turning stay controllable and a short tap is still a precise nudge.
At speed the camera leads ahead of the rig (more the faster it goes; slow rigs stay centred)
and speed lines and wake motes stream behind it. The drive model is integrated at a fixed
internal 240 Hz step and fast rigs are swept in half-tile collision steps, so the feel is the
same at every session tick rate and nothing tunnels through walls.

Measured on a flat tunnel at 60 Hz (stock parts, empty bay; regenerate with
`cargo test -p fatherload_simulation feel_report -- --ignored --nocapture`):

| Rig | Top speed (tiles/s) | First move (ms) | To 50 % (s) | To 90 % (s) | Cross 100 tiles (s) | Stop from top (tiles / s) | Reverse (s) | 100 ms tap (px) | Flight top (tiles/s) | Flight stop (tiles) | Climb top (tiles/s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Tier 1 | 2.2 | 67 | 0.05 | 0.07 | 44.5 | 0.1 / 0.12 | 0.08 | 7 | 1.9 | 0.1 | 5.9 |
| Tier 3 | 3.5 | 50 | 0.05 | 0.08 | 28.2 | 0.1 / 0.12 | 0.10 | 10 | 3.0 | 0.1 | 9.3 |
| Tier 5 | 4.8 | 50 | 0.07 | 0.12 | 20.9 | 0.2 / 0.13 | 0.12 | 13 | 4.1 | 0.2 | 12.6 |
| Tier 8 | 6.2 | 50 | 0.08 | 0.13 | 16.2 | 0.3 / 0.15 | 0.13 | 14 | 5.3 | 0.3 | 16.1 |
| Tier 10 | 7.4 | 50 | 0.08 | 0.15 | 13.6 | 0.3 / 0.17 | 0.15 | 15 | 6.3 | 0.4 | 17.8 |
| Tier 12 | 8.8 | 50 | 0.10 | 0.17 | 11.4 | 0.4 / 0.17 | 0.17 | 15 | 7.5 | 0.5 | 17.8 |
| Tier 15 | 11.4 | 50 | 0.12 | 0.20 | 8.9 | 0.7 / 0.20 | 0.22 | 15 | 9.7 | 0.7 | 17.8 |
| God mode | 25.1 | 67 | 0.30 | 0.47 | 4.3 | 2.6 / 0.33 | 0.48 | 8 | 21.3 | 2.6 | 26.7 |

"First move" is the time until the rig has visibly moved (one art pixel); "Stop from top" is
coasting from top speed with the stick released; "Reverse" is from full speed one way to half
speed the other way.

### Worlds

Every new game is a different world. "New Game", "Local Split-Screen" and "Host Online Game"
open a **New World** dialog: a random seed (re-roll it, type hex digits or any words, or paste
with `Ctrl+V`), world size (standard 240 tiles wide or large 360), resources (lean / standard /
rich), hazards (calm / standard / brutal) and **test mode**. The same seed and settings make
the same world on every machine; the pause menu shows the seed and copies it to share. Online,
the host's settings define the session.

**Test mode** is for exploring the layers quickly (a god mode): every rig starts with every
upgrade and hazard counter maxed (drill mount included), every rig part in the locker and
1,000,000 credits, and is tuned to feel like the game is beaten: rigs drive more than twice as
fast as the top engine tier (about 25 tiles a second), climb half again faster, and drill
through anything except bedrock (including lava, foundations and vault walls) the instant the
drill touches it. The
pause menu adds **Warp to Layer** (drops your rig into a safe carved pocket at the top of any
layer), **Invulnerable Hull**, **Infinite Fuel** and **No-Clip Flight** (for the whole crew;
no-clip rigs fly through solid rock with no gravity, collision or drilling: thrust flies up,
drill flies down, and turning it off frees any rig left inside rock). Depth records and
deep-reward milestones do not count. Test-mode worlds are marked on the HUD, the pause menu,
save slots and the title's last-save line; it works in split-screen and online (the host
chooses it).

A test-mode rig is also **fully stocked** (Wave 14): every bomb, charge, rocket, pilot round,
infrastructure kit, deep supply and dive charge at its stack, every weapon, pilot gear item and
cosmetic, every boss-won gate reward (the Deep Claim, Drill II and the rig systems through the
Heart Key), the Star Drive with space open, every ship line, planet gear, alien tech mark,
Starwright part and the Heartline Drive, and every star chart of the Archive. The pause menu's
**God-Mode Toolkit** holds the rest: **Infinite Supplies** (on by default: spent consumables
come back each tick), **Restock Now**, **Stock Everything**, **Unlock Story & Codex** (off by
default, so the story can be tested fresh; switching it off resets it), each Descent's
**Gate** cleared or sealed and **Ending: The Heart Woke**, the home clock's **Time of Day**
and **Day/Night Lock**, **Weather on Worlds** (forced on every planet and moon), and the
creature tools: pick any of the game's creatures and bosses, **Spawn Beside Me**, **Clear
Nearby Creatures**, **Freeze Creature AI** and the **Enemies** setting. Keys, mouse, pad and
touch all drive it (left/right or the touch arrows change a value, confirm or a tap uses a
row); every tool acts on the menu seat's own rig and online the host's world decides.

The **Atlas** (Wave 14, test mode only: `F2`, the pause menu's "Atlas" or the touch ATLAS
button; `Shift+F2` still adds a seat there) is a full-screen travel browser per seat:
categories (Depth, Places, Bosses, Settlements, Space, Players, Bookmarks, Recent), type-ahead
search (`/`, then "planet ice", "leviathan", "descent 5"), a live preview of the selected place
painted with the game's own tile art and sprites (planets and everything in space drawn by the
space renderer from orbit, with a galaxy minimap and the landing site) beside an info card
(depth or coordinates, Descent and stratum or theme, gravity, hazards, residents or boss,
distance), and Go, Prev/Next (the same kind), Random, Bookmark, Back (to where you were) and
Map (a depth map of all eight Descents, then the galaxy round you; the cursor moves the
preview, Go travels to the spot). Travel always asks first: choosing previews, only Go goes.

**The command palette (test mode, Wave 14).** In a test-mode world the backtick key (or the
touch screen's **CMD** button, which brings up the phone's keyboard in the browser) opens a
command line with Tab completion and Up/Down history; Esc or the backtick closes it. `help`
lists everything: `tp planet next|prev|nearest|random [theme]`, `tp descent 5`,
`tp depth 4200m`, `tp stratum <name>`, `tp boss leviathan`, `tp station nearest`, `tp player 2`,
`tp home|moon`, `bookmark <name>` / `tp bookmark <name>`, `tp back` (add `?` to preview first;
`go` or Enter then travels), `give all`, `give dynamite 99`, `gate all`, `ending done`,
`spawn hollow_matriarch`, `clear`, `freeze`, `time 22:00`, `weather storm`, `difficulty hard`,
the debug views `view hitboxes|fields|grid|reveal|freecam|fps [on|off]` (all off by default;
the free camera pans with the stick and zooms with Page Up/Down) and the local time scale
`speed pause|0.5|1|2|4` (`pause` alone; online play keeps real time). Launch straight into a
test world with `fatherload --god`, optionally in place with `--start-at "descent:5"`,
`"planet:random:ice"`, `"boss:colossus"`, `"depth:4200m"` or `"stratum:crystal_faults"`
(and `--seed <n>`). Commands act on the rig of the seat that opened the palette (`seat 2`
switches to another split-screen seat, such as a gamepad's) and go through the same test-mode
actions as everything else, so they work online (the host's world decides) and are refused in
normal worlds. On touch, the menu's `>` button completes and `<` recalls the last command.

Worlds keep a familiar town and a fixed order of layers (topsoil, clay beds, grey stone, slate,
basalt, then seven deep strata), but each seed picks its own layer thicknesses (so a layer can
start a dozen rows higher or lower than in another world), waves the layer boundaries, shapes
soft channels and hard shelves, and places its own caverns, worm tunnels, deep chambers, ore
veins and pods, rare motherlodes, artifact beds and clustered hazard pockets. Rock next to a
hazard pocket shows tell-tale stains, so careful drillers can read danger before breaking in.
Every world guarantees a starter ore field near spawn, an ore budget per layer, and a path down
through each layer with that layer's drill.

New worlds (generator v4) are deep: about 860 rows (900 for large worlds) instead of the classic
220, every layer about four times thicker with per-seed variation, and wide transition zones
where lenses of the next layer thicken over a dozen or more rows before it takes over. Caves,
ore, artifact beds, hazard pockets and points of interest scale with the bigger layers. The
bottom is a solid bedrock floor that nothing digs or blasts through (test mode included), and the
view shows bedrock past the world's edges and floor, never sky. Depth rules authored against the
classic stack (mineral depth curves, heat, deep-run pressure, deep rewards, quest depths) read a
rig's depth at the same relative place in its layer, so they stretch with the deeper world.

New worlds (generator v5) are also **endless left and right**: the same deep
layer stack and bedrock floor, generated 16x16-tile chunk by chunk as rigs travel (caves, ore,
hazards and points of interest come from overlapping region grids, so chunk borders are
seamless and nothing depends on the order chunks are visited). The town stays on its flat ground
in the middle; the minimap scrolls with the rig and the HUD and map say how far, and which way,
town is. Chunks far from every rig are dropped from memory (dug tiles are kept), saves hold the
settings plus the changed chunks, and online clients generate chunks themselves while the host
sends only changes. Worlds and saves from v1-v4 keep loading as the bounded worlds they were.

Generator v7 gives the endless surface **biomes and a real skyline**:
plains, forests, desert dunes, badland mesas with cliffs, snowy tundra, tall mountains (granite
peaks up to 80 tiles above town, snow caps, ledges, caves into the rock and richer ore the
higher you dig) and wetlands with lakes. Each biome has its own ground and crust, vegetation,
backdrop hills, weather (snow, dust, falling leaves, rain) and wind. The town stays flat, and
every village stands on a flat shelf of its own (on a mountain, partway up) and is built in the
style of its biome. **There is no flight ceiling**: fly as high as your fuel allows; the sky
darkens to stars, a moon and black space far up. The HUD shows depth below the local ground, or
altitude above it while flying, and the minimap and map follow mountain tops. V5 and v6 worlds
keep their placeholder surface.

### The sky

Generator v8 is v7's ground with **a sky to explore** (v9, the current one, adds the filled Moon), the start of the
game's second act. Fuel is the key: each altitude band is placed at a share of what the fuel
tank tiers can climb (measured, not guessed), so every bigger tank opens more of the sky:

- **Low isles** (about 220-540 tiles up, a tank around tier 2-4): clusters of small floating
  islands over the first cloud sea: turf on top, a rock underside in the island's colours
  (meadow, blossom, frost, amber), roots and sky crystal hanging below. Everything is
  mineable.
- **Cloud archipelago** (mid tiers): chains of medium islands stepping across the sky, with
  more cloud seas, stronger wind layers and **updrafts**: mint columns of rising motes that
  lift a rig for free.
- **The dead-air gap**: no islands, strong crosswinds and thin air. Only a near-max tank with
  high-altitude intakes crosses it.
- **Sky continents** (near max fuel): huge lands hundreds of tiles wide with slightly lower
  gravity. On generator v9 (the current one) each is a place: snow-capped mountains veined
  with **storm glass** and topped by **harpy nests** full of eggs, a lake in its basin,
  winding caves, an indestructible **aether core** glowing with **aether crystal**, a **sky
  city** (aeronaut towers, a domed hall, a cloud market, a monastery with its bell, airship
  hangars and sky folk to talk and trade with; enemies leave it alone) and a **sky temple**:
  a shrine you can only crawl into on foot (vent grates stop the rig), a lever that opens a
  bronze gate, a keystone to prise loose and carry to the vault's keystone gate, and relic
  chests inside. Swing the hand pick at a lever, keystone, gate or chest to work it (the same
  pick button on every device).
- **Orbit and the Moon**: a near-zero-gravity band where rigs drift with heavy inertia, then
  the Moon itself: one giant landmass about 2,000-2,900 tiles wide hanging still above a fixed
  column a few thousand tiles from town, grey regolith and craters on top, a rocky underside
  you can mine from below, and low gravity (gravity bombs arc far, drops float).

**Altitude physics**: the air thins with height, so engines lose thrust and burn fuel faster
(high-altitude **Altitude Intake** upgrades win it back), and it gets cold: past the
archipelago an uncovered cold level burns the hull (the **De-Icer** line covers two levels a
tier). Wind layers carry a coasting rig (steer against them, or ride them), updrafts lift it
(free lift stops at the dead-air gap, and lets go of a rig that cannot cross it once its tank
nears what crossing from there would take: only your own tank carries you over),
and gravity is per place: normal up through the islands, lighter on the continents, nearly
none in orbit, low on the Moon. A **sky gauge** under the wallet shows the band, the air and
the thrust your intakes keep, the cold against your de-icer, gravity and the wind; the map
follows the rig among the islands, which show on it. Climbing into a new band, catching an
updraft and icing up each have their own sound. Older worlds keep an empty sky and standard
air.

### Sky isles and sky towns

New worlds (generator V9) fill the sky's lower bands. The **low isles** carry pale blue
skystone in their rock, brass-bound sky caches to drill open, wind astrolabes, little
waterfalls and vents that breathe a column of rising air; about half the clusters have a
**fuel buoy**: park on its striped pad and press use for one pricey refill (450 credits, into a
low tank only, one per buoy). Sky towns sell fuel too, but rationed: until your rig can cross
the dead-air gap on its own, they only top you up to what a straight climb would have left. Some isles hang on an aether **keystone**: break it (drill it,
shoot it, or drop a gravity bomb on it) and the island falls apart, ore raining down, and the
shock brings down crumbling neighbours. The **cloud archipelago** adds cloud silver, nests,
ruins and waterfalls pouring into the void, and every chain carries a **jet stream**: slip
into the racing streaks and ride it to the next island.

One island in every stretch of the archipelago holds a **sky town**: an aeronauts' airship
dock, a floating market, a sky monastery or a cloud-farmer village, with its own people
(aeronauts, monks, storm-chasers, smugglers and more). Sky towns sell **sky crafting**, paid
in credits and sky ore: aether thrusters (side thrust and grip in thin air, three tiers) at
the docks, a pressurized hull (keeps the cold out) at the cloud farms and cloud-silver
de-icers at the monasteries; the markets buy skystone and cloud silver dear. The sky gauge
names the nearest sky town or buoy; up among the islands the music turns airy and you hear
the waterfalls. On worlds without a sky the shop locks the Altitude Intake and De-Icer.

A sky town's meeting hall is its **sky land office**: it sells plots on the small isles that
float beside the town (small and medium plots only, never on a crumbling isle; their FOR SALE
boards stand on the isle turf), the whole furniture catalogue plus a **cloud hammock** (sleeps
like a bed) and a brass barometer, and the town's own builders take the work. Sky plots take
the cloud cottage, the aerie and the sky villa (or any ground plan); nothing digs a cellar under
a floating island. The sky town's folk come out to meet you like any villager's.

**Sky enemies** (v9 worlds): the low isles stay calm, but from the cloud archipelago up the
sky has its own hunters. **Sky rays** climb above you, hang for a beat and dive through;
**wyverns** dive harder and spit fire from afar; **storm elementals** crackle and loose a
lightning arc that drains your capacitor and jams your weapon; **pirate airships** hold
station overhead, lob cannonballs from two brass turrets and drop jetpack **boarders** to fight
you on foot (down the hull and the turrets go with it); **harpy nesters** only mind you near
their nest, unless you take an egg. Temples keep a sentry and a storm elemental on guard.
Sky cities, bases and outposts are safe, Peaceful turns it all off, and every kill drops
loot (storm glass, aether crystal, sky relics).

### The Moon

Generator v9 (the current one) fills the Moon in. Getting there is the end of Act II's climb:
a top-tier fuel tank, high-altitude intakes, a de-icer for the cold of space, a **Pressurized
Hull** (vacuum crushes anything else) and a **Life Support** module (air for the crew and an
air line for the pilot). Both are new shop lines; a **Moon checklist** under the sky gauge shows
what the rig still lacks from the dead-air gap up, and a **hull breach** warning and siren
sound if it meets vacuum without them.

- **The landing**: the first time a rig touches down on the Moon's top side the sun breaks
  over the rim, a banner reads "The Moon" and a horn sting plays.
- **The surface**: bright grey regolith and dark maria, craters (and every blast crater you
  make stays), **lava tubes** under the dust (basalt-lined tunnels with skylights at both ends,
  ice frozen on their floors, a relic or two), **ice fields**, **alien crystal forests** whose
  blooms glow, three **abandoned habitats** with a relic cache each, and a rocky underside of
  deep rock you can mine from below, crystal stalks hanging off it.
- **Resources**: **helium-3** (gold-freckled dust), **moon ice**, **alien alloy** veins deep in
  the rock, and **lunar relics**. The colony buys helium-3, spare ice and relics.
- **Tranquility Station**, the lunar research colony: a landing pad, a glass dome (walk in on
  foot: there is air inside), a hab module and a comms mast. Its counter (the use button
  with the rig on the landing pad, or on foot anywhere in the colony) refuels and repairs at Moon prices, melts moon ice into premium fuel
  for free, buys the lunar haul, and sells what you cannot get below: the **Helium-3 Reactor**
  (a quarter less fuel on every engine), the **Alloy Drill Head** (the drill bites a layer
  deeper), the **Lunar Cutter** (the plasma cutter refocused through alien alloy: a long, hot
  beam that cuts rock quickly) and the **Low-G Mortar** (heavy shells on a high arc that sail
  far in low gravity). They cost credits plus helium-3 or alien alloy. Enemies never come into
  the colony.
- **The alien facility** (on foot only: its door is an energy veil a rig cannot pass): a
  stepped temple, a ledged shaft down to a hall of three dark **pylons** marked with one, two
  and three dots. Strike them with the hand pick in the order of their marks (a wrong one puts
  them all out) and the gate slides open on **the Warden's arena**: the facility's guardian
  wakes, shielded. Break its two power **conduits** with the pick (or land an EMP charge) to drop
  the shield, then bring it down with explosives. It drops the **Warden's Eye** and alien alloy,
  everyone of its crew in the facility gets the **Void Lance** (alien tech: a lance of void light
  through foes and rock), and the core seal falls.
- **The core**: a long shaft down from the arena ends at a chamber at the Moon's heart, inside a
  shell no drill can scratch. Something floats there, warm and humming. (The story has more to
  say about it.) The shaft's ledges zigzag, so there is no straight (lethal) drop, and glowing
  plinths beside the heart step back up to the shaft for a pilot without a jetpack.
- **Moon creatures**: pale **regolith crawlers** leap far in the low gravity, **crystal mites**
  swarm out of the crystal forests, and **derelict automatons** (old colony security robots, EMP
  stuns them) still patrol around the habitats.
- **On foot**: the pilot has no air out on the Moon except within the rig's life-support air
  line (6, 12 or 24 tiles by tier), inside the colony, and behind the alien facility's veil
  (it holds the old air in); it jumps about three times as high.
- **Getting home**: fall off the edge (a long ride down; a sound rig shrugs off the landing, or
  the wind lays it on an island), or build a **Lunar Uplink** (a new base module, once you have
  landed): one on the Moon beams the rig to your uplinks below or to the town, one below beams
  it up to the Moon. It needs power.

Up there sound has no air to carry it: everything is muffled, world sounds to a dull thud
through the ground.

### Balance

The balance harness (`fatherload --balance-report [--seeds N] [--minutes M] [--only
economy,progression,combat,exposure,building,climb,trip,descent]`, release build; details in
[packages/simulation/README.md](packages/simulation/README.md#balance-harness-srcbalancers-srcbalance))
plays fresh worlds with scripted bots through the real simulation: an economy bot (straight
shafts, a prospector tunnelling sideways to ore, and a mining-charge miner with its own
workshop) over an hour of game time per seed, duels of five weapon loadouts against every
enemy at its depth tiers, unarmed rigs drilling every band on Normal, a rig building every
base module, and a climb bot (altitude on one tank per fuel-tank and engine tier). Enemy difficulty is now the host's call online: the host refuses a client's
difficulty change.

The T19 pacing pass, before and after (4 seeds, 60 game minutes, 60 Hz):

| Measure | Before | After |
|---|---|---|
| Enemy hull loss drilling the clay beds unarmed (stock 100-hull rig), per minute | 192 (burrowers 192) | 29 (burrowers 29) |
| Enemy hull loss drilling grey stone unarmed, per minute (burrowers) | 152 (109) | 56 (the worst is now cave bats, 26) |
| Prospector bot: hull lost to enemies per minute / upkeep (fuel and repairs) | 13.0 / 4,738 cr | 6.3 / 3,993 cr |
| Prospector bot: credits earned per minute, whole hour / last half hour | 148 / 143 | 179 / 196 |
| Prospector bot: upgrades bought in an hour / reached grey stone | 8.8 / 0 of 4 runs | 13.0 / 3 of 4 runs (minute 52) |
| Brood lord at its shallowest lair (fossil oceans): rivet gun I / cutter II / railgun III | 63 / 29 / 18 s | 44 / 20 / 12 s |
| Ancient guardian in the machine layer: rivet gun I / rockets III / railgun III | stalemate / 50 / 41 s | 100 / 34 / 26 s |
| Mining charges vs a tier 8 drill mid-depth (stationary benchmark, shack price) | 1.12x | 0.86x |
| Mining charges vs a tier 14 drill deep down (same) | 1.66x | 1.44x |
| Workshop-crafted mining charge (cost to the player) | 8 blocks + 30 cr (a fifth of the shack) | 12 blocks + 90 cr |
| Mining-charge bot vs prospector bot, whole loop | 0.81x | 0.59x |

What changed, and why: burrowers heard drills 16 tiles away and lunged every 1.9 s, the worst
threat in every band down to the fossil oceans and the largest cost of an early run (repairs);
they now hear 10 tiles, telegraph for 850 ms, bite for 7 every 3 s and are rarer in the clay
beds. Elites took a minute or more with a fitting weapon (the guardian's shield stalled
everything but rockets and the railgun); brood lords now have 420 health (600) and guardians
480 plus a 180 shield (700 + 300). Mining charges cleared rock three times faster than a drill
and kept five deep gems in six, and the workshop pressed them for a fifth of the shack price;
their soft blast now cracks hard gems (stone-hard ores and up) one in three like any blast,
and the workshop recipe costs 12 blocks and 90 cr. Still off, for later passes: the deep
strata are harsh on an unarmed rig (fossil-ocean burrowers about 100 hull a minute, void-geode
magma wisps about 150), and the early climb is slow (drill tier 3 at about 40 minutes for the
bot, grey stone at about 50); straight shafts alone do not pay for their fuel: the stock drill
cannot bite clay (hardness 2), so a shaft-only player is held to topsoil copper and iron, earns
about 37 cr a minute in the first quarter hour, spends it on fuel (and on hopping the rig over
its own open shafts in town) and never affords drill II at 420 cr; once broke, every rescue
(fee, half tank, half hull) keeps it broke, about 45 rescues an hour. That is the naive
baseline and partly the bot (a player would tunnel to the ore beside the shaft, as the
prospector does), but the rescue's poverty trap is worth a look in a later pass.

The fuel and cargo pass (playtest: fuel should only bite on deep and long runs), before and after
(4 seeds, 60 game minutes, 60 Hz; `--only economy,progression,climb,trip`):

| Measure | Before | After |
|---|---|---|
| Stock tank / tier 2 / tier 3 / top tank (fuel) | 100 / 150 / 225 / 3,075 | 450 / 700 / 1,070 / 14,870 |
| Stock bay / tier 2 / tier 3 / top bay (slots); hopper per bay tier | 12 / 20 / 30 / 306; +20 | 12 / 22 / 37 / 607; +30 |
| Everyday trip (first drill upgrade, 50 rows down, 2 min of mining, home): fuel burned, share of a stock / tier 2 / tier 3 tank | about 160: more than a stock tank holds | 164: 36 % / 23 % / 14 % |
| Economy bot trips: median (90th percentile) share of the tank, tanks 1 / 2 / 3 | 15 (35) / 23 (38) / 25 (40) % | 3 (5) / 6 (9) / 6 (10) % |
| Prospector bot: credits per minute, whole hour / last half hour; upgrades bought | 174 / 186; 11.2 | 221 / 247; 14.5 |
| Prospector bot: minute of tier 2 / tier 3, fuel tank, cargo bay, drill | 3.0 / 32.2, 6.8 / 33.2, 13.0 / 46.2 | 2.6 / 32.3, 5.3 / 32.7, 10.8 / 33.7 |
| Shaft-only baseline: rescues an hour | 46.5 | 10.0 |
| Still-air climb ceiling (rows above row 0), top engine and tier 6 intakes, tanks 1 / 5 / 10 / 15 | 410 / 1,340 / 4,680 / 12,150 | 1,140 / 4,800 / 19,480 / 54,140 |
| The same without intakes, top engine | 360 / 900 / 2,360 / 4,980 | 790 / 2,410 / 5,240 / 5,250 |
| Climb from the surface in a V8 sky, stock engine and tank (tiles) | 120 | 397 (thin air caps a stock engine at about 480) |

V8 worlds keep their sky exactly (the bands are frozen per generator), so in a V8 world the
bigger tanks reach the bands earlier: a stock tank already climbs into the low isles, and tank
10 with a top engine and tier 2 intakes crosses the dead air (no rig crosses without intakes).
Generator V9 (new worlds) recalibrates its own bands to the new curve, with thirstier thin
air so its sky stays as tall as V8's: the first tank upgrade reaches the low isles (from 310
rows), a tier 4 tank and engine the cloud archipelago (740), only a tier 13 tank with a tier 8
engine and top intakes crosses the dead air to the sky continents (12,010), and the Moon
(15,360) takes a tier 14 tank and top intakes
([packages/simulation/README.md](packages/simulation/README.md), "Band tables per generator
version" and "V9's sky").

### Depth logistics and the Deep Descent ladder (wave 7)

The foundations for the 13,000-row Deep Descent (expansion plan Part III F10). Worlds of the
V11 generator (and any generator after it) play the Descent rules; every V1-V10 world keeps its
classic ladder exactly. Beacons, built lifts and dive charges work in every world.

- **Lifts.** Ancient lifts are builder rails with station pockets that a generator lays from
  outpost to outpost. They start dead: restore one from its lowest station and its cab carries
  the rig (no fuel) between any of its stations. Place your own lift stations on a shaft you dug
  (each gets a metal floor to stand on) and ride between them.
- **Deep outposts.** A camp in every Descent: reaching one adds it to the travel network; its
  counter refuels, repairs and buys your cargo at a fee until the crew claims it (then fuel is
  a credit a unit and the buyer keeps 5 %), sells dive charges, and a claimed outpost can be
  made home (rescues land there).
- **Depth beacons.** Plant beacons underground; from any beacon, reached outpost or the town,
  travel to any other for a little fuel and some credits by distance.
- **Dive charges.** Cut a 24-row shaft straight down and plunge down it; the landing does no
  crash damage.
- **Controls.** Press use at an outpost, a lift station or your own beacon (the prompt says
  what it opens), or pick "Depth logistics" at the bottom of the inventory anywhere: one dialog
  lists what the stop offers, every travel destination with its cost, and the field kit
  (beacon, lift station, dive charges). It works the same with keyboard, controller, touch
  (tap a row) and in split-screen.
- **Progression to the Heart** (Descent worlds). The six core lines get deep tiers up to 22,
  about two more per Descent (the shop sells a tier once someone reached its Descent), and
  eight rig systems join the shop: the Pressure Hull, Bio-Filter, Lamp II, Sonar Suite, Cryo
  Hull, Gravity Anchor, Phase Drill and Heart Key. Each Descent from III on is crushing without
  its gate system; the Bio-Filter halves gas damage, the Sonar Suite reaches further, Lamp II
  lights wider.
- **The supply line** (Wave 11). More than 450 rows below your nearest outpost, restored lift
  station or depth beacon, every second of thrust also costs a share of the tank (up to
  0.16 % a second far out; the HUD shows FAR FROM SUPPLY). Everyday trips from a beacon stay
  calm; the first expedition into a new Descent, and the climb home, are the stressful part.
  Plant beacons as you go.
- **The descent probe.** `fatherload --balance-report --only descent` has a bot play all 41
  strata on these curves; with Descents VII-VIII authored (Wave 10) and the supply line and
  the Heart's final fight counted (Wave 11) it estimates 84.2 hours to the Heart's
  fall (target 60-90, every Descent inside its window on three seeds), with something new at
  least every 20-40 minutes (details and the full table in
  [packages/simulation/README.md](packages/simulation/README.md)).

### The Descent economy (wave 8)

- **Pressure and lighting ladders.** Two new shop lines on Deep Descent worlds, one tier per
  Descent: the **Pressure Rating** (deeper than its seals are rated, the thrusters burn 35 %
  more fuel per Descent: felt, never fatal) and the **Headlamp** (a wider lamp per tier). Each
  goes on sale once someone reaches the Descent it is rated for.
- **Felt before you buy.** A rig system goes on sale only once the crew has felt why it is
  needed: the hull groaning past a gate (the crush starts gently on the gate's first rows), the
  Glow Sea's dark for Lamp II.
- **Deep shops.** Every outpost's counter sells the tiers its Descent needs (core tiers to the
  Descent's cap, the next Pressure Rating and Headlamp tiers, the hazard counters and the rig
  systems), at the town price once claimed.
- **Layer resources and workshops.** The drill now and then digs a stratum's layer resource out
  of its rock (Root Fibre, Kiln Clay, Mine Timber, Crystal Lens, Pressure Pearl, Spore Silk,
  ...; they take no cargo room). Each outpost's workshop turns them into hull patches, fuel
  cells, dive charges, blasting charges and **upgrade parts** (the next tier of a line for the
  resources plus 40 % of its price): one recipe per stratum of Descents I-III.
- **Ore pays its depth.** Ore hauled up from a deeper Descent sells for that Descent's premium
  (110 % from the Old Deep, 125 % from the Living Dark, up to 230 % from the Heart).
- **Fair placeholders.** Until Waves 9-10 fill them, the strata below Descent III never run
  hotter than the radiator on sale before them can cool.
- **First-time hints.** Each seat gets a one-line tip the first time it meets a Descent system
  (a layer resource, an outpost, an ancient lift, underrated seals, a gate, a system on sale,
  the dark, strange gravity, slick ground, fluid, beacons).
- Screenshot scenes: `outpost_deep_shop`, `outpost_workshop`, `new_world_deep_descent`,
  `layer_resource_drops`.

### Caves

Worlds from generator v6 on add **cave ecosystems**, each with its own rock,
light, loot and life: echoing **grand caverns** hung with stalactites, glowing **crystal
geodes** lined with gems, **flooded caves** (underground lakes with a silt bed, glowing algae and
cave fish; rigs float and drift slowly in the water), **lava tubes** with obsidian walls and lava
pooling on the floor, **fungal grottos** with giant mushrooms whose caps bounce a falling rig
back up, glowcaps and spore-gas pockets, **worm burrows** ribbed with old bone, **ancient
ruins** (brick halls with pillars, glowing glyphs and a relic cache), tall **chasms** and **cave
networks** of chambers strung together by tunnels. Most caves hide a treasure cache (drill it
open for credits and loot) or a rare gem vein. Caves are dark: chasms and burrows are pitch
black, geodes, grottos and lava tubes are lit by their own glow, and every rig carries a lamp
(wider with better scanners), so flares really light the way. Stalactites crack and fall when a
rig passes beneath them. Harmless critters live in caves (fireflies, glow moths, bats, cave fish,
ember moths). Entering a cave for the first time says what you found ("Found: Crystal Geode")
and marks it on the minimap and the mine map; the collection log counts the kinds found.
Older worlds keep their caves as they were.

Generator v6 and v7 worlds also have **villages** (see below); on v7 worlds no cave undercuts a
village or opens beside an underground hall. V5 worlds keep generating exactly as before.

### Villages

Out along the endless surface, every 400 to 800 m either side of town, stand villages, the
first ones a short drive from town; about one site in four also has an underground settlement
in a carved hall a few hundred metres further out and 40 to 120 m down (one of the two second
sites always does). Each village is generated from the world seed:

- **Culture and style** follow the landscape it stands in (plains, forest, hills, mesa,
  scrapland; on v7 worlds the surface biome picks it: forests and wetlands build like forest
  villages, mountains and tundra like hill villages, deserts like mesa towns, badlands like
  mesa towns or scrap camps): Frontier and Riverside villages are built of
  timber, Highland ones of dry stone, Mesa towns of adobe, Scrapper camps of scrap metal, and
  the Deepfolk cut their settlements into the rock. Names come from the culture's generator.
- **Layout**: a hamlet, village or town on flattened ground: a meeting hall and a trading post
  flank the plaza (well, statue, fountain, totem, bonfire or crystals), with the specialty's
  building, an inn, a workshop, houses, farms, lookouts and old mine heads along the street,
  lamps, trees, carts and banners between them, a landing pad at one end and signposts at both
  edges naming the next village (or town) and how far it is. Out between the villages a trodden
  trail runs over the grass, with a milestone every 80 columns naming the nearest village (or
  the town) and the distance. A village too small for an inn hears its rumours at the hall.
- **Specialty**, the reason to come back: an assay office paying well over the depot price for
  one mineral, a workshop selling rare rig parts for the crew's parts locker, a paint-and-trim
  shop selling garage looks that are otherwise locked, a smithy selling bigger packs of special
  charges whatever the town shack stocks, a fuel depot with cheap fuel and repairs, a
  cartographer who charts the nearest unknown sites and neighbouring villages onto your map,
  or a shrine whose blessings last ten minutes (30 % less hull damage, 25 % less thruster fuel,
  or 10 % more for cargo).
- **Services everywhere**: every trading post buys cargo (at a little under the depot price)
  and pumps fuel (a little dearer than town); inns sell a round for a rumour (an echo of the
  nearest unknown site marked on your map) and patch hulls.
- **People**: 4 to 12 residents with families, relationships, wants and fears (the people
  framework) who live out their day by the world clock (a 48-minute day): home and asleep at
  night, out by their doors at dawn, at work by day (keepers behind their counters, smiths by
  the smithy, farmers in the fields, couriers up and down the street), on the plaza at lunch,
  strolling after work and at the inn in the evening (homebodies go home, night owls stay late),
  with a rest day every seventh day. Each one is a procedural figure in their culture's cloth
  and their trade's garb. Walk the rig up to someone (on the street or in a room) and press
  `E` to talk; the counter's last row talks to the keeper when they are in. The talk panel
  shows their portrait, how they see you (standing and today's mood) and what you can say:
  chat, ask about rumours (once a day each person may mark a site on your map), ask about their
  life, ask a contractor about building work, see their **trade list** (each person's unique
  offer: a one-of-a-kind rig part, a recipe that works as a timed blessing, cosmetics, map
  secrets, gossip, a smuggled pack of charges, or an apprenticeship that teaches a permanent
  perk) or **give a gift** from the cargo bay (some land far better than others). Goodwill
  grows with every chat, trade and gift, and everyone remembers every player across sessions:
  friends pay less, some offers need a friendship first, a grudge closes doors, and word of
  how you treat someone reaches their family and friends (and, the other way, their rivals).
  The town has its own folk walking its street, and out on the open surface between
  settlements you will come across **wanderers' camps** around a campfire: a trader's wagon, a
  prospector's tent, a travelling surveyor or a smuggler's hideout, whose one to three
  wanderers talk, trade and remember you like anyone else. The meeting hall shows the village's prosperity, its trouble
  and how it gets on with its neighbours. Now and then people end a chat with a line of
  **local colour** about home and their people's ways, in every culture.
- **The Moon's colonists**: the research station on the Moon has six colonists in station
  jumpsuits (a quartermaster, a hydroponist, a dome tech, a selenographer, a healer and a
  mechanic) who sleep in the hab module, work under the dome and on the pad, talk, trade
  (moon globes, grow racks, map secrets of the Moon) and remember you; stories that go to the
  Moon send you to them, and they build homes on the Moon.
- **Every building with a door is enterable** (`E` at its door, as in town): rooms are dressed
  in the village's style and the building's trade. In split-screen each seat uses villages on
  its own.

Driving into a village raises a banner with its name, style and specialty ("NEW VILLAGE
DISCOVERED" the first time, "WELCOME BACK TO" later). Villages show on the minimap and the
mine map: gold with their name once visited, blue once charted, a dim outline for surface
villages not reached yet; underground settlements stay hidden until visited or charted.

### Stories

People have things on their minds, and when they have a story for you the talk panel's first
row says so: **"Need a hand with anything?"** Ask, and they pitch it in their own voice, naming
real places near their home (a derelict 116 m down in the Grey Stone, the village along the
trail, the amber island up among the clouds); "I'll do it." writes it in your **journal**. There
are 92 stories in nine kinds, each a few beats long with a twist and more than one ending:

- **Frontier**: lost brass canaries, overdue library books, a digging machine that wandered off.
- **Village**: letters between pen friends, a peace cake for two feuding families, a courier who
  didn't come. These send you to talk to real people in other settlements, who answer.
- **Sky** (once your tank can take it): a contest kite on a floating island, weather readings,
  the first map of the great land past the dead air.
- **Mystery**: the blue lights below the slate, the lamplighters who keep the deep tunnels
  tidy, the two hums, and the Moon's heart. One leads to the next.
- **Home**: a housewarming once you've put up a homestead, a bed below, a fuel station at depth.
- **Night**: watches that only count after dark: a prowler, a sleepwalker, glowcaps that bloom
  once a year at midnight.
- **On foot**: only the pilot can finish them: squeeze into a crevice, carry a wedding
  invitation in by hand.
- **Crew**: twin bells rung together, a heat core passed from rig to rig.
- **Tombs**: a lullaby carved on a tomb's seal, a lamp left at a crypt door for its builders, a
  guardian's name scratched by a child, a lucky urn carried home. Some endings break the seal
  of the tomb the story was about, which is how story-sealed tombs open.

Newer stories go further afield: the old ruins' lever rooms, the **night market** (moth silk for
its tailor, paper lanterns for its festival, a broker who sells tomorrow's secrets), empty
homesteads on the land office's books, the **sky towns** (a monastery bell that rings itself, a
smuggler's sticky parcel) and the Moon's colonists (letters, lettuce, earthrise). Night stories
now also hunt night monsters and ask you to see a whole night through to dawn, and many endings
pay a piece of furniture for your home.

Beats are the quest engine's objectives (with markers on the map and minimap), plus a few
new ones: reach a place on foot, hand-deliver at a door, keep watch at night, find a contact
and talk to them, have a module standing in your claim, bring down night monsters, be at a
place when the night ends. When the last beat is done a card tells
the climax; go back to whoever told it and pick how it ends. Endings pay credits and goods,
warm the giver (and their circle), can lift a village's fortunes, mend a feud, open someone's
special offers, unlock a garage look, or lead to a follow-up story. People remember the
stories you finished with them, and the ones you gave up. Places outside the world (no sky, no
Moon) have stand-ins: the highest ridge nearby, the deepest chamber.

The journal sits in the map's pages (`M`, then confirm: map, collection log, quest log,
journal, milestones; `Tab` / the shoulder buttons also turn pages): stories under way with
where they stand and what to do next, stories told with their epilogues, and "Give up this
story" (confirm twice). Every row works by keys, pad, mouse and touch. At most three stories
run at once; each player in split-screen or online keeps their own.

Full stories as play would bind them, with every beat, contact line, climax and ending (no
window opens):

```bash
cargo run --release --bin fatherload -- --story-report --arcs [--seed 7] [--count 30] [--generator v7]
```

Village review (no window opens): street layout, every counter's deals, residents and the
stories each village grows, nearest town first:

```bash
cargo run --release --bin fatherload -- --village-report [--seed 7] [--count 8]
```

Space review (Wave 11, no window opens): the generated space around home within `--sectors`
space sectors (Chebyshev; default 4): systems per star class, planets per system, bodies per
kind and theme (with the nearest sector each theme appears in), a radius histogram, belts,
comets, stations, derelicts, black holes, nebulae, wormhole pairs, the home departure point
and how long generation took:

```bash
cargo run --release --bin fatherload -- --space-report [--seed 7] [--sectors 8]
```

### Points of interest

Hand-built places are stamped into every world (generator v2 and later; worlds and saves from
older generators keep generating exactly as before; v3 adds per-seed layer thicknesses and
fewer, chunkier deep ore deposits on rich worlds; v4 is the deep world with stacked placement
tiers per thick layer and a bedrock floor; v5, the current generator, places them per region
along its endless width):

- **Supply caches** (every layer below the topsoil): hollows of crates. Drill a crate open for a
  fuel canister, a bomb or two, or some credits.
- **Derelict rigs** (mid to deep): crashed prospector rigs. Drill the glowing core for salvage, a
  layer material, sometimes a part, and a log entry for your collection. Loot (crates, cores,
  lockers, vault prizes) shrugs off bombs, so a blast never wastes it.
- **Vaults** (basalt and the deep strata): sealed rooms with walls no drill or bomb gets through.
  Open the lock to reach the prize: drill the floor switches in the order their pips show (a
  wrong one resets them), bomb every pressure seal in the walls, or drill both relays within 15
  seconds of each other (a timer bar over the relays shows what is left).
- **Formations**: fossil beds packed with artifacts and geodes lined with rich ore, guarded by
  their layer's hazard.
- **Rival claims**: abandoned outposts whose lockers hold infrastructure kits.

A scanner hears a "strange echo" from sites in range; from tier 3 it outlines them and shows
their kind, in the world, on the minimap and on the map, and a scanner pulse records them. The
map shows every site you have found or detected; confirm on the map opens the **collection
log** (sites visited, scanner contacts and recovered logs). Rewards are a top-up, never a
replacement for selling ore (reward tables: `packages/simulation/src/poi.rs`).

World generation tools (no window opens):

```bash
cargo run --release --bin fatherload -- --worldgen-report --seeds 300 [--size large] [--richness rich] [--hazards brutal] [--generator v2]
cargo run --release --bin fatherload -- --worldgen-preview D111-6A4E world.png [--size large] [--scale 4]
cargo run --release --bin fatherload -- --worldgen-preview D111-6A4E strip.png --first-column -400 --width 800  # a window of an endless (v6) world, cave ecosystems outlined
```

The report prints per-layer top row, ore value, ore specks (deposits of 1-3 tiles), cave share, hazard tiles, artifacts and points of
interest plus feature, POI-kind and hazard-cluster counts (min / mean / max over the seeds); the
preview writes a whole-world map (layers, caves, ores, hazards, artifacts, with motherlodes,
artifact beds and chambers outlined, and each point of interest outlined with its map icon).

People and story review (no window opens): procedural villagers (names, personalities,
quirks, wants, fears, histories, relationships), their dialogue across several visits and after
the player helps them, and the story seeds their village grows (`packages/simulation/src/people`):

```bash
cargo run --release --bin fatherload -- --story-report [--seed 7] [--count 30] [--no-world]
```

Balance review (no window opens): bot players on fresh worlds across seeds, duels, exposure
runs, the building probe and the climb bot, printed as a Markdown report (about 20 seconds for
4 seeds and an hour of game time on a multi-core machine; see the Balance section above):

```bash
cargo run --release --bin fatherload -- --balance-report --seeds 4 --minutes 60 [--only economy,climb] [--threads 8]
```

Audio review (no window opens): every sound is generated in code; this writes each one (and its
variations) to WAV with a level report and a `showcase.wav` tour through the in-game mixer (see
`packages/game/src/audio/README.md`):

```bash
cargo run --release --bin fatherload -- --render-audio target/audio-review
```

**Drops and loot.** Every tile you break drops: ore and artifacts as floating items, plain dirt,
clay, stone and hard rock as building blocks. Fly near them and your rig's magnet pulls them in
(a bigger cargo bay reaches further); ore and artifacts go to the cargo bay, blocks to the
separate block hopper, so dirt never crowds out ore. A full bay or hopper leaves drops floating
(and says so over the rig). Bombs and every other explosive spray their loot out of the blast:
a plain blast shatters about a third of the ore it breaks, a mining charge only about a sixth.
Resting drops merge into stacks; blocks vanish after 45 s, ore after 10 minutes, artifacts
never. Open the inventory (`Tab` / `Y`) to switch **Collect blocks** off or dump blocks; the
depot buys the hopper. In co-op the nearest rig with room gets each drop.

Successful return trips award streak bonuses. Emergency rescue returns the rig to the surface for a fee, drops part of the cargo at the rescue site, and records a depot invoice; lost cargo can be recovered later if there is cargo space.

### Bases

Stake a **claim beacon** anywhere outside the town, on the surface or deep underground, and the
21x21-tile square around it is yours to build in. Press `N` (hold `L3` on a controller; a tap
swaps weapons; `Numpad 8` for the arrow-keys seat) for **build mode**: the rig parks, a cursor appears beside it and a bar
along the bottom shows the piece, its cost, whether it fits and the controls. Move the cursor with
the movement keys or d-pad (or point with the mouse), place with `E` / `Enter` / left click / `A`,
take down with `X` / right click / the controller's use button (refunds the block, or most of a
module's price), rotate with `R` / `Y`, and pick pieces with `Z` / `Q`, the mouse wheel or
`LB` / `RB` (`1`-`3` jump to structure, interior and modules). The ghost turns red with the reason
when something does not fit.

- **Structure:** rammed earth, brick, stone and granite walls (one building block each from the
  block hopper, or bought for a few credits), riveted plating, glass windows and plank floors.
  Built tiles are solid like rock, but no drill, bomb or cave-in breaks them.
- **Interior:** back walls for rooms, doors that slide open for rigs, **lift ladders** (a rig on
  one climbs without fuel, even with an empty tank, and never falls hard), lamps that light the
  dark, ferns, banners, crates and workbenches.
- **Modules** (credits plus blocks; deeper finds unlock the later ones):
  **Underground fuel station** (from the clay beds): refuel for free from its reserve, which it
  synthesizes while powered; order a supply drop for credits when it runs dry. A stranded rig at
  a station can still refuel. **Generator**: powers the claim's modules, more the deeper and
  hotter it sits (every beacon also carries a small cell). **Repair bay** (grey stone): hull
  repairs at town prices while powered. **Cargo uplink** (slate): sells the bay's ore and relics
  at today's prices from anywhere, less a 15 % fee. **Storage chest**: stores cargo and blocks,
  shared with the crew. **Crew quarters** and the surface-only **homestead** (a house with a
  pantry, a trophy shelf and a cutaway interior when you step inside): set either as home and the
  rescue crew drops you there instead of in town. **Ore refinery** (grey stone): leave ore in it
  and, while powered, it refines a lump every few seconds into credits worth 40 % more than the
  market pays; collect them from its menu. **Workshop**: presses building blocks from the hopper
  (plus a few credits) into bombs, C4 and mining charges.

Walk up to a module and press use (`E` / `X`) for its menu. Claims belong to whoever staked them:
by default the whole crew builds and uses them, and the beacon's menu lets the owner allow only
services or only themselves. You hold two claims to start and one more every two layers
reached; each extra beacon costs more. Bases save with the world and are shared online (the host
checks every build action); nothing hostile spawns inside or near a claim.

### Homes

Buy a plot of land and have a house built on it. The **land office** is a row at the town
**HQ**'s desk and at every village's meeting **hall**: it lists the plots for sale nearby (small,
medium, large and estate plots at the edge of town, on each village's outskirts and at scenic
lakesides, clifftops and meadows; their **FOR SALE** boards stand on the land) and sells
furniture from a catalogue. Any other free stretch of open ground outside the town, villages and
claims can be bought where you stand: open the inventory and choose **Survey this spot** (far from
town is cheaper; levelling rough ground costs extra; floating sky islands and the Moon can be
surveyed too). You hold up to four deeds.

On foot on your plot, press use for the **site board**. Pick a blueprint (cabin, cottage, house,
workshop home, manor, and on the right ground a sky villa or a lunar habitat), then a builder:
the town's and nearby villages' builders each work in their own way (fast and loose, slow and
perfect, chatty, grumpy, steady), which sets the time, the price and the quality. Hand over
blocks from the hopper for a discount; builders who built for you before remember you and charge
less. A cabin takes about an in-game day, a house about three, a manor a week, and big plots add
time for the gardens. Work goes on in **world time** only: sleeping through the night moves it
on, and nothing happens while the game is closed. Watch it go up in stages (survey stakes, the
foundation, scaffolding and the frame, the walls, the roof, the finishing touches) with the
builder and helpers on site in hard hats during working hours (07:00-18:00); talk to them about
the job. Now and then the site needs you: a materials shortage stops work until you bring the
blocks or pay for the stock (or the builder sources it after a long wait), a builder may ask for
a piece of ore to set over the door (bring it and the house turns out better), or just tells a
story. You can pay to rush the work at any time.

When the keys are handed over, push up at the front door (or press use) to **walk in**. Walk the
rooms with the movement keys; the front door is on the left of the ground floor, stairs stand at
the right of each floor's first room, and walking off a room's edge goes through to the next.
Windows show the real sky outside (dawn, noon, dusk, the stars and the Moon; space on the Moon,
clouds on a sky island). Use the furniture: a **bed** makes the house your home point (the
rescue crew brings you here) and, from dusk, puts you to bed; a **stove** or fireplace cooks a
hot meal that patches the pilot up; **chests** and wardrobes store cargo; a **trophy case** shows
off an artifact or a rare gem from the bay; a **workbench** crafts furniture from blocks; a
telescope watches the Moon. Press use away from any furniture for the home menu: **decorate**
the room (place pieces from your stock on the floor, the wall or the ceiling, pick them up again,
change the wallpaper and the floor), the storage and the deed. Furniture comes from the land
office catalogue, the workbench, and a housewarming gift from a builder proud of the work;
every culture's makers and traders also sell their people's signature piece (a Frontier
rocking chair, a Highland loom, a Mesa clay oven that cooks, a Riverside fish tank, a Scrapper
junk sculpture, a Deepfolk glowcap lamp, an Aeronaut model airship, Cloudborn wind chimes, and
the Moon colonists' moon globe and hydroponic rack), stories pay pieces (a star chart, a
moonflower that glows after dark, a gramophone), and tombs give trophies: a **guardian's mask**
for every guardian brought down and a **relic plinth** (it shows off a relic like a trophy case)
for the first tomb seen through to the end. Newer plans: a turf-roofed **longhouse**, a cloud
cottage and an aerie (sky islands only) and a crater dome (the Moon only).

Hire the builder again for **expansions**: more rooms, another floor, a garage bay (park the rig
in it from outside and it is patched up at cost, 1 cr a point of hull), a basement with a hatch down into the mine
below the house, and a private lift from the basement to your claim underground (ride it back
up with use inside the claim). The deed sets who may come in: the crew decorates, visitors are
welcome (the default), or owners only; add co-owners from the deed. Homes save with the world
and are shared online, the house lights up its plot at night, and nothing hostile spawns on or
near a plot.

### On foot

Press `E` (controller `X`, the arrow-keys seat's `Right Ctrl`) with the rig at rest and the
pilot climbs out (anywhere a building door or base module isn't claiming the button). The rig
stays parked with its hazard lights blinking and its lamp on; it keeps its cargo, and enemies
can still go for it. Walk back beside it and press the same button to board: whatever the
pilot carried goes into the rig.

On foot you are slow vertically and weak at digging, but you fit where the rig can't:

- **Move**: walk with `A` / `D` (hold a direction to break into a run), jump with `Space`
  (controller `A`; the arrow-keys seat jumps with up). Jumps forgive a late press off a ledge
  and an early press before landing; let go early for a short hop. Run into a wall you can
  almost reach and the pilot grabs the ledge: jump or push up to pull up.
- **Climb**: `W` / `S` climb lift ladders and climbing ropes (a new build piece); jump to let go.
- **Crouch and crawl**: hold `S` to crouch; crouched you crawl through 1-tile gaps. A
  **crawlway** behind **vent grates** (a new build piece, solid to rigs, drops and creatures)
  is a pilot-only space.
- **Swim** in cave water (jump / up strokes up, down dives).
- **Hand pick**: hold the fire button (`LMB` / `V`, controller `RT`) to chip soft dirt, sand,
  snow, mud and clay where you aim (down while crouching), and in the sky an island's turf,
  soil, roots and vines and the Moon's dust: slow, for footholds, not shafts. Every chipped
  tile drops a block. Rock just clinks.
- **Explosives**: the bomb button throws the selected charge from the pilot's hands. Stand
  back: blasts hurt pilots.
- **Jetpack** (the shack's gear rack, three tiers): hold jump in the air to fly for a few
  seconds; it only recharges standing on the ground.
- **Survival**: the pilot has its own health and air (suit tiers raise both and the pack).
  Air runs out under water, in gas and very high up, then the pilot suffocates; lava burns;
  long falls hurt (about five tiles is safe). A pilot who goes down drops some of what it
  carried and wakes up in the rig (at your home point if the rig is wrecked).
- **Pack**: the pilot picks up loose drops it walks over into a small personal pack; the
  inventory (`Tab`) has rows to move stacks between the pack and the rig by hand.
- **Buildings**: on foot, push up at a door to walk straight in; walk out through the door on
  the left. Rigs still use the door with `E`. Counters that work on the rig (fuel, repairs,
  paint, selling cargo, upgrades) need the rig parked outside.
- **Stuck?** Away from the rig, press `E` twice to radio for a pickup: the pilot is hauled back
  aboard for 25 credits (also the way back to a wrecked or dry rig).
- The camera, HUD (pilot health, air, jetpack and pack), prompts and controller bindings all
  switch while you are on foot, in every split-screen seat and online.

#### Arms, gear and ruins (on foot)

- **Personal arms**: the town gunsmith (the Explosives Shack's rack) and village armories sell
  a **wrench**, a **sidearm** (never runs dry), a **shotgun** (shells) and a **rifle**
  (cartridges). Personal ammo is the pilot's own, apart from the rig's capacitor and rockets.
  The fire button shoots what the pilot holds (mouse / right stick aim, auto-aim for the
  arrow-keys seat); `X` (controller tap `L3`) swaps between the pick and every owned arm. The
  bare pick swings at a creature in reach instead of the floor. The pilot is much weaker than
  the rig in a straight fight, but nimble.
- **Gear** (all on the same rack, in tiers): armour plates, a thermal lining against the cold
  of the high sky (it hurts without one), a heat weave for the deep bands, a headlamp, climbing
  boots (push into a wall while falling to slide, jump to kick off it), a glider (hold jump
  while falling, with down or once the jetpack is dry), a hovering jetpack (Mk II: jump +
  down), a grapple launcher and the **rig link**.
- **Rope**: `R` (controller `LB`, the arrow-keys seat's `Numpad 9`, a ROPE button on touch).
  Beside the rig it clips you onto the rig's **winch**: walk off into a shaft and hold down to
  be lowered, up to be hauled back. Elsewhere it fires the grapple along your aim (or up and
  forward): up reels in, sideways swings, the button again (or jump) lets go.
- **Rig link**: away from the rig, `E` calls it over on autopilot along an open path (press
  again to radio for a pickup instead); Mk II turns its mounted gun into a sentry that covers
  you while you walk.
- **Ruins** (new worlds): sealed rooms of ancient masonry deep underground, entered on foot
  through a vent grate. **Levers** open gates (`E` beside them), **pressure plates** hold a
  gate open while weighted (park the rig on one), **keycard doors** want the card from a chest,
  and **chests** open only by hand. **Rig vaults** hide a rig-sized ore chamber behind a gate
  whose lever is up a crawlway: hop out, pull it, fly in. **Crawlway caves** are pockets in
  hardpan reached only through a crawlway.

### Tombs and crypts

New worlds (generator V10) hide sealed **tomb sites**: deep in the strata, cut into badland and
desert mesas, under villages' old quarters, on sky continents and on the Moon. Every place is
one of two kinds, fixed by the world seed and the same on every floor: a **side-view tomb** (a
sandstone pyramid; a platformer run on the pilot's own movement) or a **top-down crypt** (a
crypt-stone mausoleum over a floor hatch; a dungeon crawl seen from above). The entrance banner
and the map marker say which, its tier (depth or altitude: 1 on the mesas to 5 on the Moon)
and how the seal breaks: a **tomb key** (found in ruin chests and tomb vaults), a **blast**
beside a cracked seal, or a **story** seen through.

- **Going in**: on foot, `E` (the use button: `X` on a controller, USE on touch) at an open
  door takes you and every crewmate standing near the door inside together (split-screen and
  online). The rig waits outside and takes no damage. Floors (3 to 6 by tier) are fresh every
  run: trap halls, lever and pressure-plate puzzles, ambushes, a **shrine** halfway, a
  **treasure vault** off the main path, and the tomb's **guardian** before the way out on the
  last floor. Rooms light up on your map as you walk in.
- **Loot and death**: caches fill your **unbanked** loot; resting at a shrine banks everyone's
  loot (paid at once), revives the downed, heals once and **saves the game**. At zero health
  you are **downed**: a crewmate standing beside you for a moment (or a shrine) stands you up.
  If the whole party is down the run ends and the unbanked loot is lost. The first floor's way
  out, or the stairs after the guardian, leave with everything.
- **Relics**: taking the stairs offers each member their own three perks and a bargain (a
  blessing tied to a curse), picked with the menu keys, a controller or a tap: double jump,
  ricochet shots, the trap-sight lantern (hidden spikes glow), the vampiric pick, the glider
  boost and more, with synergies (Spring Heels and Glider Boost make Skywalker). Relics last
  the run. Every relic works in both kinds of place: movement perks change the crypt's dodge
  roll, and **Sandwalker Sandals** halve trap damage in crypts (spikes, flames, pit falls, the
  traps' arrows and boulders) as well as tombs. The trap-sight lantern lights hidden traps on
  its carrier's own screen only.
- **Unlocks**: the **tomb codex** (after the milestones in the map's pages: `Enter` from the
  milestones) lists every relic you have carried, the tombs you have mapped and cleared,
  guardians felled and the builders' **lore pages** found in vaults. Each guardian felled also
  leaves a trophy case for your home and the Crown decal for the rig.
- **Side-view tombs** (wave 5 T28) are built in their site's culture: **sandstone** tombs in
  the mesas (glyph bands, bronze sconces), grey **catacombs** with skull niches in the deep
  strata and under old quarters, pale marble and gold **sky temple vaults** on the sky
  continents, and alloy **lunar vaults** with teal light seams on the Moon; the entrance shows
  the culture too. Rooms are real platforming: stepped mounds with the lever on top, plate pits
  by the gate (it holds a few seconds after you step off), ledge and mezzanine ambushes, a climb
  of ledges up to the treasure vault, and **secrets**: false walls that hide cellar caches and
  sometimes the vault's way in.
- **Traps** all tell you before they hurt and leave a safe moment (or a safe spot): **swinging
  blades** (wait in the strip between two), **crushers** that shake and shed dust before they
  slam (and shove you out from under them), **crumbling floors** over spikes (keep moving; they
  set again), **dart throwers** whose eye glows before a dart crosses the hall at jump height,
  **sand basins** that trickle, fill and drain (water in sky and lunar vaults), **cursed
  vents** that gather wisps and burst into grave fog, and hidden spike pits. The
  **Trap-Sight Lantern** shows hidden pits and false walls and outlines each trap's reach (a
  blade's sweep, a crusher's column, a dart's line); **Sandwalker Sandals** halve trap damage.
  New side-view synergies: Cloud Dancer (Spring Heels + Featherfall: a third jump, a higher
  leap), Updraft (Glider Boost + Featherfall), Tomb Runner (Quickstep + Sandwalker) and Seer's
  Path (Trap-Sight + Sandwalker).
- **Tomb creatures**: shambling **mummies** that lurch at you, **crypt bats** dropping from the
  ceiling, **skeletal archers** that draw a long pull before each arrow (duck or jump it),
  **animated statues** standing dormant until you come close, and two **guardians**: the
  **Sunwrought Colossus** (a leaping slam that throws rubble both ways; at 60 % health it adds
  thrown rubble and raises mummies, at 30 % it enrages) and the **Hollow Matriarch** (fans of
  hexes from above; then rings of hexes and crypt bats; at the end she dives through you). Each
  run of a tomb wakes one of the two. Side-view tombs have their own run music and a guardian's
  theme, and every trap and creature its own sound. Everything plays with touch (the d-pad,
  JUMP, USE and the fire stick) as well as keyboard and controller.
- **Top-down crypts** are seen from above, closer in, in the place's own stone: **catacomb**
  (the deep strata and old quarters), **sandstone** (mesas), the **sky vault** (sky continents)
  or the **lunar vault** (the Moon), walls standing in 3/4 view. Each seat switches on its
  own: in split screen one player can crawl a crypt from above while the other flies the rig
  outside or plays a side-view tomb. In a crypt the pilot panel's JET bar becomes **ROLL**
  (full when the dodge roll is ready).
  - **Moving**: `WASD` (or the arrows, or the left stick) walk eight ways; `Space` (`A` on a
    controller, ROLL on touch) is a **dodge roll**: a quick dash you cannot be hurt in for all
    but its last moment (strikes, shots, traps and pits pass through you), then a short wait.
  - **Fighting**: aim anywhere with the mouse (or the right stick: pushed out, it also fires,
    twin-stick style; on touch, drag the big FIRE button); the arrow-keys seat aims at what
    it walks toward. Guns fire, the wrench and hand pick swing.
  - **Traps** always warn first: **floor spikes** rattle before they rise and ripple across a
    hall, **flame jets** glow then burst in turn, **arrow plates** click and a moment later
    loose arrows from slots in both walls along their line, **boulder chutes** rumble and then
    roll a boulder across the room, and **cracked floors** give way a beat after you step on
    them into **pits** (a fall hurts and you climb out at the nearest safe floor). Hidden
    ones glow for the Trap-Sight Lantern.
  - **The crypt's dead**: **skeleton packs** flank you and rattle before they lunge,
    **spectres** drift straight through the walls (they cannot strike from inside one),
    **cultists** keep their distance and throw hex bolts (a fan every third cast), **golems**
    raise their fists over a ring on the floor before they slam (never through a wall). On the last floor the **crypt
    lord** fights in three phases, each new one opened with a roar and a few risen dead: a fan
    of bolts and a ring, then charges across the hall, then a spiral of bolts and faster
    attacks, every one telegraphed (the banner shows its health and phase). The run banner
    takes the nearest clear spot at the top of the view, clear of the stats and wallet panels
    and a phone's top-row buttons, and turns compact (floor, credits, relics, objective) where
    the full one would hang over the middle of the view (a phone, a split-screen half).
  - **Relics** mean the same in a crypt in their own way: Spring Heels rolls longer, Glider
    Boost rolls further, Featherfall takes the hurt out of pits.
  - The minimap becomes the crypt's **room map** (rooms you have walked into, coloured by kind,
    doorways, the party), and the crypt has its own music, with war drums for the lord.
- **Crypt balance** (`--balance-report --only tombs`, 4 seeds, 60 Hz, the probe bot with kit
  that fits the tier, rolling through tells and shots, keeping off pits and waiting out
  spikes and flames, going for the vault): it clears 4 of 4 tier 1 crypts in about 1.6
  minutes, 4 of 4 at tier 2, 3 of 4 at tier 3, none at tier 4 and 2 of 4 at tier 5; most of its
  wipes are at the crypt lord (whose health scales gently with the site tier like the
  guardian's: 0.5x its stock at tier 1 to 0.7x at tier 5 since wave 6).
- **Side-view balance** (`--balance-report --only tombs`, 4 seeds, 60 Hz): a scripted bot (rifle then
  sidearm, aiming at what it sees within 14 tiles, first relic each floor, no dodging, no
  detours to the vault) reads the side-view traps ahead (it waits out blades, crushers, darts,
  fog and the sand, never stops on a crumbling slab) and goes back for a lever it passed. In
  side-view tombs it clears all 4 tier 1 runs in about 0.9 minutes, 5 of 8 at tiers 2-3 (1-1.1
  minutes) and 4 of 8 at tiers 4-5 (2.1-2.5 minutes, after the wave 6 guardian retune): most of its wipes are at the guardian,
  which it fights standing still with the rifle alone. Guardians sense intruders anywhere in
  their crypt (hiding under a platform does not make them forget you and heal), and the
  colossus leaps down off a ledge to reach you. The bot is far
  weaker than a player, so read these as relative numbers. Loot on offer in a whole run
  (every cache plus the guardian's hoard): about 1,000 cr at tier 1, 4,000-4,700 at tier 2,
  13,000-15,000 at tier 3, 35,000-42,000 at tier 4 and 70,000-90,000 at tier 5 (8-29 caches
  over 3 to 6 floors).

### The Deep Descent (generator V11)

The **World** row of the new-game dialog switches between the **Deep Descent** and **Classic
(V10)**: a world about **13,000 rows** deep (15 times a V10 world) in **eight
Descents** (the Crust, the Old Deep, the Living Dark, the Drowned and Frozen, the Hollow World,
the Strange Layers, the Ancients and the Heart) and **41 strata**. Since Wave 8 new worlds
start on the Deep Descent (`worldgen::DEEP_DESCENT_FOR_NEW_WORLDS`); saved worlds keep the
generator they were made with. `--deep-descent` or `--classic` picks the dialog's starting
world type; the tools take `--generator v11` (`--worldgen-report`,
`--worldgen-preview`, `--balance-report`), and the screenshot scenes `descent_1_crust` to
`descent_8_heart`, `descent_5_hollow_world`, `descent_outpost` and `descent_explored` (a
world dug to the floor, the browser memory fixture) show it.

- **Descent I, the Crust, is authored (Wave 8):** rain-soaked mud that bogs the rig down,
  roots, burrows and the lost miner's cabin in the Topsoil; aquifers that flood the tunnel that
  breaches them and clay pots in the Clay Beds; the old mine works in the Grey Stone (ride the
  carts: hold the stick along a rail, hold down at a junction for the lower branch, thrust to
  jump off; timber sets come down when cut or undercut) and **Gravel Hollow**, a miners'
  settlement beside the first outpost; slick slate shelves that slide and stalactite galleries;
  lava tubes and the first heat in the Basalt Columns. Nine creatures of its own, a music bed
  per stratum, the Fathers' expedition logs in strongboxes, and at its end **the Sinkhole**:
  pull its core and climb out (or run down the Fathers' adit) before the collapse catches you
  to win the Deep Claim (the Old Deep's gate) and Drill II. Scenes: `crust_*`.
- **Descent II, the Old Deep, is authored (Wave 8):** resonant crystal that rings its fault
  and prisms that split the lamp into beams in the Crystal Faults; gas tides, amber and fossil
  skeletons to dig out bone by bone in the Fossil Oceans; timed pressure jets and the
  **Cathedral Lift** (three valves) in the Pressure Cathedrals, beside **Cathedral Station**;
  a dose meter, mutation relics and the Glowmother in the Radioactive Hollow; builder halls
  that wake, conveyors and pistons in the Ancient Machine Layer; low-gravity geodes in the Void
  Geode Fields; heat storms and pouring storm vents in the Mantle Storm Zone. Eight creatures
  of its own, a music bed per stratum, and at its end **the Star Core**: break its four pylons
  to drop the shield, then strike the core. Beating it wins the **Pressure Hull** (never sold),
  which Descent III needs. Scenes: `old_deep_*`.
- **Descent IV, the Drowned and Frozen, is authored (Wave 9):** the rig becomes a submarine
  under water (thrust and drill trim its ballast; gyre currents carry it) in the **Underground
  Ocean**, where the scanner is a **sonar** that lights the water and an electric eel-dragon
  coils in its grotto; ice that steals traction, tunnels that refreeze and hull frost (warm the
  rig at thermal vents) in the **Brine Glaciers**; geysers on a beat (a warning hiss, then a
  jet to ride) and Old Faithful in the **Geyser Fields**; **Merrow City**, the fish-folk's
  bubble under the sea with its shop, moon pools and aquarium furniture; and the crushing,
  darkening **Pearl Abyss** (watch the pressure against your seals). At its end the
  **Leviathan**, a three-phase sea beast fought from the submarine, wins the **Sonar Suite**
  and the **Cryo Hull** (never sold). Scenes: `drowned_*`.
- **Descent V, the Hollow World, is authored (Wave 9):** break through into the **Inner Sky**,
  a cavern under an inner sun with its own day and night and wind bands; ride crestbacks
  through the **Megafauna Jungle** past the **Sunfolk Kraal**, the jungle tribes' village;
  under the **Upside-Down Isles** gravity points up (flip the rig with its rotation thrusters
  and land on the roof); in **the Sunforge** the light scorches (shelter in the shade) and the
  **Gravity Anchor** is won from the **Sun Serpent**, orbiting the inner sun in its well:
  break its sun scales, then strike its head, and hide from its flares. Scenes: `hollow_*`.
- **The far Strange Layers** (Descent VI, Wave 9): slow pools and fast lanes, time bubbles and
  creatures at their own speeds in the **Time Sludge**; total darkness in the **Echo Caves**,
  where only the sonar shows the way and blind hunters hear thrusters; colour-keyed walls
  opened by swapping the lamp filter in the **Prism Desert**; and at Descent VI's end **The
  Glitch**, which rewrites the rules of its hall (gravity up, mirrored, sideways) until its
  shards fall. It wins the **Phase Drill**, which alone cuts the builder alloy sealed over
  Descent VII. Scenes: `far_*`.
- **The Sleeping Garrison and the Seal Gate** (Descent VII part B, Wave 10): walk into a
  builders' fort and its garrison stirs. Build defences with the build kit (a fourth group,
  **Defences**: auto-turrets, war-plate barricades, shock traps; in a fort no claim is
  needed, and they work in any claim too), then hold the fort while war sentries, lancer
  drones and siege walkers climb out of its bays wave after wave (the bays glow red first);
  when the last wave falls the garrison sleeps again and its armoury pays out. Walking away
  lets it stand down. In **the Seal Gate** tremors shake stones from the ceilings (dust shows
  where), and the great door's six sockets light for the keys of every Descent you carry; with
  all six it opens. Behind it the **Gatekeeper Colossus** fights in three stages while its
  hall collapses between them (the cracked floor, then the vault; telegraphed, never onto a
  rig), and it gives the **Heart Key**, which alone cuts the Heart Seal over Descent VIII.
  Scenes: `ab_*`.
- **Wired together (Wave 9 integration):** one **Sonar Suite** for the water and the Echo
  Caves (the scanner pings both, the Leviathan's Suite reaches further in both); the deep
  counters' supplies work from the kit (an **Echo Charge** pings as far as the Suite, a **Frost
  Heater** burns hull frost off, a **Shade Canopy** cools a scorching rig); and the people of
  Merrow City, the Sunfolk Kraal and Parallax Station remember the Fathers when you talk to
  them.
- **The builders' buried city** (Descent VII part A, Wave 10): the **Buried Megacity**'s
  avenues, timetabled trams and tower lifts (cut the city's fabric and its alarm sends drones
  and searchlit sentinels after you; ride instead), the **Engine Halls** where every piston,
  crusher and belt keeps the Great Engine's beat (ride a rising piston, or be thrown off it),
  and the **Archive of Stars**, whose sealed vaults (plates, crawlway levers, coloured glass)
  hold **star charts** of real stars past the Moon. **Index, the archivist**, wakes in the
  Lantern Library and rides with each rig as a little drone: it scans for ore, warns of the
  machinery and talks of the city and the Fathers. **Ask it** (Wave 11) from the inventory's
  "Ask Index" row or after the star charts in the map's pages: about ore nearby (it marks the
  best), the crew's finds, or what it knows of the Descents, the Fathers, the Heart and the
  stars (keys, pad, mouse or a tap; each seat asks its own). The **Archive Post** is Descent
  VII's outpost, and its people remember the Fathers and talk about their city. Scenes:
  `city_*`.
- **The Heart** (Descent VIII, Wave 10): dark matter that makes a laden rig fall up in the
  **Dark Matter Veins**, gravity wells that bend light and pull everything in the
  **Singularity Fields** (slingshot round the rims; the horizon crushes), magnetic rails over
  the liquid-metal sea of the **Core Ocean**, and at the end of the builders' cable the
  **Heart**: a five-beat finale with the sleeping Star Drive behind the Heart Key's seal (the
  coils, ignition, the reforge's inner coils struck while the drive pulls, the rekindled
  ignition, the collapse and the eclipse's sweeping beam; about a quarter of an hour). **Heart's
  Edge**, the last outpost, has five residents who kept Amos's trail, with stories of their own.
  Waking it lights the cable to the Moon, plays the ending on every seat (the awakening, the
  climb, the credits and the epilogue, then the Fathers' last pages, "The Cable Lights") and
  opens space: every rig gets the **Star Drive**, and the game goes on. Scenes: `heart_*`.
- **Stations** (Wave 12): dock at a station's port and step into its hub. Who runs it decides
  what it offers: human stations (Halcyon Dock at home, trade posts, shipyards, outposts,
  relays), alien-run ones (alien tech: the phase lattice, graviton spool, xeno lens and
  symbiont weave), and derelicts to strip once. The **shipyard** fits the ship lines (hull,
  engine, Star Drive range, cargo hold, life support; five tiers, a yard fits up to its own)
  and planet gear for a world's hazards (heat shielding, cold insulation, toxic-air filters,
  high-gravity struts). The **market** trades goods for the hold (cheap where they're made,
  dear where they're wanted; your trades move the price) and buys your ore. The **bounty
  board** offers deliveries, body scans and ore quotas, paid at any board, never on a clock;
  Halcyon Dock's first board is a starter set. The **bar** has patrons with rumours: buy a
  round and the world they talk about goes on your nav chart. A first trip out affords the
  first ship-line tier in about 40 minutes. Scenes: `station_*`, `touch_station`.
- **The Fathers' trail ends** (Wave 10 story): logs 61-80 through Descents VII-VIII, Amos's
  chair at the cable root, a Descent progress page in the codex (the ending can be read again
  from it), new Descent cards, finale milestones, and every deep settlement's own story arcs.
  Strata meet in ragged transition zones, wider between Descents.
- Each Descent has an **outpost** hall (a working deep outpost: fuel, repair, selling, a
  claim and a respawn point) and a **gate chamber** at its end. The halls stand on one column a short way off the spawn, and the
  **builders' cable** of ancient lifts runs down beside them, outpost to outpost. Descent V
  breaks into the **Hollow World**: a cavern hundreds of rows tall with floating stones. The
  Strange Layers hold crystal-lined **anomaly rooms**.
- Strata have **field effects**: the Glow Sea is dark, the Underground Ocean drags, the Brine
  Glaciers slide, the Inner Sky and the Sunforge glare, Pixel Rot grows blockier the deeper
  you go, the Time Sludge slows time; Gravity Shear and Upside-Down Isles cells turn gravity
  (turn the rig with its rotation thrusters to stand on walls and ceilings; rotation is free
  all the way round there) and Mirror Strata cells mirror the screen and the controls.
- Strata hold **fluid pockets**: water in the Drowned and Frozen (and a few higher), brine in
  the Brine Glaciers, acid in the Flesh Caverns and Dark Matter Veins, liquid metal toward the
  Heart. Review scenes: `descent_gravity_shear`, `descent_upside_down_isles`,
  `descent_mirror_cell`, `descent_sunforge_glare`, `descent_underground_ocean`,
  `descent_builders_cable`.
- **The Living Dark** (Descent III, Wave 8) is authored: the **Fungal Forest** (giant mushroom
  caps bounce the rig; spore pods and spore puffers burst into clouds that blur and tint the
  screen), the **Root Labyrinth** (tunnels grow shut behind you over minutes; root axes from
  the field kit cut a shaft up that never closes, and fire burns roots away), **the Hive** (the
  friendly Amber Court sells royal jelly and hands out errands at its counter; the hostile Swarm
  sends soldier packs and breeds them in its matriarch's gallery; steal from the Court and it
  turns on you; the last errand ends in a choice: slay the matriarch or buy the Swarm's truce),
  the **Flesh Caverns** (mined flesh heals back, acid tides rise and fall in the gullets, the
  walls and the music beat with a heartbeat that quickens deeper) and the **Glow Sea** (almost
  total darkness, lit only by lure fish, glow jellies and the anglers' baits; the rig lamp is a
  candle until Lamp II). Its gate is the **Mycelial Mind**: kill its nerve nodes to expose it
  while it storms spores and possesses creatures; it grants the Bio-Filter (spore and acid
  immunity) and Lamp II. Scenes: `living_fungal_grove`, `living_spore_cloud`,
  `living_root_labyrinth`, `living_hive_court`, `living_hive_court_counter`,
  `living_hive_swarm`, `living_hive_matriarch`, `living_flesh_gullet`, `living_glow_sea`,
  `living_glow_sea_lamp_ii`, `living_mycelial_mind`, `living_mind_exposed`.
- **The first Strange Layers** (Descent VI, Wave 9) are authored. In the **Mirror Strata**,
  mirror halls flip the screen, the stick and the aim (a shimmering silver curtain marks each
  one); your **mirror twin** steps out of the glass and moves and fires as you do, mirrored (trap
  it behind rock on its side of the hall), and **mirror ore** lies in pairs: one real, one
  reflection that shatters for nothing (a reflection is flat, cold and breathes). Glassbacks
  throw frontal shots back and gazers mirror your controls while they see you. **Parallax
  Station**, the Descent VI outpost, has a research settlement beside it. In the **Gravity
  Shear**, shear halls turn gravity onto walls, then ceilings (flow lines show which way):
  turn the rig with its rotation thrusters and walk them, past tumblers rolling downhill
  whichever way that is, lodestars dragging you in and mantises dropping from above; the
  **Shear Spire** is a tower where down is the nearest wall. In **Pixel Rot** the world loses
  resolution tier by tier: bigger pixels, fewer colours, glitching bands, the rock itself in
  ever bigger blocks; glitchlings teleport between pixels (watch for their ghost), bitworms run
  the grid and dead pixels rot your view; mine a whole block of **data ore** for a data core,
  and find the **Dead Screen**. Scenes: `distortion_mirror_hall`,
  `distortion_mirror_hall_reduced`, `distortion_looking_glass_hall`, `distortion_gazer`,
  `distortion_shear_wall`, `distortion_shear_ceiling`, `distortion_shear_spire`,
  `distortion_pixel_rot_shallow`, `distortion_pixel_rot_deep`,
  `distortion_pixel_rot_deep_reduced`, `distortion_dead_screen`, `distortion_parallax_station`.
- The depth ruler names the Descent and stratum beside its marker, the mine map's footer names
  them too, and an **"entering <stratum>"** banner slides in at each new stratum (a new
  Descent shows its number).
- Saves pack a streamed world's dug chunks into one compressed stream (save format 13); a V11
  world explored to the bottom holds about 20 MiB of simulation heap.

### Space flight (wave 11)

After the Heart's ending every rig carries the **Star Drive**. Fly up past the Moon and space
opens with no loading screen. The Moon stays where it is, and the home world hangs below you
as a round world.

- **You turn the rig, not the camera.** In space the rotation thrusters turn it all the way
  round, and a spin keeps going until you counter-thrust. A laden bay spins slower, and
  turning burns a little fuel.
- The camera stays locked to your craft and the HUD stays upright. Gravity pulls you toward
  a world but never turns you.
- Thrust is relative to the craft. **Space thrust: Screen** in Options pushes the way you
  press instead, and keeps the view chart-up.
- **Cruise** (`/`) follows your heading. It speeds up far from everything and slows near
  worlds and stations: crossing a star system takes a minute or two.
- **Warp** (`B`) goes to a star system set as your waypoint. Face it, keep clear of gravity
  wells and carry the fuel. The jump is a streak of light lasting 6-30 seconds. It really
  crosses the distance, and a star in the way pulls you out.
- **Coming down**: diving too steep or too fast through a world's air heats the hull. A
  shallow glide does not, and the Re-entry Shield helps. Touch down slow and upright to
  land; tilted, you scrape and tip over.
- **Air** runs out in space and on airless worlds. Warnings come at half, a quarter and a
  tenth. Dock, land somewhere breathable, or call the **emergency beacon** from the nav chart
  to be towed home for a fee. The beacon comes by itself if the air runs out.
- **Stations**: line up with the port (the docking guide ticks off range, speed and
  alignment) and press `E`. The counter sells fuel, repairs and flight gear: the Nav Suite,
  the Re-entry Shield, the gyro, horizon hold and auto-land. All assists stay off until you
  switch them on in Options.
- **Navigation** (with a Nav Suite): `M` opens the nav chart (known star systems including
  the Archive's charts, nearby worlds, sharing a waypoint with the crew). The HUD shows the
  waypoint's distance and ETA, the warp's alignment, and arrows to the waypoint and
  crewmates. The scanner pings for new stars (nebula fog halves its reach).
- **The galaxy is real.** Every star, world, moon, belt, comet, nebula, black hole and station
  you see is the space generator's, the same for every player of a world (and the browser).
  The Archive's star charts lead to real systems, and the home system has its own station,
  **Halcyon Dock**, a short hop above the Moon.
- **Landing on a world**: come down through the top of its air and you are on its round
  ground, in your craft as it came in (nothing levels it for you). Drive all the way round a
  small moon, dig toward its core, then climb back out of its sky and you are in space again,
  right where you left it. What you dig stays dug.
- **Many places at once**: in co-op and split screen each player can be somewhere else (one on
  a moon, one docked, one at home), each with their own view, and every place keeps playing.
- Test mode's pause menu has **Grant Star Drive**.
- Scenes: `space_leaving_home`, `space_full_turn`, `space_flight_cruise`,
  `space_flight_warp`, `space_nav_hud`, `space_nav_chart`, `space_docking`, `space_station`,
  `space_reentry`, `space_landed`, `space_moon_approach`, `space_moon_ground`,
  `space_moon_far_side`, `space_moon_shaft`, `space_split_screen`, `space_split_places`,
  `space_near_black_hole`, `touch_space`; the space renderer's staged views are the other
  `space_*` scenes.
- **Living worlds** (Wave 12): every world keeps its own day. The badge under your wallet
  shows the world's name, its **local hour** (or DAY SIDE / NIGHT SIDE on a tidally locked
  world, whose star never moves), the temperature where you stand, the air and the gravity.
  Dawn and dusk sweep round the world as it turns; its night is dark and cold. **Weather**
  rolls in by the world: storms drive rain and hail sideways and push you about in the air,
  snow drifts and freezes, auroras hang over the night side. Shelter in a cave or a shaft
  and the weather cannot reach you. Deep down every world heats up toward its core; a
  radiator and heat shielding matter as at home, a de-icer on a frozen night. You breathe
  only where the air is breathable, and your rig's air refills only there. Each world's
  markets pay their own prices in the same credits: what the ground is full of sells cheap,
  what it lacks sells dear, dearer the further from home, and each settlement's boards want
  something new every local day. Small moons and crater rocks each hide **one secret**: a
  sealed vault under the crust. Scenes: `bodylife_terminator`, `bodylife_night`,
  `bodylife_locked_day`, `bodylife_locked_night`, `bodylife_locked_terminator`,
  `bodylife_storm`, `bodylife_snow`, `bodylife_aurora`.

### Friendly aliens (wave 12)

Out among the stars you are not alone. Three friendly species live on worlds near home, each
with towns of their own, a tongue you learn over time, things they trade, a story to tell and
homes for sale.

- **The Vessi** of the ice worlds: tall as two rigs on four stilt legs, bells of frost-fur with
  a lantern at the throat that glows when they sing. They live in spiral snow-vaults lit warm
  from inside round a song spire, never hurry, and prize warm things: ruby, copper, uranium.
- **The Tokkari** of the rocky worlds: low, six-limbed burrowing smiths under gem-studded
  shells, eyes on stalks. Terraced mound-halls of fused stone, round doors, forge chimneys.
  Every debt is a knot in a cord; they pay well for honest iron, platinum, mythril and silver.
- **The Siphra** of the airless moons: drifting bells of living glass trailing light, keepers of
  memory, who hang their homes as lantern-pods from crystal pylons. Their eldest remember the
  builders' ships. They prize clear crystal: diamond, emerald, skystone.
- **First contact** comes the first time you roll into one of their towns. Each species has a
  town within a short flight of home: in the home system or a neighbouring one.
- **Their tongue**: at first everything they say is in their own glyphs (rings, notched runes,
  constellations). Talk to their people each day, trade with them, take a lesson at the
  lexicon house, dig up the **lexicon stones** buried under their towns, buy a primer: the
  little words show through first, then more, until you read them like a native. Each player
  learns for themselves, and understanding buys better prices.
- **Trade**: their trader buys the ores they prize and sells their goods (primers, furniture
  for your home, a day of frost-down padding or glow pearls, hull plates, an air bloom, a star
  memory that charts a system).
- **Their story**: each species' elder asks for five things in turn as you understand more of
  their tongue: a gift of ore, a journey (deep under the town, or the far side of the world), a
  traveller carried to the next town, a monument raised (it stands for everyone after), and at
  last what they remember of **the builders**, a lead charted on your nav chart.
- **Homes**: buy a plot from the town's builder and they raise a home in their own style. Rest
  there to refill your air; on that world the emergency beacon tows you home to it.
- Scenes: `alien_vessi_town`, `alien_tokkari_town`, `alien_siphra_town`, `alien_vessi_homes`,
  `alien_first_contact`, `alien_talk_glyphs`, `alien_talk_fragments`, `alien_talk_fluent`,
  `alien_trade`, `alien_home_and_monument`.

### Hostile space (wave 12)

Beyond the home sector, space fights back. Three factions fly it, each with its own look,
tactics and boss:

- **The Rustwake Corsairs** lurk in nebulae and asteroid belts. They close in and **hail**:
  pay their demand (`E`) and they leave; refuse, run or shoot and they fight. Cutters circle and
  strafe, marauders hang back and fire bursts. Their king flies the flagship **Gilded Maw**.
- **The Bloom** swarms round living worlds: mites flock and dive to bite, stingers spit. The
  **Swarm Queen** broods them.
- **The Concord** is a machine fleet: drones fly in V formations behind lancers, fire in
  volleys and charge lances down a warning line. Patrols orbit machine worlds. Their capital
  ship is the **Arbiter**.
- **Fighting**: the trigger fires the **cannon** down your nose, so you aim by turning the
  craft with its rotation thrusters (no auto-aim). A **turret** fires where you point the mouse
  or right stick. **Missiles** (`X`, the next-weapon button) lock onto a ship close to your
  nose when fired. The **deflector** soaks hits and recharges. Buy them at a station's
  Shipyard.
- **Every boss attack is telegraphed**: a red line for a lance or a ram, a mark for a lunge, the
  shells' paths for a broadside or a spore ring (look for its gap). Move off the line.
- **You can always escape**: in a fight the warp button (`B`) spools an escape jump that needs
  no waypoint or fuel. Wrecked, the beacon tows you home and half your cargo waits in a pod
  where you fell.
- **Salvage**: fly through wrecks for credits and ore. A boss's wreck holds its tech for the
  whole crew.
- The farther from home, the more often and the harder they come; the home sector and the
  space round stations are calm. On worlds a few sectors out, raiders sometimes land nearby.
- Touch: in a fight the buttons become FIRE, MSL, ESCAPE and PAY.
- Scenes: `space_pirates_hail`, `space_swarm_flock`, `space_machine_formation`,
  `space_dogfight`, `space_boss_pirate_king`, `space_boss_swarm_queen`, `space_boss_arbiter`,
  `space_salvage`, `space_ground_raid`, `touch_space_combat`.

### The long game in space (wave 13 endgame)

Space has its own economy, made to last a rig that arrives after the Heart with millions in its
purse.

- **Space materials**: belt rocks give **Meteoric Iron**, **Kamacite**, **Volatile Ice**,
  **Cobalt Bloom**, **Star Platinum**, **Iridium** and **Voidglass** (rarer farther out; the
  rare ones run out fast in each rock), and a comet's tail gives **Comet Dust**. They sell at
  any counter, but they are worth more spent: stations sell only the four commons, at three
  times their value.
- **One credit scale**: every station price and reward is on the space scale, so an hour of
  boards, bounties or belt mining earns about what an hour at the Heart did (wave 14: the first
  ring's belts now out-earn the Heart). Home ore still sells at stations, at its plain value (no
  Descent premium). Big figures read with thousands separators.
- **Late tiers need ore**: a ship line's tiers 3-5 ask for space materials as well as credits
  (the shipyard row shows the bill). A big purse buys tiers 1-2 at once; the rest is mined.
- **The Works** (a station counter): buy common materials; raise alien tech to **Mk II and Mk
  III** at alien-run stations (talked through in the station's own tongue: Conversant, then
  Fluent); commission the **Starwright Hull and Engine**, the endgame ship, at a great human
  shipyard (tier 5 hull and engine lines first); fit the **Heartline Drive** (warps cost half the
  fuel and spool twice as fast) once the Starwright ship is yours and every faction boss is down;
  deliver the eight parts of the **Halcyon Gate** at Halcyon Dock with your crew (it rises beside
  the dock, segment by segment; finished, it jumps you to any system you know); and buy space
  paints and the Ion Blue flame from the **outfitter**.
- **Records** (a station counter): the **hunt list** (the Gilded Maw, the Swarm Queen, the
  Arbiter and every theme world's boss; pick one to set the waypoint on its lair) and your
  **completion**: worlds landed, kinds of world, species met, tongues spoken, bosses, story,
  the materials codex, the ship, the gate and your estate, and achievements.
- **Achievements**: a new Space group on the progression screen (Star Miner, Planetfall,
  Atlas of Worlds, Fluent, Crown Breaker, Starwright, Heartline, Gatebuilder, Lord of the Manor,
  Fatherload for 100 %...), each unlocking a paint, a flame, a decal or a hull pattern.
- **Estates**: a home in an alien town grows wing by wing (a bedroom annex, a kitchen, a
  garage, a greenhouse, an upper storey, a gallery and an observatory dome), raised by the
  town's builder from the Homes page. Have the **neighbours** over once a day: they leave a gift
  of space ore and a few words of their tongue.
- Scenes: `endgame_works_halcyon`, `endgame_works_starwright`, `endgame_works_alien`,
  `endgame_shipyard_bills`, `endgame_records`, `endgame_gate_building`, `endgame_gate_open`,
  `endgame_starwright_flight`, `endgame_estate`, `endgame_estate_young`,
  `endgame_achievements`, `endgame_split`, `touch_endgame`.

### Theme bosses of the first frontier (wave 14)

Every one of the twelve kinds of world has its boss now. The jungle, ocean, glacier, dune,
volcanic and crystal worlds' wait in a lair under each world's landmark, reached by a walled
shaft beside it (on an ocean world, a dry well down the Drowned Spire: drill its dry room's
floor): **the Bramble King**, **the Tidecaller**, **the Hoarfrost Wyrm**, **the Dune Tyrant**,
**the Magma Drake** and **the Geode Colossus**. Each tells its attacks with a pose, calls its
brood, rests whole if you leave, and opens a sealed vault with its relic when it falls; they
are on the hunt list with a waypoint. Small moons and crater rocks now carry the themes too (in
new worlds), so every kind of world's secret can be found. The Starwright Hull also softens a
fight, and the Works, the Halcyon Gate and the Harbour have music of their own.

- Scenes: `lair_jungle_boss`, `lair_ocean_boss`, `lair_ocean_well`, `lair_glacier_boss`,
  `lair_dunes_boss`, `lair_volcanic_boss`, `lair_crystal_boss`.

### The builders' road (wave 13 story)

Once the Heart wakes and the sky opens, the **galaxy story** begins: the builders left by the
Star Drive, and the charts Ines Calloway copied in the Archive of Stars are their roads. The
road starts at your first chart's star and runs outward through ten chapters: a waymark on a
world, the peoples of the near stars, a foundry on a machine world, a quiet world's archive,
the Concord's captain, the singing spires, a nursery beacon (where you learn who the Fathers
were named for, and who you are), the lamp-key, the long dark, and the **Harbour**, far from
home, where the Warden waits and you make one choice for the whole crew. The credits roll and
the game goes on.

- **Goals** are done in order by anyone in the crew: reach a system, scan a world from orbit
  (the scanner), land, find a relic (a lit builder pylon on the ground, or a vault door you dig
  down to), hold a site while raiders drop in, meet a species and help an elder, beat the
  Concord's Arbiter, reach the Harbour, calm the Warden, choose.
- **The journal** ("The Builders' Road", in the map's page cycle after the star charts; in space
  the nav chart's next page): set course for the current goal (it becomes your waypoint and the
  warp's target; the crew's waypoint follows the story by itself), ask Index, each chapter's
  page with its goals ticked off, the logs of the road, the discovery log and your stats.
- **The goal line** sits at the top of the screen in space and on other worlds.
- **The Warden** is a space boss: watch its warnings (volleys from its pods, a lance from its
  prow, a ring of light with a gap), break its lamps; the warp button always escapes, and it
  waits for you at its gate. On a peaceful world it lets you pass.
- **The choice**: ring the Homecoming Bell (the builders wake and come home) or keep the lamp
  (let them sleep; the road stays lit). Then the ending, the credits and an epilogue play on
  every seat; each seat skips only its own.
- **Endless exploration**: every system you enter can hold leads (lost ships, distress calls
  with an ambush, ruins to dig, survey contracts, a people's request). They go into the
  discovery log; select one to set course. Each pays and turns up a builders' fragment.
- Co-op: one road for the whole crew, wherever each of you is (split-screen seats can be on
  different worlds). Test mode's pause menu has "Warp to the Story".
- Scenes: `galaxy_journal`, `galaxy_discoveries`, `galaxy_chapter_card`, `galaxy_waymark`,
  `galaxy_hold`, `galaxy_warden`, `galaxy_harbour`, `galaxy_choice`, `galaxy_finale_lead`,
  `galaxy_credits`, `galaxy_epilogue_lamp`, `galaxy_harbour_awake`.

### The Fathers' expedition (wave 8 story)

A generation ago six people from the town went down the builders' cable to find out what hums.
The town called them **the Fathers**; Bran painted THE FATHERS' LOAD on their sled. Nobody
came back up. In a Deep Descent world their trail is the story's backbone:

- **Expedition logs**: a cipher canister at each of their camps, abandoned rigs and cairns,
  beside the builders' cable (on the far side from the town), each deeper than the last. Ines
  keyed every canister to the one before, so the logs are read **in order**: reach a later one
  first and it shows its red seal until you read the one before (then it opens by itself).
  Each canister holds a few credits. Descents I to III hold 30 logs in three chapters (the
  world was shaped; the builders were here long ago; life adapted to the dark).
- **Radio echoes and ghost recordings**: Dov Halloran's voice on the drop line, then on the
  tapes he left after a heat storm cut it, play once (a card and a voice that is not quite
  words) the first time a rig reaches their depths.
- **The expedition journal** follows the tomb codex in the map's pages (tab or confirm through
  them): logs read by chapter, a chapter's revelation once complete, echoes, anomalies and the
  trail's lead (where the next canister lies). Co-op crews share one journal.
- **Anomaly rooms**: rare sealed rooms at any depth where a deeper chapter's rule leaks through
  early: an upturned room (gravity pulls up), a rotten room (pixels and few colours), a stopped
  room (time all but frozen) and a mirror room (screen and controls mirrored). Each holds a
  cache placed where the rule makes it a small puzzle. Screen effects follow the effects
  intensity setting and never flash.
- **The settlements remember them**: Gravel Hollow's people tell of the Fathers when asked for
  news, and arriving at Gravel Hollow puts up a card with what they remember (more as the crew
  reads further); deeper down, Cathedral Station's and the Amber Court's residents and the
  people of Merrow City, the Sunfolk Kraal and Parallax Station tell their own memories in
  conversation. The Old Deep's Fathers' camps hold the journal's own logs.
- Seven story milestones (each unlocks a paint). Review scenes: `expedition_camp`,
  `expedition_log_read`, `expedition_cairn`, `expedition_journal`, `expedition_radio_echo`,
  `expedition_ghost_recording`, `anomaly_gravity_flip`, `anomaly_pixel_rot`,
  `anomaly_time_frozen`, `anomaly_mirror`.

### The Fathers below the Living Dark (wave 9 story)

- **Logs 31 to 60** carry the trail through Descents IV to VI in three more chapters: whole
  peoples live below (a sea under the rock, builder figures laid to rest in the ice, the merrow
  city where Sefa stays), the planet is hollow (an inner sun hung on the cable, the jungle
  tribes where Bran stays, Dov Halloran's grave in the sun's light), and the world wears thin
  near the builders' machine (twins, turned gravity, rotting pixels, loose time, coloured
  doors). The deeper camps look the part: a raft camp with frost on the canvas, a jungle camp
  under leaves, a camp whose mirror twin stands beside it. Camps in the open Inner Sky stand on
  a floating stone of their own.
- **Builder echoes**: below Dov's grave his tapes stop and the builders' own voices begin,
  with Ines's translations on the card (and once, out of time, Dov again).
- **Residents**: Cathedral Station's prospectors and the Amber Court's insects live in their
  halls now. Talk to them (the use key, by a resident) and each tells their own memories of the
  Fathers; the Court's people stay in while the Court is angry with the crew.
- **The codex**: after the journal in the map's pages, the hired hand's strongbox diary from
  the Crust and the builders' tablets and room inscriptions, kept once read.
- **Deeper anomaly rooms**: from Descent IV down a room may also be sideways (gravity to the
  right wall), sunlit, weightless or bent, each with a builder inscription for the codex.
- Review scenes: `residents_cathedral_station`, `residents_amber_court`,
  `residents_amber_court_hall`, `expedition_raft_camp`, `expedition_sky_camp`,
  `expedition_jungle_camp`, `expedition_dov_grave`, `expedition_thin_camp`, `expedition_codex`,
  `expedition_builder_echo`, `anomaly_sideways`, `anomaly_sunlit`, `anomaly_weightless`,
  `anomaly_lensing`.

### Fluids (wave 7, Part III F9)

Water, acid, brine and liquid metal flow as simple cellular fluids. They come with the Deep
Descent's new worlds (generator V11 places sealed aquifers and cave pools per stratum); older
worlds generate exactly as before, but any world can hold fluid once something pours it.

- **Flooding**: drill or blast into a sealed pocket and it floods the tunnel: fluid falls, spreads
  and finds its own level (it climbs U-bends), then settles still. A breach gushes; settled
  lakes cost nothing.
- **Reactions**: water quenches lava into obsidian in a hiss of steam, cools molten metal into an
  iron-ore crust (mine it), and acid slowly eats dirt, clay, sand, mud and snow, spending itself.
  Heavier fluids sink through lighter ones (metal, then brine, then acid, then water).
- **Rigs** float and drag in fluid; two thirds under, they handle like a submarine: slow and
  steerable, thrust to rise, hold drill (off the ground) to dive, the ballast pumps sipping fuel.
  Brine floats a rig up to bob at its surface; acid eats the hull and molten metal scalds it
  (lava plating helps); deep water past the hull's pressure rating (pressure bracing raises
  it) crushes slowly. Descent IV gear (Pressure Hull, Bio-Filter) plugs in here later.
- **On foot** the pilot swims in any fluid (holding its breath; brine and metal lift it up),
  burns in acid and molten metal, and creatures that cannot swim turn back at the shore.
- **Look and sound**: translucent bodies with a depth tint, rolling surfaces, falling streaks
  and drifting glints where fluid runs, bubbles, and glowing molten metal; splashes, the rush of
  a breach, steam, fizz and a bubbling underwater bed.
- **Try it**: in a test-mode world the pause menu's **Pour Test Pools** carves a row of all four
  beside the rig. Review scenes: `fluid_pools`, `fluid_rig_submerged`, `fluid_pilot_swimming`,
  `fluid_flood`, `fluid_metal_meets_water`.

### Round bodies (wave 11, Part III F12)

Planets and moons past the Moon are fully round. Their ground is laid out in rings of curved
tiles (the columns halve toward the core so tiles stay roughly square) and played on the
planet unrolled into a strip that wraps around, so digging, caves, fluids, drops and creatures
work exactly as at home. Gravity pulls toward the body's centre at the body's own strength.

- **You do the tilting**: nothing turns the craft but your rotation thrusters. Landing is a
  skill: touch down level and it is clean; tilted, the rig slides the way it leans and scrapes
  its hull; too steep and it tips over onto its side ("hold the rotation thrusters toward
  upright" rights it). On the ground it rests on its tracks, leaning where a track hangs over
  a step. Landing legs (an upgrade, not sold yet) widen the safe angle.
- **Small moons** (radius 400-1,000 tiles) are quick landings: drive all the way round one, or
  dig straight down through its strata to the core chamber.
- Placeholder grounds for now (a barren moon, a rocky world, an ice world, picked by the
  world's theme); the space waves add real ones. The ground is drawn round, bent by the polar
  renderer, with the galaxy above it. Review scenes of the strip itself (drawn flat):
  `planet_moon_surface`, `planet_moon_halving_ring`, `planet_moon_core_chamber`,
  `planet_rocky_surface`, `planet_ice_surface`, `planet_tipped_over`.

### Art and audio pass (wave 6)

A polish pass over everything from waves 3-5, presentation only (no gameplay, save, protocol or
world generation changes):

- **Pilot animation**: six-frame walk and run cycles with knee lift and arm swing, a knocked-back
  hurt flinch, a floating star-jump in the air and a bounding lope on the Moon (low gravity), and
  a downed tomb member lying flat on its back under a small red cross (it used to stand inside a
  ring). In crypts the top-down pilot strides along its heading with a swinging hand and the
  dodge roll tumbles with the helmet and pack swinging round.
- **Enemies that read**: night monsters (shadow hounds, dune stalkers, the umbral alpha,
  will-o'-wisps), sky swoopers (wyverns, sky rays, harpy nesters) and the Moon's creatures (the
  Warden and its kin) get a one-pixel rim light from white silhouettes packed into the sprite
  atlas: always on the Moon and in the dark sky high up, and at night a moonlit rim plus a dimly
  lit copy of the body drawn over the darkness, under their glowing eyes.
- **Fixes**: the builders' mule is a stocky grey mule with long ears (it read as a deer); rifle
  cartridges and shotgun shells have their own icons (they reused the rocket's), also beside the
  ammo count on the HUD; the Sun Colossus is redrawn to span exactly its contact box, so a pilot
  standing beside it is never drawn inside its arms, and its crouch keeps its feet on the floor.
- **Ambience by environment**: each seat crossfades (over about a second) between open-air beds
  (biome wind, rain, crickets, the sky isles' air), a thin whistling wind high up that fades
  toward space, the Moon's near-silence (the suit's hum and breath, radio ticks) with soft low-g
  footfalls, the lunar colony's machinery, a town's bustle (villages, sky towns, the home town)
  and tomb or crypt music, so loops never stack. The far howls of the night's prowl swell as
  night monsters gather near. Dawn and dusk play a short musical theme when the world clock
  turns, unless the moment already has its own music (the relief after a night with monsters,
  a slept-through night), so pieces never stack. Everything goes through the one mixer and its master volume.
- Review scenes: `pilot_animation_strip`, `ammo_icons`, `side_tomb_colossus_beside_pilot`, plus
  `night_*`, `sky_enemy_wyverns`, `moon_warden`, `tomb_coop_downed`, `crypt_dodge_roll` and
  `home_stage_frame` (the mule); check them at `--size 1280x720`, `1920x1080` and `844x390`.

## Controls

- `A` / `D` or left/right arrows: move horizontally and drill sideways while resting on the
  ground (in flight too with the gimbal drill mount)
- `W` / up arrow / space: thrust upward (with the crown bore, thrusting into a ceiling drills it)
- `S` / down arrow: drill downward (while resting on the ground)
- `C`: trigger an active scanner pulse when a scanner is installed
- `B`: drop, throw or place the selected explosive (a timed bomb unless you picked another)
- `Q`: select the next explosive you carry; throws follow the held direction (or your facing),
  with a dotted arc previewing where the charge will land
- `F` / `G`: remote detonator: every C4 you placed at once / one after another in placement order
  (the charges beep for a quarter second while they arm)
- Hold left mouse button or `V`: fire the hardpoint weapon at the mouse pointer (while the mouse
  rests for a few seconds, or without one, it auto-aims at the nearest target ahead)
- `X`: swap the next weapon you own onto the hardpoint
- `N`: build mode (see Bases): mouse or `W` `A` `S` `D` aim, click / `E` place, right click / `X`
  take down, `R` rotate, `Z` / `Q` or the wheel pick a piece (weapons hold fire while building)
- `E` / `Enter`: interact with buildings/interior counters, talk to the person beside you
  ("Talk to Mara"), confirm menus, or start game; with nothing else to use, climb out of the
  rig (and board it again on foot, see On foot)
- `,` / `.`: rotation thrusters: turn the rig left / right (the camera turns with the craft and
  the world turns around it; the HUD stays upright). On the home world a flying rig leans up
  to about 35 degrees and settles level when it lands or drills; strange fields (gravity
  shear, upside-down rooms, weightlessness) let it turn all the way round. Nothing ever tilts
  the rig for you: the gyro stabiliser and horizon hold assists (Options, off by default)
  work only once fitted to the rig.
- `Space`: on foot, jump (held in the air: the jetpack)
- `R`: on foot, the rope (grapple / the rig's winch / let go)
- In space (wave 11, see Space flight): `W` / `S` main and retro thrusters, `A` / `D` side
  thrusters, `,` / `.` turn the rig (a spin keeps going until you counter it), `/` engages
  or drops the Star Drive's cruise, `B` warps to the waypoint, `E` docks or undocks, `C`
  pings, and `M` opens the nav chart
- `Backspace` / `Esc`: close menus; `P` pauses/resumes
- `M`: large mine map with ore, hazard, rescue, and lost-cargo markers
- `H`: help screen
- hold `Tab`: detailed cargo/contract/status overlay
- `1`-`6`: quick-select upgrades while the shop is open
- `F5` / `F9`: quick save/load using `save.json` in the state directory
- Pause menu: save/load slots, options, exit confirmation
- `F11`: borderless fullscreen toggle (the window also resizes, maximizes and uses macOS
  native fullscreen from the green button)
- `F2` (test-mode worlds): the Atlas; inside it `W`/`S` choose, `A`/`D` change category, `/`
  searches, `Enter` goes, `X`/`Q` previous/next of the kind, `C` random, `B` bookmark, `F` back,
  `M` the map, `Esc` closes
- `+` / `-`: volume hotkeys

### Controllers

Xbox, PlayStation and Nintendo controllers (anything GLFW has a standard mapping for) work out
of the box, over USB or Bluetooth, and can be plugged in or out mid-game. Prompts switch to the
last device used: keyboard keys as keycaps, or the controller's own button glyphs.

| Action | Controller (Xbox names) | Keyboard |
| --- | --- | --- |
| Move / drill sideways | Left stick or d-pad (in the rig the d-pad drives only while the stick is centred) | `A` / `D` |
| Turn the rig (rotation thrusters) | d-pad left / right (in the rig; with the stick centred it also drives) | `,` / `.` |
| Thrust | `A`, or up on stick / d-pad (`RT` too while no weapon is mounted) | `W` / Space |
| Fire weapon | `RT` (held) | left mouse button / `V` |
| Aim weapon | Right stick (released: auto-aim at the nearest target ahead) | Mouse |
| Next weapon | tap `L3` (click the left stick; hold it for build mode) | `X` |
| Drill down | Down on stick / d-pad | `S` |
| Use building, counter, door | `X` | `E` |
| Climb out of the rig / board it | `X` (with nothing else to use) | `E` |
| On foot: jump (jetpack held in the air) / climb / crouch | `A` / stick up, down / stick down | `Space` / `W` `S` / `S` |
| On foot: hand pick or held arm | `RT` (held) | left mouse button / `V` |
| On foot: swap held arm | tap `L3` | `X` |
| On foot: rope (grapple, winch, let go) | `LB` | `R` |
| Confirm in menus / call rescue | `A` | `Enter` |
| Back / close / leave building | `B` (never pauses or quits) | `Esc` / `Backspace` |
| Menu navigation (auto-repeats) | Left stick or d-pad | arrows / `W` `S` |
| Pause | `Menu` (Start) | `Esc` / `P` |
| Inventory | `Y` | `Tab` |
| Map | `View` (Back) | `M` |
| Scanner / explosive | `LB` / `RB` | `C` / `B` |
| Next explosive | `R3` (click the right stick) | `Q` |
| Aim a throw | Right stick (else the held direction / facing) | `W` `A` `S` `D` |
| Remote detonator | `LT` tap: all at once; hold: in sequence | `F` / `G` |
| Previous / next tab or column (shop columns, quest desks, garage options) | `LB` / `RB` in menus | left / right arrows |
| Build mode | hold `L3` (a tap swaps weapons) | `N` |
| Build: place / take down / rotate | `A` / `X` / `Y` | click or `E` / right click or `X` / `R` |
| Build: piece / cursor | `LB` `RB` / d-pad or left stick | `Z` `Q` or wheel / `W` `A` `S` `D` or mouse |

The layout lives in one table (`PAD_BINDINGS` in `packages/game/src/input/gamepad.rs`).
`fatherload --list-gamepads` lists every detected controller (index, name, axis and button
counts, whether GLFW has a standard mapping); without a mapping a pad's buttons cannot be read.
Rumble is not available: raylib's desktop GLFW backend does not implement
`SetGamepadVibration`.

### Touch screens

A touch screen drives the first seat (other seats still join with controllers). The controls
appear when the screen is touched and go away when a key or controller button is pressed.

| Action | Touch |
| --- | --- |
| Drive, thrust, drill | Left thumb anywhere on the left: a stick appears under it (up flies, down drills) |
| Bomb | Tap `BOMB` to drop; drag it and let go to throw that way; drag back onto it to cancel |
| Next explosive / next weapon | `NEXT` / `GUN` |
| Fire the weapon | Hold `FIRE` (auto-aims); drag it to aim |
| Detonate | `DET` (shown while charges wait): tap for all, hold for one after another |
| Scanner | `SCAN` |
| Turn the rig (rotation thrusters) | Hold `<ROT` / `ROT>` at the top of the stick side |
| Infrastructure kits, build mode | `KIT` opens them |
| Enter / use / call rescue | Tap the prompt under the rig |
| Inventory, pause, map | `BAG`, `II`, `MAP` along the top |
| Menus | Tap a row; drag to scroll; `BACK` and `<` `>` (adjust a row, switch tabs); tap outside a dialog to close it |
| Build mode | Tap a tile to point, tap it again or `PLACE` to build; `DEL`, `TURN`, `<` `>`, `DONE` |

Options gains button size, stick side and an optional hold-to-thrust button once a touch
screen has been used. Phones (a short side under 520 points) zoom the world in and enlarge the
UI; held upright the view is a tall slice of the shaft. `fatherload --touch` makes the left
mouse button a finger on a desktop, and the `touch_*` scenes of `--screenshot-scenes` show the
controls (`--size 844x390`, `--size 390x844`). The layout and mapping live in
`packages/game/src/input/touch.rs`.

### Local split-screen

Up to four players share the screen, each in their own view. Player 1 uses WASD, player 2 the
arrow keys, and connected controllers fill seats without one (keyboardless seats first, then
the arrow-keys seat, then WASD). A spare controller joins with `A` or `Start`, taking a free
seat or adding one (up to four). A seat with a plugged-in controller is driven by that controller
only; its keyboard keys take over again while the controller is unplugged. Playing alone, the
keyboard and every controller drive the rig.

Every player has their own menus: each walks into any building, uses its counter, reads their
own inventory or map and leaves again inside their own view (with their own button prompts,
selection, scrolling and messages), while the others keep playing. Purchases and sales go to
the player who made them.

| Action | WASD seat (player 1) | Arrow-keys seat (player 2) |
| --- | --- | --- |
| Drive / walk, thrust, drill | `A` `D`, `W` / Space, `S` | left / right, up, down |
| Use building, counter, door | `E` | `Right Ctrl` or `Numpad Enter` |
| Confirm in menus | `Enter` / `E` | `Right Ctrl` or `Numpad Enter` |
| Back / close / leave building | `Esc` / `Backspace` | `Right Shift` (never pauses) |
| Menu navigation | `W` `S` (adjust `A` `D`) | up / down (adjust left / right) |
| Pause (shared pause menu) | `Esc` / `P` | `Right Alt` (Right Option) |
| Inventory / map | `Tab` / `M` | `Numpad 1` / `Numpad 2` |
| Explosive / scanner | `B` / `C` | `Numpad 0` / `Numpad .` |
| Next explosive / detonate / in sequence | `Q` / `F` / `G` | `Numpad 3` / `Numpad 4` / `Numpad 5` |
| Fire weapon / next weapon | mouse button or `V` / `X` (mouse aim) | `Numpad 6` / `Numpad 7` (auto-aim) |
| Build mode; in it: take down, previous / next piece, rotate | `N`; `X`, `Z` / `Q`, `R` | `Numpad 8`; `Numpad 6`, `Numpad 9` / `Numpad 3`, `Numpad 7` |
| Scroll a dialog | mouse wheel, `Page Up` / `Page Down` | (the selection scrolls it) |

Controllers use their usual buttons (table above) in their own seat, with the right stick
scrolling that seat's dialogs. Global keys (quick save/load, fullscreen, volume, `F2`, `F3`,
help) stay on the main keyboard.

Any player's pause opens one shared pause menu, driven by the player who opened it, which
pauses everyone (save, load, options and exit work as alone). If that player's controller is
unplugged, player 1 (keyboard) takes the menu over; the window's close prompt is always answered
by player 1 and then hands the pause menu back. The world keeps running while at
least one player is outdoors, and rests only once every player is inside a building, just as
playing alone pauses while you are inside. Online sessions never pause. What a player has open
is not saved: after loading, everyone starts outdoors.

## Surface Buildings

Press `E` (controller `X`) at a surface building to walk inside its room. Move with `A`/`D` or the stick, use the service counter with `E`/`X`, and leave through the door or `Backspace`/`Esc` (controller `B`). In split-screen each player enters and uses buildings on their own (see above).

- Fuel Station: refills fuel in partial or full service increments
- Repair Garage: repairs hull in partial or full service increments, and runs Iona's **paint
  bay** (the counter's last row; free): paint the hull, chassis and trim from 17 colours, pick a
  thruster flame colour, drill style (standard, serrated, auger, twin bit, crusher), hull pattern,
  decal and roof / nose / tail attachments. The rig in the room and a preview card wear the job
  live before you apply it; Back returns to the counter without applying. A few items (stealth
  paint, prism flame, spire drill, circuit pattern, crown decal, halo) are locked: villages with
  a paint-and-trim shop sell them (test-mode worlds unlock everything).
- Ore Depot: sells mineral/artifact cargo and shows receipt history
- HQ: completes contracts, provides named story/radio briefings, and offers/collects debt advances
- Explosives Shack: bomb packs, plus special charges (the better stocked the shack, the more it
  sells): flares (light the tunnels and chart the area around them), sticky bombs (stick where
  they hit), C4 (placed on a surface, fired by the remote detonator all at once or in sequence),
  cluster bombs (burst into a fan of bomblets), incendiaries (fire that eats dirt and clay),
  shaped charges (a long, narrow tunnel straight into the surface they stick to), mining charges
  (a wide blast that leaves ore and relics standing), EMP charges (for machines), smoke charges
  (a lingering screen that hides whatever is inside it from auto-aim and, later, from creatures)
  and scare charges (a deafening bang that breaks nothing and sends critters running). Blasts push
  rigs and loose charges around, hurt targets and set off charges nearby. Special charges are
  carried as items in the cargo bay (they take no slots); the inventory lists every type.
  The shack's **arms rack** sells and upgrades the hardpoint weapons (see Weapons below), the
  weapon capacitor and rocket packs.
- Upgrade Shop: sells drill, fuel tank, cargo bay, engine, hull, radiator, scanner, and bomb-pack upgrades,
  plus the late-game **Drill Mount**: the Gimbal Drill Mount (6,000 cr, on sale once anyone reaches
  the Crystal Faults) drills sideways in flight, and the Crown Bore Mount (24,000 cr, once anyone
  reaches the Pressure Cathedrals) drills up into a ceiling you thrust against

## Weapons

Rigs mount one weapon on the hardpoint on their dome; the gun turns toward your aim, shows a gold
notch per upgrade tier and fires from the roof:

| Weapon | Shack level | What it does |
| --- | --- | --- |
| Rivet Gun (450 cr) | 0 | Fast, light rivets; cheap on energy, chips dirt and clay |
| Harpoon (900 cr) | 0 | A barbed spear on a line: hurts small targets and reels loose loot in |
| Scatter Blaster (1,400 cr) | 1 | A close-range fan of seven pellets with a kick |
| Plasma Cutter (2,400 cr) | 1 | A beam that melts targets and slowly slices through rock |
| Rocket Launcher (3,600 cr) | 2 | Rockets that burst on impact (rocket ammo, six for 120 cr) |
| Railgun (7,500 cr) | 3 | An instant slug that ignores armour, pierces targets and punches through soft rock |
| Tether Harpoon (6,000 cr + 2 storm glass) | 2 | Hook rock and swing from the line while the trigger is held; thrust reels in |
| Arc Rifle (9,000 cr + 6 storm glass) | 3 | Lightning that leaps from its first target to three more nearby; machines hate it |
| Flak Cannon (10,000 cr + 3 storm glass, 3 aether crystal) | 3 | Proximity shells that burst into shrapnel; airships fold to it |
| Gravity Gun (11,000 cr + 6 aether crystal) | 3 | Hold to pull in loot, loose blocks it rips out, charges and foes (held foes hang helpless); let go to fling them |

The last four are **Act II weapons**: sold only on worlds with a sky to explore (generator
v9), and paid partly in sky materials mined on the continents or looted from sky enemies.

Each weapon upgrades from Mk I to Mk IV (harder hits, faster cycling, less energy). Energy
weapons draw on the rig's capacitor (100 energy, up to 360 with capacitor upgrades), which
recharges from fuel; the HUD shows the mounted gun, its capacitor gauge or ammo, and an empty
click tells you it is dry. Shots chip plain soil and rock (ore, relics and vault walls are left
alone), set off charges they hit, and never hurt crewmates (friendly fire is off). Aim with the
mouse, a controller's right stick, or let auto-aim pick the nearest target ahead (the
split-screen arrow-keys seat always auto-aims); each split-screen view draws its own reticle:
a crosshair for free aim, closing brackets when auto-aim locks on. Test-mode rigs carry every
weapon at its top tier.

## Multiplayer Status

Multiplayer is being rebuilt per `multiplayer-architecture-plan.md`:

- `packages/simulation`: the canonical `WorldState` / `simulate_tick` (all gameplay, including
  town services, runs inside fixed ticks).
- `packages/netcode`: sans-IO `HostSession` / `ClientSession` with prediction, interpolation and
  a link simulator, proven end to end by its harness.
- `packages/quic_transport`: the Quinn network thread, framing, datagrams, link conditioner and
  LAN discovery.
- `packages/game`: single-player and local split-screen (up to 4 seats; `F2` adds a seat, the
  title menu's "Local Split-Screen" starts with two) run on `HostSession` with local seats.

Online host/join is hidden in the menus until it is wired onto the same runtime.

### Screen effects (accessibility)

Strange fields deep down can pixelate, mirror, tint, fog, darken or ripple the screen
(the post-process stack, per split-screen view). Options > Screen effects sets them to Full
(default), Reduced or Off. Whatever the setting, effects never flash or strobe: every level
eases in over a fraction of a second and changes at a capped rate, exposure and bloom are
capped, ripples stay under one cycle a second and scanlines never move. A dark field still
darkens the view with effects off (darkness is gameplay); a mirror field still mirrors the
controls. Test-mode worlds can place field zones from the pause menu (Place Field Zone cycles
the presets, Clear Field Zones removes them).

## Saves and Settings

Files live in `$FATHERLOAD_STATE_DIR`, else `$XDG_STATE_HOME/fatherload`, else
`%LOCALAPPDATA%\fatherload` on Windows, else `~/.local/state/fatherload`.

- Quick save: `save.json`; save slots: `save-slot-1.json` through `save-slot-3.json`
- A save (format v8) is the host's `WorldState` plus play time. Its terrain is stored as the
  world's generation settings (seed, size, richness, hazards, generator version) and only the
  tiles that differ from freshly generated terrain; loading regenerates the rest and checks a
  terrain digest (an endless v5 world stores only its changed chunks, and the map discovery as
  one compact record per chunk). Rigs carry their goods in item containers (cargo bay, block
  hopper; special explosives are items there too) and live objects (gas clouds, boulders, drops,
  explosive charges, projectiles) are entities. v1-v5 saves (which wrote out every tile), v6
  saves (separate cargo maps and hazard lists) and v7 saves (a separate charge list and explosive
  count maps, and no crew roster) are migrated on load and re-saved as v8; saves from before
  seeded worlds keep their terrain and load as legacy worlds. A save from
  an unknown generator version is refused with a clear error. Loading starts a new session
  epoch.
- Every rig in a save remembers who plays it (v8 crew roster), so **resuming a multiplayer game
  remembers every player**: rig, cargo bay, block hopper, credits, upgrades, quests, cosmetics and
  explosives. Players who are not connected are parked outside the live world (not simulated or
  drawn, but saved) and get their rig back, exactly where they left it, when they rejoin from
  their own device (with a "Welcome back, <name>" banner). Loading a save that holds other
  players offers **Host this world online** straight away; the host's lobby lists every saved
  player ("OFFLINE, WAITING" until they join). A brand-new player gets a fresh rig beside them.
  Someone joining from a new machine or cleared browser storage can **claim** their saved rig in
  the lobby, and the host approves or denies it (the rig then belongs to the new device). Local
  split-screen seats take their saved rigs back in seat order (a save loaded from the title
  starts with as many seats as it had). Saves from before the roster load with every rig; the
  rigs beyond the host's seats are parked instead of deleted.
- `profile.json` holds this install's persistent player id (random, made on first run) and
  display name (edit it there; `FATHERLOAD_PLAYER_NAME` overrides it for one run). It is sent
  when joining so hosts recognize you; in the browser it lives in the site's IndexedDB with the
  saves. A second game window on the same install (or a client on the host's own install) joins
  with a fresh rig that is not tied to the profile. Claiming another saved rig while playing your
  own parks your own rig (it can be claimed back) rather than discarding it.
- Slot UI displays depth and credits; the title shows the latest save's depth, credits and playtime.
- Settings are stored in `settings.json` and persist volume/fullscreen preferences and
  the window size/maximized state.
