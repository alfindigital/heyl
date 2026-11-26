//! Live title-edit commit construction against synthetic data.

use heyl_app::edit_title::title_delta;
use heyl_domain::{LoginId, Timestamp};
use heyl_vault::{Document, fold};
use serde_json::json;

#[test]
fn title_delta_preserves_other_fields_and_refuses_stale_inputs() {
    let id = LoginId::parse("00000000-0000-4000-8000-000000000002").unwrap();
    let previous = "2026-11-26T00:00:00.000Z";
    let now = Timestamp::parse("2026-11-26T00:00:01.000Z").unwrap();
    let mut vault = Document::empty();
    vault.content.insert(
        "logins".into(),
        json!({
            (id.to_string()): {
                "title": "Before", "displayHeadline": "Before", "updateTime": previous,
                "password": {"encrypted": "synthetic"}, "unknown": [1, 2, 3]
            },
            "neighbor": {"title": "Other"}
        }),
    );
    let delta = title_delta(&vault, id, "Before", "After", now)
        .unwrap()
        .unwrap();
    let entries = delta.content["logins"].as_object().unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[&id.to_string()];
    assert_eq!(entry["title"], "After");
    assert_eq!(entry["displayHeadline"], "Before");
    assert_eq!(entry["editTime"], now.to_string());
    assert_eq!(entry["updateTime"], now.to_string());
    assert_eq!(
        entry["password"],
        vault.content["logins"][id.to_string()]["password"]
    );
    assert_eq!(entry["unknown"], json!([1, 2, 3]));
    let updated = fold(&[vault, delta]).unwrap();
    assert_eq!(updated.content["logins"]["neighbor"]["title"], "Other");
    assert!(title_delta(&updated, id, "Before", "Stale", now).is_err());
    assert!(
        title_delta(&updated, id, "After", "After", now)
            .unwrap()
            .is_none()
    );
}
