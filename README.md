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

The dark caves below the topsoil are home to enemies, creatures and machines that grow tougher with depth: bat swarms, burrowers that hear your drill through the rock, acid spitters, rock mimics posing as boulders, armoured shellbacks, magma wisps in the mantle, brood nests and their brood lords, rogue mining drones, sentry turrets, excavators that chew toward you and shielded ancient guardians. Each has a tell before it strikes (a screech, a wind-up, a charge glow) and a counter: bombs crack armour, EMP charges knock machines out and drop shields, flares lure bats away. A hit knocks the rig back and leaves it a moment of invulnerability; the surface, the town and your outposts stay safe, kills drop ore and materials, and cleared lairs stay cleared. The New World dialog (and the host's pause menu) sets enemies to Peaceful, Normal or Hard; god mode makes rigs immune.

The upgrade shop sells long upgrade lines (up to 15 tiers: endgame bays of ~300 slots, tanks of ~3,000 fuel, hulls of ~2,500, drills that plow through the hardest rock) and a hazard counter for every danger: Heat Shielding (each layer runs hotter; radiator and shielding together decide how deep you stay cool), Lava Plating (lava is drillable but burns; the top tier is lava-proof), Gas Scrubber, Pressure Bracing (pressure pockets, explosive pockets, magma vents) and Boulder Deflector.

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

Generator v8 (the current one) is v7's ground with **a sky to explore**, the start of the
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
  gravity (their interiors come in a later update).
- **Orbit and the Moon**: a near-zero-gravity band where rigs drift with heavy inertia, then
  the Moon itself: one giant landmass about 2,000-2,900 tiles wide hanging still above a fixed
  column a few thousand tiles from town, grey regolith and craters on top, a rocky underside
  you can mine from below, and low gravity (gravity bombs arc far, drops float).

**Altitude physics**: the air thins with height, so engines lose thrust and burn fuel faster
(high-altitude **Altitude Intake** upgrades win it back), and it gets cold: past the
archipelago an uncovered cold level burns the hull (the **De-Icer** line covers two levels a
tier). Wind layers carry a coasting rig (steer against them, or ride them), updrafts lift it,
and gravity is per place: normal up through the islands, lighter on the continents, nearly
none in orbit, low on the Moon. A **sky gauge** under the wallet shows the band, the air and
the thrust your intakes keep, the cold against your de-icer, gravity and the wind; the map
follows the rig among the islands, which show on it. Climbing into a new band, catching an
updraft and icing up each have their own sound. Older worlds keep an empty sky and standard
air.

### Balance

The balance harness (`fatherload --balance-report [--seeds N] [--minutes M] [--only
economy,progression,combat,exposure,building,climb]`, release build; details in
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
  and how it gets on with its neighbours.
- **Every building with a door is enterable** (`E` at its door, as in town): rooms are dressed
  in the village's style and the building's trade. In split-screen each seat uses villages on
  its own.

Driving into a village raises a banner with its name, style and specialty ("NEW VILLAGE
DISCOVERED" the first time, "WELCOME BACK TO" later). Villages show on the minimap and the
mine map: gold with their name once visited, blue once charted, a dim outline for surface
villages not reached yet; underground settlements stay hidden until visited or charted.

Village review (no window opens): street layout, every counter's deals, residents and the
stories each village grows, nearest town first:

```bash
cargo run --release --bin fatherload -- --village-report [--seed 7] [--count 8]
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
  snow, mud and clay where you aim (down while crouching): slow, for footholds, not shafts.
  Every chipped tile drops a block. Rock just clinks.
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
- `Space`: on foot, jump (held in the air: the jetpack)
- `Backspace` / `Esc`: close menus; `P` pauses/resumes
- `M`: large mine map with ore, hazard, rescue, and lost-cargo markers
- `H`: help screen
- hold `Tab`: detailed cargo/contract/status overlay
- `1`-`6`: quick-select upgrades while the shop is open
- `F5` / `F9`: quick save/load using `save.json` in the state directory
- Pause menu: save/load slots, options, exit confirmation
- `F11`: borderless fullscreen toggle (the window also resizes, maximizes and uses macOS
  native fullscreen from the green button)
- `+` / `-`: volume hotkeys

### Controllers

Xbox, PlayStation and Nintendo controllers (anything GLFW has a standard mapping for) work out
of the box, over USB or Bluetooth, and can be plugged in or out mid-game. Prompts switch to the
last device used: keyboard keys as keycaps, or the controller's own button glyphs.

| Action | Controller (Xbox names) | Keyboard |
| --- | --- | --- |
| Move / drill sideways | Left stick or d-pad left/right | `A` / `D` |
| Thrust | `A`, or up on stick / d-pad (`RT` too while no weapon is mounted) | `W` / Space |
| Fire weapon | `RT` (held) | left mouse button / `V` |
| Aim weapon | Right stick (released: auto-aim at the nearest target ahead) | Mouse |
| Next weapon | tap `L3` (click the left stick; hold it for build mode) | `X` |
| Drill down | Down on stick / d-pad | `S` |
| Use building, counter, door | `X` | `E` |
| Climb out of the rig / board it | `X` (with nothing else to use) | `E` |
| On foot: jump (jetpack held in the air) / climb / crouch | `A` / stick up, down / stick down | `Space` / `W` `S` / `S` |
| On foot: hand pick | `RT` (held) | left mouse button / `V` |
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
