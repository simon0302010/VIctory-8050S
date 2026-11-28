# VIctory-8050S

A fork of [VIctory](https://github.com/Marvisak/VIctory) (by way of
Belobug/VIctory-Belobug-AMD), a compatibility layer that runs an Xbox Series
title from files the user supplies, **made for one machine: a Framework
Desktop with the Ryzen AI Max 385 and its Radeon 8050S, on Linux through
GE-Proton.**

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher.

## The idea

The console has one memory that CPU and GPU share. Upstream VIctory runs on
PCs with a graphics card of their own, so it keeps copies of the title's
memory on the card, watches every page the title writes and copies changes
over, both ways, every frame. **The 8050S has unified memory too**: the card
can read and write the title's own bytes where the title has them, and every
copy, compare and write watch for that memory is work that need not exist.

So this fork:

1. makes the card use the title's memory in place on Linux (which, under
   Wine, first needed the host import of that memory to work at all),
2. fixes what Wine and Proton make slow for this title, in Wine itself where
   that is where the cost is,
3. measures every step on this machine and keeps what helps
   (`linux/bench-scene.sh`, the debug panel),
4. then turns to the picture: textures, lighting, ray tracing.

It runs the Series S profile unless told (`--console series-x`): its memory
and its speed are closer to this machine's.

## Setup

1. GE-Proton 11-14 and victory-sandbox (builds VIctory
   for Windows on Linux and runs it in an offline bubblewrap sandbox).
2. The patched wineserver (see below), built into a copy of GE-Proton; your
   GE-Proton stays as it is:

       linux/wine/build-wineserver.sh
       victory-sandbox setup --repo <this checkout> --proton ~/.local/share/Steam/compatibilitytools.d/GE-Proton11-14-x86_64-victory

3. `victory-sandbox build` (it also builds and installs the userptr shim) and
   `victory-sandbox run`. For the debug panel and scripted runs, build with
   `--build-args "-p victory --features diagnostics"`.
4. Once per build of the translator: warm the pipelines, so the first minutes
   do not compile them (`victory.exe warm` in the sandbox, after one run with
   `VICTORY_PIPELINE_BOOK=1` to record them; about 9 minutes for 83,000).

## Where it stands on the 8050S (2026-11-27)

Framework Desktop, Ryzen AI Max 385, Radeon 8050S, 32 GB (512 MB set aside
for the GPU), Arch Linux, kernel 7.3, Mesa 26.3 (RADV), GE-Proton 11-14.
The intro's 3D scene, reached unattended by `linux/bench-scene.sh`, Series S
profile, ray tracing off; median of the frame lines with 100 draws a frame or
more (several runs each: the scene streams, runs differ):

| Step | Median ms a frame | |
|---|---|---|
| Upstream switches, debug logging | about 1,100 | 0.3-1 fps; the card busy about 7% of the time |
| Renderer patches 1, 2, 5, 6, 7 on (needs the userptr shim) | 943 | |
| + one mapping thread, texture refreshes that do not wait for remaps | 289-435 | this fork's defaults under Wine |
| + **patched wineserver** | 94-103 | about 10 fps |
| + card compare fixed for Wine, `VICTORY_CARD_BUFFERS_MIN_MB=1` | **76-122** (3 runs) | 8-13 fps; one 6 s stall right after the world loads in one run |
| + renderer port 1 off, thread-locals without std's guard (`fast_local`) | 73-79 (5 runs; means 77-94) | ~13 fps; no stall after loading (with port 1: means 153-356, 0.3-1.3 s frames in 4 of 4 runs) |
| + constant-buffer fills done on the card (`VICTORY_CARD_STEPS=1`) | **67-70** (streamed-in median, 2 runs; 78-80 before) | ~15 fps; the submitting thread no longer waits for flights (30-38 ms a frame before) |

