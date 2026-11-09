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
| `patch`     | The 41 designed patches (below), their playback metadata (category, variations, priority, instance cap, reverb send, pitch/gain jitter, ducking) and `render_patch`: design, a 30 Hz sub-cut (12 dB/oct on one-shots, 24 dB/oct on loops; also removes DC), 1 ms / 12 ms edge fades (one-shots) or a 0.3 s equal-power seam fold (loops), loudness normalization. |
| `bank`      | Every patch's variations pre-rendered in parallel. One-shots get 4 variations, loops 1. |
| `mixer`     | 24 voices (+8 slots for stolen voices to fade out over 6 ms). Per voice: gain, pan, playback rate, low-pass (distance muffling), reverb send, all smoothed per 64-frame block and ramped across it. One-shots pick a variation (never the last one) and jitter pitch/gain. Loops are keyed (`LoopKey`): refreshed each frame, faded in/out (60 ms / 100 ms), and fade on their own if not refreshed for 0.35 s. Voice stealing: oldest instance past a patch's cap, else the lowest-priority oldest voice; a request outranked by everything playing is dropped. Big patches duck lower-priority voices (10 ms attack, 0.35 s hold, 0.25 s release). Reverb bus, smoothed volume, peak limiter at 0.95. No allocation in the render path. |
| `cues`      | `SoundCue` -> patch, gameplay event -> cue (exhaustive, so a new event kind must decide how it sounds), and `SoundSource` (interface / player / world point). |
| `director`  | Per frame: places one-shot cues, and drives everything continuous from the snapshot (table below). |
| `analysis`  | Peak, RMS, DC, loudness (loudest 100 ms window after a 100 Hz high-pass), spectral centroid (FFT), loop-seam ratio. |
| `render_tool` | `drillgame --render-audio <dir> [--seed N]`. |

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
| `pickup` | feedback | Two quick FM glints a fifth apart from a pentatonic set. |
| `coins` | feedback | Drawer thunk, 5-7 metallic coin hits (inharmonic partials 1 : 2.76 : 5.4 : 8.93) rising in pitch, then a register "ching" bell. |
| `purchase` | feedback | C-E-G-C brass arpeggio (detuned saws, enveloped low-pass), sparkle bells, 110 ms damped echo. |
| `crate_open` | feedback | Latch clack, lid thump, creaking hinge (band-passed saw with vibrato), two sparkle bells. |
| `salvage_open` | feedback | Metal clank, pneumatic hiss sweeping up, low thunk, a bell. |
| `bomb_arm` | feedback | Metal clunk then an arming beep (1.76 kHz pulse). |
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
| `poi_discovered` | big | Mysterious sting: detuned-saw pad (A3, E4, B4 entering in turn) under a slowly moving low-pass, two FM bells, 300 ms echo. |
| `vault_fanfare` | big | Door boom and rumble, a C-E-G brass arpeggio into a held C major chord, a run of bells. |
| `milestone` | big | D major brass swell under a quick ascending bell arpeggio. |
| `rescue` | big | Tractor beam: FM sweep rising three octaves with 12 Hz tremolo and shimmering noise, resolving to an E/B chord and a bell. |
| `loop_thrust` | loop | Pink-noise band hiss with flutter, a sputtering 2.8 kHz jet band, a little brown-noise rumble and a 55/82.5 Hz hum, saturated. |
| `loop_drill_soft` | loop | Motor (95 Hz saw + 190 Hz pulse), pink-noise scrape modulated by a 16 Hz rotation, clods. |
| `loop_drill_hard` | loop | Motor (120/240 Hz saws), heavily saturated 2.2 kHz grind with 24 Hz chatter and random roughness, faint metal squeal partials. |
| `loop_gas_hiss` | loop | High-passed and band-passed white noise with a slow swell. |
| `loop_lava_sizzle` | loop | Crackling sizzle, random pops, rising bubble blips, low rumble. |
| `loop_boulder_rumble` | loop | Brown-noise rumble rolling at 4 Hz, a band-passed grind, knocks with cracks, grit. |

Loops are 4 s long (a 2 s loop of frozen noise becomes recognisable over minutes of drilling). Periodic parts use frequencies with whole cycles in the loop (multiples of 0.5 Hz); scattered events (pops, knocks, grit) are spread evenly at a steady density, so nothing marks the restart.

## What the director drives

| Sound | Source | Behaviour |
|-------|--------|-----------|
| Thrust | each seat holding thrust while driving, with fuel | Ignition one-shot; loop gain/rate/low-pass follow a smoothed throttle (rise ~80 ms, fall ~140 ms) and open up while climbing. |
| Drilling | each seat's active drill | Soft or hard loop by the target tile's material (dirt/clay/gas soft; rock, fixtures hard; ore/artifacts hard plus glints every 0.25-0.6 s), rate from drill speed, crunch transients every 1.2-3 chips (clods every 3-6 on soft ground), and a break-through when the tile it was drilling is gone. |
| Landing / wall bump | velocity change of each seat's rig | Light / medium / heavy by fall speed (heavy at the damaging 330 px/s), gain by impact; a bump when most horizontal speed vanishes in a frame without drilling. |
| Alarms | fuel below 25% / hull below 30% | Low warnings stay sparse (every 8 s at the threshold, down to 4 s), since a quarter tank can last minutes; below 10% fuel / 12% hull the critical patches take over within 2 s and repeat every 2 s down to 1 s when empty. The two alarms never overlap. Silent in menus and while disabled. |
| Gas / lava / boulders | hazards near each seat's rig | Loops scaled by proximity (lava within 3.5 tiles; gas and boulders by world distance, loose boulders quieter than falling ones). |
| Bomb fuses | each bomb | Ticks every 0.08-0.5 s as the fuse burns, placed in the world. |
| Toasts | new shell or seat message | A chime 0.2 s later, cancelled if another sound plays first (so purchases and saves do not double up). |

Spatialization: interface sounds are centred. A local seat's own sounds pan toward its
split-screen view (±0.6 at the view centres). World sounds (explosions, fuses, other players'
rigs, hazards) take the nearest local rig: full level within 6 tiles, then falling off to
silence at 40 tiles, low-passed as they get farther, and panned by which side of the rig they are
on (kept on that seat's side of the screen). All of it is then scaled by the master volume.

## Reviewing

```bash
cargo run --release --bin drillgame -- --render-audio target/audio-review [--seed N]
```

Writes `<patch>_<variation>.wav` for every variation (loops twice over, so the seam is audible),
`showcase.wav` (a ~41 s tour through the real mixer: menus, a flight, drilling dirt, rock and
ore, a bomb with fuse and blast, damage and alarms, gas and lava, discoveries and rewards) and
`levels.tsv` with each file's duration, peak, loudness vs target, RMS, DC, spectral centroid,
loop seam ratio and flags (`CLIP`, `DC`, `EDGE`, `SEAM`, `LEVEL`). The same checks run as tests
(`audio::tests::every_patch_meets_the_quality_bar`).
