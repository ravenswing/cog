use anyhow::{Context, Result};
use colored::Colorize;
use regex::Regex;
use serde::Deserialize;
use serde_yaml::Value;
use std::cmp::min;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Define a struct that matches the expected structure of your TOML file
#[derive(Deserialize)]
struct Config {
    vault: String,
    work_vault: String,
    // api_file: String,
    // steam_id: u64,
    // steam_user: String,
    // friend_id: u64,
}

fn add_inline_title(note: &Path) -> Result<()> {
    let content = fs::read_to_string(note).with_context(|| {
        format!(
            "{} Failed to read file: {}",
            "Error".bold().red(),
            note.display()
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
        let stem = note.file_stem().unwrap_or_default().to_string_lossy();
        println!("{} {} already has title.", "NOTE:".yellow(), stem);
        // Exit early ->
        return Ok(());
    }

    // Remove filename annotations
    let stem = note.file_stem().unwrap_or_default().to_string_lossy();

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
    fs::write(note, new_content)
        .with_context(|| format!("Failed to write to file: {}", note.display()))?;

    Ok(())
}

fn get_tags(note: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(note).ok()?;
    let mut tags = HashSet::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut yaml_str = String::new();
    let mut body_start = 0;

    // 1. Isolate the YAML Frontmatter
    if lines.first() == Some(&"---") {
        if let Some(end_idx) = lines.iter().skip(1).position(|&x| x == "---") {
            let end_idx = end_idx + 1; // Adjust index because of the .skip(1)
            body_start = end_idx + 1; // Body starts after the closing "---"

            // Reconstruct the YAML block to pass to serde_yaml
            yaml_str = lines[1..end_idx].join("\n");
        }
    }

    // 2. Parse YAML with serde_yaml
    if !yaml_str.is_empty() {
        // Parse into a generic YAML Mapping
        if let Ok(Value::Mapping(map)) = serde_yaml::from_str::<Value>(&yaml_str) {
            // Obsidian accepts both "tag" and "tags"
            let tag_keys = [
                Value::String("tags".to_string()),
                Value::String("tag".to_string()),
            ];

            for key in tag_keys {
                if let Some(val) = map.get(&key) {
                    match val {
                        // Handles `tags: game` or `tags: game, rpg`
                        Value::String(s) => {
                            for t in s.split(',') {
                                let clean_tag = t.trim().trim_start_matches('#');
                                if !clean_tag.is_empty() {
                                    tags.insert(clean_tag.to_string());
                                }
                            }
                        }
                        // Handles `tags: [game, rpg]` or bulleted lists
                        Value::Sequence(seq) => {
                            for item in seq {
                                if let Value::String(s) = item {
                                    let clean_tag = s.trim().trim_start_matches('#');
                                    if !clean_tag.is_empty() {
                                        tags.insert(clean_tag.to_string());
                                    }
                                }
                            }
                        }
                        _ => {} // Ignore other YAML types (e.g., numbers, nested maps)
                    }
                }
            }
        }
    }

    // 3. Parse Inline Tags from the Body
    let body = lines[body_start..min(lines.len(), body_start + 5)].join("\n");
    if let Ok(re) = Regex::new(r"(?:^|\s)#([a-zA-Z0-9_\-\/]+)") {
        for cap in re.captures_iter(&body) {
            if let Some(tag) = cap.get(1) {
                tags.insert(tag.as_str().to_string());
            }
        }
    }

    // 4. Convert to a sorted Vec
    let mut tag_vec: Vec<String> = tags.into_iter().collect();
    tag_vec.sort();

    Some(tag_vec)
}

fn note_has_tag(note: &Path, tag: &str) -> Option<PathBuf> {
    match get_tags(note) {
        Some(v) => {
            if v.contains(&tag.to_string()) {
                Some(note.to_owned())
            } else {
                None
            }
        }
        None => None,
    }
}

fn organise_by_tag(vault: &Path, tag: &str, dir_name: Option<&str>) -> Result<()> {
    // Set directory name from input or just the tag name
    let tag_dir = vault.join(dir_name.unwrap_or(tag));
    println!(
        "Organising notes tagged {:?} into directory {:?}",
        &tag, &tag_dir
    );
    // Create the directory if it doesn't exits
    if !tag_dir.exists() {
        fs::create_dir(&tag_dir)
            .with_context(|| format!("Unable to make directory at: {}", tag_dir.display()))?;
    }

    // Get tagged notes in the *root* vault only
    let notes: Vec<_> = fs::read_dir(vault)?
        // from all the files, only get those that are valid files (and unwrap)
        .filter_map(Result::ok)
        // only get those that have the extension .md
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        // only get those that have the tag
        .filter_map(|note| note_has_tag(&note.path(), tag))
        .collect();

    // Move all the tagged notes into the correct folder
    for note in notes {
        let new_path = &tag_dir.join(&note.file_name().unwrap());
        println!("  Moving {:?} -> {:?}", &note, &new_path);
        fs::rename(&note, new_path)?;
    }

    Ok(())
}

fn process_vault(vault: &Path) -> Result<()> {
    println!("Processing vault: {}", vault.display());
    // Find all the notes in the vault

    let note_files = WalkDir::new(vault)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("md"));

    for _entry in note_files {
        // add_inline_title(entry.path())?;
    }

    organise_by_tag(vault, "game", Some("games"))?;

    println!("Finished processing.");
    Ok(())
}

// anyhow::Result<()> - catches errors without horrible types!
fn main() -> Result<()> {
    // Resolve the home directory
    let home_dir = home::home_dir().context("Could not find the home directory")?;

    // Construct the path to the TOML config
    let config_path = home_dir.join(".config/rhg.toml");

    // Read and parse the TOML file using anyhow's Context trait
    let config_contents = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let conf: Config = toml::from_str(&config_contents)
        .context("Failed to parse TOML configuration. Please check your syntax.")?;

    // Resolve the vault path and run the title adder
    let vault = PathBuf::from(conf.vault);

    process_vault(&vault)?;

    Ok(())
}
