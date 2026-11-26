//! Stateful loopback backend for the title-edit CLI scenario.

use std::sync::{Arc, Mutex};

use heyl_grpc::{HeyloginApi, RecordedApi, Request, Scenario, Server};
use heyl_ports::ApiError;
use heyl_proto::{
    CreateCommitRequest, CreateCommitResponse, ListAuthenticatorsRequest,
    ListAuthenticatorsResponse, ListCommitsRequest, ListCommitsResponse, SyncRequest, SyncResponse,
    list_commits_response::Commit,
};

use super::{TestDir, invoke, parse_stdout, path_of};

const VAULT: &str = "6e577b16-1303-47c3-8579-37e59cb71276";
const LOGIN: &str = "f1bcfdf7-c3ca-4fc6-925d-f57d5e8919bd";
const COMMIT: &str = "00000000-0000-4000-8000-000000000001";
const TITLE: &str = "Loopback edited title";

struct Backend {
    replay: RecordedApi,
    state: Mutex<State>,
    expose_write: bool,
}

#[derive(Default)]
struct State {
    initial: Option<ListCommitsResponse>,
    write: Option<CreateCommitRequest>,
    reads: usize,
}

#[async_trait::async_trait]
impl HeyloginApi for Backend {
    async fn sync_sync(&self, request: Request<SyncRequest>) -> Result<SyncResponse, ApiError> {
        self.replay.sync_sync(request).await
    }

    async fn authenticator_list(
        &self,
        request: Request<ListAuthenticatorsRequest>,
    ) -> Result<ListAuthenticatorsResponse, ApiError> {
        self.replay.authenticator_list(request).await
    }

    async fn vault_list_commits(
        &self,
        request: Request<ListCommitsRequest>,
    ) -> Result<ListCommitsResponse, ApiError> {
        assert_eq!(request.message.vault_id, VAULT);
        let initial = { self.state.lock().unwrap().initial.clone() };
        let mut response = match initial {
            Some(initial) => initial,
            None => self.replay.vault_list_commits(request).await?,
        };
        let mut state = self.state.lock().unwrap();
        state.reads += 1;
        if state.initial.is_none() {
            state.initial = Some(response.clone());
        }
        if let Some(write) = &state.write
            && self.expose_write
        {
            response.newer_commits.push(Commit {
                id: COMMIT.to_owned(),
                blob: write.new_commit_blob.clone(),
            });
        }
        Ok(response)
    }

    async fn vault_create_commit(
        &self,
        request: Request<CreateCommitRequest>,
    ) -> Result<CreateCommitResponse, ApiError> {
        let mut state = self.state.lock().unwrap();
        let last = state
            .initial
            .as_ref()
            .unwrap()
            .newer_commits
            .last()
            .unwrap();
        assert_eq!(request.message.vault_id, VAULT);
        assert_eq!(request.message.latest_commit_id, last.id);
        assert!(request.message.update_time.is_some());
        assert!(!request.message.new_commit_blob.is_empty());
        assert!(state.write.replace(request.message).is_none());
        Ok(CreateCommitResponse {
            commit_id: COMMIT.to_owned(),
            ..Default::default()
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_title_sends_one_commit_and_confirms_it_over_loopback() {
    exercise(true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_title_reports_unverified_when_read_back_is_stale() {
    exercise(false).await;
}

async fn exercise(expose_write: bool) {
    let scenario = Scenario::load(&path_of("get")).expect("get fixture");
    let state = TestDir::new(
        if expose_write {
            "edit_title_success"
        } else {
            "edit_title_unverified"
        },
        &scenario.meta.store,
    );
    let seed = &scenario.meta.first_draw;
    let pair = &scenario.steps[0];
    let server = Server::start(Arc::new(RecordedApi::new(pair.calls.clone())))
        .await
        .expect("a loopback port");
    let outcome = invoke(pair, &server.endpoint(), &state, seed).await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);

    let mut step = scenario
        .steps
        .iter()
        .find(|step| {
            step.argv.get(1).is_some_and(|arg| arg == "-V")
                && step.argv.get(2).is_some_and(|arg| arg == VAULT)
        })
        .expect("private vault fixture")
        .clone();
    step.argv = [
        "edit-title",
        "--vault-id",
        VAULT,
        "--login-id",
        LOGIN,
        "--expect-title=",
        "--title",
        TITLE,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let backend = Arc::new(Backend {
        replay: RecordedApi::new(step.calls.clone()),
        state: Mutex::new(State::default()),
        expose_write,
    });
    server.serve_from(backend.clone());
    let outcome = invoke(&step, &server.endpoint(), &state, seed).await;

    if expose_write {
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
        assert_eq!(
            parse_stdout(&outcome.stdout, "edit_title", 1),
            [serde_json::json!({"login_id": LOGIN, "title": TITLE})]
        );
    } else {
        assert_eq!(outcome.code, heyl_app::ExitCode::Failure as i32);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.contains("could not be verified"));
    }
    assert!(server.unmatched().is_empty());
    let state = backend.state.lock().unwrap();
    assert!(state.write.is_some());
    assert_eq!(state.reads, 2);
}
