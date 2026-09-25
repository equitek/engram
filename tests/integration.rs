/// Integration tests for engram CLI.
///
/// All tests use `ENGRAM_TEST_EMBED=1` to activate the mock embedding provider,
/// allowing them to run without Ollama or any network access.
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Return the path to the built `engram` binary.
fn engram_bin() -> PathBuf {
    // `cargo test` puts the binary in target/debug
    let mut path = std::env::current_exe()
        .expect("current_exe")
        .parent()
        .expect("parent of test binary")
        .parent()
        .expect("parent of deps dir")
        .to_path_buf();
    path.push("engram");
    path
}

/// Run engram with the given args, using a temporary DB path and mock embeddings.
fn run(db_dir: &TempDir, args: &[&str]) -> std::process::Output {
    let db_path = db_dir.path().join("index.db");
    Command::new(engram_bin())
        .args(args)
        .env("ENGRAM_TEST_EMBED", "1")
        .env("ENGRAM_DB_PATH", db_path.to_str().unwrap())
        .output()
        .expect("failed to execute engram")
}

/// Run engram with --index pointing at a temporary DB, using mock embeddings.
fn run_with_index(db_dir: &TempDir, args: &[&str]) -> std::process::Output {
    let db_path = db_dir.path().join("index.db");
    let mut full_args = vec!["--index", db_path.to_str().unwrap()];
    full_args.extend_from_slice(args);
    Command::new(engram_bin())
        .args(&full_args)
        .env("ENGRAM_TEST_EMBED", "1")
        .output()
        .expect("failed to execute engram")
}

/// Run engram with the embed-fail env var set, simulating a missing Ollama model.
fn run_with_embed_fail(db_dir: &TempDir, args: &[&str]) -> std::process::Output {
    let db_path = db_dir.path().join("index.db");
    Command::new(engram_bin())
        .args(args)
        .env("ENGRAM_TEST_EMBED", "1")
        .env("ENGRAM_TEST_EMBED_FAIL", "1")
        .env("ENGRAM_DB_PATH", db_path.to_str().unwrap())
        .output()
        .expect("failed to execute engram")
}

/// Helper: get stdout as String.
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper: get stderr as String.
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

// ---- Test cases ----

#[test]
fn add_single_file() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("hello.md");
    fs::write(
        &file,
        "Hello, world! This is a test document about Rust programming.",
    )
    .unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files"),
        "unexpected output: {text}"
    );
}

#[test]
fn add_directory_recursive() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();

    // Create nested structure
    let sub = data.path().join("subdir");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        data.path().join("top.md"),
        "Top-level document about testing.",
    )
    .unwrap();
    fs::write(sub.join("nested.txt"), "Nested document about integration.").unwrap();
    // Non-supported extension should be skipped
    fs::write(data.path().join("skip.bin"), b"\x00\x01\x02 not text").unwrap();

    let out = run(
        &db,
        &["add", "--no-progress", "-r", data.path().to_str().unwrap()],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    // Should index 2 supported files (top.md + nested.txt)
    assert!(
        text.contains("Indexed 2 files"),
        "unexpected output: {text}"
    );
}

#[test]
fn readd_unchanged_file_is_noop() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("stable.md");
    fs::write(&file, "This document never changes.").unwrap();

    // First add
    let out1 = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out1.status.success());
    assert!(stdout(&out1).contains("Indexed 1 files"));

    // Second add — same content, should skip
    let out2 = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out2.status.success());
    let text = stdout(&out2);
    assert!(
        text.contains("0 unchanged") || text.contains("1 unchanged"),
        "expected skip, got: {text}"
    );
    // 0 newly indexed
    assert!(
        text.contains("Indexed 0 files"),
        "expected 0 indexed, got: {text}"
    );
}

#[test]
fn search_returns_results_after_indexing() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("rust_guide.md");
    fs::write(
        &file,
        "Rust is a systems programming language focused on safety and performance.",
    )
    .unwrap();

    // Index
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());

    // Search
    let out = run(&db, &["search", "systems programming"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    // Should find at least one result containing the file path
    assert!(
        text.contains("rust_guide.md"),
        "expected result, got: {text}"
    );
}

#[test]
fn search_empty_index_returns_no_results() {
    let db = TempDir::new().unwrap();
    // Create an empty index by adding then searching on a fresh DB
    // We need to init the DB first — add a file then remove it, or just search
    // directly which should fail gracefully.
    let data = TempDir::new().unwrap();
    let file = data.path().join("temp.md");
    fs::write(&file, "temporary").unwrap();

    // Init the DB by adding a file
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());

    // Remove it
    let out = run(&db, &["remove", file.to_str().unwrap()]);
    assert!(out.status.success());

    // Search empty index
    let out = run(&db, &["search", "anything"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("No results"),
        "expected no results, got: {text}"
    );
}

#[test]
fn utf8_multibyte_content() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("unicode.md");
    fs::write(
        &file,
        "日本語のテスト文書。Ñoño café résumé naïve. Emoji: 🦀🔥✨ Деревья и горы.",
    )
    .unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files"),
        "unexpected output: {text}"
    );
}

