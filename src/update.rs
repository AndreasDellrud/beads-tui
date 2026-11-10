//! Best-effort release discovery. Network work never owns application state.
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use semver::Version;
use serde::Deserialize;

const ENDPOINT: &str = "https://api.github.com/repos/AndreasDellrud/beads-tui/releases/latest";
const RELEASES: &str = "https://github.com/AndreasDellrud/beads-tui/releases/tag/";
const TIMEOUT: Duration = Duration::from_secs(5);
const COOLDOWN: Duration = Duration::from_secs(15 * 60);
const BODY_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseNotice {
    pub installed: String,
    pub available: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Disabled,
    Checking,
    UpToDate,
    Available(ReleaseNotice),
    Failed(String),
}

pub struct UpdateChecker {
    state: UpdateState,
    pending: Option<Receiver<Result<UpdateState, String>>>,
    started: Option<Instant>,
    installed: String,
}

impl UpdateChecker {
    pub fn disabled() -> Self {
        Self {
            state: UpdateState::Disabled,
            pending: None,
            started: None,
            installed: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    pub fn start() -> Self {
        let mut checker = Self::disabled();
        if enabled(
            &checker.installed,
            std::env::var_os("BTUI_UPDATE_CHECK").as_deref(),
        ) {
            checker.state = UpdateState::UpToDate;
            checker.recheck();
        }
        checker
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn remaining_cooldown(&self) -> Duration {
        self.started
            .map_or(Duration::ZERO, |at| COOLDOWN.saturating_sub(at.elapsed()))
    }

    /// Returns false when disabled, already checking, or inside the attempt cooldown.
    pub fn recheck(&mut self) -> bool {
        if self.state == UpdateState::Disabled
            || self.pending.is_some()
            || !self.remaining_cooldown().is_zero()
        {
            return false;
        }
        let installed = self.installed.clone();
        self.spawn(move || fetch_release(&installed, ENDPOINT, TIMEOUT, true));
        true
    }

    fn spawn(&mut self, check: impl FnOnce() -> Result<UpdateState, String> + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        self.started = Some(Instant::now());
        self.state = UpdateState::Checking;
        match thread::Builder::new()
            .name("btui-release-check".into())
            .spawn(move || {
                let _ = sender.send(check());
            }) {
            Ok(_) => self.pending = Some(receiver),
            Err(_) => self.state = UpdateState::Failed("Could not start release check".into()),
        }
    }

    pub fn poll(&mut self) {
        self.poll_at(Instant::now());
    }

    fn poll_at(&mut self, now: Instant) {
        let Some(receiver) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Disconnected) => Err("Release check stopped unexpectedly".into()),
            Err(TryRecvError::Empty) => {
                if self
                    .started
                    .is_none_or(|at| now.saturating_duration_since(at) < TIMEOUT)
                {
                    return;
                }
                Err("Release check timed out".into())
            }
        };
        self.pending = None;
        self.state = result.unwrap_or_else(UpdateState::Failed);
    }

    #[cfg(test)]
    pub(crate) fn with_state(state: UpdateState) -> Self {
        Self {
            state,
            ..Self::disabled()
        }
    }
}

fn enabled(installed: &str, setting: Option<&std::ffi::OsStr>) -> bool {
    setting != Some(std::ffi::OsStr::new("0"))
        && Version::parse(installed).is_ok_and(|v| v.pre.is_empty() && v.build.is_empty())
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn release_state(installed: &str, body: &str) -> Result<UpdateState, String> {
    let release: Release =
        serde_json::from_str(body).map_err(|_| "Invalid release metadata".to_owned())?;
    let available = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "Invalid release version".to_owned())?;
    let current = Version::parse(installed).map_err(|_| "Invalid installed version".to_owned())?;
    if release.draft
        || release.prerelease
        || !available.pre.is_empty()
        || !available.build.is_empty()
        || available.cmp_precedence(&current).is_le()
    {
        return Ok(UpdateState::UpToDate);
    }
    Ok(UpdateState::Available(ReleaseNotice {
        installed: installed.into(),
        available: available.to_string(),
        // Construct from the validated tag instead of trusting a metadata-supplied URL.
        url: format!("{RELEASES}{}", release.tag_name),
    }))
}

fn fetch_release(
    installed: &str,
    endpoint: &str,
    timeout: Duration,
    https_only: bool,
) -> Result<UpdateState, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(https_only)
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(endpoint)
        .header("User-Agent", concat!("btui/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|error| match error {
            ureq::Error::Timeout(_) => "Release check timed out".to_owned(),
            _ => "Could not reach GitHub releases".to_owned(),
        })?;
    match response.status().as_u16() {
        200 => {}
        403 | 429 => return Err("GitHub release check rate limited or denied".into()),
        status => return Err(format!("GitHub release check returned HTTP {status}")),
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()
        .map_err(|_| {
            "Could not read release metadata (timeout, invalid text, or size limit)".to_owned()
        })?;
    release_state(installed, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn metadata(tag: &str, draft: bool, prerelease: bool) -> String {
        serde_json::json!({"tag_name": tag, "draft": draft, "prerelease": prerelease}).to_string()
    }

    #[test]
    fn numeric_versions_and_stable_channel_policy() {
        let newer = release_state("0.4.3", &metadata("v0.10.0", false, false)).unwrap();
        assert_eq!(
            newer,
            UpdateState::Available(ReleaseNotice {
                installed: "0.4.3".into(),
                available: "0.10.0".into(),
                url: format!("{RELEASES}v0.10.0"),
            })
        );
        for (tag, draft, prerelease) in [
            ("v0.4.3", false, false),
            ("0.3.9", false, false),
            ("v1.0.0", true, false),
            ("v1.0.0", false, true),
            ("1.0.0-rc.1", false, false),
            ("1.0.0+dev", false, false),
        ] {
            assert_eq!(
                release_state("0.4.3", &metadata(tag, draft, prerelease)),
                Ok(UpdateState::UpToDate)
            );
        }
        assert_eq!(
            release_state("2.0.0", &metadata("v1.0.0", false, false)),
            Ok(UpdateState::UpToDate)
        );
    }

    #[test]
    fn opt_out_and_development_builds_disable_network() {
        for installed in ["1.0.0-dev", "1.0.0+local", "invalid"] {
            assert!(!enabled(installed, None));
        }
        assert!(!enabled("1.0.0", Some(std::ffi::OsStr::new("0"))));
        assert!(enabled("1.0.0", None));
        let mut checker = UpdateChecker::disabled();
        assert!(!checker.recheck());
        checker.poll();
        assert_eq!(checker.state(), &UpdateState::Disabled);
    }

    #[test]
    fn rejects_malformed_and_unsafe_metadata() {
        for body in ["not json", "{}", r#"{"tag_name":"v1.0.0"}"#] {
            assert!(release_state("0.4.3", body).is_err());
        }
        for tag in [
            "latest",
            "v1.0.0\n",
            "v1.0.0/evil",
            "https://evil.invalid",
            "1.00.0",
        ] {
            assert!(release_state("0.4.3", &metadata(tag, false, false)).is_err());
        }
    }

    // Local HTTP exercises the real request and body handling without public-network dependencies.
    fn server(status: u16, body: String, delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..n]).contains("User-Agent: btui/")
                    || String::from_utf8_lossy(&request[..n]).contains("user-agent: btui/")
            );
            thread::sleep(delay);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        (url, worker)
    }

    #[test]
    fn http_success_rate_limit_server_error_and_malformed_body() {
        for (status, body, expected) in [
            (200, metadata("v0.5.0", false, false), None),
            (403, "".into(), Some("rate limited")),
            (429, "".into(), Some("rate limited")),
            (500, "".into(), Some("HTTP 500")),
            (302, "".into(), Some("HTTP 302")),
            (200, "bad json".into(), Some("Invalid release metadata")),
            (200, "x".repeat(BODY_LIMIT as usize + 1), Some("size limit")),
        ] {
            let (url, worker) = server(status, body, Duration::ZERO);
            let result = fetch_release("0.4.3", &url, Duration::from_secs(2), false);
            if let Some(expected) = expected {
                assert!(result.unwrap_err().contains(expected));
            } else {
                assert!(matches!(result.unwrap(), UpdateState::Available(_)));
            }
            worker.join().unwrap();
        }
    }

    #[test]
    fn network_timeout_and_offline_are_expected_failures() {
        let (url, worker) = server(200, "{}".into(), Duration::from_millis(200));
        assert!(
            fetch_release("0.4.3", &url, Duration::from_millis(30), false)
                .unwrap_err()
                .contains("timed out")
        );
        worker.join().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert_eq!(
            fetch_release("0.4.3", &url, Duration::from_secs(1), false).unwrap_err(),
            "Could not reach GitHub releases"
        );
    }

    #[test]
    fn nonblocking_poll_deadline_disconnect_and_cooldown() {
        let mut checker = UpdateChecker::with_state(UpdateState::UpToDate);
        // Keep the worker waiting until the test explicitly releases it.
        let (release, wait) = mpsc::channel();
        checker.spawn(move || {
            wait.recv().unwrap();
            Ok(UpdateState::UpToDate)
        });
        assert!(!checker.recheck());
        checker.poll();
        assert_eq!(checker.state(), &UpdateState::Checking);
        let started = checker.started.unwrap();
        checker.poll_at(started + TIMEOUT);
        assert_eq!(
            checker.state(),
            &UpdateState::Failed("Release check timed out".into())
        );
        release.send(()).unwrap();
        assert!(!checker.recheck());
        assert!(!checker.remaining_cooldown().is_zero());
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        checker.pending = Some(receiver);
        checker.poll();
        assert_eq!(
            checker.state(),
            &UpdateState::Failed("Release check stopped unexpectedly".into())
        );
        checker.started = Some(Instant::now() - COOLDOWN);
        assert_eq!(checker.remaining_cooldown(), Duration::ZERO);
    }

    #[test]
    fn completed_worker_delivers_failure_without_retries() {
        let mut checker = UpdateChecker::with_state(UpdateState::Checking);
        let (sender, receiver) = mpsc::channel();
        sender.send(Err("offline".into())).unwrap();
        checker.pending = Some(receiver);
        checker.poll();
        assert_eq!(checker.state(), &UpdateState::Failed("offline".into()));
        checker.poll();
        assert_eq!(checker.state(), &UpdateState::Failed("offline".into()));
        assert!(checker.pending.is_none());
    }
}
