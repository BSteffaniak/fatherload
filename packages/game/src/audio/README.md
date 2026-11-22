# Procedural audio

The game ships no audio files. Every sound is synthesized in code at startup (on a background
thread, ~1 s in release builds, ~20 MB of samples), then played by a software mixer on raylib's
audio thread.

```
gameplay events --effects--> PlacedCue ---+
shell (menus, services) ---> SoundCue ----+--> Director --MixerCommand--> Mixer --> raylib stream
RenderSnapshot (rigs, drill, hazards) ----+       (audio.rs AudioBus)      (audio thread)
```

## Modules

| Module      | Role |
|-------------|------|
| `dsp`       | 44.1 kHz building blocks: seeded RNG, polyBLEP saw/square (DC-free pulse) and integrated triangle, sine, white/pink/brown noise, ADSR, percussive and glide envelopes, Simper state-variable filter (LP/HP/BP, safe under per-sample modulation), two-operator FM, saturation, wavefolding, decimation, damped feedback delay, Freeverb-style stereo reverb, equal-power pan, cubic Hermite resampling. |
| `patch`     | The designed patches (below), their playback metadata (category, variations, priority, instance cap, reverb send, pitch/gain jitter, ducking) and `render_patch`: design, a 30 Hz sub-cut (12 dB/oct on one-shots, 24 dB/oct on loops; also removes DC), 1 ms / 12 ms edge fades (one-shots) or a 0.3 s equal-power seam fold (loops), loudness normalization. |
| `bank`      | Every patch's variations pre-rendered in parallel. One-shots get 4 variations, loops 1. |
| `mixer`     | 24 voices (+8 slots for stolen voices to fade out over 6 ms). Per voice: gain, pan, playback rate, low-pass (distance muffling), reverb send, all smoothed per 64-frame block and ramped across it. One-shots pick a variation (never the last one) and jitter pitch/gain. Loops are keyed (`LoopKey`): refreshed each frame, faded in/out (60 ms / 100 ms), and fade on their own if not refreshed for 0.35 s. Voice stealing: oldest instance past a patch's cap, else the lowest-priority oldest voice; a request outranked by everything playing is dropped. Big patches duck lower-priority voices (10 ms attack, 0.35 s hold, 0.25 s release). Reverb bus, smoothed volume, peak limiter at 0.95. No allocation in the render path. |
| `cues`      | `SoundCue` -> patch, gameplay event -> cue (exhaustive, so a new event kind must decide how it sounds), and `SoundSource` (interface / player / world point). |
| `director`  | Per frame: places one-shot cues, and drives everything continuous from the snapshot (table below). |
| `analysis`  | Peak, RMS, DC, loudness (loudest 100 ms window after a 100 Hz high-pass), spectral centroid (FFT), loop-seam ratio. |
| `render_tool` | `fatherload --render-audio <dir> [--seed N]`. |

## Loudness

Each patch is normalized so its loudness hits its category's target, then pulled down if its
peak would pass -1 dBFS (very transient sounds such as landings end up 1-4 dB under target).

| Category  | Target | Patches |
|-----------|--------|---------|
| interface | -24 dB | menu sounds, toast |
| loop      | -22 dB | continuous sounds (the director scales them further) |
| alarm     | -21 dB | fuel/hull alarms |
| feedback  | -20 dB | pickups, coins, crates, tools |
| impact    | -17 dB | landings, bumps, damage, drill transients |
| big       | -15 dB | explosion, stings, fanfares |

## Patch designs

