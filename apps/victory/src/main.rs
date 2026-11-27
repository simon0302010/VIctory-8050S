//! `victory`: the command-line front end.
//!
//! Every command reads the user's own files and writes only to stdout or to
//! the user's cache directory (outside the repository).

mod census;
mod inspect;
#[cfg(all(windows, feature = "diagnostics"))]
mod replay;
mod run;
mod shader;
mod translate;
#[cfg(windows)]
mod warm;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "victory",
    version,
    about = "Run an Xbox Series title natively on Windows"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Summarise one PE image: headers, sections, imports, TLS, build id.
    Inspect {
        file: PathBuf,
        /// List every imported function.
        #[arg(long)]
        imports: bool,
        /// List every exported function.
        #[arg(long)]
        exports: bool,
    },
    /// Aggregate the imports of every PE image under the given paths and
    /// show which ones our runtime covers. Defaults to the configured
    /// game and test-title directories.
    Census {
        paths: Vec<PathBuf>,
        /// Do not count host pass-through as covered.
        #[arg(long)]
        strict: bool,
        /// List every missing `dll!function`, most used first.
        #[arg(long)]
        missing: bool,
    },
    /// List the `dll!export` pairs an import library describes.
    LibExports {
        lib: PathBuf,
        /// List every export, not just per-DLL counts.
        #[arg(long)]
        list: bool,
    },
    /// Find shader containers in files or directories (also inside engine
    /// archives, if uncompressed) and report their layout.
    ShaderInfo {
        paths: Vec<PathBuf>,
        /// Print one line per container found and show why paths were skipped.
        #[arg(short, long)]
        verbose: bool,
        /// Also list the decoded native instructions (implies --verbose).
        #[arg(long)]
        disasm: bool,
    },
    /// Run every shader found (containers, SGD5 files) through the
    /// translator and report what it cannot handle yet.
    ///
    /// VICTORY_HOST_WAVE overrides the host wave width, including with --warm.
    /// VICTORY_HOST_MAX_WAVE is a compatibility alias. When both values are
    /// valid, VICTORY_HOST_WAVE takes precedence. Zero means unknown width.
    /// For ordinary translation, VICTORY_HOST_RDNA=1 selects RDNA float settings.
    Translate {
        paths: Vec<PathBuf>,
        /// Check emitted DXIL with dxil.dll's validator (`VICTORY_DXIL`,
        /// else the newest Windows SDK's).
        #[arg(long)]
        validate: bool,
        /// Also create a host D3D12 pipeline for each emitted shader.
        #[arg(long)]
        pipelines: bool,
        /// Print one line per program and outcome.
        #[arg(long)]
        list: bool,
        /// Worker threads (default: every logical core).
        #[arg(long)]
        jobs: Option<usize>,
        /// Warm the run's shader cache: a file of shader hashes (the 16
        /// hex digits a run's log names a shader by, anywhere in a line);
        /// the containers with those hashes are translated into the
        /// cache and nothing else is done. Set `VICTORY_HOST_WAVE` and
        /// `VICTORY_VIEW_LIMIT` to what the run's log says of the host.
        #[arg(long)]
        warm: Option<PathBuf>,
    },
    /// Translate every shader found (as `translate` finds them) into the
    /// shader cache, so that no run has to: a few minutes, once per version
    /// of the translator. Translations an earlier build of the same version
    /// cached are made again, and those that came out different counted.
    /// In diagnostics builds, VICTORY_LOG=debug shows skipped paths.
    Shaders {
        paths: Vec<PathBuf>,
        /// Worker threads (default: every logical core).
        #[arg(long)]
        jobs: Option<usize>,
        /// Also write every shader that does not translate to this file,
        /// one line each: hash, where it was found, why.
        #[arg(long)]
        list: Option<PathBuf>,
    },
    /// Make the host pipelines of every pipeline the title ever created
    /// (runs with `VICTORY_PIPELINE_BOOK=1` write them down), so that the
    /// driver's cache on disk has them before a run needs one: once per change
    /// of the translator or the driver. Goes on where it was stopped.
    #[cfg(windows)]
    Warm {
        /// Processes at once (default: two thirds of the logical cores).
        #[arg(long)]
        jobs: Option<usize>,
        /// At most this many pipelines.
        #[arg(long)]
        limit: Option<usize>,
        /// Those made before too.
        #[arg(long)]
        again: bool,
        /// Only the pipelines earlier runs drew with.
        #[arg(long)]
        used: bool,
        /// (What the processes it starts are given.)
        #[arg(long, hide = true)]
        these: Option<String>,
    },
    /// Prepare, map and bind a title executable; with --execute, run it.
    Run {
        exe: PathBuf,
        /// What unimplemented imports do when called.
        #[arg(long, value_enum, default_value_t = Unimplemented::Trap)]
        unimplemented: Unimplemented,
        /// Disable host pass-through (everything must be implemented by us).
        #[arg(long)]
        strict: bool,
        /// Actually start the title. Without it, stop after binding.
        #[arg(long)]
        execute: bool,
        /// The console model the title sees (device type, memory). Series S
        /// unless told: this fork targets the Radeon 8050S, whose budget
        /// is closer to it.
        #[arg(long, value_enum, default_value_t = Console::SeriesS)]
        console: Console,
        /// Arguments for the title, after `--` (it reads the whole command
        /// line; a title that relaunches itself passes some).
        #[arg(last = true)]
        title_args: Vec<String>,
    },
    /// Run a captured frame's GPU work again (diagnostics builds; see
    /// docs/replay.md).
    #[cfg(all(windows, feature = "diagnostics"))]
    Replay(replay::Replay),
    /// Show the resolved configuration and check it.
    Config,
}

