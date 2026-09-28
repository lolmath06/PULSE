use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::writer::ConfigWriter;
use super::*;
use crate::history::testing::TempDir;

fn path(dir: &TempDir) -> PathBuf {
    config_path(dir.path())
}

#[test]
fn a_missing_file_starts_fresh_with_a_versioned_document() {
    let dir = TempDir::new("ui-fresh");
    let store = UiConfigStore::open(&path(&dir));

    assert_eq!(store.load_outcome(), &LoadOutcome::Fresh);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.document, json!({ "version": 1 }));
    assert_eq!(snapshot.read_only, None);
    assert!(
        !path(&dir).exists(),
        "nothing is written until something changes"
    );
}

#[test]
fn sections_round_trip_through_the_file() {
    let dir = TempDir::new("ui-roundtrip");
    let store = UiConfigStore::open(&path(&dir));
    let dashboards =
        json!({ "activeId": "default", "items": [{ "id": "default", "widgets": [] }] });

    let revision = store
        .set_section("dashboards", dashboards.clone())
        .expect("set");
    assert_eq!(revision, 2);
    assert!(store.flush().expect("flush"));
    assert!(
        !store.flush().expect("second flush"),
        "nothing left to write"
    );

    let reopened = UiConfigStore::open(&path(&dir));
    assert_eq!(reopened.load_outcome(), &LoadOutcome::Loaded);
    assert_eq!(reopened.section("dashboards"), Some(dashboards));
}

#[test]
fn unknown_sections_and_non_objects_are_refused() {
    let dir = TempDir::new("ui-refuse");
    let store = UiConfigStore::open(&path(&dir));

    assert_eq!(
        store.set_section("secrets", json!({})),
        Err(UiConfigError::UnknownSection("secrets".into()))
    );
    assert_eq!(
        store.set_section("dashboards", json!([1, 2])),
        Err(UiConfigError::NotAnObject("dashboards".into()))
    );
    assert_eq!(store.revision(), 1);
}

#[test]
fn an_oversized_document_is_refused_and_the_previous_value_kept() {
    let dir = TempDir::new("ui-large");
    let store = UiConfigStore::open(&path(&dir));
    store
        .set_section("settings", json!({ "a": 1 }))
        .expect("small");

    let huge = json!({ "blob": "x".repeat(MAX_DOCUMENT_BYTES) });
    assert!(matches!(
        store.set_section("settings", huge),
        Err(UiConfigError::TooLarge { .. })
    ));
    assert_eq!(store.section("settings"), Some(json!({ "a": 1 })));
}

#[test]
fn a_corrupt_file_is_quarantined_and_the_backup_used() {
    let dir = TempDir::new("ui-corrupt");
    {
        let store = UiConfigStore::open(&path(&dir));
        store
            .set_section("settings", json!({ "closeBehavior": "quit" }))
            .expect("set");
        store.flush().expect("first write");
        store
            .set_section("settings", json!({ "closeBehavior": "keep-running" }))
            .expect("set");
        store.flush().expect("second write, first becomes .bak");
    }
    std::fs::write(path(&dir), b"{ this is not json").expect("corrupt");

    let store = UiConfigStore::open(&path(&dir));

    match store.load_outcome() {
        LoadOutcome::RecoveredFromBackup { quarantined, .. } => {
            let quarantined = quarantined.clone().expect("moved aside");
            assert_eq!(
                std::fs::read(&quarantined).expect("kept"),
                b"{ this is not json",
                "the corrupt file is kept for diagnosis, never deleted"
            );
        }
        other => panic!("expected recovery, got {other:?}"),
    }
    assert_eq!(
        store.section("settings"),
        Some(json!({ "closeBehavior": "quit" }))
    );
}

