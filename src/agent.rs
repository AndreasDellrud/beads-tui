use std::{env, fs, io, os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentKind {
    Codex,
    Claude,
}

impl AgentKind {
    pub const ALL: [Self; 2] = [Self::Codex, Self::Claude];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }

    pub const fn executable(self) -> &'static str {
        self.id()
    }

    pub const fn herdr_kind(self) -> &'static str {
        self.id()
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|agent| agent.id() == id)
    }
}

#[derive(Debug)]
pub struct AgentSelection {
    available: Vec<AgentKind>,
    selected: Option<usize>,
    config_path: Option<PathBuf>,
    diagnostic: Option<String>,
}

impl AgentSelection {
    pub fn load() -> Self {
        let available = detect_available(env::var_os("PATH").as_deref());
        let config_path = config_path(
            env::var_os("XDG_CONFIG_HOME").as_deref(),
            env::var_os("HOME").as_deref(),
        );
        Self::from_parts(available, config_path)
    }

    fn from_parts(available: Vec<AgentKind>, config_path: Option<PathBuf>) -> Self {
        let mut diagnostic = None;
        let saved = config_path
            .as_deref()
            .and_then(|path| match fs::read_to_string(path) {
                Ok(contents) => match parse_config(&contents) {
                    Ok(agent) => Some(agent),
                    Err(error) => {
                        diagnostic = Some(format!("Ignoring {}: {error}", path.display()));
                        None
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => {
                    diagnostic = Some(format!("Could not read {}: {error}", path.display()));
                    None
                }
            });
        let selected = saved.and_then(|saved| {
            available
                .iter()
                .position(|agent| *agent == saved)
                .or_else(|| {
                    diagnostic = Some(format!(
                        "Saved agent {} is unavailable; using {}",
                        saved.display_name(),
                        available
                            .first()
                            .map_or("no agent", |agent| agent.display_name())
                    ));
                    None
                })
        });

        Self {
            selected: selected.or_else(|| (!available.is_empty()).then_some(0)),
            available,
            config_path,
            diagnostic,
        }
    }

    pub fn selected(&self) -> Option<AgentKind> {
        self.selected.map(|index| self.available[index])
    }

    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    pub fn take_diagnostic(&mut self) -> Option<String> {
        self.diagnostic.take()
    }

    pub fn next_agent(&mut self) -> io::Result<Option<AgentKind>> {
        self.shift(1)
    }

    pub fn previous_agent(&mut self) -> io::Result<Option<AgentKind>> {
        self.shift(-1)
    }

    fn shift(&mut self, delta: isize) -> io::Result<Option<AgentKind>> {
        let Some(current) = self.selected else {
            return Ok(None);
        };
        let len = self.available.len() as isize;
        self.selected = Some((current as isize + delta).rem_euclid(len) as usize);
        if let Err(error) = self.persist() {
            self.selected = Some(current);
            return Err(error);
        }
        Ok(self.selected())
    }

    fn persist(&self) -> io::Result<()> {
        let (Some(path), Some(agent)) = (&self.config_path, self.selected()) else {
            return Ok(());
        };
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "agent config has no parent directory",
            )
        })?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, serialize_config(agent))?;
        fs::rename(temporary, path)
    }
}

fn detect_available(path: Option<&std::ffi::OsStr>) -> Vec<AgentKind> {
    AgentKind::ALL
        .into_iter()
        .filter(|agent| executable_on_path(agent.executable(), path))
        .collect()
}

fn executable_on_path(executable: &str, path: Option<&std::ffi::OsStr>) -> bool {
    path.into_iter()
        .flat_map(env::split_paths)
        .map(|directory| directory.join(executable))
        .any(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn config_path(xdg: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    xdg.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(PathBuf::from).map(|path| path.join(".config")))
        .map(|base| base.join("btui/config.toml"))
}

fn parse_config(contents: &str) -> Result<AgentKind, String> {
    let mut default = None;
    for line in contents.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| "expected `default_agent = \"...\"`".to_owned())?;
        if key.trim() != "default_agent" || default.is_some() {
            return Err("expected one `default_agent` setting".to_owned());
        }
        let value = value.trim();
        let id = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| "default_agent must be a quoted string".to_owned())?;
        default = AgentKind::from_id(id);
        if default.is_none() {
            return Err(format!("unsupported agent `{id}`"));
        }
    }
    default.ok_or_else(|| "missing `default_agent` setting".to_owned())
}

fn serialize_config(agent: AgentKind) -> String {
    format!("default_agent = \"{}\"\n", agent.id())
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsStr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    fn temporary_directory() -> PathBuf {
        env::temp_dir().join(format!(
            "btui-agent-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn config_round_trips_supported_agents() {
        for agent in AgentKind::ALL {
            assert_eq!(parse_config(&serialize_config(agent)), Ok(agent));
        }
        assert!(parse_config("default_agent = \"shell\"").is_err());
        assert!(parse_config("not toml").is_err());
    }

    #[test]
    fn config_path_prefers_xdg_and_falls_back_to_home() {
        assert_eq!(
            config_path(Some(OsStr::new("/xdg")), Some(OsStr::new("/home/a"))),
            Some(PathBuf::from("/xdg/btui/config.toml"))
        );
        assert_eq!(
            config_path(None, Some(OsStr::new("/home/a"))),
            Some(PathBuf::from("/home/a/.config/btui/config.toml"))
        );
    }

    #[test]
    fn detection_and_cycling_are_deterministic_and_persistent() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("codex"), "").unwrap();
        fs::write(directory.join("claude"), "").unwrap();
        for executable in ["codex", "claude"] {
            let mut permissions = fs::metadata(directory.join(executable))
                .unwrap()
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(directory.join(executable), permissions).unwrap();
        }
        assert_eq!(
            detect_available(Some(directory.as_os_str())),
            vec![AgentKind::Codex, AgentKind::Claude]
        );

        let config = directory.join("config/btui/config.toml");
        let mut selection =
            AgentSelection::from_parts(AgentKind::ALL.to_vec(), Some(config.clone()));
        assert_eq!(selection.selected(), Some(AgentKind::Codex));
        assert_eq!(selection.next_agent().unwrap(), Some(AgentKind::Claude));
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            serialize_config(AgentKind::Claude)
        );
        assert_eq!(selection.previous_agent().unwrap(), Some(AgentKind::Codex));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unavailable_saved_agent_falls_back_with_diagnostic() {
        let directory = temporary_directory();
        let config = directory.join("btui/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, serialize_config(AgentKind::Claude)).unwrap();
        let mut selection = AgentSelection::from_parts(vec![AgentKind::Codex], Some(config));

        assert_eq!(selection.selected(), Some(AgentKind::Codex));
        assert!(selection.take_diagnostic().unwrap().contains("unavailable"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_persistence_does_not_change_the_active_agent() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        let blocking_file = directory.join("not-a-directory");
        fs::write(&blocking_file, "").unwrap();
        let mut selection = AgentSelection::from_parts(
            AgentKind::ALL.to_vec(),
            Some(blocking_file.join("config.toml")),
        );

        assert!(selection.next_agent().is_err());
        assert_eq!(selection.selected(), Some(AgentKind::Codex));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn no_installed_agent_is_explicit_and_safe() {
        let mut selection = AgentSelection::from_parts(Vec::new(), None);
        assert_eq!(selection.selected(), None);
        assert_eq!(selection.next_agent().unwrap(), None);
    }
}