#[test]
fn large_file_produces_multiple_chunks() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("large.md");

    // Create content larger than CHUNK_SIZE (6000 chars)
    let paragraph = "This is a paragraph about software architecture and design patterns. ";
    let content: String = paragraph.repeat(200); // ~14000 chars
    assert!(content.len() > 6000, "test content too small");
    fs::write(&file, &content).unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    // Should still index as 1 file (just with multiple chunks internally)
    assert!(
        text.contains("Indexed 1 files"),
        "unexpected output: {text}"
    );

    // Verify by searching — should find results
    let out = run(&db, &["search", "software architecture"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("large.md"),
        "expected result from large file"
    );
}

#[test]
fn status_reports_correct_counts() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();

    fs::write(data.path().join("a.md"), "Document alpha about databases.").unwrap();
    fs::write(data.path().join("b.txt"), "Document beta about networking.").unwrap();
    fs::write(data.path().join("c.md"), "Document gamma about compilers.").unwrap();

    // Index all files
    let out = run(
        &db,
        &["add", "--no-progress", data.path().to_str().unwrap()],
    );
    assert!(out.status.success());
    assert!(stdout(&out).contains("Indexed 3 files"));

    // Status should report 3 documents
    let out = run(&db, &["status"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Documents : 3"), "unexpected status: {text}");
}

#[test]
fn deleted_file_keeps_index_entry() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("ephemeral.md");
    fs::write(&file, "This file will be deleted from the filesystem.").unwrap();

    // Index the file
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Indexed 1 files"));

    // Delete the file from the filesystem
    fs::remove_file(&file).unwrap();

    // The index entry should still be present — status still shows 1 doc
    let out = run(&db, &["status"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("Documents : 1"),
        "index entry should persist after fs delete: {text}"
    );

    // Search should still find it
    let out = run(&db, &["search", "deleted"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("ephemeral.md"),
        "expected result from deleted file"
    );
}

#[test]
fn readd_changed_file_reindexes() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("mutable.md");
    fs::write(&file, "Original content about quantum computing.").unwrap();

    // First add
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Indexed 1 files"));

    // Modify the file
    fs::write(
        &file,
        "Updated content about machine learning and neural networks.",
    )
    .unwrap();

    // Re-add — should re-index (not skip)
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files") && text.contains("0 unchanged"),
        "expected re-index, got: {text}"
    );
}

#[test]
fn add_fails_when_model_missing() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("test.md");
    fs::write(&file, "Some content to index.").unwrap();

    let out = run_with_embed_fail(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(
        !out.status.success(),
        "expected non-zero exit, got success. stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("ollama pull nomic-embed-text"),
        "expected actionable error message, got: {err}"
    );
}

#[test]
fn search_fails_when_model_missing() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("indexed.md");
    fs::write(&file, "A document about memory and semantic search.").unwrap();

    // Index successfully with mock embeddings (no fail mode)
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "setup failed: {}", stderr(&out));

    // Search with embed-fail mode — should fail with actionable error
    let out = run_with_embed_fail(&db, &["search", "memory"]);
    assert!(
        !out.status.success(),
        "expected non-zero exit, got success. stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("ollama pull nomic-embed-text"),
        "expected actionable error message, got: {err}"
    );
}

// ---- --index flag tests ----

#[test]
fn index_flag_overrides_db_path() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("custom.md");
    fs::write(&file, "Content for the custom index path test.").unwrap();

    // Use --index flag instead of ENGRAM_DB_PATH env var
    let out = run_with_index(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("Indexed 1 files"));

    // Status should show the custom path
    let out = run_with_index(&db, &["status"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains(db.path().join("index.db").to_str().unwrap()),
        "expected custom index path in status, got: {text}"
    );

    // Search should work
    let out = run_with_index(&db, &["search", "custom"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("custom.md"));
}

#[test]
fn index_flag_creates_separate_index() {
    let db1 = TempDir::new().unwrap();
    let db2 = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();

    let file_a = data.path().join("alpha.md");
    fs::write(&file_a, "Alpha document about databases.").unwrap();
    let file_b = data.path().join("beta.md");
    fs::write(&file_b, "Beta document about networking.").unwrap();

    // Index file_a into db1
    let out = run_with_index(&db1, &["add", "--no-progress", file_a.to_str().unwrap()]);
    assert!(out.status.success());

    // Index file_b into db2
    let out = run_with_index(&db2, &["add", "--no-progress", file_b.to_str().unwrap()]);
    assert!(out.status.success());

    // db1 should have 1 doc, db2 should have 1 doc
    let out1 = run_with_index(&db1, &["status"]);
    assert!(stdout(&out1).contains("Documents : 1"));

    let out2 = run_with_index(&db2, &["status"]);
    assert!(stdout(&out2).contains("Documents : 1"));

    // Search db1 — should find alpha, not beta
    let out = run_with_index(&db1, &["search", "databases"]);
    assert!(stdout(&out).contains("alpha.md"));

    // Search db2 — should find beta, not alpha
    let out = run_with_index(&db2, &["search", "networking"]);
    assert!(stdout(&out).contains("beta.md"));
}

// ---- --json flag tests ----

#[test]
fn search_json_outputs_valid_json() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("rust_guide.md");
    fs::write(
        &file,
        "Rust is a systems programming language focused on safety and performance.",
    )
    .unwrap();

    // Index
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());

    // Search with --json
    let out = run(&db, &["search", "--json", "systems programming"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);

    // Should be valid JSON array
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse JSON: {e}\noutput: {text}"));

    assert!(!parsed.is_empty(), "expected at least one result");

    // Each result should have path, snippet, distance
    let first = &parsed[0];
    assert!(
        first["path"]
            .as_str()
            .unwrap_or("")
            .contains("rust_guide.md"),
        "expected path to contain rust_guide.md, got: {first}"
    );
    assert!(
        first.get("snippet").is_some(),
        "expected snippet field, got: {first}"
    );
    assert!(
        first.get("distance").is_some(),
        "expected distance field, got: {first}"
    );
}

