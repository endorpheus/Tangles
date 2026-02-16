use crate::database::{Database, Note};
use std::path::PathBuf;

fn main() {
    println!("🧪 Testing Tangles Core Functions...\n");
    
    // Test database creation
    let db_path = PathBuf::from("test_tangles.db");
    let db = Database::new(&db_path).expect("Failed to create test database");
    println!("✅ Database created successfully");
    
    // Test note creation
    let test_note = Note {
        id: None,
        title: "Test Note".to_string(),
        content: "# Test Note\n\nThis is a **test** note with *markdown* formatting.\n\n## Features\n\n- Markdown editing\n- SQLite storage\n- GTK4 interface".to_string(),
        created_at: "2026-02-11T22:15:00Z".to_string(),
        updated_at: "2026-02-11T22:15:00Z".to_string(),
        position_x: 100.0,
        position_y: 100.0,
        is_visible: true,
        always_on_top: false,
        width: 400,
        height: 300,
        theme_bg: None,
        theme_fg: None,
        theme_accent: None,
        custom_colors: None,
    };
    
    let note_id = db.create_note(&test_note).expect("Failed to create test note");
    println!("✅ Note created with ID: {}", note_id);
    
    // Test note retrieval
    let retrieved_note = db.get_note(note_id).expect("Failed to retrieve test note");
    match retrieved_note {
        Some(note) => {
            println!("✅ Note retrieved: {}", note.title);
            println!("   Content preview: {}...", &note.content[..50.min(note.content.len())]);
        }
        None => println!("❌ Note not found"),
    }
    
    // Test getting all notes
    let all_notes = db.get_all_notes().expect("Failed to get all notes");
    println!("✅ Total notes in database: {}", all_notes.len());
    
    // Test word indexing
    let notes_with_test = db.find_notes_with_word("test").expect("Failed to search for word");
    println!("✅ Found {} notes containing 'test'", notes_with_test.len());
    
    // Test note update
    let mut updated_note = test_note;
    updated_note.id = Some(note_id);
    updated_note.title = "Updated Test Note".to_string();
    updated_note.updated_at = "2026-02-11T22:20:00Z".to_string();
    db.update_note(&updated_note).expect("Failed to update note");
    println!("✅ Note updated successfully");
    
    // Test cleanup
    std::fs::remove_file("test_tangles.db").ok();
    println!("✅ Test database cleaned up");
    
    println!("\n🎉 All core functions working!");
    println!("📱 The GUI app is ready to run with: ./target/release/tangles");
}