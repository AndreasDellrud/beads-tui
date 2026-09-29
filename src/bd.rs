use std::{io, process::Command};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub design: String,
    #[serde(default)]
    pub acceptance_criteria: String,
    pub status: String,
    pub priority: u8,
    pub issue_type: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub dependency_count: u64,
    #[serde(default)]
    pub dependent_count: u64,
    #[serde(default)]
    pub comment_count: u64,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub closed_at: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default, deserialize_with = "deserialize_related_issues")]
    pub dependencies: Vec<RelatedIssue>,
    #[serde(default, deserialize_with = "deserialize_related_issues")]
    pub dependents: Vec<RelatedIssue>,
    #[serde(default)]
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RelatedIssue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Comment {
    #[serde(default)]
    pub author: String,
    pub text: String,
    #[serde(default)]
    pub created_at: String,
}

fn deserialize_related_issues<'de, D>(deserializer: D) -> Result<Vec<RelatedIssue>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListView {
    #[default]
    Active,
    Ready,
    Closed,
}

impl ListView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Ready => "ready",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListSort {
    #[default]
    Priority,
    Updated,
    Created,
}

impl ListSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::Updated => "updated",
            Self::Created => "created",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListOptions {
    pub view: ListView,
    pub sort: ListSort,
}

pub trait IssueSource: Send + Sync + 'static {
    fn list(&self, options: ListOptions) -> Result<Vec<Issue>>;
    fn show(&self, id: &str) -> Result<Issue>;
    fn revision(&self) -> Result<String>;
}

pub struct CliSource;

impl IssueSource for CliSource {
    fn list(&self, options: ListOptions) -> Result<Vec<Issue>> {
        list(options)
    }

    fn show(&self, id: &str) -> Result<Issue> {
        show(id)
    }

    fn revision(&self) -> Result<String> {
        revision()
    }
}

pub fn list(options: ListOptions) -> Result<Vec<Issue>> {
    list_with(&ProcessRunner, options)
}

pub fn show(id: &str) -> Result<Issue> {
    show_with(&ProcessRunner, id)
}

pub fn revision() -> Result<String> {
    revision_with(&ProcessRunner)
}

fn list_with(runner: &impl CommandRunner, options: ListOptions) -> Result<Vec<Issue>> {
    let mut arguments = vec![
        "--readonly".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
        "--limit".to_owned(),
        "0".to_owned(),
    ];
    match options.view {
        ListView::Active => {
            arguments.extend(["--status".to_owned(), "open,in_progress,blocked".to_owned()])
        }
        ListView::Ready => arguments.push("--ready".to_owned()),
        ListView::Closed => arguments.extend(["--status".to_owned(), "closed".to_owned()]),
    }
    arguments.extend(["--sort".to_owned(), options.sort.label().to_owned()]);
    if options.sort != ListSort::Priority {
        arguments.push("--reverse".to_owned());
    }
    let references: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let output = run(runner, &references)?;
    parse_issues(&output).context("bd list returned unexpected JSON")
}

fn show_with(runner: &impl CommandRunner, id: &str) -> Result<Issue> {
    let id_argument = format!("--id={id}");
    let output = run(
        runner,
        &[
            "--readonly",
            "show",
            &id_argument,
            "--json",
            "--include-comments",
            "--include-dependents",
        ],
    )?;
    parse_issue(&output).context("bd show returned unexpected JSON")
}

#[derive(Deserialize)]
struct VersionControlStatus {
    branch: String,
    commit: String,
}

fn revision_with(runner: &impl CommandRunner) -> Result<String> {
    let output = run(runner, &["--readonly", "vc", "status", "--json"])?;
    let status: VersionControlStatus =
        serde_json::from_slice(&output).context("bd vc status returned unexpected JSON")?;
    Ok(format!("{}:{}", status.branch, status.commit))
}