#[test]
fn search_json_empty_results_outputs_empty_array() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("temp.md");
    fs::write(&file, "temporary").unwrap();

    // Init and then empty the index
    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());
    let out = run(&db, &["remove", file.to_str().unwrap()]);
    assert!(out.status.success());

    // Search with --json on empty index
    let out = run(&db, &["search", "--json", "nonexistent"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("failed to parse JSON: {e}\noutput: {text}"));
    assert!(parsed.is_empty(), "expected empty array, got: {parsed:?}");
}

#[test]
fn search_json_works_with_show_path() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("doc.md");
    fs::write(&file, "A document about testing and validation.").unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());

    // --json with --show-path should still include all fields in JSON
    let out = run(&db, &["search", "--json", "--show-path", "testing"]);
    assert!(out.status.success());
    let parsed: Vec<serde_json::Value> = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert!(!parsed.is_empty());
    assert!(parsed[0]["path"].as_str().unwrap().contains("doc.md"));
}

#[test]
fn search_json_distance_is_float() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("doc.md");
    fs::write(&file, "A document about Rust programming safety.").unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success());

    let out = run(&db, &["search", "--json", "Rust"]);
    assert!(out.status.success());
    let parsed: Vec<serde_json::Value> = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert!(!parsed.is_empty());
    // distance should be a number (f32 serialized as f64)
    assert!(
        parsed[0]["distance"].as_f64().is_some(),
        "expected distance to be a number, got: {}",
        parsed[0]
    );
}

#[test]
fn add_json_file_is_default_supported() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("data.json");
    fs::write(&file, r#"{"topic": "Rust programming language"}"#).unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files"),
        "json should be indexed by default: {text}"
    );
}

#[test]
fn add_log_file_is_default_supported() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("app.log");
    fs::write(&file, "2024-01-01 INFO: Rust service started successfully").unwrap();

    let out = run(&db, &["add", "--no-progress", file.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files"),
        "log should be indexed by default: {text}"
    );
}

#[test]
fn add_ext_flag_indexes_custom_extension() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();

    // .csv is not a built-in extension
    let csv_file = data.path().join("data.csv");
    fs::write(&csv_file, "name,description\nRust,A programming language").unwrap();
    let md_file = data.path().join("notes.md");
    fs::write(&md_file, "Notes about Rust programming.").unwrap();

    let out = run(
        &db,
        &[
            "add",
            "--no-progress",
            "-r",
            "--ext",
            "csv",
            data.path().to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 2 files"),
        "should index both .md and .csv: {text}"
    );
}

#[test]
fn add_ext_flag_with_dot_prefix() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let file = data.path().join("data.yaml");
    fs::write(&file, "topic: Rust programming").unwrap();

    let out = run(
        &db,
        &[
            "add",
            "--no-progress",
            "--ext",
            ".yaml",
            file.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Indexed 1 files"),
        ".yaml with dot prefix should work: {text}"
    );
}

#[test]
fn ext_flag_does_not_persist_to_next_run() {
    let db = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let yaml_file = data.path().join("a.yaml");
    fs::write(&yaml_file, "topic: Rust programming").unwrap();

    // First run with --ext yaml indexes it
    let out1 = run(
        &db,
        &[
            "add",
            "--no-progress",
            "--ext",
            "yaml",
            yaml_file.to_str().unwrap(),
        ],
    );
    assert!(out1.status.success());
    assert!(stdout(&out1).contains("Indexed 1 files"));

    // Second run without --ext should not index yaml
    let yaml2 = data.path().join("b.yaml");
    fs::write(&yaml2, "topic: Go programming").unwrap();
    let out2 = run(&db, &["add", "--no-progress", yaml2.to_str().unwrap()]);
    assert!(out2.status.success());
    let text = stdout(&out2);
    assert!(
        text.contains("No supported files found"),
        "yaml should not be supported without --ext: {text}"
    );
}
