//! File-backed title edits for explicitly marked synthetic vaults.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use clap::Args;
use heyl_domain::LoginId;
use serde_json::Value;

use crate::output::{self, Format};

#[derive(Debug, Args)]
pub struct EditTitle {
    /// Path to a JSON fixture marked heyl-dummy-v1.
    #[arg(long, value_name = "FILE", required_unless_present = "vault_id")]
    pub dummy_vault: Option<PathBuf>,
    /// UUID of an existing personal heylogin vault.
    #[arg(long, conflicts_with = "dummy_vault")]
    pub vault_id: Option<String>,
    /// UUID of the login to change.
    #[arg(long)]
    pub login_id: String,
    /// Current title. Supply an empty string for an untitled entry.
    #[arg(long)]
    pub expect_title: String,
    /// New title.
    #[arg(long)]
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
enum EditError {
    #[error("login ID must be a UUID")]
    InvalidId,
    #[error("the new title cannot be blank")]
    BlankTitle,
    #[error("expected a JSON vault fixture marked heyl-dummy-v1 with version 2")]
    InvalidFixture,
    #[error("active login not found")]
    NoLogin,
    #[error("title conflict: the current title differs from --expect-title")]
    Conflict,
    #[error("could not access or save the local vault: {0}")]
    Io(#[from] io::Error),
}

pub fn run(path: &Path, args: &EditTitle, format: Format) -> std::process::ExitCode {
    match edit(path, args) {
        Ok(id) => {
            output::title_edited(&id.to_string(), &args.title, format);
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("heyl: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn edit(path: &Path, args: &EditTitle) -> Result<LoginId, EditError> {
    let id = LoginId::parse(&args.login_id).map_err(|_| EditError::InvalidId)?;
    if args.title.trim().is_empty() {
        return Err(EditError::BlankTitle);
    }

    let path = path.canonicalize()?;
    let _lock = lock_fixture(&path)?;
    let mut fixture: Value =
        serde_json::from_slice(&fs::read(&path)?).map_err(|_| EditError::InvalidFixture)?;
    if fixture["format"] != "heyl-dummy-v1"
        || fixture["version"].as_u64() != Some(heyl_vault::DESCRIPTOR_VERSION_HEYMERGE)
    {
        return Err(EditError::InvalidFixture);
    }
    let logins = fixture
        .pointer_mut("/content/logins")
        .and_then(Value::as_object_mut)
        .ok_or(EditError::InvalidFixture)?;
    let login = logins
        .get_mut(&id.to_string())
        .and_then(Value::as_object_mut)
        .ok_or(EditError::NoLogin)?;
    if login.get("isDeleted") == Some(&Value::Bool(true))
        || login.get("isArchived") == Some(&Value::Bool(true))
    {
        return Err(EditError::NoLogin);
    }
    let current = match login.get("title") {
        Some(value) => value.as_str().ok_or(EditError::InvalidFixture)?,
        None => "",
    };
    if current != args.expect_title {
        return Err(EditError::Conflict);
    }
    if current != args.title {
        login.insert("title".into(), Value::String(args.title.clone()));
        save(&path, &fixture)?;
    }
    Ok(id)
}

fn lock_fixture(path: &Path) -> io::Result<File> {
    // The lock survives replacement of the JSON file. All CLI writers hold it
    // from the initial read through the atomic replacement.
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    file.lock()?;
    Ok(file)
}

fn save(path: &Path, fixture: &Value) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temp.as_file_mut(), fixture)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}