Where a frame goes now (frame lines, once the world has streamed in: 55-80
ms, up to 18 fps): the card is busy about 63% of a frame (~35 ms); of the
~21 ms it idles, ~16 ms is our submitting thread preparing (refreshes of
title memory copies ~14 ms) and ~5 ms the title not submitting; the
submitting thread also waits ~17 ms a frame for the card. Right after the
world loads, most runs have one stall of several seconds (the card and the
copy-back thread waiting; no GPU fault in the kernel log): next to look at.

**What the card itself does in a frame** (`VICTORY_GPU_TIMING=1
VICTORY_D3D12_DRED=1`, diagnostics build; each list goes alone between two
timestamps, 3 frame lines): **25-33 ms**, of it the title's own lists 21-29
ms, our preludes (refreshes on the card) under 1 ms, the compares of copies in
the card's memory about 3 ms. The title's time is spread thin: some 1,500
kinds of commands, none over 0.45 ms a frame; dispatches 8-10 ms, indirect
work 7-11 ms, alias copies 1.6 ms, draws 1.5 ms. So the card's work fits a
30 fps frame already; the ~35 ms "busy" of a normal frame is that plus
gaps between lists and memory shared with our CPU copying. Lowering the
resolution would win little now; the frame is lost on the CPU side and in
waits (each submission and wait for it costs at least ~270 us under Wine).

What was found on the way, measured:

