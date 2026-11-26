//! Offline title-edit probe against synthetic vault commits.

use heyl_vault::{Document, decode, encode, fold, login::logins};
use serde_json::{Value, json};

const TARGET: &str = "00000000-0000-4000-8000-000000000002";
const OTHER: &str = "00000000-0000-4000-8000-000000000003";
const BEFORE: &str = "2026-11-26T00:00:00.000Z";
const AFTER: &str = "2026-11-26T00:01:00.000Z";

struct DummyVault {
    commits: Vec<Vec<u8>>,
}

impl DummyVault {
    fn open(&self) -> Document {
        let decoded: Vec<_> = self
            .commits
            .iter()
            .map(|blob| decode(blob).unwrap())
            .collect();
        fold(&decoded).unwrap()
    }

    fn edit_title(&mut self, revision: usize, title: &str) -> Result<(), &'static str> {
        if revision != self.commits.len() {
            return Err("stale revision");
        }
        let mut current = self.open();
        let mut target = current.content["logins"][TARGET].clone();
        target["title"] = json!(title);
        target["updateTime"] = json!(AFTER);
        current.content.clear();
        current
            .content
            .insert("logins".into(), json!({ (TARGET): target }));
        self.commits.push(encode(&current).unwrap());
        Ok(())
    }
}

#[test]
fn title_edit_round_trips_and_stale_edits_leave_the_vault_intact() {
    let mut doc = Document::empty();
    doc.content.insert(
        "logins".into(),
        json!({
            (TARGET): {
                "title": "Before", "displayHeadline": "Dummy name", "updateTime": BEFORE,
                "password": {"encrypted": "synthetic", "isEmpty": false},
                "unknownField": {"keep": ["all", "values"] }
            },
            (OTHER): {"title": "Other login", "updateTime": BEFORE}
        }),
    );
    let mut vault = DummyVault {
        commits: vec![encode(&doc).unwrap()],
    };
    let revision = vault.commits.len();
    vault.edit_title(revision, "After").unwrap();
    assert_eq!(vault.edit_title(revision, "Stale"), Err("stale revision"));
    assert_eq!(vault.commits.len(), 2);

    let reopened = vault.open();
    let display = logins(&reopened);
    assert!(
        display
            .iter()
            .any(|login| login.id.to_string() == TARGET && login.title == "After")
    );
    assert_eq!(
        reopened.content["logins"][OTHER],
        doc.content["logins"][OTHER]
    );
    let mut actual = reopened.content["logins"][TARGET].clone();
    actual["title"] = json!("Before");
    actual["updateTime"] = json!(BEFORE);
    assert_eq!(actual, doc.content["logins"][TARGET]);
    let delta = decode(&vault.commits[1]).unwrap();
    assert_eq!(delta.content["logins"].get(OTHER), None::<&Value>);
}
