//! Run the real CLI against a persisted synthetic vault.

use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};

const TARGET: &str = "00000000-0000-4000-8000-000000000002";
const OTHER: &str = "00000000-0000-4000-8000-000000000003";

fn edit(path: &Path, expected: &str, title: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_heyl"))
        .args(["--format", "json", "edit-title", "--dummy-vault"])
        .arg(path)
        .args([
            "--login-id",
            TARGET,
            "--expect-title",
            expected,
            "--title",
            title,
        ])
        .output()
        .unwrap()
}

#[test]
fn edits_the_title_on_disk_and_rejects_stale_or_non_dummy_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dummy vault.json");
    let fixture = json!({
        "format": "heyl-dummy-v1",
        "version": 2,
        "content": {"logins": {
            (TARGET): {
                "title": "Before", "displayHeadline": "Dummy name",
                "password": {"encrypted": "synthetic-private"},
                "unknownField": ["preserve", 123]
            },
            (OTHER): {"title": "Neighbor"}
        }}
    });
    fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();

    let success = edit(&path, "Before", "After");
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&success.stdout).unwrap(),
        json!({"login_id": TARGET, "title": "After"})
    );
    let updated_bytes = fs::read(&path).unwrap();
    let mut expected = fixture.clone();
    expected["content"]["logins"][TARGET]["title"] = json!("After");
    assert_eq!(
        serde_json::from_slice::<Value>(&updated_bytes).unwrap(),
        expected
    );

    let conflict = edit(&path, "Before", "Stale");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("title conflict"));
    assert_eq!(fs::read(&path).unwrap(), updated_bytes);

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| edit(&path, "After", "First"));
        let second = scope.spawn(|| edit(&path, "After", "Second"));
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_ne!(first.status.success(), second.status.success());
    let (winner, rejected) = if first.status.success() {
        ("First", second)
    } else {
        ("Second", first)
    };
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("title conflict"));
    let persisted: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted["content"]["logins"][TARGET]["title"], winner);

    let mut non_dummy = fixture;
    non_dummy.as_object_mut().unwrap().remove("format");
    let original = serde_json::to_vec(&non_dummy).unwrap();
    fs::write(&path, &original).unwrap();
    let rejected = edit(&path, "Before", "Rejected");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("heyl-dummy-v1"));
    assert_eq!(fs::read(&path).unwrap(), original);
}
