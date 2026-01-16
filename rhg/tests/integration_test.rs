use note_tagger::Note;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_real_file_workflow() {
    // 1. Setup: Create a temp dir and a real file inside it
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("real_note.md");

    let initial_content = "---\ntitle: My Note\n---\n# Body Content";
    fs::write(&file_path, initial_content).unwrap();

    // 2. Action: Load -> Add Tag -> Save
    let note = Note::load(file_path.clone()).unwrap();
    note.add_tag("integration-test").unwrap().save().unwrap();

    // 3. Verification: Read the file back from disk
    let final_content = fs::read_to_string(&file_path).unwrap();

    // Check that the tag was added
    assert!(final_content.contains("tags"));
    assert!(final_content.contains("integration-test"));

    // Check that the body is preserved
    assert!(final_content.contains("# Body Content"));
}
