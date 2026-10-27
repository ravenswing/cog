use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::cmp::Reverse;
use std::error::Error as StdError;
use std::fs::{self, File};
use std::io::{self, IsTerminal};
use std::iter;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::prelude::*;

use cog::check::check_vault;
use cog::config::Config;
use cog::lock::RunLock;
use cog::organise::{apply_moves, plan_moves};
use cog::query::{Match, TagQuery, count_tags, tagged_notes};
use cog::state;
use cog::sync::{Action, apply, plan_sync};
use cog::tags::Tag;
use cog::title::{apply_titles, plan_titles};
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

    /// Show when the vaults were last synced and what a sync would do now
    Status {
        /// Leave out notes that only changed in the main vault, as
        /// `sync --no-back-update` would
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

    /// List notes whose frontmatter Obsidian can't read, or that aren't UTF-8
    Check {
        #[arg(long, value_enum, default_value_t = VaultName::Main)]
        vault: VaultName,
    },

    /// List every tag in a vault with how many notes have it
    Tags {
        #[arg(long, value_enum, default_value_t = VaultName::Main)]
        vault: VaultName,

        #[arg(long, value_enum, default_value_t = TagOrder::Count)]
        sort: TagOrder,
    },

    /// List notes with the given tags, one path per line
    Find {
        /// Tag to look for, with or without the `#`. Repeat for more
        #[arg(long = "tag", required = true)]
        tags: Vec<Tag>,

        /// Match notes with any of the tags, rather than all of them
        #[arg(long)]
        any: bool,

        /// Leave out notes with this tag. Repeat for more
        #[arg(long)]
        without: Vec<Tag>,

        #[arg(long, value_enum, default_value_t = VaultName::Main)]
        vault: VaultName,
    },

    /// Download data from the Steam Web API
    #[cfg(feature = "steam")]
    #[command(subcommand)]
    Steam(SteamCommand),

    /// Move notes in the vault root with a tag into their own directory
    Organise {
        /// Tag to look for, with or without the `#`
        tag: Tag,

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

#[cfg(feature = "steam")]
#[derive(Subcommand)]
enum SteamCommand {
    /// Save the player summary and owned games to the configured data path
    Fetch,
}

#[derive(Clone, Copy, ValueEnum)]
enum TagOrder {
    /// Most used first
    Count,
    /// Alphabetical
    Name,
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
        Command::Status { no_back_update } => status(&config, !no_back_update),
        Command::Title { vault, dry_run } => title(&config, vault, dry_run),
        Command::Organise {
            tag,
            dir,
            vault,
            dry_run,
        } => organise(&config, &tag, dir.as_deref(), vault, dry_run),
        Command::Check { vault } => check(&config, vault),
        Command::Tags { vault, sort } => tags(&config, vault, sort),
        Command::Find {
            tags,
            any,
            without,
            vault,
        } => {
            let mode = if any { Match::Any } else { Match::All };
            let query = TagQuery {
                tags,
                mode,
                without,
            };
            find(&config, vault, &query)
        }
        #[cfg(feature = "steam")]
        Command::Steam(SteamCommand::Fetch) => steam_fetch(&config),
    }
}

fn title(config: &Config, vault: VaultName, dry_run: bool) -> Result<ExitCode> {
    let _lock = lock_unless(dry_run)?;
    let vault = vault.open(config)?;
    let plan = plan_titles(&vault, &config.sync.skip_dirs)?;

    let report = if dry_run {
        for edit in &plan.done {
            tracing::info!("would add title: {}", edit.path);
        }
        plan
    } else {
        apply_titles(&vault, plan)
    };

    for e in &report.errors {
        tracing::error!("{}", chain(e));
    }
    println!("{}{report}", if dry_run { "Dry run: " } else { "" });
    Ok(exit_code(!report.has_errors()))
}

fn check(config: &Config, vault: VaultName) -> Result<ExitCode> {
    let vault = vault.open(config)?;
    let report = check_vault(&vault, &config.sync.skip_dirs)?;
    for e in &report.errors {
        tracing::warn!("{}", chain(e));
    }
    println!("{report}");
    Ok(exit_code(!report.has_errors()))
}

fn tags(config: &Config, vault: VaultName, sort: TagOrder) -> Result<ExitCode> {
    let vault = vault.open(config)?;
    let report = tagged_notes(&vault, &config.sync.skip_dirs)?;
    for e in &report.errors {
        tracing::warn!("{}", chain(e));
    }

    let mut counts: Vec<_> = count_tags(&report.done).into_iter().collect();
    if let TagOrder::Count = sort {
        // Stable, so tags with the same count stay in name order
        counts.sort_by_key(|&(_, count)| Reverse(count));
    }
    let width = counts.iter().map(|(_, n)| n.to_string().len()).max();
    for (tag, count) in counts {
        println!("{count:>width$}  #{tag}", width = width.unwrap_or(0));
    }
    Ok(exit_code(!report.has_errors()))
}

fn find(config: &Config, vault: VaultName, query: &TagQuery) -> Result<ExitCode> {
    let vault = vault.open(config)?;
    let report = tagged_notes(&vault, &config.sync.skip_dirs)?;
    for e in &report.errors {
        tracing::warn!("{}", chain(e));
    }
    for note in report.done.iter().filter(|note| query.matches(&note.tags)) {
        println!("{}", note.path);
    }
    Ok(exit_code(!report.has_errors()))
}

fn organise(
    config: &Config,
    tag: &Tag,
    dir: Option<&str>,
    vault: VaultName,
    dry_run: bool,
) -> Result<ExitCode> {
    let _lock = lock_unless(dry_run)?;
    let vault = vault.open(config)?;
    let plan = plan_moves(&vault, tag, dir)?;

    let plan = if dry_run {
        for m in &plan.moves {
            tracing::info!("would move: {} -> {}", m.from, m.to);
        }
        plan
    } else {
        apply_moves(&vault, plan)
    };

    for path in &plan.clashes {
        tracing::warn!("not moved, target already exists: {path}");
    }
    for e in &plan.errors {
        tracing::error!("{}", chain(e));
    }
    println!("{}{plan}", if dry_run { "Dry run: " } else { "" });
    Ok(exit_code(plan.errors.is_empty()))
}

#[cfg(feature = "steam")]
fn steam_fetch(config: &Config) -> Result<ExitCode> {
    let steam = config
        .steam
        .as_ref()
        .context("no [steam] section in the config")?;
    let fetched = cog::steam::fetch_player_data(steam)?;
    println!(
        "Saved {} and {} ({} games)",
        fetched.summary.display(),
        fetched.owned_games.display(),
        fetched.game_count
    );
    Ok(ExitCode::SUCCESS)
}

/// Takes the run lock, unless this is a dry run, which changes nothing.
fn lock_unless(dry_run: bool) -> Result<Option<RunLock>> {
    if dry_run {
        return Ok(None);
    }
    Ok(Some(RunLock::acquire(&state::default_path())?))
}

fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn status(config: &Config, back_update: bool) -> Result<ExitCode> {
    let src = Vault::open(&config.vaults.work)?;
    let dest = Vault::open(&config.vaults.main)?;
    let last_sync = state::read_last_sync(&state::default_path())?;
    let plan = plan_sync(&src, &dest, last_sync, &config.sync, back_update)?;
    for e in &plan.errors {
        tracing::warn!("{}", chain(e));
    }

    let last = if last_sync == UNIX_EPOCH {
        "never".to_owned()
    } else {
        // A sync time in the future means the clock went back since
        let elapsed = last_sync.elapsed().unwrap_or_default();
        ago(elapsed)
    };
    println!("Last sync: {last}");
    println!("Next sync: {}", plan.summary());
    for note in plan.notes.iter().filter(|n| n.action == Action::Conflict) {
        println!("Conflict: {}", note.path);
    }
    Ok(exit_code(plan.errors.is_empty()))
}

/// Describes how long ago something happened, to the largest whole unit.
fn ago(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let (count, unit) = match secs {
        0..60 => return "just now".to_owned(),
        60..3600 => (secs / 60, "minute"),
        3600..86400 => (secs / 3600, "hour"),
        _ => (secs / 86400, "day"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

fn sync(config: &Config, dry_run: bool, back_update: bool) -> Result<ExitCode> {
    let src = Vault::open(&config.vaults.work)?;
    let dest = Vault::open(&config.vaults.main)?;
    let state_path = state::default_path();
    let _lock = lock_unless(dry_run)?;
    let last_sync = state::read_last_sync(&state_path)?;

    tracing::info!(
        "Syncing {} -> {}",
        src.root().display(),
        dest.root().display()
    );
    let plan = plan_sync(&src, &dest, last_sync, &config.sync, back_update)?;

    for note in &plan.notes {
        match &note.action {
            Action::Skip(_) => tracing::debug!("{}: {}", note.action, note.path),
            Action::Conflict => tracing::warn!("conflict: {}", note.path),
            action if dry_run => tracing::info!("would {action}: {}", note.path),
            _ => {}
        }
    }

    let plan = if dry_run {
        plan
    } else {
        let plan = apply(plan, &src, &dest);
        match plan.completed_at() {
            Some(time) => state::write_last_sync(&state_path, time)
                .context("synced, but failed to record the sync time")?,
            None => tracing::warn!("sync time not recorded, so failed notes are retried next run"),
        }
        plan
    };

    for e in &plan.errors {
        tracing::error!("{}", chain(e));
    }

    let summary = plan.summary();
    println!("{}{summary}", if dry_run { "Dry run: " } else { "" });
    if summary.conflicts > 0 {
        println!("Conflicts were changed in both vaults and need merging by hand.");
    }

    Ok(exit_code(summary.errors == 0))
}

/// Formats an error with each of its causes.
fn chain(e: &(dyn StdError + 'static)) -> String {
    iter::successors(Some(e), |&e| e.source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

/// Logs to stderr at the requested level, and everything to a log file.
///
/// Logging to stderr keeps stdout for results, so they can be piped.
fn init_logging(cli: &Cli) -> Result<()> {
    let level = match (cli.quiet, cli.verbose) {
        (true, _) => LevelFilter::WARN,
        (false, 0) => LevelFilter::INFO,
        (false, 1) => LevelFilter::DEBUG,
        (false, _) => LevelFilter::TRACE,
    };
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
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
                .with(stderr)
                .with(file_layer)
                .init();
            Ok(())
        }
        Err(e) => {
            tracing_subscriber::registry().with(stderr).init();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_uses_the_largest_whole_unit() {
        let cases = [
            (0, "just now"),
            (59, "just now"),
            (60, "1 minute ago"),
            (150, "2 minutes ago"),
            (3600, "1 hour ago"),
            (86_399, "23 hours ago"),
            (86_400, "1 day ago"),
            (90 * 86_400, "90 days ago"),
        ];
        for (secs, expected) in cases {
            assert_eq!(ago(Duration::from_secs(secs)), expected, "{secs}");
        }
    }
}
