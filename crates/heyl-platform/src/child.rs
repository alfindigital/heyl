//! `heyl run`'s child process.
//!
//! heyl spawns the command after `--` with the resolved variables added to its
//! own environment, inherits stdin, and waits. Four things make that more than
//! a `Command::status`:
//!
//! * **Interrupts belong to the child.** Ctrl-C reaches every process attached
//!   to the terminal. Were heyl to die of it, the shell would get its prompt
//!   back while the child is still cleaning up and printing — so heyl ignores
//!   interrupts for as long as the child runs, and reports whatever the child
//!   made of them.
//! * **The exit code is the child's**, so `heyl run … -- make test` slots into
//!   scripts unchanged. A Unix child killed by a signal reports `128 + signal`,
//!   as a shell does.
//! * **Windows finds scripts.** `npm`, `yarn` and friends are `.cmd` files.
//!   `std::process::Command` only ever appends `.exe`, so `heyl run -- npm test`
//!   would fail where `npm test` works; [`resolve`] applies `PATHEXT` the way
//!   `cmd.exe` does.
//! * **Secrets are masked in its output.** Given values to mask, heyl pipes
//!   the child's stdout and stderr through a [`Masker`] each, which turns every
//!   occurrence into stars, and only returns once both pipes are drained. With
//!   nothing to mask both streams are inherited, as they are under `--no-mask`:
//!   a pipe is not a terminal, and some programs behave differently without one.
//!
//! The variables are copied into the `Command` and the `Command` is dropped
//! right after the spawn: those copies are ordinary allocations, and the
//! shorter they live the better.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use zeroize::Zeroizing;

use crate::mask::Masker;

/// Run `program` with `args`, adding `variables` to the inherited environment,
/// and wait for it. Every occurrence of a value in `mask` is replaced by stars
/// in what it writes to stdout and stderr.
///
/// Returns the child's exit code.
///
/// # Errors
/// The spawn's own error — typically `NotFound` for a command that does not
/// exist, `PermissionDenied` for one that cannot be executed.
pub async fn run<'a>(
    program: &OsStr,
    args: &[OsString],
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
    mask: &[&str],
) -> std::io::Result<i32> {
    run_with(
        program,
        args,
        variables,
        mask,
        tokio::io::stdout(),
        tokio::io::stderr(),
    )
    .await
}

/// [`run`], writing the masked output to `stdout` and `stderr`.
async fn run_with<'a>(
    program: &OsStr,
    args: &[OsString],
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
    mask: &[&str],
    stdout: impl AsyncWrite + Unpin,
    stderr: impl AsyncWrite + Unpin,
) -> std::io::Result<i32> {
    // Registered before the spawn, held until the wait ends: while a handler
    // is registered, an interrupt no longer terminates heyl.
    let _interrupts = IgnoreInterrupts::register();

    let masking = !Masker::new(mask.iter().copied()).is_empty();
    let mut command = tokio::process::Command::new(resolve(program));
    command.args(args).envs(variables);
    if masking {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = command.spawn()?;
    drop(command);

    if !masking {
        return Ok(code(child.wait().await?));
    }
    let out = child.stdout.take().expect("stdout is piped");
    let err = child.stderr.take().expect("stderr is piped");
    // Both pipes are drained before the code is returned: whatever the child
    // printed last must not be lost to heyl exiting first.
    let (status, (), ()) = tokio::join!(
        child.wait(),
        pump(out, stdout, Masker::new(mask.iter().copied())),
        pump(err, stderr, Masker::new(mask.iter().copied())),
    );
    Ok(code(status?))
}

/// Copy `from` to `to` through `masker`, until `from` ends.
///
/// Flushed after every read, so prompts and progress show up as they are
/// written. If `to` fails — `heyl run … | head` closing its end — copying
/// stops and `from` is closed, so the child sees the broken pipe it would
/// have seen writing there itself.
async fn pump(
    mut from: impl AsyncRead + Unpin,
    mut to: impl AsyncWrite + Unpin,
    mut masker: Masker,
) {
    let mut buf = Zeroizing::new(vec![0u8; 8192]);
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let chunk = masker.push(&buf[..n]);
        if to.write_all(&chunk).await.is_err() || to.flush().await.is_err() {
            return;
        }
    }
    let rest = masker.finish();
    let _ = to.write_all(&rest).await;
    let _ = to.flush().await;
}

/// The exit code a shell would report for `status`.
#[cfg(unix)]
fn code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt as _;
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

/// The exit code a shell would report for `status`.
#[cfg(not(unix))]
fn code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

/// Handlers that swallow interrupts, for as long as they are alive.
struct IgnoreInterrupts {
    #[cfg(unix)]
    _signals: Vec<tokio::signal::unix::Signal>,
    #[cfg(windows)]
    _ctrl_c: Option<tokio::signal::windows::CtrlC>,
    #[cfg(windows)]
    _ctrl_break: Option<tokio::signal::windows::CtrlBreak>,
}

