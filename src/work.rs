use std::{
    env, io,
    path::PathBuf,
    process::{Command, ExitStatus, Output},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{agent::AgentKind, bd::Issue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkRequest {
    pub issue_id: String,
    pub repository: PathBuf,
    pub branch_name: String,
    pub prompt: String,
}

impl WorkRequest {
    pub fn for_issue(issue: &Issue, repository: PathBuf) -> Result<Self, String> {
        match issue.status.as_str() {
            "closed" => return Err(format!("{} is closed and cannot be started", issue.id)),
            "blocked" => {
                return Err(format!(
                    "{} is blocked; resolve its dependencies before starting work",
                    issue.id
                ));
            }
            "deferred" => return Err(format!("{} is deferred and cannot be started", issue.id)),
            _ => {}
        }

        Ok(Self {
            issue_id: issue.id.clone(),
            repository,
            branch_name: branch_name(&issue.id, &issue.title),
            prompt: prompt_for(&issue.id),
        })
    }
}

pub fn prompt_for(issue_id: &str) -> String {
    format!(
        "Begin implementation of Beads task {issue_id} in this repository.\n\n\
         Before changing files:\n\
         1. Read AGENTS.md and any repository instructions.\n\
         2. Run `bd prime` and `bd show {issue_id}`. Read its dependencies, comments, and acceptance criteria, and confirm it is ready.\n\
         3. If it is blocked, stop and report the blocker instead of implementing it.\n\
         4. Claim it with `bd update {issue_id} --claim`.\n\
         5. Create or switch to a focused feature branch unless the repository workflow says otherwise. If you started in a detached managed worktree, create the branch in that worktree before committing.\n\n\
         Then implement the task, run the repository's validation, and update or close the Beads task as appropriate. Keep Beads canonical; do not rely on a task-description copy in this prompt."
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchTarget {
    Foreground,
    Herdr { workspace_id: String },
}

impl LaunchTarget {
    pub fn detect() -> Self {
        Self::from_environment(|key| env::var(key).ok())
    }

    fn from_environment(get: impl Fn(&str) -> Option<String>) -> Self {
        if get("HERDR_ENV").as_deref() == Some("1")
            && let Some(workspace_id) = get("HERDR_WORKSPACE_ID").filter(|id| !id.is_empty())
        {
            return Self::Herdr { workspace_id };
        }
        Self::Foreground
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl CommandSpec {
    fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
        }
    }

    fn in_directory(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

pub trait CommandRunner {
    fn output(&self, command: &CommandSpec) -> io::Result<Output>;
    fn status(&self, command: &CommandSpec) -> io::Result<ExitStatus>;
}

pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn output(&self, command: &CommandSpec) -> io::Result<Output> {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        process.output()
    }

    fn status(&self, command: &CommandSpec) -> io::Result<ExitStatus> {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        process.status()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchReport {
    pub message: String,
    pub warning: Option<String>,
}

pub struct WorkLauncher<R = ProcessRunner> {
    runner: R,
}

impl Default for WorkLauncher<ProcessRunner> {
    fn default() -> Self {
        Self {
            runner: ProcessRunner,
        }
    }
}

impl<R: CommandRunner> WorkLauncher<R> {
    #[cfg(test)]
    fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    pub fn launch_foreground(
        &self,
        request: &WorkRequest,
        agent: AgentKind,
    ) -> Result<LaunchReport> {
        let command = foreground_command(request, agent);
        let status = self.runner.status(&command).with_context(|| {
            format!(
                "could not start {}; install `{}` and ensure it is on PATH",
                agent.display_name(),
                agent.executable()
            )
        })?;
        if !status.success() {
            bail!(
                "{} exited with {}; press w to retry or q to quit",
                agent.display_name(),
                display_status(status)
            );
        }
        Ok(LaunchReport {
            message: format!(
                "{} returned from {} · refreshing Beads",
                agent.display_name(),
                request.issue_id
            ),
            warning: None,
        })
    }

    pub fn launch_herdr(
        &self,
        request: &WorkRequest,
        agent: AgentKind,
        workspace_id: &str,
    ) -> Result<LaunchReport> {
        let create = CommandSpec::new(
            "herdr",
            [
                "worktree",
                "create",
                "--workspace",
                workspace_id,
                "--branch",
                &request.branch_name,
                "--base",
                "HEAD",
                "--label",
                &format!("{}: {}", agent.display_name(), request.issue_id),
                "--no-focus",
            ],
        );
        let created: HerdrWorktreeResponse = self
            .run_json(&create)
            .context("could not create isolated Herdr worktree")?;
        let created_workspace_id = created.result.workspace.workspace_id;
        let pane_id = created.result.root_pane.pane_id;
        let agent_name = agent_name(agent, &request.issue_id, std::process::id(), &pane_id);

        let start = CommandSpec::new(
            "herdr",
            [
                "agent",
                "start",
                &agent_name,
                "--kind",
                agent.herdr_kind(),
                "--pane",
                &pane_id,
            ],
        );
        if let Err(error) = self
            .run_success(&start)
            .with_context(|| format!("could not start {} in Herdr", agent.display_name()))
        {
            match self.agent_info(&agent_name) {
                Ok(info) if info.pane_id == pane_id && info.agent_status == "blocked" => {
                    self.focus_and_wait_for_confirmation(&agent_name)
                        .context("the agent is still waiting for startup confirmation; its Herdr worktree was preserved")?;
                }
                Ok(info) if info.pane_id == pane_id => {
                    return Err(error.context(format!(
                        "the agent is present with status {}; its Herdr worktree was preserved",
                        info.agent_status
                    )));
                }
                _ => return Err(self.cleanup_failed_worktree(&created_workspace_id, error)),
            }
        }

        let prompt = CommandSpec::new(
            "herdr",
            [
                "agent",
                "prompt",
                &agent_name,
                &request.prompt,
                "--wait",
                "--timeout",
                "5000",
            ],
        );
        if let Err(error) = self.run_success(&prompt).with_context(|| {
            format!(
                "could not submit the Beads prompt to {}",
                agent.display_name()
            )
        }) {
            match self.agent_info(&agent_name) {
                Ok(info) if info.pane_id == pane_id && info.agent_status == "blocked" => {
                    self.focus_and_wait_for_confirmation(&agent_name).context(
                        "the agent is still waiting for confirmation; its Herdr worktree was preserved",
                    )?;
                    if let Err(error) = self.run_success(&prompt).context(
                        "could not confirm prompt activity after confirmation; the live Herdr worktree was preserved",
                    ) {
                        match self.agent_info(&agent_name) {
                            Ok(info)
                                if info.pane_id == pane_id
                                    && info.agent_status == "working" => {}
                            _ => return Err(error),
                        }
                    }
                }
                Ok(info) if info.pane_id == pane_id && info.agent_status == "working" => {}
                Ok(info) if info.pane_id == pane_id => {
                    return Err(error.context(format!(
                        "the agent is present with status {}; its Herdr worktree was preserved",
                        info.agent_status
                    )));
                }
                _ => {
                    return Err(
                        error.context("the live Herdr worktree was preserved for inspection")
                    );
                }
            }
        }

        let focus = CommandSpec::new("herdr", ["agent", "focus", &agent_name]);
        let warning = self.run_success(&focus).err().map(|error| {
            format!(
                "{} started, but Herdr could not focus it: {error:#}",
                agent.display_name()
            )
        });

        Ok(LaunchReport {
            message: format!(
                "Started {} for {} in Herdr",
                agent.display_name(),
                request.issue_id
            ),
            warning,
        })
    }

    fn run_success(&self, command: &CommandSpec) -> Result<Vec<u8>> {
        let output = self
            .runner
            .output(command)
            .with_context(|| format!("could not start `{}`", command.program))?;
        if !output.status.success() {
            let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if diagnostic.is_empty() {
                bail!("`{}` failed without a diagnostic", command.program);
            }
            bail!("{diagnostic}");
        }
        Ok(output.stdout)
    }

    fn run_json<T: for<'de> Deserialize<'de>>(&self, command: &CommandSpec) -> Result<T> {
        let output = self.run_success(command)?;
        serde_json::from_slice(&output).context("Herdr returned unexpected JSON")
    }

    fn agent_info(&self, agent_name: &str) -> Result<HerdrAgent> {
        let get = CommandSpec::new("herdr", ["agent", "get", agent_name]);
        let response: HerdrAgentResponse = self.run_json(&get)?;
        Ok(response.result.agent)
    }

    fn focus_and_wait_for_confirmation(&self, agent_name: &str) -> Result<()> {
        let focus = CommandSpec::new("herdr", ["agent", "focus", agent_name]);
        self.run_success(&focus)
            .context("could not focus the agent confirmation screen")?;
        let wait = CommandSpec::new(
            "herdr",
            [
                "agent",
                "wait",
                agent_name,
                "--until",
                "idle",
                "--until",
                "done",
                "--timeout",
                "300000",
            ],
        );
        self.run_success(&wait)
            .context("confirmation was not completed within five minutes")?;
        Ok(())
    }

    fn cleanup_failed_worktree(&self, workspace_id: &str, error: anyhow::Error) -> anyhow::Error {
        let close = CommandSpec::new("herdr", ["worktree", "remove", "--workspace", workspace_id]);
        match self.run_success(&close) {
            Ok(_) => error.context(
                "the incomplete Herdr worktree was removed; press Shift-W to retry here",
            ),
            Err(cleanup) => error.context(format!(
                "also could not remove incomplete Herdr worktree {workspace_id}: {cleanup:#}; press Shift-W to retry here"
            )),
        }
    }
}

pub fn foreground_command(request: &WorkRequest, agent: AgentKind) -> CommandSpec {
    CommandSpec::new(agent.executable(), [&request.prompt]).in_directory(&request.repository)
}

fn branch_name(issue_id: &str, title: &str) -> String {
    let slug = slug(&format!("{issue_id}-{title}"));
    format!("work/{}", &slug[..slug.len().min(56)])
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn agent_name(agent: AgentKind, issue_id: &str, process_id: u32, pane_id: &str) -> String {
    let issue_slug = slug(issue_id);
    let pane_slug = slug(pane_id);
    let suffix = format!("-{process_id}-{pane_slug}");
    let prefix = format!("btui-{}-", agent.id());
    let available = 32usize.saturating_sub(prefix.len() + suffix.len());
    format!(
        "{prefix}{}{}",
        &issue_slug[..issue_slug.len().min(available)],
        suffix
    )
}

fn display_status(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "a signal".to_owned(), |code| format!("status {code}"))
}

#[derive(Deserialize)]
struct HerdrWorktreeResponse {
    result: HerdrWorktreeResult,
}

#[derive(Deserialize)]
struct HerdrWorktreeResult {
    workspace: HerdrWorkspace,
    root_pane: HerdrPane,
}

#[derive(Deserialize)]
struct HerdrWorkspace {
    workspace_id: String,
}

#[derive(Deserialize)]
struct HerdrPane {
    pane_id: String,
}

#[derive(Deserialize)]
struct HerdrAgentResponse {
    result: HerdrAgentResult,
}

#[derive(Deserialize)]
struct HerdrAgentResult {
    agent: HerdrAgent,
}

#[derive(Deserialize)]
struct HerdrAgent {
    agent_status: String,
    pane_id: String,
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, os::unix::process::ExitStatusExt, sync::Mutex};

    use super::*;

    struct FakeRunner {
        outputs: Mutex<VecDeque<io::Result<Output>>>,
        statuses: Mutex<VecDeque<io::Result<ExitStatus>>>,
        commands: Mutex<Vec<CommandSpec>>,
    }

    impl FakeRunner {
        fn new(outputs: Vec<Output>, statuses: Vec<ExitStatus>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().map(Ok).collect()),
                statuses: Mutex::new(statuses.into_iter().map(Ok).collect()),
                commands: Mutex::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn output(&self, command: &CommandSpec) -> io::Result<Output> {
            self.commands.lock().unwrap().push(command.clone());
            self.outputs.lock().unwrap().pop_front().unwrap()
        }

        fn status(&self, command: &CommandSpec) -> io::Result<ExitStatus> {
            self.commands.lock().unwrap().push(command.clone());
            self.statuses.lock().unwrap().pop_front().unwrap()
        }
    }

    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn request() -> WorkRequest {
        WorkRequest {
            issue_id: "btui-abc.1".to_owned(),
            repository: PathBuf::from("/work/repo"),
            branch_name: "work/btui-abc-1-launch-work".to_owned(),
            prompt: prompt_for("btui-abc.1"),
        }
    }

    fn issue_with_status(status: &str) -> Issue {
        Issue {
            id: "btui-abc.1".to_owned(),
            title: "Launch work".to_owned(),
            description: "Must not enter prompt".to_owned(),
            design: String::new(),
            acceptance_criteria: String::new(),
            status: status.to_owned(),
            priority: 1,
            issue_type: "task".to_owned(),
            assignee: String::new(),
            labels: Vec::new(),
            dependency_count: 0,
            dependent_count: 0,
            comment_count: 0,
            updated_at: String::new(),
            created_at: String::new(),
            closed_at: String::new(),
            owner: String::new(),
            notes: String::new(),
            dependencies: Vec::new(),
            dependents: Vec::new(),
            comments: Vec::new(),
        }
    }

    #[test]
    fn request_keeps_beads_canonical_and_rejects_ineligible_statuses() {
        let request = WorkRequest::for_issue(&issue_with_status("open"), "/repo".into()).unwrap();
        assert!(request.prompt.contains("bd show btui-abc.1"));
        assert!(request.prompt.contains("AGENTS.md"));
        assert!(request.prompt.contains("bd update btui-abc.1 --claim"));
        assert!(!request.prompt.contains("Must not enter prompt"));

        for status in ["blocked", "closed", "deferred"] {
            assert!(WorkRequest::for_issue(&issue_with_status(status), "/repo".into()).is_err());
        }
    }

    #[test]
    fn launch_target_requires_complete_herdr_context() {
        let target = LaunchTarget::from_environment(|key| match key {
            "HERDR_ENV" => Some("1".to_owned()),
            "HERDR_WORKSPACE_ID" => Some("w7".to_owned()),
            _ => None,
        });
        assert_eq!(
            target,
            LaunchTarget::Herdr {
                workspace_id: "w7".to_owned()
            }
        );
        assert_eq!(
            LaunchTarget::from_environment(|_| None),
            LaunchTarget::Foreground
        );
    }

    #[test]
    fn foreground_launch_passes_cwd_and_prompt_without_a_shell() {
        let runner = FakeRunner::new(Vec::new(), vec![ExitStatus::from_raw(0)]);
        let launcher = WorkLauncher::with_runner(runner);
        launcher
            .launch_foreground(&request(), AgentKind::Codex)
            .unwrap();
        assert_eq!(
            launcher.runner.commands.lock().unwrap().as_slice(),
            [CommandSpec::new("codex", [request().prompt.as_str()]).in_directory("/work/repo")]
        );

        let claude = foreground_command(&request(), AgentKind::Claude);
        assert_eq!(claude.program, "claude");
        assert_eq!(claude.cwd, Some(PathBuf::from("/work/repo")));
    }

    #[test]
    fn herdr_launch_creates_starts_prompts_and_focuses() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);
        let report = launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap();
        let commands = launcher.runner.commands.lock().unwrap();

        assert_eq!(commands.len(), 4);
        assert_eq!(
            commands[0].args[0..4],
            ["worktree", "create", "--workspace", "w7"]
        );
        assert!(
            commands[0]
                .args
                .windows(2)
                .any(|args| args == ["--branch", "work/btui-abc-1-launch-work"])
        );
        assert_eq!(commands[1].args[0..2], ["agent", "start"]);
        assert!(
            commands[1]
                .args
                .windows(2)
                .any(|args| args == ["--kind", "codex"])
        );
        assert_eq!(commands[2].args[0..2], ["agent", "prompt"]);
        assert_eq!(commands[3].args[0..2], ["agent", "focus"]);
        assert!(commands[2].args[3].contains("bd show btui-abc.1"));
        assert!(commands[2].args.iter().any(|arg| arg == "--wait"));
        assert_eq!(report.warning, None);
    }

    #[test]
    fn herdr_uses_the_selected_executor_kind_and_shared_prompt() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        let report = launcher
            .launch_herdr(&request(), AgentKind::Claude, "w7")
            .unwrap();
        let commands = launcher.runner.commands.lock().unwrap();

        assert!(
            commands[1]
                .args
                .windows(2)
                .any(|args| args == ["--kind", "claude"])
        );
        assert_eq!(commands[2].args.get(3), Some(&request().prompt));
        assert!(report.message.contains("Claude Code"));
    }

    #[test]
    fn prompt_timeout_is_success_when_the_agent_is_observed_working() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let working = r#"{"result":{"agent":{"agent_status":"working","pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(0, "{}", ""),
                output(1, "", "timeout"),
                output(0, working, ""),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        let report = launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap();

        assert!(report.message.contains("Started Codex"));
    }

    #[test]
    fn failed_herdr_start_removes_the_created_worktree() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(1, "", "agent unavailable"),
                output(1, "", "agent not found"),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);
        let error = launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap_err();
        let commands = launcher.runner.commands.lock().unwrap();

        assert!(format!("{error:#}").contains("incomplete Herdr worktree was removed"));
        assert_eq!(
            commands.last().unwrap().args,
            ["worktree", "remove", "--workspace", "w9"]
        );
    }

    #[test]
    fn blocked_herdr_start_focuses_confirmation_then_submits_prompt() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let blocked = r#"{"result":{"agent":{"agent_status":"blocked","pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(1, "", "agent_not_ready"),
                output(0, blocked, ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap();
        let commands = launcher.runner.commands.lock().unwrap();

        assert_eq!(commands[2].args[0..2], ["agent", "get"]);
        assert_eq!(commands[3].args[0..2], ["agent", "focus"]);
        assert_eq!(commands[4].args[0..2], ["agent", "wait"]);
        assert!(commands[4].args.iter().any(|arg| arg == "300000"));
        assert_eq!(commands[5].args[0..2], ["agent", "prompt"]);
        assert!(!commands.iter().any(|command| {
            command.args.first().is_some_and(|arg| arg == "worktree")
                && command.args.get(1).is_some_and(|arg| arg == "remove")
        }));
    }

    #[test]
    fn blocked_prompt_is_retried_once_after_confirmation() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let blocked = r#"{"result":{"agent":{"agent_status":"blocked","pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(0, "{}", ""),
                output(1, "", "agent_blocked"),
                output(0, blocked, ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap();
        let commands = launcher.runner.commands.lock().unwrap();
        let prompt_count = commands
            .iter()
            .filter(|command| command.args.get(1).is_some_and(|arg| arg == "prompt"))
            .count();

        assert_eq!(prompt_count, 2);
        assert!(!commands.iter().any(|command| {
            command.args.first().is_some_and(|arg| arg == "worktree")
                && command.args.get(1).is_some_and(|arg| arg == "remove")
        }));
    }

    #[test]
    fn confirmation_timeout_preserves_the_live_agent_worktree() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let blocked = r#"{"result":{"agent":{"agent_status":"blocked","pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(1, "", "agent_not_ready"),
                output(0, blocked, ""),
                output(0, "{}", ""),
                output(1, "", "timeout"),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        let error = launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap_err();
        let commands = launcher.runner.commands.lock().unwrap();

        assert!(format!("{error:#}").contains("Herdr worktree was preserved"));
        assert!(!commands.iter().any(|command| {
            command.args.first().is_some_and(|arg| arg == "worktree")
                && command.args.get(1).is_some_and(|arg| arg == "remove")
        }));
    }

    #[test]
    fn foreground_nonzero_exit_is_recoverable() {
        let runner = FakeRunner::new(Vec::new(), vec![ExitStatus::from_raw(7 << 8)]);
        let launcher = WorkLauncher::with_runner(runner);

        let error = launcher
            .launch_foreground(&request(), AgentKind::Codex)
            .unwrap_err();

        assert!(format!("{error:#}").contains("status 7"));
        assert!(format!("{error:#}").contains("press w to retry"));
    }

    #[test]
    fn focus_failure_keeps_the_started_agent_and_returns_a_warning() {
        let created =
            r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"pane_id":"w9:p1"}}}"#;
        let runner = FakeRunner::new(
            vec![
                output(0, created, ""),
                output(0, "{}", ""),
                output(0, "{}", ""),
                output(1, "", "focus unavailable"),
            ],
            Vec::new(),
        );
        let launcher = WorkLauncher::with_runner(runner);

        let report = launcher
            .launch_herdr(&request(), AgentKind::Codex, "w7")
            .unwrap();
        let commands = launcher.runner.commands.lock().unwrap();

        assert!(report.warning.unwrap().contains("focus unavailable"));
        assert_eq!(commands.len(), 4);
        assert!(!commands.iter().any(|command| {
            command.args.first().is_some_and(|arg| arg == "worktree")
                && command.args.get(1).is_some_and(|arg| arg == "remove")
        }));
    }

    #[test]
    fn generated_agent_names_are_valid_and_bounded() {
        let name = agent_name(
            AgentKind::Codex,
            "BTUI/very.long.issue.identifier.with.symbols",
            4242,
            "w7:p99",
        );

        assert!(name.starts_with("btui-"));
        assert!(name.len() <= 32);
        assert!(name.chars().all(|character| character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'));
    }

    #[test]
    fn generated_branch_names_are_readable_and_bounded() {
        let branch = branch_name(
            "BTUI/u9v.10",
            "Expose Start work agent shifter and complete acceptance!",
        );

        assert!(branch.starts_with("work/btui-u9v-10-expose-start-work"));
        assert!(branch.len() <= 61);
        assert!(!branch.contains(' '));
    }
}
