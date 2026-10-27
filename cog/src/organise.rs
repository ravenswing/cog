use std::fs;
use std::path::Path;

use crate::error::{IoContext, Result};
use crate::note::Note;

/// Moves notes in the vault root tagged `tag` into a subdirectory.
///
/// The directory is named `dir_name`, or `tag` if not given.
pub fn organise_by_tag(vault: &Path, tag: &str, dir_name: Option<&str>) -> Result<()> {
    // Set directory name from input or just the tag name
    let tag_dir = vault.join(dir_name.unwrap_or(tag));
    tracing::info!(
        "Organising notes tagged {:?} into directory {:?}",
        &tag,
        &tag_dir
    );
    // Create the directory if it doesn't exits
    if !tag_dir.exists() {
        fs::create_dir(&tag_dir).ctx("create directory", &tag_dir)?;
    }

    // Get tagged notes in the *root* vault only
    let notes: Vec<_> = fs::read_dir(vault)
        .ctx("read directory", vault)?
        // from all the files, only get those that are valid files (and unwrap)
        .filter_map(Result::ok)
        // only get those that have the extension .md
        .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        // only get those that have the tag
        .map(|entry| entry.path())
        .filter(|path| Note::load(path).is_ok_and(|note| note.tags.contains(tag)))
        .collect();

    // Move all the tagged notes into the correct folder
    for note in notes {
        let new_path = &tag_dir.join(note.file_name().unwrap());
        tracing::info!("  Moving {:?} -> {:?}", &note, &new_path);
        fs::rename(&note, new_path).ctx("move", &note)?;
    }

    Ok(())
}