- **Renderer port 1's userptr import stalls the card and the title.** Its
  one region (185 MB of the title's graphics memory, the only userptr in the
  process) is walked page by page by amdgpu at every command submission
  (`hmm_range_fault`: 36% of a core on vkd3d's queue thread, perf), and the
  kernel invalidates it whenever a mapping or protection in it changes
  (streaming, write watching, guards). Every run with it had frames of 0.3-1.3
  s after the world loaded; without it none, at the same steady frame time.
  Off by default now. (This is also the likely reason in-place memory was
  slow before.)
- **std's thread-locals on windows-gnu** run a guard with two locked
  operations on one process-wide counter and an `FlsSetValue` at every access:
  13.7% of the CPU time in our code, the five recording threads fighting over
  one cache line. `crates/victory-gpu/src/fast_local.rs` reads the TEB slot
  directly.
- **Missing heads**: in about 3 runs out of 4, every character was drawn
  without the skin of its head (hair, eyes and teeth floating) for the
  whole run. Cause: the wait for pipelines still being built had one budget
  per frame that the first waiting thread took whole, so another recording
  thread meeting a new pipeline at the same moment dropped its work. At
  world entry that hit a one-time compute program (`7879df3c`) by chance.
  Now threads waiting at the same time share one deadline
  (`crates/victory-gpu/src/pipeline/make.rs` `wait`): no draws dropped while
  pipelines are made, faces in 6 runs out of 6. (The earlier four-program
  skip was a workaround with no real effect and is gone.)
- **Black or green-speckled faces**: in 2 runs of 3 a character's face
  skin was black (or speckled green) for the whole run, already in the
  colour G-buffer. The title bakes each character's head skin once while
  it loads, from textures it picks by index; the pass that uploads those
  textures before a bake refused new ones once the card was at its memory
  line (the 10.8 GB RADV reports), which every run reaches 5-15 s after the
  first bake. The bake then sampled nothing (black) or a slot's old texture
  (green). Now a pass ahead of a bake makes them past the line
  (`VICTORY_BAKE_OVER_LINE_MB`, 1024 per pass; 0-1.6 GB per run in all):
  faces right in 6 runs of 6, frame time unchanged (63-67 ms).
- **Indirect light**: with ray tracing off, rooms and faces had only direct
  light; `VICTORY_AMBIENT=1` (the stand-in estimated from the frame) is a
  Linux default now.
- **Wine's `memcmp`** (ucrtbase) compares eight bytes a step, and Rust's
  slice compares on windows-gnu call it; `apps/victory/src/fastmem.rs`
  defines an AVX2 one for the executable (0.16 of a core in the profile).
- **The card is not the limit**: its own work is 25-33 ms a frame (title
  21-29 ms), see below. No thread is saturated any more either (perf: the
  recording threads ~31% of a core each, the title's main thread 63%): the
  frame is the threads waiting for each other.

- **Wine's view list.** The title shows its page pool through ~63,000 views
  of 64 KiB and maps some 2,000 pages a second. wineserver keeps a process's
  views in a list and walked it on every map and unmap: one map cost 25 us
  with 1,000 views, 210 us with 65,536 (`xmem` test `view_scaling`), 2.3 ms in
  the running title, and wineserver used 92% of a core while the title's
  threads waited for it. With the views indexed by address: 8.4 us at 65,536.
- **The host import of title memory.** libdrm asks the kernel for
  anonymous-only userptrs; every section under Wine is a shared file mapping,
  so RADV refused it and vkd3d-proton, silently, gave the card memory of its
  own. Renderer patch 1 read zeros there. The shim drops that flag; the probe
  (`VICTORY_INPLACE_PROBE=1`) then finds the page pool readable and writable
  in place.
- **Mapping by copying is no way out**: 96% of the pages the title maps are
  shown at another address already, so copies would part.
- **In place for everything** (`VICTORY_PORT_ALL=1` with `VICTORY_INPLACE=2`)
  puts 1.3 GB in place. With `VICTORY_LATE_WAIT=0` (which its docs ask for)
  every submission waits for the card (539 ms median, before the wineserver
  patch). With late wait kept, on the patched server, the card becomes the
  limit: busy 98-100% of the frame and a median of 255-271 ms, against
  94-103 ms with copies. A copy out of in-place memory is barely slower than
  out of a buffer of ours (probe: 0.9 against 0.7 ms for 64 MB), so the
  suspect is not the memory but its upkeep: under Wine write watching and the
  guards change page protections, every change invalidates the card's import
  of those pages (the kernel's userptr notifier), and the driver faults the
  range in again before the next submission. In place pays only for memory
  nothing guards. `VICTORY_PORT_ALL=1` alone: no change (95-108 ms). The
  3,669 buffers in place each had a heap (a kernel userptr object) of their
  own; sharing heaps (`VICTORY_INPLACE_GROUP=1`) made it worse (341-396 ms),
  so their number is not the cause.

## What this fork adds

| Piece | What it does |
|---|---|
| `linux/wine` | The wineserver patch (views indexed by address) and `build-wineserver.sh`, which builds GE-Proton's own server with it (GE's exact Wine tree and patches, so the server protocol matches) into a copy of that GE-Proton. |
| `linux/userptr-shim` | Preloaded into the game by victory-sandbox: drops `AMDGPU_GEM_USERPTR_ANONONLY` from RADV's host imports, so Wine's sections can be the card's memory. `VICTORY_USERPTR_SHIM=0` leaves it out. |
| Linux defaults | Under Wine, switches you did not set: renderer patches 1, 2, 5, 6, 7, `VICTORY_CARD_BUFFERS_MIN_MB=1`, `VICTORY_MAP_THREADS=1`, `VICTORY_TEXTURE_NO_WAIT=1` (the last two only matter without the patched server). `VICTORY_LINUX_DEFAULTS=0`: as upstream. |
| Shaders of ours under Wine | Wine's HLSL compiler embeds no root signature and has no `Load` on `RWByteAddressBuffer`; the card compare (host copies in the card's memory) and the in-place probe were never made there. Both now make their root signatures in code and use structured buffers. |
| Debug panel | `VICTORY_PANEL=1` (or F3): FPS, frame graph, GPU, VRAM, CPU, RAM, and Linux rows: the card idle because the title had not submitted (`TITLE`) or because we worked (`OURS`), copying back; preparing's copies, texture refreshes and their waits for remaps; the title's page maps and unmaps a second and ms each; write-watch ms, guard faults, pipeline waits a frame; MB in place, Wine, and whether the shim is loaded. Rows turn orange where a frame is lost. |
| `linux/bench-scene.sh` | The intro's 3D scene unattended (diagnostics build: Enter pressed by `VICTORY_KEYS`), summarised; extra switches from the environment. |
| `VICTORY_PORT_ALL=1` | Renderer patch 1 for any stretch of the page pool, not only the 32-bit heap (off). |

## Upstream: where it stood on Windows (2026-11-26)


Tested on one machine only: Windows 11, Intel i9-12900K, 64 GB RAM, Radeon
RX 7900 XTX, ray tracing off.

| | |
|---|---|
| City street, Series S profile, renderer patches 1, 2, 5, 6 and 7 on | about 45 ms a frame (about 22 fps, 44.9 ms in run merge-5) |
| Same, patches 1, 2, 5 and 6 | 45.6 ms, p95 47 ms |
| Same, patches 1, 2 and 5 | 47.3 ms (about 21 fps), p95 49 ms |
| Series X profile, patches 1+2 | about 53 ms (about 19 fps) |
| Same scene before the patches | 57-75 ms (13-17 fps) |
| Shaders the title uses that translate | all of them (25,790 of 25,985; the rest are unused geometry shaders) |
| Memory | about 30 GB of private memory, 10.5 GB of VRAM: **64 GB of RAM recommended** for now |

It started at 280 ms a frame in the same scenes three days earlier. What sets
the frame now is the one thread that submits the title's work to the card
(about 40 ms); the card itself needs about 36 ms and now waits about 7 ms a frame with nothing queued.

### Renderer patches (off by default upstream; on here under Wine)

| Switch | What it does |
|---|---|
| `VICTORY_PORT_CONSTANTS=1` | The title's 174 MB constants heap is read by the card where the title writes it: no uploads, no fills |
| `VICTORY_PORT_DESCRIPTORS=1` | The title's descriptor-table stores are patched (byte-checked) to report the slots they write, instead of a scan of ~600,000 slots a frame |
| `VICTORY_PORT_CBV=1` | Translated shaders read constant buffers in place as raw bytes (~22,000 fills a frame to none; -4.7 ms) |
| `VICTORY_PORT_GUARD=1` | The write guards' lock is shared by faults and refreshes, with a claim per 64 KiB block; the submission's planning runs beside it (title threads' waits 29 ms to ~2 ms a frame; -2 ms) |
| `VICTORY_PORT_JOURNAL=1` | The heaps' records are asked for at the title's submit, and the journal's notes are split into 64 parts with locks of their own (the submit thread's waits ~5 ms + 1 ms to ~0.3 ms; -1 to -3 ms) |

### Other switches

| Switch | What it does |
|---|---|
| `--console series-x` | The Series X profile (this fork runs the Series S one unless told: 1080p output, about 5 ms faster, a softer picture) |
| `VICTORY_OUTPUT=1920x1080` | The title renders to that output size itself (Series X profile; about 16 ms less card time) |
| `VICTORY_FRAME_INTERVAL_MS=16.667` | Lets the title go above 30 fps (menus and videos reach 60; the game's clock stays right) |
| `VICTORY_AMBIENT_SETTLE` | How fast light settles after walking indoors (on by default, `250,3`; `0` turns it off) |
| `VICTORY_TEXTURE_STORE=1` | Converted textures kept between runs (about 8 ms a frame for now: off) |

What is known to be wrong:

- Trees, signs and far objects flicker in the distance in some places.
- Lighting is too bright or too dark after walking indoors, until the title has
  run for a while; rooms with little direct light are too dark without `-RayTracing`.
- Textures of what just came into view appear late while the title streams.
- The first run of a new build stutters while the driver makes pipelines
  (`victory warm` makes them ahead).
- Recording a frame in the world makes the title's own watchdog end it.

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, a Direct3D 12 GPU with shader
model 6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC build
tools, and your own dump of the title.

```powershell
git clone https://github.com/avatarmanz/VIctory-7900XTX
cd VIctory-7900XTX
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe <path of your copy's executable>
```

`scripts\play.ps1 -Safe` leaves out the newest, faster ways; try it first when
something is wrong. `-RayTracing` turns the title's ray-traced indirect light on.

For PC FSR 4.1.2, set `[paths].fidelityfx_dir` in `victory.toml` or
`VICTORY_FIDELITYFX_DIR` to the folder containing AMD's signed
`amd_fidelityfx_upscaler_dx12.dll`. Use the DirectX 12 upscaler in
`Kits/FidelityFX/signedbin`. [AMD FSR SDK 2.4.0](https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/tree/v2.4.0/Kits/FidelityFX/signedbin)

`scripts\play.ps1 -Exe <path> -Fsr4K quality` selects 2560×1440 input for
the retail 1.0.2.1 Series X 4K SDR profile. `balanced` selects 2258×1270
and `performance` selects 1920×1080. All three output 3840×2160. With
`victory run` directly, set `VICTORY_FSR_4K` to the preset name.
The log reports the provider at context creation and the actual input and
output sizes at the first successful temporal dispatch. The title profile
and PC FidelityFX bridge were ported from [DexNom/VIctory](https://github.com/DexNom/VIctory/commit/ab34a8b21477c94da2a6cf07249138e8bc84602e).

On a hybrid-graphics PC, the runtime and offline pipeline checks prefer a
high-performance adapter with Shader Model 6.6. The runtime logs its adapter
name, PCI vendor and device IDs, and dedicated VRAM. Set
`VICTORY_D3D12_ADAPTER` to a zero-based DXGI index or part of the adapter's
name to select it explicitly. With the launcher, pass
`-More @{VICTORY_D3D12_ADAPTER='NVIDIA'}`.

Before the first run, ahead of time (both can be stopped and go on later):

```powershell
target\diag\victory.exe shaders <the shader archives>   # every shader translated, minutes
target\diag\victory.exe warm --used                     # pipelines earlier runs drew with, for a new driver or build
```

Commands, debugging switches and upstream's own description:
[docs/README-fxceless.md](docs/README-fxceless.md). Every `VICTORY_...` switch is
described in a comment where it is read.

## What is different here

In short; the reasons and the measurements are in [docs/findings.md](docs/findings.md).

- **List calls are written down, not carried out, on the title's threads**
  (`VICTORY_THIN`): eight threads of ours translate them.
- **Submissions are prepared by a thread of ours and joined**
  (`VICTORY_ASYNC_SUBMIT`, `VICTORY_JOIN_PARTS`), with what can be spread over
  threads spread: refreshes, compares (AVX2), descriptor records, landings.
- **What shaders wrote comes back beside the write guards**, by the dwords that
  changed, on a thread of its own (`VICTORY_RELAY`, `VICTORY_LAND`).
- **Buffer fills that need bytes still on the card are done on the card**
  (`VICTORY_CARD_STEPS`) instead of waiting for it.
- **The title's writes are caught more cheaply**: one view opened at a fault,
  blocks that rest, places in the title's code that look before they write.
- **Textures go up on a copy queue of their own** and a slot shows one only after
  its upload ran, which also ended a device loss that came every 15 minutes or so.
- **Boats float**: textures that shaders write and the title's CPU reads go back
  to title memory (from fxceless and Geckomacabre, on by default here).
- **Frame record and replay** with per-pass times and shader variants compared
  turn by turn ([docs/replay.md](docs/replay.md)).

## Credits

This is a tree of other people's work with more on top. The history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors: the loader, the runtime services, the Direct3D 12 back end and
  the shader recompiler.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**: the fork
  this one started from, and the people whose pull requests it carries.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**: ray tracing fixes,
  cube gradients, sparse textures.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  the ambient light idea, pipeline workers, and work by romainhedouin.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**.
- **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: the guard over
  every view of a page (with Spexivus, joroyt and tightfall), and Joshua Odom's
  reads into title memory.
- **Geckomacabre** and **cxy0072019-sketch**: the texture read-back.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read for how the same
  title behaves under another layer; no code of it is here.
- This fork's changes were made with [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