struct CommandResult {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

trait CommandRunner {
    fn run(&self, arguments: &[&str]) -> io::Result<CommandResult>;
}

struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, arguments: &[&str]) -> io::Result<CommandResult> {
        let output = Command::new("bd").args(arguments).output()?;
        Ok(CommandResult {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn run(runner: &impl CommandRunner, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = runner
        .run(arguments)
        .context("could not start bd; install Beads and ensure `bd` is on PATH")?;

    if !output.success {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if message.is_empty() {
            bail!("bd failed without a diagnostic");
        }
        bail!("bd failed: {message}");
    }

    Ok(output.stdout)
}

fn parse_issues(json: &[u8]) -> Result<Vec<Issue>, serde_json::Error> {
    serde_json::from_slice(json)
}

fn parse_issue(json: &[u8]) -> Result<Issue, serde_json::Error> {
    let mut issues = parse_issues(json)?;
    issues.pop().ok_or_else(|| {
        serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "bd show returned no issue",
        ))
    })
}

// A feed is an optional invalidation hint, never a replacement for bd list/show.
// Keeping one coalesced dirty bit bounds memory even when writers outpace frames.
#[derive(Clone, Debug, Default)]
pub struct EventUpdate {
    pub changed: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct EventState {
    update: EventUpdate,
    cursor: u64,
    end: Option<FeedEnd>,
}

#[derive(Clone)]
enum FeedEnd {
    Unavailable,
    Rebaseline(u64),
    Failed(String),
}

pub struct EventMonitor {
    state: std::sync::Arc<std::sync::Mutex<EventState>>,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

const EVENT_RECORD_LIMIT: u64 = 4 * 1024 * 1024;
const EVENT_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

impl EventMonitor {
    pub fn start() -> Result<Self> {
        Self::spawn(event_command, EVENT_RETRY)
    }

    fn spawn(
        command: impl Fn(u64) -> Command + Send + 'static,
        retry: std::time::Duration,
    ) -> Result<Self> {
        use std::sync::{Arc, Mutex, mpsc};
        let state = Arc::new(Mutex::new(EventState::default()));
        let shared = Arc::clone(&state);
        let (stop, stopped) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("btui-bd-events".into())
            .spawn(move || {
                loop {
                    if stopped.try_recv().is_ok() {
                        break;
                    }
                    let cursor = {
                        let mut state = shared.lock().expect("events state poisoned");
                        state.end = None;
                        state.cursor
                    };
                    let outcome = follow_events(command(cursor), &shared, &stopped);
                    let Some(outcome) = outcome else { break };
                    let mut state = shared.lock().expect("events state poisoned");
                    match outcome {
                        FeedEnd::Unavailable => {
                            state.cursor = 0;
                            state.update.error = None;
                        }
                        FeedEnd::Rebaseline(head) => {
                            state.cursor = head;
                            state.update.changed = true;
                            state.update.error =
                                Some("event history expired; refreshing from bd".into());
                        }
                        FeedEnd::Failed(error) => {
                            state.cursor = 0;
                            state.update.changed = true;
                            state.update.error = Some(error);
                        }
                    }
                    drop(state);
                    if stopped.recv_timeout(retry).is_ok() {
                        break;
                    }
                }
            })
            .context("could not start the bd event monitor")?;
        Ok(Self {
            state,
            stop,
            worker: Some(worker),
        })
    }

    pub fn poll(&self) -> EventUpdate {
        let mut state = self.state.lock().expect("events state poisoned");
        EventUpdate {
            changed: std::mem::take(&mut state.update.changed),
            error: state.update.error.clone(),
        }
    }
}

impl Drop for EventMonitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn event_command(cursor: u64) -> Command {
    let mut command = Command::new("bd");
    command.args([
        "--readonly",
        "events",
        "tail",
        "--since",
        &cursor.to_string(),
        "--follow",
        "--json",
    ]);
    command
}

fn follow_events(
    mut command: Command,
    state: &std::sync::Arc<std::sync::Mutex<EventState>>,
    stopped: &std::sync::mpsc::Receiver<()>,
) -> Option<FeedEnd> {
    use std::{process::Stdio, sync::Arc, time::Duration};
    let mut child = match command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Some(FeedEnd::Failed(format!(
                "could not start bd events: {error}"
            )));
        }
    };
    let output = child.stdout.take().expect("piped stdout");
    let errors = child.stderr.take().expect("piped stderr");
    let output_state = Arc::clone(state);
    let error_state = Arc::clone(state);
    // Readers never wait for UI consumption and never retain an unbounded queue.
    let stdout = std::thread::spawn(move || read_event_output(output, &output_state));
    let stderr = std::thread::spawn(move || read_event_errors(errors, &error_state));
    let mut cancelled = false;
    let exit = loop {
        if stopped.recv_timeout(Duration::from_millis(50)).is_ok() {
            cancelled = true;
            break None;
        }
        if state.lock().expect("events state poisoned").end.is_some() {
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                set_feed_end(
                    state,
                    FeedEnd::Failed(format!("bd events wait failed: {error}")),
                );
                break None;
            }
        }
    };
    // Also reap after cancellation or parser failure, before joining pipe readers.
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout.join();
    let _ = stderr.join();
    if cancelled {
        return None;
    }
    let mut state = state.lock().expect("events state poisoned");
    Some(state.end.take().unwrap_or_else(|| {
        FeedEnd::Failed(format!(
            "bd events stopped{}; polling continues",
            exit.map_or(String::new(), |s| format!(" ({s})"))
        ))
    }))
}