#[derive(Clone, Copy, ValueEnum)]
enum Unimplemented {
    Trap,
    Zero,
}

#[derive(Clone, Copy, ValueEnum)]
enum Console {
    SeriesX,
    SeriesS,
}

/// This fork's defaults under Wine (Linux, Proton): switches the user did not
/// set, set before any part of the runtime reads them. On a Radeon 8050S under
/// GE-Proton the renderer patches cut the frame, port 1 needs the userptr shim
/// (linux/userptr-shim), and while wineserver walks its view list on every map
/// one mapping thread and texture refreshes that do not wait for remaps halve
/// the median frame (linux/bench-scene.sh). `VICTORY_LINUX_DEFAULTS=0` leaves
/// everything as upstream has it.
#[cfg(windows)]
const LINUX_DEFAULTS: [(&str, &str); 9] = [
    // Title buffers of 1 MiB or more get copies in the card's memory, whose
    // changes the card itself finds (the compare, which Wine could not make
    // before): snapshots and CPU compares of what shaders wrote, 24 ms and
    // 531 MB a frame, go to 1.9 ms and 34 MB.
    ("VICTORY_CARD_BUFFERS_MIN_MB", "1"),
    // Renderer port 1 off: its one region (185 MB of the title's graphics
    // memory) is a userptr import, which amdgpu walks page by page at every
    // submission (36% of a core on vkd3d's queue thread) and invalidates
    // whenever the title or write watching changes a mapping or protection
    // in it. Runs with it had stalls of 0.3-1.3 s a frame after the world
    // loaded (4 of 4), runs without none (2 of 2: worst line 120 ms); once
    // streamed in both run 72-76 ms a frame (2026-11-27).
    ("VICTORY_PORT_CONSTANTS", "0"),
    ("VICTORY_PORT_DESCRIPTORS", "1"),
    ("VICTORY_PORT_CBV", "1"),
    ("VICTORY_PORT_GUARD", "1"),
    ("VICTORY_PORT_JOURNAL", "1"),
    // Fills of constant buffers that read what flights in the air may
    // write are done on the card, in the flights' order, instead of the
    // submitting thread waiting for the flights (~5 settles, 30-38 ms of
    // waiting a frame): medians of the streamed-in frames 67-70 ms against
    // 78-80 (2 runs each, 2026-11-27).
    ("VICTORY_CARD_STEPS", "1"),
    ("VICTORY_MAP_THREADS", "1"),
    ("VICTORY_TEXTURE_NO_WAIT", "1"),
];

/// Sets [`LINUX_DEFAULTS`] that are unset, under Wine; returns those set.
#[cfg(windows)]
fn linux_defaults() -> Vec<&'static str> {
    if !victory_hle::wine::on() || std::env::var("VICTORY_LINUX_DEFAULTS").is_ok_and(|v| v.trim() == "0") {
        return Vec::new();
    }
    let mut set = Vec::new();
    for (name, value) in LINUX_DEFAULTS {
        if std::env::var_os(name).is_none() {
            // SAFETY: the first thing `main` does, before any other thread.
            unsafe { std::env::set_var(name, value) };
            set.push(name);
        }
    }
    set
}

