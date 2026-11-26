# Memory in the Rivet apps

`crates/rivet-mem` holds what every app does about memory. It's re-exported as `rivet_kit::mem`:

- `alloc::setup()`, first in `main`: the allocator's settings, and a thread that hands free
  memory back to the system when the app is quiet. Rivet Shell, Files and Terminal call it.
- `alloc::released(bytes)` and `alloc::trim_soon()`: after a big free (a closed pane, pictures a
  cache let go of), the free memory goes back to the system half a second later, off the
  thread that draws.
- `ByteCache`: a cache kept under a size in bytes, least recently used out first. Rivet Media's
  thumbnails use it (128 MB, textures made at the size they're shown).
- `shared_file(path)`: a file's bytes read once per process. `rivet_look::fonts::data` uses it, so
  every egui context (Rivet Shell has one per surface and popup) shares one copy of each font,
  and fontconfig's answers are kept so a new context doesn't run `fc-match` again.
- `Usage` (RSS, PSS, anonymous, swap of any process) and `heap()`: for checks, benchmarks and
  rivetfetch's `rivet-memory` module. `RIVET_MEM_LOG=1` prints both at every trim and every
  quiet check.

## The allocator

Rivet keeps glibc's malloc and sets two things:

- **A 4 MB trim threshold.** Free memory at the top of the heap goes back to the system once
  there's 4 MB of it, instead of only past glibc's moving threshold, which grows up to 32 MB.
- **A 4 MB mmap threshold.** Setting the trim threshold also fixes the mmap threshold, at glibc's
  128 KB unless it's set too. At 128 KB every buffer of a few hundred kilobytes is mapped and
  unmapped each time, which costs CPU. At 4 MB, a folder's list or a decoded picture comes from
  the heap, and a wallpaper or a screen-sized buffer goes back to the system as soon as it's
  freed.

The idle thread checks the free heap every 30 seconds and runs `malloc_trim` when 16 MB or more
is free.

`RIVET_MALLOC=default` skips all of this, to compare. `GLIBC_TUNABLES` and `MALLOC_*_` set in
the environment win over Rivet's settings.

### Why not mimalloc, and why not fewer arenas

On the same binaries (main at 2599554), each app ran a long scripted session under plain glibc,
tuned glibc and mimalloc (`LD_PRELOAD`). PSS was taken at the end of the session:

| App | glibc | glibc, 2 arenas + 4 MB trim | mimalloc |
| --- | ---: | ---: | ---: |
| Rivet Shell | 100 MB | 49 MB | 99 MB |
| Rivet Files | 208 MB | 176 MB | 288 MB |

mimalloc saved nothing in Shell and grew Files, so it isn't worth a C dependency. Capping the
arenas looked free until the CPU benchmark below: at 2 arenas, Files' parallel read of a
100,000-item folder took 22 s of CPU against 8.8 s, because its threads queue for the arena
locks. At 8 arenas it took 10.0 s, and at 16 it took 8.8 s. Once trims run, the arena cap saves
nothing more: Shell ended at 34 MB uncapped and 37 MB at 2 arenas. So Rivet leaves the arena
count alone.

## Before and after

Main at 2599554 against this branch. Release builds, each session in its own headless labwc
(pixman, so egui draws through llvmpipe and textures count as process memory). The scripts are in
`~/.capy/work/apps19/` on the machine they ran on (`soak.py`).

- **Rivet Shell**: two 1920×1080 outputs, 60 rounds of `msg wallpaper next` through a folder of
  3,000 1080p photos, each round opening and closing the launcher, quick settings, calendar,
  notifications, power, sound and weather popups.
- **Rivet Files**: icon view of the same 3,000 photos, 40 rounds of 12 Page Downs, End, Home,
  zoom in twice and out twice, and a trip to another folder and back. Then it leaves for a folder
  with no pictures.
- **Rivet Terminal**: 30 rounds of three windows one after another, each with a 50,000-line
  scrollback and closing when its command ends: `seq 1 300000`, 2 MB of base64, and 400 lines
  of colored text.

| | Before | After |
| --- | ---: | ---: |
| Shell PSS at start | 64 MB | 45 MB |
| Shell PSS after the session | 108 MB | 49 MB |
| Shell PSS idle 2 minutes later | 108 MB | 46 MB |
| Shell idle CPU | 2.2% | 2.2% |
| Files PSS after the session, folder left (two runs each) | 173, 183 MB | 128, 113 MB |
| Files PSS idle 2 minutes later | 205, 183 MB | 137, 113 MB |
| Files idle CPU | 0.23% | 0.23% |
| Terminal PSS after every window closed | 73 MB | 69 MB |
| Terminal anonymous memory after every window closed | 50 MB | 41 MB |
| Terminal idle CPU | 0% | 0% |

These sessions are timed, so how much work gets done in them depends on how busy the machine
is. CPU used during the session varied up to threefold between identical runs, and isn't
comparable. CPU was compared on fixed work instead, median of 5 runs:

| Fixed work | Before | After |
| --- | ---: | ---: |
| `rivet --bench` on a shuffled 100,000-item folder | 8.96 s | 9.00 s |
| `rivet-shell --screenshot` | 0.45 s | 0.42 s |
| `rivet-terminal --screenshot` | 0.34 s | 0.26 s |
| Thumbnails of 3,000 photos, 4 times through, at one zoom | 7.8 s | 3.6–4.5 s |
| The same, zooming to 192 px on every fifth page | 8.2 s | 9.2 s |

The thumbnail rows come from a harness that drives `rivet_media::thumbs::Thumbnailer` from a
headless egui context, frame by frame. At one zoom, every thumbnail stays in the budget, where
the old cache of 800 kept reloading. Changing the zoom constantly costs more, because a bigger
zoom remakes the visible thumbnails at the bigger size.

## Found on the way

- **A Terminal window covered by another doesn't close when its command ends.** The compositor
  sends a covered window no frames, so the window never sees that its program exited. It keeps
  its scrollback until it's shown again. The soak opens windows one at a time because of this.
  With three overlapping windows of 50,000 lines, one stays open, holding about 170 MB.
