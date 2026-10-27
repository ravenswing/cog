use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::error::Error as StdError;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

use cog::config::Config;
use cog::organise::organise_by_tag;
use cog::state;
use cog::sync::{Action, apply, plan_sync};
use cog::title::add_titles;
use cog::vault::Vault;

/// Obsidian vault management.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Config file [default: $COG_CONFIG_PATH or ~/.config/cog/cog.toml]
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Show more output (repeat for even more)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Only show warnings and errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sync the work vault into the main vault
    Sync {
        /// Show what would change without writing anything
        #[arg(long)]
        dry_run: bool,

        /// Don't copy notes that only changed in the main vault back to work
        #[arg(long)]
        no_back_update: bool,
    },

    /// Add an H1 title from the filename to notes without one
    Title {
        #[arg(long, value_enum, default_value_t = VaultName::Work)]
        vault: VaultName,

        /// Show which notes would be titled without changing them
        #[arg(long)]
        dry_run: bool,
    },

    /// Move notes in the vault root with a tag into their own directory
    Organise {
        /// Tag to look for, with or without the `#`
        tag: String,

        /// Directory to move notes into [default: the tag]
        #[arg(long)]
        dir: Option<String>,

        #[arg(long, value_enum, default_value_t = VaultName::Main)]
        vault: VaultName,

        /// Show which notes would move without moving them
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum VaultName {
    Main,
    Work,
}

impl VaultName {
    fn open(self, config: &Config) -> Result<Vault> {
        let path = match self {
            Self::Main => &config.vaults.main,
            Self::Work => &config.vaults.work,
        };
        Ok(Vault::open(path)?)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Err(e) = init_logging(&cli) {
        eprintln!("warning: file logging disabled: {e:#}");
    }

    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            tracing::error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let config_path = cli.config.unwrap_or_else(Config::default_path);
    let config = Config::load(&config_path)?;

    match cli.command {
        Command::Sync {
            dry_run,
            no_back_update,
        } => sync(&config, dry_run, !no_back_update),
        Command::Title { vault, dry_run } => title(&config, vault, dry_run),
        Command::Organise {
            tag,
            dir,
            vault,
            dry_run,
        } => organise(&config, &tag, dir.as_deref(), vault, dry_run),
    }
}

fn title(config: &Config, vault: VaultName, dry_run: bool) -> Result<ExitCode> {
    let vault = vault.open(config)?;
    let report = add_titles(&vault, &config.sync.skip_dirs, dry_run)?;

    let verb = if dry_run {
        "would add title"
    } else {
        "added title"
    };
    for path in &report.added {
        tracing::info!("{verb}: {}", path.display());
    }
    for e in &report.errors {
        tracing::error!("{}", chain(e));
    }
    println!(
        "{}{} titles added, {} errors",
        if dry_run { "Dry run: " } else { "" },
        report.added.len(),
        report.errors.len()
    );
    Ok(exit_code(report.errors.is_empty()))
}

fn organise(
    config: &Config,
    tag: &str,
    dir: Option<&str>,
    vault: VaultName,
    dry_run: bool,
) -> Result<ExitCode> {
    let vault = vault.open(config)?;
    let report = organise_by_tag(&vault, tag, dir, dry_run)?;

    let verb = if dry_run { "would move" } else { "moved" };
    for name in &report.moved {
        tracing::info!("{verb}: {}", name.display());
    }
    for name in &report.clashes {
        tracing::warn!("not moved, target already exists: {}", name.display());
    }
    for e in &report.errors {
        tracing::error!("{}", chain(e));
    }
    println!(
        "{}{} moved, {} clashes, {} errors",
        if dry_run { "Dry run: " } else { "" },
        report.moved.len(),
        report.clashes.len(),
        report.errors.len()
    );
    Ok(exit_code(report.errors.is_empty()))
}

fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn sync(config: &Config, dry_run: bool, back_update: bool) -> Result<ExitCode> {
    let src = Vault::open(&config.vaults.work)?;
    let dest = Vault::open(&config.vaults.main)?;
    let state_path = state::default_path();
    let last_sync = state::read_last_sync(&state_path)?;

    tracing::info!(
        "Syncing {} -> {}",
        src.root().display(),
        dest.root().display()
    );
    let plan = plan_sync(&src, &dest, last_sync, &config.sync, back_update)?;

    for note in &plan.notes {
        match &note.action {
            Action::Skip(_) => tracing::debug!("{}: {}", note.action, note.path.display()),
            Action::Conflict => tracing::warn!("conflict: {}", note.path.display()),
            action if dry_run => tracing::info!("would {action}: {}", note.path.display()),
            _ => {}
        }
    }

    let apply_errors = if dry_run {
        Vec::new()
    } else {
        let errors = apply(&plan, &src, &dest);
        state::write_last_sync(&state_path, plan.started)
            .context("synced, but failed to record the sync time")?;
        errors
    };

    for e in plan.errors.iter().chain(&apply_errors) {
        tracing::error!("{}", chain(e));
    }

    println!(
        "{}{} created, {} updated, {} back-updated, {} conflicts, {} skipped, {} errors",
        if dry_run { "Dry run: " } else { "" },
        plan.count(&Action::Create),
        plan.count(&Action::Update),
        plan.count(&Action::BackUpdate),
        plan.count(&Action::Conflict),
        plan.skipped(),
        plan.errors.len() + apply_errors.len(),
    );
    if plan.count(&Action::Conflict) > 0 {
        println!("Conflicts were changed in both vaults and need merging by hand.");
    }

    Ok(exit_code(plan.errors.is_empty() && apply_errors.is_empty()))
}

/// Formats an error with each of its causes.
fn chain(e: &dyn StdError) -> String {
    let mut out = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        out.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    out
}

/// Logs to stdout at the requested level, and everything to a log file.
fn init_logging(cli: &Cli) -> Result<()> {
    let level = match (cli.quiet, cli.verbose) {
        (true, _) => LevelFilter::WARN,
        (false, 0) => LevelFilter::INFO,
        (false, 1) => LevelFilter::DEBUG,
        (false, _) => LevelFilter::TRACE,
    };
    let stdout = tracing_subscriber::fmt::layer()
        .without_time()
        .with_target(false)
        .with_filter(level);

    let log_path = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .context("no state directory")?
        .join("cog/cog.log");
    let file = log_path
        .parent()
        .map(fs::create_dir_all)
        .transpose()
        .and_then(|_| File::create(&log_path))
        .with_context(|| format!("failed to open {}", log_path.display()));

    match file {
        Ok(file) => {
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .with_filter(LevelFilter::DEBUG);
            tracing_subscriber::registry()
                .with(stdout)
                .with(file_layer)
                .init();
            Ok(())
        }
        Err(e) => {
            tracing_subscriber::registry().with(stdout).init();
            Err(e)
        }
    }
}
