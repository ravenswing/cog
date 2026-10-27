use regex::Regex;
use serde_norway::Value;
use std::cmp::min;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the sorted frontmatter and inline tags of a note.
pub fn get_tags(note: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(note).ok()?;
    let mut tags = HashSet::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut yaml_str = String::new();
    let mut body_start = 0;

    // 1. Isolate the YAML Frontmatter
    if lines.first() == Some(&"---")
        && let Some(end_idx) = lines.iter().skip(1).position(|&x| x == "---")
    {
        let end_idx = end_idx + 1; // Adjust index because of the .skip(1)
        body_start = end_idx + 1; // Body starts after the closing "---"

        // Reconstruct the YAML block to pass to serde_norway
        yaml_str = lines[1..end_idx].join("\n");
    }

    // 2. Parse YAML with serde_norway
    if !yaml_str.is_empty() {
        // Parse into a generic YAML Mapping
        if let Ok(Value::Mapping(map)) = serde_norway::from_str::<Value>(&yaml_str) {
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

/// Returns the note's path if it carries `tag`.
pub fn note_has_tag(note: &Path, tag: &str) -> Option<PathBuf> {
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
