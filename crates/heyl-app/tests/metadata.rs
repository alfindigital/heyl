//! Metadata projection with a synthetic vault.

use heyl_app::metadata::select;
use heyl_domain::VaultId;
use heyl_vault::{Document, login::logins};
use serde_json::json;

#[test]
fn metadata_search_returns_only_live_entries_and_public_fields() {
    let vault = VaultId::parse("00000000-0000-4000-8000-000000000001").unwrap();
    let matching = "00000000-0000-4000-8000-000000000002";
    let archived = "00000000-0000-4000-8000-000000000003";
    let mut doc = Document::empty();
    doc.content.insert(
        "logins".into(),
        json!({
            (matching): {
                "displayHeadline": "Dummy Shop",
                "title": "Shop",
                "websites": [
                    "https://example.test/path?token=fixture-private",
                    "EXAMPLE.TEST/other",
                    "https://fixture-private@another.test",
                    "javascript:fixture-private"
                ],
                "username": "fixture-private",
                "note": "fixture-private",
                "password": { "encrypted": "fixture-private", "isEmpty": false }
            },
            (archived): {
                "displayHeadline": "Archived Shop",
                "isDeleted": true,
                "isArchived": true
            }
        }),
    );

    let logins = logins(&doc);
    let rows = select(vault, logins.clone(), Some(" EXAMPLE.TEST "));
    assert_eq!(
        serde_json::to_value(rows).unwrap(),
        json!([{
            "vault_id": vault.to_string(),
            "login_id": matching,
            "title": "Shop",
            "domains": ["example.test"]
        }])
    );
    assert_eq!(select(vault, logins.clone(), None).len(), 1);
    assert_eq!(select(vault, logins.clone(), Some("dummy")).len(), 1);
    assert!(select(vault, logins, Some("fixture-private")).is_empty());
}
