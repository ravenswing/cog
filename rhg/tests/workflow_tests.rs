use note_tagger::Note;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_add_tag() {
    // Create a temp dir and a real file inside it
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("real_note.md");

    let initial_content = "---\ntitle: My Note\n---\n# Body Content";
    fs::write(&file_path, initial_content).unwrap();

    // Load -> Add Tag -> Save
    let note = Note::load(file_path.clone()).unwrap();
    note.add_tag("integration-test").unwrap().save().unwrap();

    // Check that the tag was added
    let new_note = Note::load(file_path.clone()).unwrap();
    assert!(new_note
        .frontmatter
        .get("tags")
        .unwrap()
        .as_sequence()
        .unwrap()
        .contains(&serde_yaml::Value::String("integration-test".to_string())));

    // Check that the body is preserved
    assert_eq!(new_note.body.trim(), "# Body Content");
}
