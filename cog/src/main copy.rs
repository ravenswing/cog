use anyhow::{bail, Result};
use clap::Parser;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

use note_tagger::Note;

#[derive(Parser)]
#[command(name = "note_tagger")]
#[command(version = "1.0")]
struct Cli {
    #[arg(short, long)]
    path: PathBuf,
    #[arg(short, long)]
    tag: String,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    if !args.path.is_dir() {
        bail!("The path provided is not a directory: {:?}", args.path);
    }

    println!("Scanning {:?}...", args.path);

    let paths: Vec<PathBuf> = fs::read_dir(&args.path)?
        .flatten()
        .map(|entry| entry.path())
        .collect();

    paths.par_iter().for_each(|path| {
        if path.extension().map_or(false, |ext| ext == "md") {
            let result = Note::load(path.clone())
                .and_then(|note| note.add_tag(&args.tag))
                .and_then(|note| note.save());

            if let Err(e) = result {
                eprintln!("Skipping {:?}: {}", path.file_name().unwrap_or_default(), e);
            }
        }
    });

    Ok(())
}