| Patch | Category | Design |
|-------|----------|--------|
| `ui_move` | interface | 2.3 kHz sine blip gliding down 25% in 12 ms, plus a 6 ms high-passed noise click. |
| `ui_confirm` | interface | Two rising notes (G5, D6), pulse + sine through an enveloped low-pass. |
| `ui_back` | interface | Two falling triangle notes (A5, D5). |
| `ui_error` | interface | Two buzzy pulses: detuned 150/159 Hz squares, low-passed, saturated and folded. |
| `toast` | interface | Soft FM bell (E6) with a quieter fifth above. |
| `pickup` | feedback | Two quick FM glints a fifth apart from a pentatonic set (ore and materials collected). |
| `pickup_block` | feedback | A block clacking into the hopper: a short knock (420 -> 260 Hz thump, band-passed pink noise) and a tiny upward triangle blip. |
| `pickup_artifact` | feedback | An artifact collected: a low FM bell under three rising pentatonic glints. |
| `stowage_full` | interface | Cargo bay or hopper full: two muffled clunks (saturated thump plus a dull clank) against a closed hatch. |
| `block_dump` | impact | Blocks tipped out: a brown-noise tumble with nine scattered clacks settling. |
| `coins` | feedback | Drawer thunk, 5-7 metallic coin hits (inharmonic partials 1 : 2.76 : 5.4 : 8.93) rising in pitch, then a register "ching" bell. |
| `purchase` | feedback | C-E-G-C brass arpeggio (detuned saws, enveloped low-pass), sparkle bells, 110 ms damped echo. |
| `crate_open` | feedback | Latch clack, lid thump, creaking hinge (band-passed saw with vibrato), two sparkle bells. |
| `salvage_open` | feedback | Metal clank, pneumatic hiss sweeping up, low thunk, a bell. |
| `bomb_arm` | feedback | Metal clunk then an arming beep (1.76 kHz pulse). Also dropping a mining charge and placing C4 or a shaped charge. |
| `throw_whoosh` | feedback | A thrown charge: band-passed pink noise swishing up then down, a light clink. |
| `charge_stick` | feedback | A charge sticking to rock: a short thunk, a brown-noise squelch and a square-wave arming chirp. |
| `detonator` | feedback | Remote detonator: a switch clank and two rising square beeps (E6, A6). |
| `small_blast` | impact | Bomblet / cluster pop: a tight high-passed crack, a short low-passed body and a small sub boom, debris grains. |
| `cluster_burst` | impact | A cluster bomb bursting: a thump and six tinny clanks of bomblets scattering over a hiss. |
| `shaped_blast` | big | Shaped charge: a hard crack, a band-passed jet roaring away (4 kHz gliding to 500 Hz), a sub boom and falling chunks. |
| `deep_blast` | big | Mining charge: a very low saturated boom, a long brown-noise rumble and settling debris. |
| `fire_whoosh` | impact | Incendiary: a soft thump and a low-passed rush of flame opening to 2.6 kHz, crackle grains. |
| `fire_crackle` | feedback | Fire spreading: a pink-noise breath with dry high-passed pops. |
| `emp_pulse` | impact | EMP: a bit-crushed saw zap gliding down to 90 Hz, an FM ring and a short echo. |
| `flare_ignite` | feedback | A flare striking: a high-passed scratch fluttering like a sputter, and a small pop. |
| `creature_screech` | alarm | A creature noticing a rig: a warbling FM screech rising then falling, with a band-passed breath. |
| `creature_hiss` | feedback | A creature winding up: a swelling band-passed hiss over a low-passed growling saw. |
| `acid_spit` | feedback | Acid launched: a wet glottal sine pop, a high-passed spray and bubbling pops. |
| `creature_hurt` | impact | A creature hit: a falling FM yelp and a pink-noise squelch. |
| `creature_death` | impact | A creature dying: a long falling FM wail, a wet crunch and a low thump. |
| `machine_alert` | alarm | A machine locking on: two quick low-passed square beeps, the second a third higher. |
| `machine_charge` | feedback | A machine charging a shot or a dash: a resonant saw whine rising with quickening tremolo. |
| `machine_zap` | feedback | An energy bolt: a bit-crushed saw zap gliding down from 2.4-3 kHz. |
| `machine_hurt` | impact | A machine hit: an inharmonic metal clank and a crackle of high-passed sparks. |
| `machine_death` | impact | A machine destroyed: a low-passed bang over a sub thump, a power-down saw whine and rattling clanks. |
| `enemy_bite` | impact | A bite or ram on a rig: a crushed, saturated noise crunch, a thump and a hull knock. |
| `smoke_burst` | impact | A smoke charge: a soft canister pop and a long, swirling low-passed exhale. |
| `scare_bang` | big | A scare charge: a sharp white-noise crack, a ringing 3.3 kHz whine and a short boom, with a slap of echo. |
| `rivet_shot` | feedback | A rivet: a tight band-passed snap, a metallic tick and a small thump. |
| `scatter_shot` | impact | A scatter blast: a high crack, a wide pink-noise burst, a thump and a spray of pellet grains. |
| `cutter_zap` | feedback | One plasma-cutter pulse: a saw and FM buzz through a wobbling band-pass (back to back they hum). |
| `rocket_launch` | impact | A rocket: a thump and a roaring band-passed whoosh rising to 2.4 kHz. |
| `harpoon_shot` | impact | A harpoon: a taut plucked twang and the high whizz of line paying out. |
| `railgun_shot` | impact | A railgun: a rising capacitor whine, a hard crack, a long inharmonic metal ring and a deep thump. |
| `weapon_dry` | interface | The empty click of a dry trigger: two small clanks. |
| `weapon_mount` | interface | A weapon locking onto the hardpoint: two clacks and a servo chirp. |
| `impact_rock` | feedback | A shot chipping rock: a dusty pink-noise tick with grit. |
| `impact_hit` | feedback | A shot striking a target: a solid thwack. |
| `target_destroyed` | impact | A target breaking apart: a thump, a crunch and a clatter of parts. |
| `build_place` | impact | A block set in place: a gritty pink-noise scrape, a dense knock and a settle of grit. |
| `module_built` | feedback | A module bolted down: a heavy clunk and clank, an air-tool rattle, a two-bell ready chime. |
| `build_remove` | impact | Deconstruction: four ratchet ticks falling in pitch, then a pop and a clink. |
| `claim_stake` | feedback | A claim staked: the beacon's spike thumped in, a rising G-C-G brass call sign, a bell and an echo. |
| `fuel_pump` | feedback | Refuelling at a base station: a nozzle clunk, a run of rising sine bubbles over a band-passed flow. |
| `supply_drop` | feedback | A supply drop: a falling triangle whistle, the canister thudding home with a clank and a bell. |
| `repair_weld` | feedback | A repair bay at work: high-passed arc crackle and a low-passed saw servo whirr, a done bell. |
| `storage_stow` | feedback | Stowing in a chest: a wooden lid thunk and two latch clinks. |
| `uplink_beam` | feedback | The cargo uplink: a sine and FM shimmer charging up two octaves, sparkle bells as it beams away. |
| `home_set` | feedback | A home point set: a warm G-major chord on soft brass and a bell. |
| `fuse_tick` | feedback | 30 ms tick: high-passed noise snap plus a 3.2 kHz blip. The director speeds it up (and raises its pitch) as the fuse burns. |
| `deploy` | feedback | Six ratchet clicks, a heavy thunk and a metal ping. |
| `scanner_pulse` | feedback | Sonar ping gliding down, FM shimmer, a band-pass whoosh sweeping up, echoes (180 ms, 45% feedback). |
| `drill_bite` | impact | Saturated band-passed noise bite with a short low knock (drill engaging). |
| `drill_glint` | feedback | High FM chime (ratio 3.01) with a fifth above: ore glinting under the drill. |
| `thrust_ignite` | feedback | Brown-noise "whoomp" through a low-pass opening to 2.5 kHz, a low thump and a band-pass swell. |
| `land_light` / `_medium` / `_heavy` | impact | Sine thump gliding down (heavier tiers lower, longer, more saturated), low-passed body noise, a metal clank and knock; heavy adds a decimated crunch and debris grains. |
| `wall_bump` | impact | Short mid thump, band-passed body, small clank. |
| `hull_hit` | impact | Distorted, decimated noise crunch, three ringing hull resonances, low thump. |
| `drill_crunch` | impact | 50 ms gritty burst (band-passed noise, saturated, decimated) with a knock. Pitched down and quieter it doubles as soft-material clods. |
| `break_soft` / `break_hard` | impact | Break-through: thump, low rubble rumble and crumbling grains (dirt: dull and low; rock: a cracking transient and bright chips, saturated). |
| `vault_reset` | impact | Square/saw glissando down two octaves, then a clank and thump. |
| `alarm_fuel_low` | alarm | Two-tone beep (880 / 660 Hz). |
| `alarm_fuel_critical` | alarm | Three fast, brighter beeps (C6). |
| `alarm_hull_low` | alarm | Warbling triangle (520 Hz, 14 Hz vibrato) over a sine an octave down. |
| `alarm_hull_critical` | alarm | Detuned saws (620 / 655 Hz), resonant low-pass, saturated, 18 Hz tremolo. |
| `explosion` | big | Layered: noise crack, sub boom (60 -> 28 Hz, saturated), brown-noise body with a falling low-pass, band-passed mid roar, 60 debris grains and chunk knocks over 2.5 s; ducks other sounds by up to 55%. |
| `spore_puff` | feedback | A spore pod bursting (Wave 8): a fat low "pof" of noise, an airy settling hiss and tiny pops. |
| `root_creak` | feedback | Roots tightening: a resonant band-passed saw creak bending in pitch, fibrous cracks. |
| `flesh_squelch` | feedback | Flesh knitting shut: a wet low-passed squelch that sucks closed over a falling gulp. |
| `hive_chime` | feedback | The Amber Court: a buzzing G and three rising marimba notes. |
| `mind_roar` | big | The Mycelial Mind: a warbling FM growl over a sub swell, breath full of spores. |
| `loop_fungal` | loop | Fungal Forest music: a breathing A/E pad, slow pentatonic glass bells, spore hiss. |
| `loop_roots` | loop | Root Labyrinth music: a low bowed ostinato (D, E flat, D, C) over a creaking drone and knocks. |
| `loop_hive` | loop | The Hive's music: a buzzing wing drone, a busy marimba figure in G, mandible clicks. |
| `loop_heartbeat` | loop | Flesh Caverns music: four lub-dubs a loop over a sour minor-second drone; the director plays it at the stratum's heartbeat. |
| `loop_glow_sea` | loop | Glow Sea music: near silence, a low moan and slow distant glass chimes. |
| `loop_mind` | loop | The Mind's fight: pounding toms and a detuned saw choir gliding a minor third. |
| `poi_discovered` | big | Mysterious sting: detuned-saw pad (A3, E4, B4 entering in turn) under a slowly moving low-pass, two FM bells, 300 ms echo. |
| `vault_fanfare` | big | Door boom and rumble, a C-E-G brass arpeggio into a held C major chord, a run of bells. |
| `milestone` | big | D major brass swell under a quick ascending bell arpeggio. |
| `rescue` | big | Tractor beam: FM sweep rising three octaves with 12 Hz tremolo and shimmering noise, resolving to an E/B chord and a bell. |
| `village_arrival` | big | Arriving in a village for the first time: a soft triangle pad (G3, B3, D4) under a plucked G-major bell arpeggio and a welcoming high bell, short echo. |
| `shrine_chime` | feedback | A shrine blessing: three high temple bells (C6, E6, G6) rung in turn, shimmering into an echo. |
| `hatch_open` / `hatch_close` | feedback | The pilot climbing out / in (Part II F5): a latch clank and thunk with a pneumatic band-passed hiss rising (opening) or settling (closing) around it. |
| `pilot_jump` | feedback | A boot scuff and a short airy whoosh sweeping up. |
| `pilot_land` / `_hard` | impact | Boots landing: a dull sine thud and grit; the hard landing is lower and longer with a gear clank and a winded formant grunt. |
| `pilot_hurt` | impact | A padded smack on the suit and a short formant "oof". |
| `pilot_down` | big | A falling sigh over a body thud, then the suit's flatline beep fading out (light, a respawn follows). |
| `alarm_oxygen` | alarm | Two quick breathy triangle pips (1320 / 1100 Hz). |
| `jetpack_burst` / `jetpack_sputter` | impact | The jetpack lighting: an ignition pop and a short saturated roar with hiss; running dry: three band-passed coughs, each weaker. (While it burns the director runs `loop_thrust` as its roar.) |
| `pick_chip` / `pick_clink` | impact | The hand pick: a small metal tick and crumbling dirt grains; on rock, a bright ringing clink. Breaking a block loose plays `break_soft`. |
| `splash` | impact | A plunge thump, a falling spray and a bubbling tail. |
| `ledge_grab` | feedback | Gloves catching a ledge: a leathery slap and a scrape of boots. |
| `loop_thrust` | loop | Pink-noise band hiss with flutter, a sputtering 2.8 kHz jet band, a little brown-noise rumble and a 55/82.5 Hz hum, saturated. |
| `loop_drill_soft` | loop | Motor (95 Hz saw + 190 Hz pulse), pink-noise scrape modulated by a 16 Hz rotation, clods. |
| `loop_drill_hard` | loop | Motor (120/240 Hz saws), heavily saturated 2.2 kHz grind with 24 Hz chatter and random roughness, faint metal squeal partials. |
| `loop_gas_hiss` | loop | High-passed and band-passed white noise with a slow swell. |
| `loop_lava_sizzle` | loop | Crackling sizzle, random pops, rising bubble blips, low rumble. |
| `loop_boulder_rumble` | loop | Brown-noise rumble rolling at 4 Hz, a band-passed grind, knocks with cracks, grit. |
| `loop_wind` | loop | Surface wind: pink noise through a howling band-pass whose centre wanders slowly, a narrow whistle above it, gusts swelling the level, brown-noise buffeting. |
| `upgrade_fanfare` | big | An upgrade fitted at the shop: ratchet clicks and an air-tool hiss as the part bolts on, a square-wave power-up sweep, then a rising C-E-G-C brass fanfare onto a held chord with sparkle bells. |
| `milestone_fanfare` | big | A first-time milestone and its cosmetic unlock: a G-B-D brass call, a bell answer and a glittering high shimmer. |
| `act_two_fanfare` | big | The sky opens (Act I -> II): a deep swell and wind rush, a broad rising brass anthem over a sustained C major chord, slow high bells and a long echo; ducks everything else. |
| `pistol_pop` | impact | The pilot's sidearm: a band-passed snap over a small thump and a short room echo. |
| `rifle_crack` | impact | The pilot's rifle: a high-passed supersonic crack, a body thump and a rolling brown-noise tail. |
| `shotgun_boom` | impact | The pilot's shotgun: a sub thump under a falling low-passed pink-noise blast, then the pump's two clacks. |
| `wrench_swing` | impact | A melee swing: a rising band-passed whoosh into a heavy metal clang. |
| `grapple_fire` | feedback | The grapple or the winch: a pneumatic thunk, a whistling saw zip gliding up and a metal bite. |
| `glider_open` | feedback | The glider opening: a cloth snap and a soft low-passed rush of air. |
| `lever_clunk` | feedback | A ruin lever or plate: a heavy thump and clank, a four-tick ratchet and a settling knock. |
| `gate_grind` | impact | A ruin gate moving: a wobbling low saw groan, rattling grains and a final clang. |
| `keycard_beep` | feedback | A keycard door accepting its card: a reader chirp and two rising square beeps. |
| `loop_rain` | loop | Rain: a soft band-passed pink-noise hiss over a lower body, ~70 droplet ticks a second and a few heavier drips. |
| `loop_tomb_run` | loop | A side-view tomb's run music (WP-TOMBS T28): a 73.5/110 Hz triangle drone under a slow plucked FM line in D phrygian (90 bpm, two bars of three), a frame drum on the bar. |
| `loop_tomb_boss` | loop | The guardian's theme: toms on every eighth (120 bpm, accents on the beat), a low plucked D ostinato and brass stabs a half step apart (D, E flat, D, D flat). |
| `blade_swish` / `dart_fire` / `bone_rattle` | feedback | A blade sweeping past (a band-passed whoosh up and back, a thin ring); a dart loosed (a click and a falling whistle); bones rattling (dry band-passed clicks). |
| `crusher_slam` / `colossus_slam` | impact | A crusher (a heavy stone thud, a crunch, grit); the colossus landing (a very low boom, a stone crack, a long rubble rain). |
| `floor_crumble` / `sand_pour` / `fog_burst` / `statue_grind` | feedback | A slab giving way; sand or water starting to pour (a swelling hiss over a low rush); a cursed vent (a breathy rush and a ghostly bending tone); stone grinding. |
| `mummy_groan` / `matriarch_wail` | impact | A mummy's low saw voice through shifting formants; the matriarch's two FM voices rising and falling with an uneasy vibrato. |
| `dodge_roll` | feedback | A crypt dodge roll (T29): a pink-noise cloth swish sweeping up and down and a soft shoulder thud. |
| `arrow_twang` | feedback | A trap's arrow loosed (also its shot cue): a plucked saw string dropping a little in pitch and the shaft's hiss. |
| `spike_shing` | impact | Floor spikes rising: a bright metallic clank, a stony knock and a high scrape. |
| `pit_drop` | impact | A fall into a pit: a whistle dropping away, a far thud and pebbles. |
| `lord_roar` | big | The crypt lord waking or changing phase: a growling saw through a sweeping vowel filter over a sub and breath noise. |
| `stone_slam` | impact | A golem's slam or a boulder let loose: a damped low thump under a band-passed crack and a spray of debris grains. |
| `hex_bolt` | feedback | A hex being cast (cultists, the lord's tells and shots): an FM swirl rising an octave. |
| `loop_crypt` | loop | A crypt's music: a breathing triangle drone on D and A, a hollow low-passed saw choir and a four-note bell motif with a far echo, over cold air. |
| `loop_crypt_boss` | loop | The crypt lord's theme: a war drum twice a second with a tom answer and bright sixteenth ticks, a growling saw ostinato (D D F E) and dissonant brass stabs on the downbeats. |
| `loop_high_wind` | loop | The thin air high up (wave 6): a faint band-passed whistle wandering over a high-passed breathy hiss, in slow gusts, with no low buffeting. |
| `loop_moon` | loop | The Moon's near-silence: the suit's 110/220 Hz life-support hum, two helmet breaths a loop (band-passed pink noise), a faint servo whine and scattered radio ticks. |
| `moon_step` | feedback | A boot on the Moon, felt through the suit: a soft low thud, a muffled regolith crunch and the knee servo's short whirr. |
| `loop_town` | loop | A town's bustle: far voices (pink noise through two drifting vowel formants, in syllables), a smith's three strikes twice a loop, a creaking cart wheel and birds. |
| `loop_colony` | loop | The lunar colony's rooms: a 55 Hz recycler motor with a fan beat, a vent's hiss, room rumble and two-tone console chirps. |
| `dawn_theme` | big | Dawn on the world clock: a warm G major brass pad swelling up from the root, a rising pentatonic line of FM bells (G A B D E G) and a held high D bell. |
| `dusk_theme` | big | Dusk: a low E minor brass pad, a falling aeolian bell line (E D B A G E) and a far horn note fading out. |
| `fluid_rush` | impact | A fluid pocket breached (wave 7, F9): a low gulp, then a churning gush of low-passed brown and band-passed pink noise that swells and drains away, with bubbles bursting through it. |
| `steam_hiss` | feedback | Water on lava or molten metal: a band-passed pink sizzle sweeping down into a breathy hiss over a low boil, with crackles as the crust sets. |
| `acid_fizz` | feedback | Acid eating ground: a fast band-passed fizz of tiny bubbles over a low, wobbling gurgle. |
| `rig_splash` | impact | A rig plunging under: a deep body-hit, a falling wall of spray and a trail of big bubbles past the hull. |
| `loop_bubbles` | loop | Under fluid: a muffled low-passed rumble, a 62 Hz pump hum breathing twice a loop and a steady, even stream of bubbles. |
| `radio_echo` | big | The Fathers' expedition (wave 8 story): Dov Halloran on the drop line. A voice that is not words: a buzzing glottal saw through two moving formant band-passes (five vowels), syllable by syllable with breaths and a falling sentence contour, squeezed into a 380-2,700 Hz telephone band, bit-crushed and saturated, with static bursts at the edges and once mid-sentence. Ducks the mix a little. |
| `ghost_voice` | big | One of Dov's ghost recordings: the same formant voice off an old tape (wow and flutter on the pitch, pink hiss, a wider 160-3,800 Hz band), a recorder click at the start and a slow damped tape echo. Ducks the mix a little. |
| `log_canister` | feedback | A cipher canister opening: a small brass clank (three decaying partials), two tumbler clicks and a paper rustle. |
| `anomaly_hum` | big | Stepping into an anomaly room: a slow detuned saw-and-fifth swell whose low-pass opens and closes over 2.4 s, with a glassy FM shimmer on top. Steady: nothing pulses fast. |

Loops are 4 s long (a 2 s loop of frozen noise becomes recognisable over minutes of drilling). Periodic parts use frequencies with whole cycles in the loop (multiples of 0.5 Hz); scattered events (pops, knocks, grit) are spread evenly at a steady density, so nothing marks the restart.

## What the director drives

| Sound | Source | Behaviour |
|-------|--------|-----------|
| Thrust | each seat holding thrust while driving, with fuel | Ignition one-shot; loop gain/rate/low-pass follow a smoothed throttle (rise ~80 ms, fall ~140 ms) and open up while climbing. |
| Drilling | each seat's active drill | Soft or hard loop by the target tile's material (dirt/clay/gas soft; rock, fixtures hard; ore/artifacts hard plus glints every 0.25-0.6 s), rate from drill speed, crunch transients every 1.2-3 chips (clods every 3-6 on soft ground), and a break-through when the tile it was drilling is gone. |
| Landing / wall bump | velocity change of each seat's rig | Light / medium / heavy by fall speed (heavy at the damaging 330 px/s), gain by impact; a bump when most horizontal speed vanishes in a frame without drilling. |
| Alarms | fuel below 25% / hull below 30% | Low warnings stay sparse (every 8 s at the threshold, down to 4 s), since a quarter tank can last minutes; below 10% fuel / 12% hull the critical patches take over within 2 s and repeat every 2 s down to 1 s when empty. The two alarms never overlap. Silent in menus and while disabled. |
| Gas / lava / boulders | hazards near each seat's rig | Loops scaled by proximity (lava within 3.5 tiles; gas and boulders by world distance, loose boulders quieter than falling ones). |
| Wind / rain | each seat's rig out on a biome world's surface | Wind by biome (tundra and mountains gusty, deserts dry, forests calm) that strengthens and brightens with altitude up to ~200 tiles, then thins to silence in space (~700 tiles up); rain over wetlands. Both fade out within a few rows underground. |
| Tomb music | each seat's player in a side-view tomb run (WP-TOMBS T28) | `loop_tomb_run` while exploring, swapped for `loop_tomb_boss` while the run's guardian is awake; silent while picking relics. |
| Bomb fuses | each bomb | Ticks every 0.08-0.5 s as the fuse burns, placed in the world (C4 only once the detonator fires it; flares and burning fires never). |
| Toasts | new shell or seat message | A chime 0.2 s later, cancelled if another sound plays first (so purchases and saves do not double up). |
| Crypt music | each seat whose player is in a top-down crypt run (T29) | `loop_crypt` while exploring; `loop_crypt_boss` instead once the crypt lord is up and until it falls. |
| Environment (wave 6) | each seat | Smoothed shares (about 1.2 s to crossfade) of being in a tomb or crypt run, in vacuum, in the lunar colony and in a town (a surface village or sky town by `village_at`, not an underground village's cave hall, or the home town's span; looked up four times a second). The open-air beds (wind, rain, crickets, sky air, waterfalls) are scaled by `(1 - tomb) (1 - vacuum) (1 - colony)`, wind by a further 40% and crickets 60% less in town, the prowl by `(1 - tomb) (1 - vacuum)`, so beds crossfade by environment instead of stacking. |
| High wind | each seat out in the open | `loop_high_wind` fades in from 150 to 300 tiles up as the surface wind thins, then itself grows quieter and thinner (lower cutoff, a touch higher pitch) up to ~800 tiles, where space is silent. |
| Moon / colony / town | each seat | `loop_moon` in vacuum (the Moon and orbit) outside the colony, `loop_colony` in the colony's rooms, `loop_town` in a town; all silent in a tomb run. |
| Moon footsteps | each seat whose pilot walks (every 0.56 s) or runs (0.4 s) on the ground under low gravity | `moon_step` panned with the seat, muffled like the seat's own sounds in vacuum. |
| Dawn / dusk themes | the world clock turning to dawn or dusk | `dawn_theme` / `dusk_theme` once, centred, if any seat is out in the open (not in a tomb run or in vacuum), 0.6 s after the turn. It yields to the music the moment already has: no theme within 4 s of `dawn_relief` (the dawn after a night with monsters), `night_skip` or `night_fall`, so pieces never stack. Not on the first frame or after a pause. |
| Under fluid | each seat whose actor (rig or pilot) is in a fluid cell at least half full (wave 7, F9) | `loop_bubbles` at 0.7, panned with the seat; silent the moment it surfaces. A breach plays `fluid_rush` where it opened, reactions `steam_hiss` or `acid_fizz` where they happened, and a rig going under `rig_splash` (`splash` coming up), from the player. |

Seat loops are keyed per seat in blocks of 32 sources (`SEAT_LOOP_SLOTS`), one per continuous
sound, so a seat's beds never collide with the next seat's.

Spatialization: interface sounds are centred. A local seat's own sounds pan toward its
split-screen view (±0.6 at the view centres). World sounds (explosions, fuses, other players'
rigs, hazards) take the nearest local rig: full level within 6 tiles, then falling off to
silence at 40 tiles, low-passed as they get farther, and panned by which side of the rig they are
on (kept on that seat's side of the screen). All of it is then scaled by the master volume.

## Reviewing

```bash
cargo run --release --bin fatherload -- --render-audio target/audio-review [--seed N]
```

Writes `<patch>_<variation>.wav` for every variation (loops twice over, so the seam is audible),
`showcase.wav` (a ~41 s tour through the real mixer: menus, a flight, drilling dirt, rock and
ore, a bomb with fuse and blast, damage and alarms, gas and lava, discoveries and rewards) and
`levels.tsv` with each file's duration, peak, loudness vs target, RMS, DC, spectral centroid,
loop seam ratio and flags (`CLIP`, `DC`, `EDGE`, `SEAM`, `LEVEL`). The same checks run as tests
(`audio::tests::every_patch_meets_the_quality_bar`).