fn set_feed_end(state: &std::sync::Mutex<EventState>, end: FeedEnd) {
    let mut state = state.lock().expect("events state poisoned");
    if state.end.is_none() {
        state.end = Some(end);
    }
}

fn read_event_output(reader: impl io::Read, state: &std::sync::Mutex<EventState>) {
    use io::{BufRead, Read};
    let mut reader = io::BufReader::new(reader);
    let mut record = Vec::new();
    loop {
        let remaining = EVENT_RECORD_LIMIT.saturating_sub(record.len() as u64);
        let read = reader
            .by_ref()
            .take(remaining + 1)
            .read_until(b'\n', &mut record);
        match read {
            Ok(0) => {
                if !record.iter().all(u8::is_ascii_whitespace) {
                    set_feed_end(state, FeedEnd::Failed("incomplete bd event record".into()));
                }
                return;
            }
            Err(error) => {
                set_feed_end(
                    state,
                    FeedEnd::Failed(format!("could not read bd events: {error}")),
                );
                return;
            }
            _ => {}
        }
        if record.len() as u64 > EVENT_RECORD_LIMIT {
            set_feed_end(
                state,
                FeedEnd::Failed("bd event exceeds the 4 MiB record limit".into()),
            );
            return;
        }
        if record.iter().all(u8::is_ascii_whitespace) {
            record.clear();
            continue;
        }
        match serde_json::from_slice::<serde_json::Value>(&record) {
            Ok(value) => {
                let result = accept_event(value, &mut state.lock().expect("events state poisoned"));
                if let Err(end) = result {
                    set_feed_end(state, end);
                    return;
                }
                record.clear();
            }
            // Initial errors can be pretty-printed JSON; mid-follow records are JSONL.
            Err(error) if error.is_eof() => {}
            Err(_) => {
                set_feed_end(state, FeedEnd::Failed("malformed bd event JSON".into()));
                return;
            }
        }
    }
}

