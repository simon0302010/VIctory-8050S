# CLAUDE.md: VIctory-8050S working notes

Read this first. It is the hand-over between sessions: what this fork is, how
to build, run and measure it, everything found so far (with numbers), the
traps, and the plan. Keep it current: when something is found, measured or
decided, update the relevant section here **and** the README, then commit.

## 1. What this is

- A fork of VIctory (Marvisak/VIctory → Belobug/VIctory-Belobug-AMD), a
  compatibility layer that runs an Xbox Series title (GTA VI,
  `game_scarlett_master.exe`) on PC. Rust workspace, Windows target; we build
  it for `x86_64-pc-windows-gnu` and run it under GE-Proton on Linux.
- **This fork targets exactly one machine:** Framework Desktop, Ryzen AI Max
  385, Radeon 8050S (RDNA 3.5, 32 CU, unified memory), 32 GB RAM (512 MB BIOS
  carve-out; amdgpu GTT 15.7 GB; RADV reports vram_mb=10797), Arch Linux,
  kernel 7.3, Mesa 26.3 (RADV), GE-Proton 11-14 (wine-11.0 staging,
  vkd3d-proton cae8353e). Linux + Proton only; Windows does not matter here.
- **The idea:** the console has one memory shared by CPU and GPU; upstream
  VIctory, made for discrete GPUs, keeps copies of title memory on the card and
  watches/copies/compares every frame. The 8050S has unified memory too, so
  that work should mostly not be needed. Plus: fix what Wine/Proton make slow,
  in Wine itself where that is where the cost is. Measure everything.
- **Goal:** a playable, stable 30 fps (33 ms/frame), Series S profile, then
  the picture (textures, lighting, ray tracing).
- The user (GitHub `simon0302010`) owns the fork. Pronouns unknown: use
  they/them. They write in English; the system locale is German (tool output
  is German).

## 2. Repositories, paths, remotes