fn main() -> Result<()> {
    #[cfg(windows)]
    let defaults = linux_defaults();
    victory_core::logging::init("info");
    #[cfg(windows)]
    if !defaults.is_empty() {
        tracing::info!(switches = ?defaults, "this fork's Linux defaults (VICTORY_LINUX_DEFAULTS=0: none)");
    }
    // The audio recorder `run` starts for `VICTORY_AUDIO_CAPTURE`.
    #[cfg(all(windows, feature = "diagnostics"))]
    if victory_hle::modules::audio::capture::child() {
        return Ok(());
    }
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect {
            file,
            imports,
            exports,
        } => inspect::run(&file, imports, exports),
        Command::Census {
            paths,
            strict,
            missing,
        } => census::run(paths, strict, missing),
        Command::LibExports { lib, list } => inspect::lib_exports(&lib, list),
        Command::ShaderInfo {
            paths,
            verbose,
            disasm,
        } => shader::run(paths, verbose || disasm, disasm),
        Command::Translate {
            paths,
            validate,
            pipelines,
            list,
            jobs,
            warm,
        } => {
            // `VICTORY_VIEW_LIMIT=<null slot>`: translate as on a host whose view heap is capped
            if let Some(n) = std::env::var("VICTORY_VIEW_LIMIT").ok().and_then(|v| v.parse().ok()) {
                victory_shader::emit::set_view_heap_limit(n);
            }
            let wave = translate::host_wave::select(
                std::env::var("VICTORY_HOST_WAVE").ok().as_deref(),
                std::env::var("VICTORY_HOST_MAX_WAVE").ok().as_deref(),
                |message| eprintln!("{message}"),
            );
            if let Some(file) = warm {
                return translate::warm(paths, &file, jobs, wave);
            }
            translate::run(paths, validate, pipelines, list, jobs, wave)
        }
        Command::Shaders { paths, jobs, list } => translate::precompile(paths, jobs, list),
        #[cfg(windows)]
        Command::Warm { jobs, limit, again, used, these } => warm::run(jobs, limit, again, used, these),
        Command::Run {
            exe,
            unimplemented,
            strict,
            execute,
            console,
            title_args,
        } => {
            victory_hle::runtime::set_title_args(title_args);
            run::run(&exe, unimplemented, strict, execute, console)
        }
        #[cfg(all(windows, feature = "diagnostics"))]
        Command::Replay(options) => replay::run(options),
        Command::Config => show_config(),
    }
}

fn show_config() -> Result<()> {
    let cfg = victory_core::Config::load()?;
    let status = |p: &Option<PathBuf>| match p {
        Some(p) if p.exists() => format!("{}", p.display()),
        Some(p) => format!("{}  (does not exist)", p.display()),
        None => "(not set)".to_owned(),
    };
    println!(
        "config file : {}",
        cfg.source
            .as_ref()
            .map_or("(none)".into(), |p| p.display().to_string())
    );
    println!("game_dir    : {}", status(&cfg.game_dir));
    println!("cache_dir   : {}", cfg.cache_dir.display());
    println!("data_dir    : {}", cfg.data_dir.display());
    for t in &cfg.test_titles {
        println!("test_title  : {}", status(&Some(t.clone())));
    }
    for p in cfg
        .game_dir
        .iter()
        .chain(&cfg.test_titles)
        .chain([&cfg.cache_dir, &cfg.data_dir])
    {
        if let Err(e) = victory_core::ensure_outside_repo(p) {
            println!("POLICY VIOLATION: {e}");
        }
    }
    Ok(())
}

/// Paths given on the command line, or the configured game + test titles.
pub(crate) fn default_paths(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    if !paths.is_empty() {
        return Ok(paths);
    }
    let cfg = victory_core::Config::load()?;
    let all: Vec<PathBuf> = cfg.game_dir.into_iter().chain(cfg.test_titles).collect();
    anyhow::ensure!(
        !all.is_empty(),
        "no paths given and none configured (see victory.example.toml)"
    );
    Ok(all)
}