fn accept_event(value: serde_json::Value, state: &mut EventState) -> Result<(), FeedEnd> {
    let invalid = || FeedEnd::Failed("unsupported bd event record; polling continues".into());
    if value.get("error").is_some() || value.get("code").is_some() {
        if value["code"] == "events_journal_truncated" {
            return Err(FeedEnd::Rebaseline(
                value["head"].as_u64().ok_or_else(invalid)?,
            ));
        }
        return Err(invalid());
    }
    let seq = value["seq"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    let op = value["op"].as_str().ok_or_else(invalid)?;
    let id = value["issue_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(invalid)?;
    let issue = value.get("issue").ok_or_else(invalid)?;
    if !matches!(
        op,
        "create" | "update" | "close" | "delete" | "dep_add" | "dep_remove" | "comment"
    ) || !(issue.is_object() && issue["id"] == id
        || issue.is_null() && matches!(op, "delete" | "dep_add" | "dep_remove"))
    {
        return Err(invalid());
    }
    if seq <= state.cursor {
        return Ok(());
    }
    if seq != state.cursor + 1 {
        return Err(FeedEnd::Failed(
            "gap in bd events; rebuilding from bd".into(),
        ));
    }
    // This cursor acknowledges an invalidation, not applied issue data. A retry
    // always keeps full reconciliation; it must not be reused for delta replay.
    state.cursor = seq;
    state.update.changed = true;
    state.update.error = None;
    Ok(())
}

fn read_event_errors(reader: impl io::Read, state: &std::sync::Mutex<EventState>) {
    use io::{BufRead, Read};
    let mut reader = io::BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        match reader.by_ref().take(8193).read_until(b'\n', &mut line) {
            Ok(0) => return,
            Err(_) => {
                set_feed_end(
                    state,
                    FeedEnd::Failed("could not read bd event diagnostics".into()),
                );
                return;
            }
            _ => {}
        }
        let text = String::from_utf8_lossy(&line);
        if text.contains("events journal is disabled")
            || text.contains("unknown command \"events\"")
        {
            set_feed_end(state, FeedEnd::Unavailable);
            return;
        }
        // Diagnostics are bounded and visible. Unknown stderr is not proof of a
        // supported healthy feed; fall back instead of leaving a silent watcher.
        if !text.trim().is_empty() {
            set_feed_end(
                state,
                FeedEnd::Failed(format!("bd events: {}", text.trim())),
            );
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, io};

    use super::*;

    const LIST_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/bd-list.json");
    const SHOW_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/bd-show.json");

    struct FakeRunner {
        result: RefCell<Option<io::Result<CommandResult>>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeRunner {
        fn succeeds(stdout: &[u8]) -> Self {
            Self::returns(Ok(CommandResult {
                success: true,
                stdout: stdout.to_vec(),
                stderr: Vec::new(),
            }))
        }

        fn exits_with(stderr: &str) -> Self {
            Self::returns(Ok(CommandResult {
                success: false,
                stdout: Vec::new(),
                stderr: stderr.as_bytes().to_vec(),
            }))
        }

        fn returns(result: io::Result<CommandResult>) -> Self {
            Self {
                result: RefCell::new(Some(result)),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, arguments: &[&str]) -> io::Result<CommandResult> {
            self.calls.borrow_mut().push(
                arguments
                    .iter()
                    .map(|argument| (*argument).to_owned())
                    .collect(),
            );
            self.result
                .borrow_mut()
                .take()
                .expect("fake runner called more than once")
        }
    }

    // These are actual CLI captures from separate disposable embedded stores,
    // not hand-written approximations of the two releases' wire contracts.
    #[test]
    fn released_cli_contracts_match_every_view_sort_and_enriched_read() {
        for fixture in [
            include_str!("../tests/fixtures/bd-1.2.2.json"),
            include_str!("../tests/fixtures/bd-1.3.0.json"),
        ] {
            let fixture: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let captures = fixture["captures"].as_array().unwrap();
            let replay = |name: &str| {
                let capture = captures.iter().find(|c| c["name"] == name).unwrap();
                assert_eq!(capture["success"], true);
                FakeRunner::succeeds(capture["stdout"].as_str().unwrap().as_bytes())
            };
            let assert_command = |runner: FakeRunner, name: &str| {
                let capture = captures.iter().find(|c| c["name"] == name).unwrap();
                let expected: Vec<String> =
                    serde_json::from_value(capture["args"].clone()).unwrap();
                assert_eq!(runner.calls.into_inner(), [expected]);
            };
            for view in [ListView::Active, ListView::Ready, ListView::Closed] {
                for sort in [ListSort::Priority, ListSort::Updated, ListSort::Created] {
                    let name = format!("{}-{}", view.label(), sort.label());
                    let runner = replay(&name);
                    let issues = list_with(&runner, ListOptions { view, sort }).unwrap();
                    let mut ids: Vec<_> = issues.iter().map(|issue| issue.id.as_str()).collect();
                    ids.sort();
                    let expected = match view {
                        ListView::Active => vec!["compat-blocked", "compat-child", "compat-ready"],
                        ListView::Ready => vec!["compat-ready"],
                        ListView::Closed => vec!["compat-closed"],
                    };
                    assert_eq!(ids, expected);
                    if view == ListView::Active {
                        let blocked = issues.iter().find(|i| i.id == "compat-blocked").unwrap();
                        assert_eq!(blocked.status, "open");
                        assert_eq!(blocked.dependency_count, 1);
                        assert_eq!(blocked.comment_count, 1);
                        // List edges are IDs, not enriched RelatedIssue records.
                        assert!(blocked.dependencies.is_empty());
                    }
                    assert_command(runner, &name);
                }
            }
            let runner = replay("show");
            let issue = show_with(&runner, "compat-blocked").unwrap();
            assert_eq!(issue.dependencies[0].id, "compat-ready");
            assert_eq!(issue.dependents[0].id, "compat-child");
            assert_eq!(issue.comments[0].text, "A fixture comment");
            assert_eq!(issue.design, "");
            assert_eq!(issue.labels, Vec::<String>::new());
            assert_command(runner, "show");
            let runner = replay("revision");
            assert!(revision_with(&runner).unwrap().starts_with("main:"));
            assert_command(runner, "revision");
            let missing = captures.iter().find(|c| c["name"] == "missing").unwrap();
            let diagnostic = missing["stderr"].as_str().unwrap().trim();
            let runner = FakeRunner::exits_with(diagnostic);
            assert_eq!(
                show_with(&runner, "compat-missing")
                    .unwrap_err()
                    .to_string(),
                format!("bd failed: {diagnostic}")
            );
        }
    }

    #[test]
    fn schema_refusal_keeps_the_upstream_migration_guidance() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/bd-1.3.0-schema-refusal.json"
        ))
        .unwrap();
        let diagnostic = fixture["stderr"].as_str().unwrap();
        let runner = FakeRunner::exits_with(diagnostic);
        let error = list_with(&runner, ListOptions::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("database is at v53, binary expects v66"));
        assert!(error.contains("read-only open cannot migrate"));
        assert_eq!(error, format!("bd failed: {}", diagnostic.trim()));
    }

    #[test]
    fn event_command_is_readonly_json_and_resumes_the_given_cursor() {
        let command = event_command(42);
        assert_eq!(command.get_program(), "bd");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--readonly",
                "events",
                "tail",
                "--since",
                "42",
                "--follow",
                "--json"
            ]
        );
    }

    #[test]
    fn event_records_coalesce_duplicates_and_cover_all_operations() {
        let mut state = EventState::default();
        for (index, op) in [
            "create",
            "update",
            "close",
            "delete",
            "dep_add",
            "dep_remove",
            "comment",
        ]
        .iter()
        .enumerate()
        {
            let issue = if matches!(*op, "delete" | "dep_remove") {
                serde_json::Value::Null
            } else {
                serde_json::json!({"id":"test-1"})
            };
            let record =
                serde_json::json!({"seq":index + 1,"op":op,"issue_id":"test-1","issue":issue});
            assert!(accept_event(record.clone(), &mut state).is_ok());
            state.update.changed = false;
            assert!(accept_event(record, &mut state).is_ok());
            assert!(!state.update.changed);
        }
        assert_eq!(state.cursor, 7);
        let gap =
            serde_json::json!({"seq":9,"op":"update","issue_id":"test-1","issue":{"id":"test-1"}});
        assert!(matches!(
            accept_event(gap, &mut state),
            Err(FeedEnd::Failed(_))
        ));
        assert_eq!(state.cursor, 7);
    }

    #[test]
    fn initial_and_streaming_truncation_rebaseline_without_skipping_silently() {
        for input in [
            "{\n  \"code\": \"events_journal_truncated\",\n  \"head\": 42,\n  \"schema_version\": 1\n}\n",
            "{\"code\":\"events_journal_truncated\",\"head\":42}\n",
        ] {
            let state = std::sync::Mutex::new(EventState::default());
            read_event_output(input.as_bytes(), &state);
            assert!(matches!(
                state.lock().unwrap().end,
                Some(FeedEnd::Rebaseline(42))
            ));
        }
    }

    #[test]
    fn malformed_incomplete_oversized_and_unknown_records_fail_closed() {
        for input in [
            "not json\n".to_owned(),
            "{\"seq\":".to_owned(),
            "x".repeat(EVENT_RECORD_LIMIT as usize + 1),
            "{\"seq\":1,\"op\":\"future\",\"issue_id\":\"x\",\"issue\":{\"id\":\"x\"}}\n"
                .to_owned(),
            "{\"seq\":1,\"op\":\"update\",\"issue_id\":\"x\",\"issue\":null}\n".to_owned(),
        ] {
            let state = std::sync::Mutex::new(EventState::default());
            read_event_output(input.as_bytes(), &state);
            assert!(matches!(
                state.lock().unwrap().end,
                Some(FeedEnd::Failed(_))
            ));
            assert_eq!(state.lock().unwrap().cursor, 0);
        }
    }

    #[test]
    fn unsupported_and_disabled_feeds_are_normal_fallbacks() {
        for text in [
            "Error: unknown command \"events\" for \"bd\"\n",
            "note: the events journal is disabled for this workspace\n",
        ] {
            let state = std::sync::Mutex::new(EventState::default());
            read_event_errors(text.as_bytes(), &state);
            assert!(matches!(
                state.lock().unwrap().end,
                Some(FeedEnd::Unavailable)
            ));
        }
        let state = std::sync::Mutex::new(EventState::default());
        read_event_output(b"".as_slice(), &state);
        assert!(!state.lock().unwrap().update.changed);
        assert_eq!(state.lock().unwrap().cursor, 0);
    }

    #[cfg(unix)]
    #[test]
    fn truncated_child_restarts_at_head_and_recovers() {
        use std::time::{Duration, Instant};
        let monitor = EventMonitor::spawn(|cursor| {
            let mut command = Command::new("sh");
            if cursor == 0 {
                command.args(["-c", "printf '{\n\"code\":\"events_journal_truncated\",\n\"head\":42\n}\n'"]);
            } else {
                assert_eq!(cursor, 42);
                command.args(["-c", "printf '%s\n' '{\"seq\":43,\"op\":\"update\",\"issue_id\":\"x\",\"issue\":{\"id\":\"x\"}}'; exec sleep 60"]);
            }
            command
        }, Duration::from_millis(10)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while monitor.state.lock().unwrap().cursor != 43 {
            assert!(
                Instant::now() < deadline,
                "feed did not resume from truncation head"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let update = monitor.poll();
        assert!(update.changed);
        assert!(update.error.is_none());
        drop(monitor);
    }

    #[cfg(unix)]
    #[test]
    fn event_child_is_reaped_on_drop_and_failing_children_back_off() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
            time::{Duration, Instant},
        };
        // Shell exists only as a controlled test child. Production directly execs bd.
        let path = std::env::temp_dir().join(format!("btui-event-child-{}", std::process::id()));
        let child_path = path.clone();
        let monitor = EventMonitor::spawn(move |_| {
            let mut command = Command::new("sh");
            command.args(["-c", "echo $$ > \"$1\"; printf '%s\\n' '{\"seq\":1,\"op\":\"update\",\"issue_id\":\"x\",\"issue\":{\"id\":\"x\"}}'; exec sleep 60", "fixture"]);
            command.arg(&child_path);
            command
        }, Duration::from_secs(30)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !monitor.poll().changed {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid = std::fs::read_to_string(&path).unwrap();
        let before = Instant::now();
        drop(monitor);
        assert!(before.elapsed() < Duration::from_secs(2));
        let alive = Command::new("kill")
            .args(["-0", pid.trim()])
            .output()
            .unwrap();
        assert!(!alive.status.success(), "event child survived monitor drop");
        std::fs::remove_file(path).unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);
        let monitor = EventMonitor::spawn(
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                let mut command = Command::new("sh");
                command.args([
                    "-c",
                    "while :; do printf 'diagnostic pressure\\n' >&2; done",
                ]);
                command
            },
            Duration::from_secs(30),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while monitor.poll().error.is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let before = Instant::now();
        drop(monitor);
        assert!(before.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn list_uses_readonly_json_command_and_parses_fixture() {
        let runner = FakeRunner::succeeds(LIST_FIXTURE);

        let issues = list_with(&runner, ListOptions::default()).unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].id, "btui-1");
        assert_eq!(issues[0].labels, ["ui"]);
        assert_eq!(issues[0].dependency_count, 1);
        assert!(issues[0].dependencies.is_empty());
        assert_eq!(issues[0].comment_count, 0);
        assert_eq!(issues[1].description, "");
        assert_eq!(
            runner.calls.into_inner(),
            [[
                "--readonly",
                "list",
                "--json",
                "--limit",
                "0",
                "--status",
                "open,in_progress,blocked",
                "--sort",
                "priority"
            ]]
        );
    }

    #[test]
    fn show_uses_safe_id_argument_and_parses_fixture() {
        let runner = FakeRunner::succeeds(SHOW_FIXTURE);

        let issue = show_with(&runner, "btui--flag-like").unwrap();

        assert_eq!(issue.title, "Polish the browser");
        assert_eq!(issue.acceptance_criteria, "Navigation remains readable.");
        assert_eq!(issue.dependencies[0].id, "btui-0");
        assert_eq!(issue.dependents[0].id, "btui-2");
        assert_eq!(issue.comments[0].text, "This needs to stay readable.");
        assert_eq!(
            runner.calls.into_inner(),
            [[
                "--readonly",
                "show",
                "--id=btui--flag-like",
                "--json",
                "--include-comments",
                "--include-dependents"
            ]]
        );
    }

    #[test]
    fn revision_uses_supported_readonly_version_control_status() {
        let runner =
            FakeRunner::succeeds(br#"{"branch":"main","commit":"abc123","schema_version":1}"#);

        let revision = revision_with(&runner).unwrap();

        assert_eq!(revision, "main:abc123");
        assert_eq!(
            runner.calls.into_inner(),
            [["--readonly", "vc", "status", "--json"]]
        );
    }

    #[test]
    fn missing_executable_has_actionable_context() {
        let runner = FakeRunner::returns(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No such file or directory",
        )));

        let error = list_with(&runner, ListOptions::default())
            .unwrap_err()
            .to_string();

        assert!(error.contains("could not start bd"));
        assert!(error.contains("on PATH"));
    }

    #[test]
    fn workspace_error_preserves_bd_diagnostic() {
        let runner = FakeRunner::exits_with(
            "Error: No active beads workspace found.\nHint: run 'bd init' to create a new database",
        );

        let error = list_with(&runner, ListOptions::default())
            .unwrap_err()
            .to_string();

        assert!(error.contains("No active beads workspace found"));
        assert!(error.contains("bd init"));
    }

    #[test]
    fn nonzero_exit_without_stderr_is_still_explained() {
        let runner = FakeRunner::exits_with("");

        let error = list_with(&runner, ListOptions::default())
            .unwrap_err()
            .to_string();

        assert_eq!(error, "bd failed without a diagnostic");
    }

    #[test]
    fn empty_show_output_is_rejected() {
        let runner = FakeRunner::succeeds(b"[]");

        let error = show_with(&runner, "btui-1").unwrap_err().to_string();

        assert_eq!(error, "bd show returned unexpected JSON");
    }

    #[test]
    fn missing_required_identity_field_is_rejected() {
        let malformed =
            br#"[{"title":"No identity","status":"open","priority":2,"issue_type":"task"}]"#;

        let error = parse_issues(malformed).unwrap_err().to_string();

        assert!(error.contains("missing field `id`"));
    }

    #[test]
    fn ready_and_recently_updated_options_become_bd_flags() {
        let runner = FakeRunner::succeeds(b"[]");

        list_with(
            &runner,
            ListOptions {
                view: ListView::Ready,
                sort: ListSort::Updated,
            },
        )
        .unwrap();

        assert_eq!(
            runner.calls.into_inner(),
            [[
                "--readonly",
                "list",
                "--json",
                "--limit",
                "0",
                "--ready",
                "--sort",
                "updated",
                "--reverse"
            ]]
        );
    }

    #[test]
    fn closed_view_is_requested_explicitly() {
        let runner = FakeRunner::succeeds(b"[]");

        list_with(
            &runner,
            ListOptions {
                view: ListView::Closed,
                sort: ListSort::Created,
            },
        )
        .unwrap();

        assert_eq!(
            runner.calls.into_inner(),
            [[
                "--readonly",
                "list",
                "--json",
                "--limit",
                "0",
                "--status",
                "closed",
                "--sort",
                "created",
                "--reverse"
            ]]
        );
    }
}