| What | Where |
|---|---|
| This repo (local checkout) | `/home/simon/Dokumente/Rust/victory/VIctory-Belobug-AMD`, branch `import-coolfox` |
| Published fork | `https://github.com/simon0302010/VIctory-8050S`, branch `main`. Remote name **`personal`**. Push with `git push personal HEAD:main`. |
| `origin` remote | Belobug/VIctory-Belobug-AMD: **deleted upstream**, do not fetch/push. |
| victory-sandbox (user's own tool, now a git repo) | `/home/simon/Dokumente/Rust/victory-sandbox` (`src/main.rs`); install with `cargo install --path .` → `~/.cargo/bin/victory-sandbox` |
| Sandbox settings | `~/.config/victory-sandbox/settings` (repo, game, proton, build_args) |
| Runtime, saves, caches, logs | `~/.local/share/victory-sandbox/runtime` (`logs/run-<epoch>.log`, `logs/latest.txt`, `cache/`, `state/`); inside the sandbox it is `Z:\victory\` / `/victory` |
| Game | `/run/media/simon/Daten/GTA VI/game_scarlett_master.exe` (read-only in sandbox) |
| Original GE-Proton (do not modify) | `~/.local/share/Steam/compatibilitytools.d/GE-Proton11-14-x86_64` |
| Our Proton copy (patched wineserver; hard-linked copy) | `~/.local/share/Steam/compatibilitytools.d/GE-Proton11-14-x86_64-victory` (Steam name `GE-Proton11-14-x86_64-victory`) |
| Upstream sources for reference/building | `/home/simon/Dokumente/Rust/victory/upstream-src/` : `vkd3d-proton` (at cae8353e), `proton-wine` (ValveSoftware proton_11.0, sparse), `ge-proton` (GE-Proton11-14 tag with `wine`, `wine-staging`, `wineopenxr` submodules; `build-server/` holds our wineserver build) |
| cool_fox (source of the 5 cherry-picked commits) | `https://github.com/DoubleGhosty/cool_fox` |

Linux defaults now include `VICTORY_PORT_CONSTANTS=0` (port 1 off), `VICTORY_CARD_STEPS=1`, `VICTORY_AMBIENT=1` (the four-shader skip is gone: heads fixed, see 7c). Compare runs by the median of the last 9 frame lines (streamed in); the first lines are loading.

Current sandbox settings: `repo` = this checkout, `proton` = the `-victory`
copy, `build_args = -p victory --features diagnostics` (needed for
`VICTORY_KEYS`, debug panel extras, copystat lines).

## 3. Build, run, measure

```sh
victory-sandbox build                 # builds working tree (uncommitted changes included) + userptr shim, installs into runtime
victory-sandbox run                   # plays; log in runtime/logs/run-<epoch>.log
VICTORY_PANEL=1 victory-sandbox run   # debug overlay (F3 toggles)
linux/bench-scene.sh <label> [lines]  # unattended intro 3D scene, prints mean/median ms/frame
VICTORY_X=1 linux/bench-scene.sh ...  # extra switches come from the environment (VICTORY_*, VKD3D_*, RADV_*, MESA_* pass into the sandbox)
```

- Type-check: `cargo check --target x86_64-pc-windows-gnu -p victory-gpu [--features diagnostics] [--tests]` (fast; it really checks).
- Tests needing Windows APIs: build `cargo test --release --target x86_64-pc-windows-gnu -p <crate> --lib --no-run`, copy the `.exe` from `target/x86_64-pc-windows-gnu/release/deps/` into `~/.local/share/victory-sandbox/runtime/`, run `cd ~; victory-sandbox exec proton run 'Z:\victory\<name>.exe' <filter> --ignored --nocapture --test-threads 1`.
  - `xmem` test `view_scaling` (victory-hle): Wine view map cost vs view count.
  - `panel` test `a_box_is_painted` with `VICTORY_PANEL_PPM='Z:\victory\logs\panel.ppm'` dumps the HUD; convert to PNG with a small python zlib script and view it.
- **Sound off for every run we start** (user's request, 2026-11-27): `victory-sandbox run --no-audio`; `bench-scene.sh` does it.
- `bench-scene.sh` presses Enter every 5 s (`VICTORY_KEYS`), waits for N
  frame lines with ≥100 draws, kills the game, summarizes. The 3D scene
  starts ~35 s in. Runs vary a lot (streaming): **always do ≥2-3 runs** and
  compare medians; the mean is wrecked by load stalls.
- Frame-rate line every ~5 s: `INFO victory_gpu::host: X fps, Y ms/frame (P preparing: copies…, textures…; G on the GPU; B copying back…)`. Useful companions (diagnostics build): `the card's clock per frame` (idle split: while our submitting thread worked vs the title had not submitted; our waits for the card), `preparing per frame, the submitting thread`, `copystat: copies per frame` (watches, MB checked/unchanged, guard faults), `copystat: recording per frame` (snapshots, MB compared back), `asking about title memory per frame`, `texture refreshes per frame`, `the relay per frame`, `host copies in the card's memory`, `renderer port 1 (...)`, `title memory in place:`, `dropped N/frame without a host pipeline: <reason>`. Strip ANSI first: `sed 's/\x1b\[[0-9;]*m//g'`.
- `VICTORY_DSTORAGE_LOG='Z:\victory\logs\dstorage.log'`: streaming log; its `M` lines (1/s) = xmem map costs: cols after time: calls, pages, ms, longest, lock-wait, alias calls, alias ms, unmap n, ms, split n, ms, map n, ms, commit n, ms, **pages mapped, of them aliased** (our addition).
- `VICTORY_INPLACE_PROBE=1`: probe at start (+ census at 30/150 s): whether title memory can be the card's memory (private / sections), costs.
- Pipeline warm-up: run once with `VICTORY_PIPELINE_BOOK=1`, then `VICTORY_DATA_DIR='Z:\victory\state' VICTORY_CACHE_DIR='Z:\victory\cache' victory-sandbox exec proton run 'Z:\victory\victory.exe' warm` (~9 min for 83,556; `--used` does not work: it needs the pipeline library, which vkd3d-proton lacks). Done already for the current translator.
- Kernel log: `journalctl -k --since "30 min ago" | grep -i amdgpu`.
- `perf` is installed but cannot attach to wineserver/game (other user namespace, `perf_event_paranoid=2`). System-wide tracing needs the user to run `sudo sysctl kernel.perf_event_paranoid=-1`; ask first.
- CPU per thread without perf: sample `/proc/<pid>/task/*/stat` utime+stime twice.

## 4. Traps (learned the hard way)

- **Never `pkill -f 'victory.exe run'`**: it matches the calling shell's own command line and kills the tool call (exit 144). Use `pkill -f '[v]ictory.exe run'`.
- Same for `pgrep -f 'script.sh'` in a wait loop: it matches the loop's own shell, so the loop never ends (2026-11-28: every wait ran into its timeout). Use `pgrep -f '[s]cript.sh'`, or wait on the PID. `bench-scene.sh` now stops waiting when the game exits (window closed).
- Foreground `sleep` in tool calls is blocked; inside scripts it is fine.
- **`VICTORY_CARD_BUFFERS=0` froze the whole machine** (hard reset, journal lost its last 15 min; 2026-11-28, second run with it). Do not run it again.
- **`victory-sandbox run --rt` (ray tracing) crashed the user's window manager (Hyprland) on 2026-11-27. Do not run with ray tracing unless the user asks.**
- Long jobs (builds, warm-up): run in background and wait for the notification; do not poll.
- Running downloaded scripts (e.g. GE's prep script) was refused once by the auto-mode classifier; the user then explicitly allowed patching/building wineserver. Ask before running other third-party build scripts.
- `victory-sandbox` passes only `VICTORY_*, PROTON_*, VKD3D_*, DXVK_*, RADV_*, MESA_*, AMD_*, WINEDEBUG` into the sandbox (plus LD_PRELOAD for the shim).
- Wine's HLSL compiler (vkd3d-shader via D3DCompile): **embeds no root signature** (so `CreateRootSignature` from a blob fails E_FAIL → make it with `crate::rootsig::make`) and **has no `Load` on `RWByteAddressBuffer`** (E5005 → use `RWStructuredBuffer<uint>` word-indexed, same bytes). Compile our shaders with `crate::host::compile` (logs errors). Any of our own shaders still using the old pattern silently fails under Wine: grep `CreateRootSignature(0` and `RWByteAddressBuffer`.
- vkd3d-proton's `OpenExistingHeapFromFileMapping` maps a view, imports it, and **unmaps it at once**: with the shim the import follows the view → never use it; open sections through a whole view we keep (`OpenExistingHeapFromAddress1`).
- vkd3d-proton host import failure **falls back silently** to its own allocation and reports success ("Failed to import host memory" only with `VKD3D_DEBUG=warn`).
- `xmem::on_wine()` (hle) and `victory_gpu::wine::on()` (gpu; re-exported as `victory_hle::wine`) detect Wine.
- Edition 2024: `std::env::set_var` is unsafe; done once at the top of `main` (Linux defaults).
- The README had external edits between my reads twice; re-read before editing.
- Commit frequently (user's request), small commits, push to `personal HEAD:main`. Commit trailer:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Ck4nRpWH6VV2Mnex5soPyy
  ```
  (use the session line the current system reminder gives, if different).

## 5. What has been done (commits on `main`, newest last)

1. Forked as standalone public repo (no "forked from"); `a-publier` pushed as `main`.
2. Cherry-picked the user's 5 cool_fox commits (`1ce5e6e`…`7426b19`): indirect clamps with Wine's compiler, one back edge per loop header, 256 MiB pipeline worker stack, compose/ambient/depth-plane/keep-awake root signatures in code (`rootsig.rs`).
3. `0a3c1d5` Series S by default (`--console series-x` for the other).
4. In-place probe fixes (`491672e`, `94ca540`, `c65aee0`).
5. `e0593f3` **userptr shim** (`linux/userptr-shim/userptr_shim.c`): LD_PRELOAD wrapper of `drmCommandWriteRead` that drops `AMDGPU_GEM_USERPTR_ANONONLY` (libdrm passes flags 0xe) for `DRM_AMDGPU_GEM_USERPTR` (0x11). Real function looked up via `dlopen("libdrm.so.2", RTLD_NOLOAD)` (RTLD_NEXT fails: the loader opens RADV RTLD_LOCAL; that bug made all GPU allocations fail once). victory-sandbox builds it into `runtime/linux/libvictory_userptr.so` and sets `LD_PRELOAD` (`VICTORY_USERPTR_SHIM=0` disables; `VICTORY_USERPTR_SHIM_LOG=1` logs). Result: page pool sections readable/writable by the card in place (probe VERDICT all true).
6. `b54093f` **wineserver patch** `linux/wine/0001-server-index-process-views-by-address.patch` (process views also in an rb-tree by base; `find_mapped_view`, `find_mapped_addr`, `is_valid_view_addr` use it) + `linux/wine/build-wineserver.sh` (clones GE tag, applies GE's Wine patch section 2 of `protonprep-valve-staging.sh`, our patches, `make_vulkan`, `make_specfiles`, configure without most deps, `make server/wineserver`, installs into `<proton>-victory` via `cp -al`, renames it in `compatibilitytool.vdf`). Built and in use. Also `xmem` `view_scaling` bench, alias counters, `linux/bench-scene.sh`.
7. `86e3e3f` **debug panel Linux rows** (`crates/victory-gpu/src/host/panel.rs`, canvas 540×434): IDLE (TITLE/OURS/BACK), PREP (CPY/TEX/REMAP), MAPS (/s and ms each; counters `victory_gpu::wine::TITLE_MAPS`, fed always by `xmem::counted`), TRACK (write-watch ms, guard faults, pipeline wait), LINUX (MB in place, WINE, SHIM). Orange when bad.
8. `75f184b`, `e2cdf4c` **Linux defaults** (`apps/victory/src/main.rs` `LINUX_DEFAULTS`, set before anything reads env, only under Wine, `VICTORY_LINUX_DEFAULTS=0` disables): `VICTORY_CARD_BUFFERS_MIN_MB=1`, `VICTORY_PORT_CONSTANTS/DESCRIPTORS/CBV/GUARD/JOURNAL=1`, `VICTORY_MAP_THREADS=1`, `VICTORY_TEXTURE_NO_WAIT=1`.
9. `564f728` `VICTORY_PORT_ALL=1` (port 1 for any stretch of a section, not only <4 GiB). Off.
10. `59865fa` `VICTORY_INPLACE_GROUP=1` (in-place buffers share 64 MiB-chunk heaps). Off (did not help).
11. `09db1f1` **card compare works under Wine** (`CARD_COMPARE` in `host.rs`: root signature via `rootsig` incl. new `Param::Srv`, structured buffers). Enables host copies in the card's memory.
12. README rewritten around the fork (idea, setup, measurement table, findings). CLAUDE.md (this file).

victory-sandbox repo: `e1626b1` original, `e26e367` builds/preloads the shim.

## 6. Measurements (intro 3D scene, bench-scene.sh, Series S, RT off)

| State | Median ms/frame |
|---|---|
| Start (upstream switches, debug log) | ~1,100 (GPU busy ~7%) |
| Ports 1,2,5,6,7 (+shim) | 943 |
| + MAP_THREADS=1, TEXTURE_NO_WAIT=1 | 289-435 |
| + patched wineserver | 94-103 (those two switches then irrelevant: 99-103 either way) |
| + card compare + CARD_BUFFERS_MIN_MB=1 | 76-122 (3 runs); 55-80 steady after streaming, up to 18 fps |
| + port 1 off, fast_local | 73-79 (5 runs), means 77-94, no post-load stall |
| + `VICTORY_CARD_STEPS=1` | **67-70** streamed-in median (78-80 without), 0 settles |

Micro: Wine view map with 1k/65k views: 25/210 µs before, 15/8.4 µs after patch; unmap+map at 65k: 506 → 9.9 µs. In game before: ~2,240 maps/s, ~1,100 unmaps/s at ~2.3/1.5 ms each; wineserver at 92% of a core.

Current breakdown of a ~55 ms frame: card busy ~63% (~35 ms, title rendering + our uploads/compare passes); card idle ~21 ms = ~16 ms our submitting thread preparing (refreshes ~14 ms) + ~5 ms title not submitting; submitting thread waits ~17 ms/frame for the card; relay (copy-back) ~20 ms on its own thread; ~2,550 dispatches/frame dropped (ray-tracing structure builder off). Most runs: one 3-6 s stall right after the world loads ("on the GPU" and "copying back" both ~5.7 s); no amdgpu fault in dmesg.

**Step 1 result (GPU timing, 2026-11-27, commit `271685b`):** with
`VICTORY_GPU_TIMING=1 VICTORY_D3D12_DRED=1` each group of lists runs alone
between two timestamp lists (`stamps::bracket`); copystat prints `the card's
time by list (timing run), per frame, between stamps around each`. Intro
scene: card's own work 25-33 ms/frame = title lists 21-29 ms, preludes <1 ms,
card compares ~3 ms. Title time is flat (1,500 command kinds, none >0.45
ms/frame; dispatch 8-10, indirect 7-11, alias copies 1.6, draws 1.5). The
minimum send+signal round trip per list under Wine is ~270 us (timing mode's
wall clock is thus ~75 ms/frame of overhead; its fps mean nothing). ⇒ The
GPU is not the limit; resolution/FSR (step 5) is not worth it now. Go to
step 2/3 (waits, CPU copying).

**Step 2 result (2026-11-27):** the ~16.5 ms/frame the submitting thread
stood waiting were ~3 settles/frame for "title memory read by a fill of
upload memory" (small constant-buffer fills, +0x220/+0xf0/+0x340, inside
ranges a flight's shader declares it writes). `VICTORY_FILL_TITLE=1` removes
them (waits 16.5 → 1-2 ms, settles 3 → 0.4/frame) but frames are **not
faster** (steady 54-86 ms); `=2` in an in-run A/B (`VICTORY_AB=10,VICTORY_FILL_TITLE`):
medians 57-63 vs 55-65, p90 a bit lower. Not made a default (mode 1 can read
a constant before a flight's write lands). Why no gain: **the CPU is
saturated** — the process uses 420-600 ms CPU per 55-85 ms frame (7-8.5 of 8
cores), and the title records on 5 threads that each spend 62-74 ms/frame
inside our list calls (8-10k calls each, ~7 us/call, 234-343 ms/frame
total). Frame time ≈ recording time per thread; the submitting thread is
mostly idle (between submissions 52 ms, 4 ms of it running). ⇒ Step 3 must
cut list-call CPU first. `VICTORY_CARD_STEPS=1/2` (the fills/copies done on
the card) not yet tried.

**Step 3 so far (2026-11-27): where list-call CPU goes.** Diagnostics
builds log `list calls by place` (`with_state` is `track_caller`) and `list
calls by place, parts` (`copystat::mark("label")` inside a call adds the
time since the previous mark to (place, label); marks in `bind`, `tables`,
`readable`, `execute_indirect`). Marks cost ~0.3 us each, which inflates
the parts. Unmarked: `execute_indirect` (list.rs ~1917) 24-30 us × 2.5-5k
calls = 74-122 ms/frame; `dispatch` ~9 us × 3-4k = 30-37 ms; `dispatch_mesh`
~35 us × ~360 = 12-14 ms. ~80% of it is `State::tables` (descriptor
tables): an indirect call makes ~18 views readable (86k/frame from indirect
alone) and most tables are new per draw (~6k new fresh tables/frame), so
the per-list `ready_once` fast path rarely applies. Per view ~1 us spread
over: descriptor lookup (`descriptor::view`, `record_at`/`make_listed`),
`readable` buffer part (deferred refresh + gpu_writes bookkeeping),
`readable` texture part (upload_from, twin_use, ensure_state). No single
hotspot at this granularity → needs a sampling profile (perf) to go
further.

**perf works** (user ran `sysctl kernel.perf_event_paranoid=-1`; resets on
reboot). Scripts: `linux/perf/record-scene.sh <dir>` (bench + 20 s `perf
record -a`; copy `runtime/victory.exe` into `<dir>` first), `python3
linux/perf/ours.py <dir> <dir>/victory.exe [n]` (our functions by samples),
`python3 linux/perf/bythread.py <dir> <exe> <base hex> <comm or tid>...` (one
thread's samples by function; base from `<dir>/maps.txt`). Recipe: start `linux/bench-scene.sh`, once in game `perf record -a
-F 1999 -- sleep 20`, then `perf script -F comm,tid,ip,dso`. victory.exe
is not at its preferred base (0x140000000) under Wine: take the base from
`/proc/<pid>/maps` (the mapping of victory.exe at file offset 0), RVA = ip −
base, names from `x86_64-w64-mingw32-nm -C` (or addr2line with line
tables); its code shows as anon (`perf-<pid>.map`), mixed with the title's
own code (outside victory.exe's range). Profile 1 (before fast_local), 20 s
all CPUs: title code ≈2.6 cores, ours ≈2.9, kernel ≈2. Ours: 13.7% std
`thread_local` `guard::enable` (fixed: `fast_local.rs`, commit `11e135d`),
then safe_copy/merge ~19%, `State::tables` ~5-7%, zlib+bcpack (streaming)
~6%, XMA/Opus audio decoding ~9% (runs even with `--no-audio`), SipHash
~2.5%. After fast_local: ours −11% samples; per-thread: 5 recorders ~31%
of a core each, title main thread 63%, **vkd3d_queue 38% (36% in kernel:
`hmm_vma_walk_pmd`/`vm_normal_page`, amdgpu re-validating userptr BOs on
every CS)** — no thread saturated any more: the frame is latency-bound.

**Step 4 (post-load stall) found (2026-11-27):** renderer port 1's region
is the only userptr BO (185 MB; `VICTORY_USERPTR_SHIM_LOG=1` lines land in
the run log). amdgpu `hmm_range_fault`s every userptr BO at every CS (RADV
puts all BOs in every submission) and its MMU notifier fires on any
mapping/protection change in the range. Port 1 on: 4/4 runs had 0.3-1.3 s
frame lines after loading (means 153-356); off: 4/4 runs none (means 77-94),
steady state equal (72-79). Linux default `VICTORY_PORT_CONSTANTS=0` now
(commit after `6cb8a6a`); bench-scene.sh no longer forces the port
switches. Consequence for in place (step 3 structural idea): any userptr
import pays per-CS page walks proportional to its size; in place needs
driver-allocated memory mapped into the title instead of imports.

Negative results (keep, do not retry blindly):
- Copy-mapping the page pool instead of Wine views: impossible, 96% of mapped pages are aliases (shown elsewhere already; the title maps pages into a pool view and the GPU's `CopyPageMappings` aliases them into tiled heaps).
- `VICTORY_INPLACE=1` alone: 356 MB in place, no fps change (most memory is page pool = view memory, which `inplace` refuses).
- `VICTORY_INPLACE=2` + `VICTORY_LATE_WAIT=0`: every submission waits for the card (539 ms, pre-wineserver-patch).
- `VICTORY_INPLACE=2` + `PORT_ALL` with late wait on (patched server): card 98-100% busy, 255-271 ms. Grouped heaps: 341-396 ms. Hypothesis (unconfirmed): under Wine, write-watch resets/guards are `mprotect`, which fires the userptr MMU notifier; amdgpu invalidates the import (and may wait for the GPU), then refaults the range at the next CS. Needs kernel tracing to confirm (amdgpu tracepoints / perf with paranoid -1).
- `PORT_ALL=1` alone: 95-108 ms (no gain; most refusals are private memory).

## 7. Key code map

- `crates/victory-hle/src/modules/xmem.rs`: title memory; page pool = sections (`CreateFileMappingW`, SEC_COMMIT under Wine) shown through 64 KiB views (`MapViewOfFile3` into placeholders); `change`/`map`/`unmap`/`map_alias`; `counted()` times calls; `ALIASING`, `note_shown`; `arrange` makes <4 GiB reservations one stretch for port 1; tests incl. `view_scaling`, `map_bench`.
- `crates/victory-gpu/src/shadow.rs`: host copies of title buffers (`Shadow`), in-place form `Shadow::placed`, card buffers (`cards()`, `VICTORY_CARD_BUFFERS_MIN_MB`).
- `crates/victory-gpu/src/inplace.rs`: probe, census, `place_for` (levels via `VICTORY_INPLACE`), `place_part`, grouping (`VICTORY_INPLACE_GROUP`), `bytes_in_place`.
- `crates/victory-gpu/src/port.rs`: renderer port 1 (regions = our own view of a section stretch, opened as heap); `all()` = `VICTORY_PORT_ALL`.
- `crates/victory-gpu/src/watch.rs`: write watching + guards (port 6 there too); `copystat.rs`: per-frame counters (`peek`).
- `crates/victory-gpu/src/texture.rs`: texture refreshes, `no_wait()`, `REFRESH_WAITS`.
- `crates/victory-gpu/src/host.rs`: frame line, card clock, preparing, late wait (`late_stage`, relay → stage 3 by default), card compare (`CARD_COMPARE`, `make_card_work`, `card_ready`), `compile()`.
- `crates/victory-gpu/src/rootsig.rs`: root signatures in code (`Param::{Constants, Uav, Srv, SrvTable, UavTable}`).
- `crates/victory-gpu/src/wine.rs`: `on()`, `shim()`, `TITLE_MAPS`, `note_map`.
- `crates/victory-gpu/src/host/panel.rs`: HUD.
- `apps/victory/src/main.rs`: CLI (`run`, `warm`, `--console`), `LINUX_DEFAULTS`.
- `apps/victory/src/fsr_4k.rs`, `crates/victory-hle/src/modules/fidelityfx.rs`: PC FSR 4.1.2 bridge (Series X 4K output; `VICTORY_FSR_4K`, needs AMD's signed `amd_fidelityfx_upscaler_dx12.dll`).
- `crates/victory-gpu/src/pipeline/known.rs` + `apps/victory/src/warm.rs`: pipeline book and warm-up.
- `crates/victory-hle/src/modules/gameinput.rs`: `VICTORY_KEYS` scripted input (diagnostics).

## 7b. State at the end of the 2026-11-27 session

Steady (streamed-in) frame ~65-70 ms (~15 fps), no post-load stall. The card
is not the limit (own work 25-33 ms) and no thread is saturated. The
critical path is now **the submitting thread**, busy ~48 ms a frame in
submissions (diag line `thread N submitted`): refreshes ~25 ms (flushed on the
`victory-wide-*` pool; write watch = Wine `NtGetWriteWatch` → `PAGEMAP_SCAN`
under the process-wide `virtual_mutex`, 1,255 asks/frame over 8.3 GB of which
8.2 GB unchanged, 103 ms CPU over all threads), snapshots ~9 ms, copies in the
card's memory ~6 ms, planning card steps ~3.5 ms, copies/fills ~5 ms. The
card idles ~29 ms a frame behind it. Recording threads ~30 ms/frame each.

Tried this session, kept: GPU timing stamps (`271685b`), list-call places
and marks (`VICTORY_MARKS=1`), fast_local (`11e135d`), port 1 off
(`300718f`), `VICTORY_CARD_STEPS=1` (`fb52a1b`), AVX2 `memcmp`
(`apps/victory/src/fastmem.rs`; Wine's ucrtbase memcmp is 8 B/step; no
measurable fps change, 69/77). Not kept: `VICTORY_FILL_TITLE=1/2` (card
steps does it correctly), `VICTORY_CARD_STEPS=2` (65-66 vs 67-70: maybe 2-3
ms, within noise; its doc names a stale-read risk), port 1 on.

Next ideas, by expected gain:
1. Make the submitting thread's refresh cheaper: fewer/smaller write-watch
   asks (most of 8.3 GB/frame is unchanged; ranges asked per copy span), or
   take `NtGetWriteWatch` out of `virtual_mutex` contention in Wine (we
   already build wineserver; ntdll.so would be the same recipe), or move
   the asking off the critical path (`VICTORY_ASK_EARLY`, `VICTORY_ASK_SPREAD`
   exist: measure).
2. Snapshots (9 ms) and card copies (6 ms) on the submitting thread: can they
   go to the pool / relay?
3. CPU elsewhere: audio decoding (XMA/Opus) ~9% of our CPU, zlib/bcpack
   streaming ~6%, SipHash maps ~2.5% (`RandomState` on hot `HashMap<usize,_>`),
   QPC/rdtscp from diagnostics timing ~1.3% of all samples.
4. Port 1 / in place done right: driver-allocated memory mapped into the
   title instead of userptr imports (any userptr costs a page walk per CS).

## 7c. Picture survey (2026-11-27, late)

Screenshots: `VICTORY_CAPTURE='Z:\victory\logs\<dir>' VICTORY_CAPTURE_WORLD=1
VICTORY_CAPTURE_WORLD_ALSO=200,400,...` (half-size PNGs + `-ops.txt` per
world-relative frame listing every draw and drop) in runtime/logs/shots-*.
Found and fixed:
- Interiors/faces black: the indirect-light stand-in `VICTORY_AMBIENT` was
  off; now a Linux default `1`.
- 6 compute pipelines skipped as "the driver's compiler crashed on it before"
  (in `cache/pipelines/warm-bad.txt`: "hung for 180 s" in an old warm-up);
  they build fine now (`VICTORY_WARM_BAD=0` run). File emptied, backup
  `warm-bad.txt.bak-2026-11-27`.
Still wrong (user confirms; existed before this session):
- ~~Heads missing~~ fixed 2026-11-28 (see below). Skin elsewhere green
  (arms/neck, frame 2836 of shots-warmbad): not rechecked since.
- **Black noise** on textures and floors; **black triangles/planes** blinking.
- Checkerboard/comb artifacts on some surfaces (shadow edges, windows).
Other forks (trustno1-coder/VIctory-NVIDIA, DexNom/VIctory, Marvisak/VIctory
upstream 479 commits past our base) look right on their hardware. A trial
merge of upstream/main: 72 files in conflict (host.rs 63 hunks, draw.rs 46,
descriptor/host.rs 30). Single DexNom fixes (921d8d3b sparse textures) don't
apply either. Integer-target blending (DexNom ddb73326) we already have.

### 7d. The 2026-11-28 sessions: heads fixed, what was learned

**Heads (FIXED, commit `8698b5de`).** In ~3 of 4 runs every character lacked
the skin of its head (hair, eyes, teeth floating) for the whole run; good
runs had all faces. Cause: the pipeline wait budget (`pipeline/make.rs`
`wait`, `VICTORY_PIPELINE_WAIT_MS` 500, shared across threads) was taken
whole by the first waiting thread; other recording threads meeting an
unmade pipeline meanwhile dropped their draw at once. At world entry 63-174
draws were dropped per run; bad runs always included compute
`7879df3c227ee5ad` (one-time, world's first frame). Fix: concurrent waiters
share one wall-clock deadline. Result: 0 drops, faces 6/6 runs; the
4-program `VICTORY_SKIP_PIPELINES` default removed (median 67-70 ms after).
Found by naming shaders in the "DROPPED being made: <hashes>" listing lines
and diffing good vs bad runs.
- **Lesson: a picture bug that is all-or-nothing per run → diff what loading
  dropped / did between good and bad runs. Judge only over 3-6 runs; single
  runs misled the whole day** (the 4-program skip, the "d583dc23
  regression", a TLS theory were all noise).
- **Still to check: facial expressions.** The four face-network programs
  (6a04b371, 164a8fa9, 13c8daa0, da2fdff4; 20 programs in the face list of
  frame-1152-ops) run again now. 164a8fa9/13c8daa0 are exact on RADV
  (test `face_network_elementwise_programs_compute_on_halves`); the others
  untested; nobody has looked at a talking close-up yet (diner waitress
  ~world+300). Next: capture a frame sequence of a dialogue and compare
  mouth shapes, or ask the user to watch.

**Other things established that day (keep):**
- Translator cache: keyed by `victory_shader::TRANSLATOR_VERSION` (now 2);
  **bump it whenever the translator changes**, or old translations are
  served silently. `VICTORY_SHADER_KEY=source` = a separate cache set per
  build (used for tests). Warm-up redone for version 2 (83,556, 0 bad).
- d583dc23's host half removed; its table d16 lookup is `VICTORY_D16_TABLES=1`
  (off, in the cache key).
- `fast_local`: TLS index ≥64 read via `TlsGetValue` (harmless fix).
- The title's GPU timestamps say 65-130 ms/frame (written at submission
  end); `VICTORY_TITLE_GPU_MS=<ms>` fakes them. No effect on heads.
- Head-skin bake at load (d126bcae/fcee6129/c0a7fe77 into 2048² targets →
  95988cc1 BC-encodes into R32G32B32A32_UINT 32x32/1x1 scratch → copies into
  128x128 BC3): works, identical in good and bad runs.
- Ruled out as picture causes: late wait off, card copies off, card steps,
  barrier after every dispatch, bake upload budget, sound, dstorage (no
  request failures in any run), `VICTORY_INPLACE=1`.
- DexNom/VIctory (remote `dexnom`, fetched; merge base ffe310eb 11-24, 340
  commits theirs): their dstorage page-0 fix (d09f3aa8) does not apply (our
  retry path differs; no failures logged). Their GPU commits not in our tree
  are listed by `git log ffe310eb..dexnom/main -- crates/victory-gpu`;
  candidates for the remaining picture bugs: 921d8d3b (quad sampling, sparse
  textures), bc3cd4b0, cd94d353/940d32f6 (shader-written pages), 4051149c,
  0f637570/27c3dc9d (page table for raw addresses), 77053a0c (shader
  accuracy), 13bbd6fe. Most will not cherry-pick cleanly.

**Diagnostic tools added (diagnostics builds):**
- `VICTORY_CAPTURE='Z:\victory\logs\<dir>'` + `VICTORY_CAPTURE_WORLD=1`
  (+ `VICTORY_CAPTURE_WORLD_ALSO=30,300`) or `VICTORY_CAPTURE_FRAMES=a,b`:
  screenshots + `frame-N-ops.txt` (every op, drops with shader hashes;
  ops span several lines: join with awk on `^[0-9]+ list `).
- `VICTORY_DUMP_TARGETS=1`: every render target/copy destination of listed
  frames as PNG (G-buffer: 1280x720 f27 colour, f28 normals/material);
  unsupported formats log their non-zero byte count.
- `VICTORY_DUMP_BAKES=1`: lists the first 8 frames with a bake draw.
- `VICTORY_BUFDUMP=<hashes>`: buffers those programs bind, read back.
- `VICTORY_DISPATCH_BARRIER=1`, `VICTORY_SKIP_PIPELINES=<hashes>`.
- Logs: "the title's GPU time as its timestamp pairs say", "a texture the
  host writes and one filled from title memory overlap", "a block texture of
  the bake".
- Scripts `linux/check/faces.sh <label>` (screens at world+30/+300 into a
  contact sheet) and `linux/check/targets-at-world.sh <label>` (G-buffer
  dump at world+30); sheets go to `$CHECK_OUT` (default /tmp). Both use
  `VICTORY_SHADER_KEY=source`.

Still open in the picture: odd lighting, black noise on textures/floors,
black triangles/planes blinking, checkerboard/comb artifacts.

## 8. Plan (agreed direction: playable 30 fps first, then picture)

Next steps in order. **When resuming: the heads are fixed (7c); next the other picture bugs (lighting, black noise, blinking black geometry, checkerboard artifacts); frame rate after.**

1. ~~**Measure the GPU's ~35 ms.**~~ Done, see the step 1 result in section 6: 25-33 ms, title 21-29. Use the GPU timing mode (`VICTORY_GPU_TIMING`; note it forces late wait off, see `late_stage`) or a capture to split title rendering vs our own passes (uploads of card copies ~45 MB/frame, compare passes, texture uploads, ambient/compose). The 8050S has ~2.5× the Series S's compute at similar bandwidth, so the title's own rendering should fit well under 33 ms; if not, look at translated shader cost. This decides whether resolution scale / FSR is worth anything.
2. ~~**Overlap CPU and GPU.**~~ Measured, see step 2 result: the waits are removable but the CPU is saturated; recording is the critical path. Today preparing (~16 ms idle card), submitting and waiting (~17 ms) are serialized. Goal: frame ≈ max(GPU, CPU) ≈ 35-40 ms. Look at the late-wait/relay machinery (`host.rs` `late_stage`, flights, settles, "waits for every earlier shader write", "submissions sent ahead") for what forces the 17 ms of waits.
3. **Cut CPU copying** (in progress, see 7b): refreshes ~14 ms, relay ~20 ms, the 3.6 GB/frame "watches checked" (mostly unchanged). Structural fix: make in-place work under Wine without the GPU slowdown: confirm the mprotect/userptr-notifier hypothesis (ask the user for `sudo sysctl kernel.perf_event_paranoid=-1`, then `perf record -a -e 'amdgpu:*'` / count `mprotect` + `amdgpu_mn_*`), then keep in-place memory out of write watching/guards entirely (it needs neither), or patch Wine's write-watch to not re-protect such ranges.
4. ~~**The post-load stall**~~ Done: renderer port 1's userptr (see step 4 result). find what the card and relay wait 5.7 s for (log around it, `VKD3D_DEBUG=warn`, RADV_DEBUG hang detection off by default; check pipelines made at that moment, huge copies: one run showed 1.8 GB of copies in the first world frame).
5. **Then** resolution/upscaling if the GPU is still the limit: the Series S profile already upscales with the title's own console FidelityFX (`amd_fidelityfx_loader_xbox`, `crates/victory-loader/src/resolution.rs`, `VICTORY_FFX_SCALE`); lowering its internal scale is the cheap knob. The fork's PC FSR 4.1.2 bridge targets Series X 4K output (more GPU work) and FSR 4 needs RDNA 4 (8050S is RDNA 3.5: likely FSR 3.1 fallback). FSR 3 frame generation only as a last touch (latency).
6. **Picture:** ray tracing off drops ~2,550 dispatches/frame (lighting), textures waited up to 33 s for memory in early runs (should be better now), `a pipeline creation method that is not implemented` warnings (~10 kinds), `placed unordered texture view not mirrored` (writes lost), `indirect kind 2: prepare failed` (10/frame), 2 compute pipelines refused, 6 pipelines crash the driver compiler (`warm-bad.txt`). Try `victory-sandbox run --rt` once frame time allows.

Other notes:
- RAM is tight: 31 GB, the game + copies used ~20 GB and swapped 6 GB in the first runs. Fewer copies (in place) also fixes this.
- `VICTORY_PORT_CBV` etc. read env in other crates (victory-shader); that is why defaults are set as env vars in `main`.
- Upstream README sections below the fork's part describe Windows results; keep them, mark ours clearly.
- When a measured change helps: make it a Linux default (with a comment saying the measurement), add a row to the README table, commit, push.
