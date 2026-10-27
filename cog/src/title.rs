use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Adds an H1 title based on the filename to a note without one.
pub fn add_inline_title(note: &Path) -> Result<()> {
    let content = fs::read_to_string(note)
        .with_context(|| format!("Failed to read file: {}", note.display()))?;

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
        tracing::info!("{} already has title.", stem);
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
