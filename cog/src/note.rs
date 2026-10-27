use anyhow::{Context, Result, bail};
use serde_norway::Value;
use std::fs;
use std::path::PathBuf;

/// Represents a single Markdown note with separated Frontmatter and Body.
#[derive(Debug, Clone)]
pub struct Note {
    /// Path to the file on disk
    pub path: PathBuf,
    /// Note title - derived from filename
    pub title: String,
    /// Dynamic YAML data (preserves unknown fields)
    pub frontmatter: Value,
    /// The Markdown content (everything after the second ---)
    pub body: String,
    /// Keep track of changes that need saving
    pub is_modified: bool,
}

impl Note {
    /// Reads a file, splits frontmatter/body, and parses YAML
    pub fn load(path: PathBuf) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        // Extract title from filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Split manually
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() < 3 {
            bail!(
                "File {:?} does not appear to have valid frontmatter delimiters",
                path
            );
        }

        let frontmatter: Value = serde_norway::from_str(parts[1])
            .with_context(|| format!("Invalid YAML in {:?}", path))?;

        Ok(Note {
            path,
            title,
            frontmatter,
            body: parts[2].to_string(),
            is_modified: false,
        })
    }

    /// MODIFY: Adds a tag if missing. Consumes and returns Self.
    pub fn add_tag(mut self, new_tag: &str) -> Result<Self> {
        // get the tags
        let tags_seq = match self.frontmatter.get_mut("tags") {
            Some(val) => val.as_sequence_mut(),
            None => {
                self.frontmatter["tags"] = Value::Sequence(serde_norway::Sequence::new());
                self.frontmatter["tags"].as_sequence_mut()
            }
        };

        if let Some(tags) = tags_seq {
            let tag_value = Value::String(new_tag.to_string());
            if !tags.contains(&tag_value) {
                tags.push(tag_value);
                self.is_modified = true;
            }
        } else {
            bail!("The 'tags' field in {:?} is not a list.", self.title);
        }

        Ok(self)
    }

    /// SAVE: Writes to disk ONLY if modified.
    pub fn save(self) -> Result<()> {
        if !self.is_modified {
            return Ok(());
        }

        let yaml_output = serde_norway::to_string(&self.frontmatter)?;
        let new_content = format!("---\n{}---{}", yaml_output, self.body);

        fs::write(&self.path, new_content)
            .with_context(|| format!("Failed to write to {:?}", self.path))?;

        tracing::info!("Updated: {}", self.title);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_dummy_note(yaml_str: &str) -> Note {
        let frontmatter: Value = serde_norway::from_str(yaml_str).unwrap();
        Note {
            path: PathBuf::from("test.md"),
            title: "Test Note".to_string(),
            frontmatter,
            body: "\nSome content".to_string(),
            is_modified: false,
        }
    }

    #[test]
    fn test_add_tag_to_empty_frontmatter() {
        let note = create_dummy_note("{}");
        let updated = note.add_tag("rust").unwrap();
        assert!(updated.is_modified);
        assert_eq!(updated.frontmatter["tags"][0].as_str().unwrap(), "rust");
    }

    #[test]
    fn test_idempotency() {
        let note = create_dummy_note("tags: [rust]");
        let updated = note.add_tag("rust").unwrap();
        assert!(!updated.is_modified); // Should NOT be dirty
    }
}
