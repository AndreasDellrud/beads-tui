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