#[test]
fn corrupt_without_a_backup_falls_back_to_defaults() {
    let dir = TempDir::new("ui-reset");
    std::fs::write(path(&dir), b"[1,2,3]").expect("write");

    let store = UiConfigStore::open(&path(&dir));

    assert!(matches!(store.load_outcome(), LoadOutcome::Reset { .. }));
    assert_eq!(store.snapshot().document, json!({ "version": 1 }));
    store
        .set_section("settings", json!({}))
        .expect("still usable");
    store.flush().expect("and writable");
}

#[test]
fn a_newer_version_is_never_overwritten() {
    let dir = TempDir::new("ui-newer");
    let newer = br#"{ "version": 7, "dashboards": { "hologram": true } }"#;
    std::fs::write(path(&dir), newer).expect("write");

    let store = UiConfigStore::open(&path(&dir));

    assert_eq!(
        store.load_outcome(),
        &LoadOutcome::NewerVersion { found: 7 }
    );
    assert!(store.snapshot().read_only.is_some());
    store
        .set_section("settings", json!({ "x": 1 }))
        .expect("applies in memory");
    assert!(matches!(store.flush(), Err(UiConfigError::ReadOnly(_))));
    assert_eq!(std::fs::read(path(&dir)).expect("read"), newer);
}

#[test]
fn unknown_top_level_keys_are_dropped_on_load() {
    let dir = TempDir::new("ui-unknown");
    std::fs::write(
        path(&dir),
        br#"{ "version": 1, "settings": {}, "futureThing": {}, "dashboards": 5 }"#,
    )
    .expect("write");

    let document = UiConfigStore::open(&path(&dir)).snapshot().document;

    assert_eq!(document, json!({ "version": 1, "settings": {} }));
}

#[test]
fn an_atomic_write_leaves_no_temporary_file_and_keeps_a_backup() {
    let dir = TempDir::new("ui-atomic");
    let target = path(&dir);
    write_atomic(&target, b"{\"version\":1}").expect("first");
    write_atomic(&target, b"{\"version\":1,\"settings\":{}}").expect("second");

    assert!(!sibling(&target, ".tmp").exists());
    assert_eq!(
        std::fs::read(sibling(&target, ".bak")).expect("bak"),
        b"{\"version\":1}"
    );
    assert_eq!(
        std::fs::read(&target).expect("new"),
        b"{\"version\":1,\"settings\":{}}"
    );
}

#[test]
fn update_section_changes_in_place() {
    let dir = TempDir::new("ui-update");
    let store = UiConfigStore::open(&path(&dir));
    store
        .set_section(
            "overlays",
            json!({ "items": [{ "id": "a", "locked": false }] }),
        )
        .expect("set");

    store
        .update_section("overlays", |overlays| {
            overlays["items"][0]["locked"] = json!(true);
        })
        .expect("update");

    assert_eq!(
        store.section("overlays").expect("present")["items"][0]["locked"],
        true
    );
}

#[test]
fn the_writer_coalesces_a_burst_and_flushes_on_shutdown() {
    let dir = TempDir::new("ui-writer");
    let store = Arc::new(UiConfigStore::open(&path(&dir)));
    let writer = ConfigWriter::spawn(Arc::clone(&store), Duration::from_secs(60));

    for i in 0..50 {
        store
            .set_section("settings", json!({ "i": i }))
            .expect("set");
        writer.notify();
    }
    assert!(!path(&dir).exists(), "still inside the quiet period");

    writer.shutdown();

    let reopened = UiConfigStore::open(&path(&dir));
    assert_eq!(reopened.section("settings"), Some(json!({ "i": 49 })));
}

#[test]
fn the_writer_writes_after_the_quiet_period() {
    let dir = TempDir::new("ui-writer-quiet");
    let store = Arc::new(UiConfigStore::open(&path(&dir)));
    let writer = ConfigWriter::spawn(Arc::clone(&store), Duration::from_millis(20));

    store
        .set_section("settings", json!({ "a": 1 }))
        .expect("set");
    writer.notify();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while store.is_dirty() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(!store.is_dirty());
    assert!(path(&dir).exists());
    writer.shutdown();
}