impl IgnoreInterrupts {
    /// Best-effort: a handler that cannot be registered leaves heyl as
    /// interruptible as any other program, which is no reason not to run.
    fn register() -> Self {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Self {
                _signals: [SignalKind::interrupt(), SignalKind::quit()]
                    .into_iter()
                    .filter_map(|kind| signal(kind).ok())
                    .collect(),
            }
        }
        #[cfg(windows)]
        {
            Self {
                _ctrl_c: tokio::signal::windows::ctrl_c().ok(),
                _ctrl_break: tokio::signal::windows::ctrl_break().ok(),
            }
        }
    }
}

/// The program to spawn for `program`.
///
/// On Windows, a bare name without an extension is looked up on `PATH` with
/// every `PATHEXT` extension in turn, as `cmd.exe` does — the first file that
/// exists wins. Anything else, and every name on other platforms, is passed
/// through for the OS to find.
fn resolve(program: &OsStr) -> PathBuf {
    #[cfg(windows)]
    if let Some(found) = search_path(
        program,
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("PATHEXT").as_deref(),
    ) {
        return found;
    }
    PathBuf::from(program)
}

/// `PATH` × `PATHEXT` lookup for a bare name, or `None` to leave it alone.
#[cfg(any(windows, test))]
fn search_path(program: &OsStr, path: Option<&OsStr>, pathext: Option<&OsStr>) -> Option<PathBuf> {
    let name = std::path::Path::new(program);
    // A path, or a name that already has an extension, is taken as given.
    if name.components().count() != 1 || name.extension().is_some() {
        return None;
    }
    let pathext = pathext
        .and_then(OsStr::to_str)
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    let extensions: Vec<&str> = pathext.split(';').filter(|e| !e.is_empty()).collect();

    std::env::split_paths(path?).find_map(|dir| {
        extensions.iter().find_map(|ext| {
            let mut file = program.to_os_string();
            file.push(ext);
            let candidate = dir.join(file);
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("heyl-child-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_bare_name_finds_a_script_by_pathext() {
        let dir = scratch("pathext");
        std::fs::write(dir.join("tool.cmd"), "").expect("write");
        // Lower-case extensions keep the test meaningful on case-sensitive
        // file systems, where the suite also runs.
        let found = search_path(
            OsStr::new("tool"),
            Some(dir.as_os_str()),
            Some(OsStr::new(".exe;.cmd")),
        );
        assert_eq!(found, Some(dir.join("tool.cmd")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn earlier_extensions_win() {
        let dir = scratch("order");
        std::fs::write(dir.join("tool.exe"), "").expect("write");
        std::fs::write(dir.join("tool.cmd"), "").expect("write");
        let found = search_path(
            OsStr::new("tool"),
            Some(dir.as_os_str()),
            Some(OsStr::new(".exe;.cmd")),
        )
        .expect("found");
        assert_eq!(found, dir.join("tool.exe"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_and_names_with_an_extension_are_left_alone() {
        let dir = scratch("alone");
        std::fs::write(dir.join("tool.cmd"), "").expect("write");
        for given in ["tool.cmd", "./tool", "sub/tool"] {
            assert_eq!(
                search_path(OsStr::new(given), Some(dir.as_os_str()), None),
                None,
                "{given}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A shell command line, for the platform at hand.
    fn shell(script: &str) -> (OsString, Vec<OsString>) {
        if cfg!(windows) {
            ("cmd".into(), vec!["/c".into(), script.into()])
        } else {
            ("sh".into(), vec!["-c".into(), script.into()])
        }
    }

    async fn capture(script: &str, mask: &[&str]) -> (i32, String, String) {
        let (program, args) = shell(script);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(
            &program,
            &args,
            [("SECRET", "hunter22")],
            mask,
            &mut out,
            &mut err,
        )
        .await
        .expect("spawn");
        let text = |b: Vec<u8>| String::from_utf8(b).expect("utf-8").replace("\r\n", "\n");
        (code, text(out), text(err))
    }

    #[tokio::test]
    async fn secrets_in_stdout_and_stderr_become_stars() {
        let script = if cfg!(windows) {
            "echo pw=%SECRET%& echo err=%SECRET% 1>&2& exit 3"
        } else {
            "echo pw=$SECRET; echo err=$SECRET >&2; exit 3"
        };
        let (code, out, err) = capture(script, &["hunter22"]).await;
        assert_eq!(code, 3);
        assert_eq!(out.trim_end(), "pw=********");
        assert_eq!(err.trim_end(), "err=********");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_secret_written_in_two_parts_is_still_masked() {
        let (_, out, _) = capture(
            "printf 'pw=hun'; sleep 0.2; printf 'ter22 done'",
            &["hunter22"],
        )
        .await;
        assert_eq!(out, "pw=******** done");
    }

    #[test]
    fn a_name_found_nowhere_is_left_for_the_os() {
        let dir = scratch("nowhere");
        assert_eq!(
            search_path(OsStr::new("no-such-tool"), Some(dir.as_os_str()), None),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
