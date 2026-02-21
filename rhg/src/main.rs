use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Define a struct that matches the expected structure of your TOML file
#[derive(Deserialize)]
struct Config {
    work_vault: String,
}

pub fn add_inline_title(vault: &Path) -> Result<()> {
    // Find all the notes in the vault
    let note_files = WalkDir::new(vault)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("md"));

    // Iterate through the files
    for entry in note_files {
        let path = entry.path();

        let content = fs::read_to_string(path).with_context(|| {
            format!(
                "{} Failed to read file: {}",
                "Error".bold().red(),
                path.display()
            )
        })?;

        // Use Vec<&str> to avoid allocating new strings for every line
        let lines: Vec<&str> = content.lines().collect();

        // Find all hlines in the file
        let hlines: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|&(_, x)| *x == "---")
            .map(|(i, _)| i)
            .collect();

        // Match hline pattern for properties header or not
        let insert_at = if hlines.len() > 1 && hlines[0] < 3 {
            hlines[1] + 1
        } else {
            0
        };

        // Detect if the file already has an H1 title at that location
        if lines.len() > insert_at && lines[insert_at].starts_with("# ") {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            println!("{} {} already has title.", "NOTE:".yellow(), stem);
            continue;
        }

        // Remove filename annotations
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();

        // Efficiently split using split_once instead of allocating a Vec
        let title_base = match stem.split_once(" - ") {
            Some((_, rest)) => rest,
            None => &stem,
        };

        // Capitalize the first letter safely
        let mut chars = title_base.chars();
        let title = match chars.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        };

        let new_title_line = format!("# {}", title);

        // Build the new file contents efficiently
        let mut new_content = String::with_capacity(content.len() + new_title_line.len() + 2);

        for (i, line) in lines.iter().enumerate() {
            if i == insert_at {
                new_content.push_str(&new_title_line);
                new_content.push('\n');
            }
            new_content.push_str(line);
            new_content.push('\n');
        }

        // Handle edge case where we insert at the end of an empty file
        if lines.is_empty() && insert_at == 0 {
            new_content.push_str(&new_title_line);
            new_content.push('\n');
        }

        // Add context to the write operation as well
        fs::write(path, new_content)
            .with_context(|| format!("Failed to write to file: {}", path.display()))?;
    }

    Ok(())
}

// anyhow::Result<()> - catches errors withouth horrible types!
fn main() -> Result<()> {
    // Resolve the home directory
    let home_dir = home::home_dir().context("Could not find the home directory")?;

    //Construct the path to the TOML config
    let config_path = home_dir.join(".config/rhg/rhg.toml");

    // 3. Read and parse the TOML file using anyhow's Context trait
    let config_contents = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let conf: Config = toml::from_str(&config_contents)
        .context("Failed to parse TOML configuration. Please check your syntax.")?;

    // 4. Resolve the vault path and run the title adder
    let vault = PathBuf::from(conf.work_vault);

    println!("Processing vault: {}", vault.display());
    add_inline_title(&vault)?;
    println!("Finished processing.");

    Ok(())
}
