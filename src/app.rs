use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use ratatui::widgets::ListState;

use crate::bd::{CliSource, Issue, IssueSource, ListOptions, ListSort, ListView, RelatedIssue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Browser,
    Issue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollPane {
    IssueList,
    Preview,
    IssueBody,
    Relationships,
}

enum LoadRequest {
    List {
        generation: u64,
        options: ListOptions,
    },
    Detail {
        generation: u64,
        id: String,
    },
}

enum LoadResponse {
    List {
        generation: u64,
        result: Result<Vec<Issue>, String>,
    },
    Detail {
        generation: u64,
        id: String,
        result: Box<Result<Issue, String>>,
    },
    Prefetch {
        id: String,
        result: Box<Result<Issue, String>>,
    },
    Revision(Result<String, String>),
}

struct Loader {
    requests: Sender<LoadRequest>,
    responses: Receiver<LoadResponse>,
    prefetcher: Prefetcher,
    revision_requests: Sender<()>,
}

impl Loader {
    fn new(source: Box<dyn IssueSource>) -> Self {
        let source: Arc<dyn IssueSource> = Arc::from(source);
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let loader_source = Arc::clone(&source);
        let loader_responses = response_tx.clone();

        thread::Builder::new()
            .name("btui-bd-loader".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    match request {
                        LoadRequest::List {
                            generation,
                            options,
                        } => {
                            let result = loader_source
                                .list(options)
                                .map_err(|error| format!("{error:#}"));
                            if loader_responses
                                .send(LoadResponse::List { generation, result })
                                .is_err()
                            {
                                break;
                            }
                        }
                        LoadRequest::Detail { generation, id } => {
                            let result = loader_source
                                .show(&id)
                                .map_err(|error| format!("{error:#}"));
                            if loader_responses
                                .send(LoadResponse::Detail {
                                    generation,
                                    id,
                                    result: Box::new(result),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            })
            .expect("failed to start the bd loader thread");

        let (revision_tx, revision_rx) = mpsc::channel();
        let revision_source = Arc::clone(&source);
        let revision_responses = response_tx.clone();
        thread::Builder::new()
            .name("btui-bd-revision".to_owned())
            .spawn(move || {
                while revision_rx.recv().is_ok() {
                    let result = revision_source
                        .revision()
                        .map_err(|error| format!("{error:#}"));
                    if revision_responses
                        .send(LoadResponse::Revision(result))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to start the bd revision thread");

        Self {
            requests: request_tx,
            responses: response_rx,
            prefetcher: Prefetcher::new(source, response_tx),
            revision_requests: revision_tx,
        }
    }

    fn request_list(&self, generation: u64, options: ListOptions) -> Result<(), String> {
        self.requests
            .send(LoadRequest::List {
                generation,
                options,
            })
            .map_err(|_| "the bd loader stopped unexpectedly".to_owned())
    }

    fn request_detail(&self, generation: u64, id: String) -> Result<(), String> {
        self.requests
            .send(LoadRequest::Detail { generation, id })
            .map_err(|_| "the bd loader stopped unexpectedly".to_owned())
    }

    fn warm_details(&self, ids: Vec<String>) {
        self.prefetcher.replace_pending(ids);
    }

    fn prioritize_detail(&self, id: String) {
        self.prefetcher.prioritize(id);
    }

    fn request_revision(&self) -> Result<(), String> {
        self.revision_requests
            .send(())
            .map_err(|_| "the bd revision monitor stopped unexpectedly".to_owned())
    }
}

#[derive(Default)]
struct PrefetchState {
    pending: VecDeque<String>,
    in_flight: HashSet<String>,
    stopped: bool,
}

struct Prefetcher {
    state: Arc<(Mutex<PrefetchState>, Condvar)>,
}

impl Prefetcher {
    fn new(source: Arc<dyn IssueSource>, responses: Sender<LoadResponse>) -> Self {
        let state = Arc::new((Mutex::new(PrefetchState::default()), Condvar::new()));
        for worker in 0..2 {
            let worker_state = Arc::clone(&state);
            let worker_source = Arc::clone(&source);
            let worker_responses = responses.clone();
            thread::Builder::new()
                .name(format!("btui-bd-prefetch-{worker}"))
                .spawn(move || {
                    loop {
                        let id = {
                            let (lock, ready) = &*worker_state;
                            let mut state = lock.lock().expect("prefetch state poisoned");
                            while state.pending.is_empty() && !state.stopped {
                                state = ready.wait(state).expect("prefetch state poisoned");
                            }
                            if state.stopped {
                                break;
                            }
                            let id = state
                                .pending
                                .pop_front()
                                .expect("prefetch request disappeared");
                            state.in_flight.insert(id.clone());
                            id
                        };
                        let result = worker_source
                            .show(&id)
                            .map_err(|error| format!("{error:#}"));
                        {
                            let (lock, _) = &*worker_state;
                            lock.lock()
                                .expect("prefetch state poisoned")
                                .in_flight
                                .remove(&id);
                        }
                        if worker_responses
                            .send(LoadResponse::Prefetch {
                                id,
                                result: Box::new(result),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .expect("failed to start a bd prefetch thread");
        }
        Self { state }
    }

    fn replace_pending(&self, ids: Vec<String>) {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().expect("prefetch state poisoned");
        if !state.stopped {
            let mut seen = state.in_flight.clone();
            state.pending = ids
                .into_iter()
                .filter(|id| seen.insert(id.clone()))
                .collect();
            ready.notify_all();
        }
    }

    fn prioritize(&self, id: String) {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().expect("prefetch state poisoned");
        if !state.stopped && !state.in_flight.contains(&id) {
            state.pending.retain(|pending| pending != &id);
            state.pending.push_front(id);
            ready.notify_one();
        }
    }
}

impl Drop for Prefetcher {
    fn drop(&mut self) {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().expect("prefetch state poisoned");
        state.stopped = true;
        state.pending.clear();
        ready.notify_all();
    }
}

pub struct App {
    pub issues: Vec<Issue>,
    pub visible: Vec<usize>,
    pub detail: Option<Issue>,
    pub list_state: ListState,
    pub filter: String,
    pub filtering: bool,
    pub list_scroll: usize,
    pub preview_scroll: u16,
    pub issue_scroll: u16,
    pub relationship_scroll: usize,
    pub error: Option<String>,
    pub loading_list: bool,
    pub showing_refresh: bool,
    pub auto_refresh_error: Option<String>,
    pub view: ListView,
    pub sort: ListSort,
    pub screen: Screen,
    pub loading_detail: bool,
    pub relationship_index: usize,
    pub work_message: Option<String>,
    pub work_error: Option<String>,
    list_page_len: usize,
    relationship_page_len: usize,
    preview_scroll_max: u16,
    issue_scroll_max: u16,
    loader: Loader,
    generation: u64,
    detail_generation: u64,
    detail_cache: HashMap<String, Issue>,
    revision: Option<String>,
    checking_revision: bool,
    refresh_after_load: bool,
    next_revision_check: Instant,
}

const REVISION_POLL_INTERVAL: Duration = Duration::from_secs(2);

impl App {
    pub fn load() -> Self {
        Self::with_source(Box::new(CliSource))
    }

    fn with_source(source: Box<dyn IssueSource>) -> Self {
        let mut app = Self {
            issues: Vec::new(),
            visible: Vec::new(),
            detail: None,
            list_state: ListState::default(),
            filter: String::new(),
            filtering: false,
            list_scroll: 0,
            preview_scroll: 0,
            issue_scroll: 0,
            relationship_scroll: 0,
            error: None,
            loading_list: false,
            showing_refresh: false,
            auto_refresh_error: None,
            view: ListView::default(),
            sort: ListSort::default(),
            screen: Screen::default(),
            loading_detail: false,
            relationship_index: 0,
            work_message: None,
            work_error: None,
            list_page_len: 0,
            relationship_page_len: 0,
            preview_scroll_max: 0,
            issue_scroll_max: 0,
            loader: Loader::new(source),
            generation: 0,
            detail_generation: 0,
            detail_cache: HashMap::new(),
            revision: None,
            checking_revision: false,
            refresh_after_load: false,
            next_revision_check: Instant::now(),
        };
        app.refresh();
        app
    }

    pub fn poll(&mut self) {
        while let Ok(response) = self.loader.responses.try_recv() {
            match response {
                LoadResponse::List { generation, result } => {
                    if generation != self.generation {
                        continue;
                    }
                    self.loading_list = false;
                    self.showing_refresh = false;
                    match result {
                        Ok(issues) => {
                            let selected_id = self.selected_issue().map(|issue| issue.id.clone());
                            self.issues = issues;
                            self.rebuild_visible_preserving(selected_id);
                            self.refresh_open_issue_if_stale();
                            self.error = None;
                            if self.revision.is_none() && !self.checking_revision {
                                self.request_revision(Instant::now());
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                    if self.refresh_after_load {
                        self.refresh_after_load = false;
                        self.refresh_quietly();
                    }
                }
                LoadResponse::Detail {
                    generation,
                    id,
                    result,
                } => {
                    if generation != self.detail_generation {
                        continue;
                    }
                    self.loading_detail = false;
                    match *result {
                        Ok(issue) => {
                            self.detail_cache.insert(id.clone(), issue.clone());
                            if self.screen == Screen::Issue
                                && self.detail.as_ref().is_some_and(|current| current.id == id)
                            {
                                self.detail = Some(issue);
                                self.relationship_index = 0;
                                self.error = None;
                                self.warm_relationship_cache();
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                LoadResponse::Prefetch { id, result } => {
                    if let Ok(issue) = *result {
                        self.detail_cache.insert(id, issue);
                    }
                }
                LoadResponse::Revision(result) => {
                    self.checking_revision = false;
                    self.next_revision_check = Instant::now() + REVISION_POLL_INTERVAL;
                    match result {
                        Ok(revision) => {
                            self.auto_refresh_error = None;
                            let changed = self
                                .revision
                                .as_ref()
                                .is_some_and(|current| current != &revision);
                            self.revision = Some(revision);
                            if changed {
                                if self.loading_list {
                                    self.refresh_after_load = true;
                                } else {
                                    self.refresh_quietly();
                                }
                            }
                        }
                        Err(error) => {
                            self.auto_refresh_error = Some(error);
                        }
                    }
                }
            }
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if !self.checking_revision && now >= self.next_revision_check {
            self.request_revision(now);
        }
    }

    pub fn refresh(&mut self) {
        self.request_list(true);
    }

    pub fn work_issue(&self) -> Option<&Issue> {
        match self.screen {
            Screen::Browser => self.selected_issue(),
            Screen::Issue => self.detail.as_ref(),
        }
    }

    pub fn clear_work_feedback(&mut self) {
        self.work_message = None;
        self.work_error = None;
    }

    pub fn clear_work_success(&mut self) {
        self.work_message = None;
    }

    pub fn dismiss_work_error(&mut self) -> bool {
        self.work_error.take().is_some()
    }

    pub fn report_work_started(&mut self, message: String, warning: Option<String>) {
        self.work_message = Some(message);
        self.work_error = warning;
    }

    pub fn report_work_error(&mut self, error: impl Into<String>) {
        self.work_message = None;
        self.work_error = Some(error.into());
    }

    fn refresh_quietly(&mut self) {
        self.request_list(false);
    }

    fn request_list(&mut self, show_activity: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.loading_list = true;
        self.showing_refresh |= show_activity;
        self.error = None;
        let options = ListOptions {
            view: self.view,
            sort: self.sort,
        };
        if let Err(error) = self.loader.request_list(self.generation, options) {
            self.loading_list = false;
            self.showing_refresh = false;
            self.error = Some(error);
        }
    }

    fn request_revision(&mut self, now: Instant) {
        self.checking_revision = true;
        self.next_revision_check = now + REVISION_POLL_INTERVAL;
        if let Err(error) = self.loader.request_revision() {
            self.checking_revision = false;
            self.auto_refresh_error = Some(error);
        }
    }

    pub fn push_filter(&mut self, character: char) {
        self.filter.push(character);
        self.rebuild_visible();
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.rebuild_visible();
    }

    pub fn clear_filter(&mut self) {
        if !self.filter.is_empty() {
            self.filter.clear();
            self.rebuild_visible();
        }
    }

    pub fn set_view(&mut self, view: ListView) {
        if self.view != view {
            self.view = view;
            self.refresh();
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            ListSort::Priority => ListSort::Updated,
            ListSort::Updated => ListSort::Created,
            ListSort::Created => ListSort::Priority,
        };
        self.refresh();
    }

    pub fn select_next(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let next = self
            .list_state
            .selected()
            .map_or(0, |selected| (selected + 1).min(self.visible.len() - 1));
        self.list_state.select(Some(next));
        self.ensure_selected_issue_visible();
        self.select_detail();
    }

    pub fn select_previous(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let previous = self
            .list_state
            .selected()
            .map_or(0, |selected| selected.saturating_sub(1));
        self.list_state.select(Some(previous));
        self.ensure_selected_issue_visible();
        self.select_detail();
    }

    pub fn select_first(&mut self) {
        if !self.visible.is_empty() {
            self.list_state.select(Some(0));
            self.ensure_selected_issue_visible();
            self.select_detail();
        }
    }

    pub fn select_last(&mut self) {
        if !self.visible.is_empty() {
            self.list_state.select(Some(self.visible.len() - 1));
            self.ensure_selected_issue_visible();
            self.select_detail();
        }
    }

    pub fn scroll(&mut self, pane: ScrollPane, delta: i16) {
        match pane {
            ScrollPane::IssueList => {
                let max = self.visible.len().saturating_sub(self.list_page_len);
                self.list_scroll = add_signed_clamped(self.list_scroll, delta, max);
            }
            ScrollPane::Preview => {
                self.preview_scroll = add_signed_clamped(
                    usize::from(self.preview_scroll),
                    delta,
                    usize::from(self.preview_scroll_max),
                ) as u16;
            }
            ScrollPane::IssueBody => {
                self.issue_scroll = add_signed_clamped(
                    usize::from(self.issue_scroll),
                    delta,
                    usize::from(self.issue_scroll_max),
                ) as u16;
            }
            ScrollPane::Relationships => {
                let max = self
                    .relationships()
                    .len()
                    .saturating_sub(self.relationship_page_len);
                self.relationship_scroll = add_signed_clamped(self.relationship_scroll, delta, max);
            }
        }
    }

    pub fn update_list_viewport(&mut self, page_len: usize) {
        let changed = self.list_page_len != page_len;
        self.list_page_len = page_len;
        self.list_scroll = self
            .list_scroll
            .min(self.visible.len().saturating_sub(page_len));
        if changed {
            self.ensure_selected_issue_visible();
        }
    }

    pub fn update_preview_scroll_max(&mut self, max: u16) {
        self.preview_scroll_max = max;
        self.preview_scroll = self.preview_scroll.min(max);
    }

    pub fn update_issue_scroll_max(&mut self, max: u16) {
        self.issue_scroll_max = max;
        self.issue_scroll = self.issue_scroll.min(max);
    }

    pub fn update_relationship_viewport(&mut self, page_len: usize) {
        let changed = self.relationship_page_len != page_len;
        self.relationship_page_len = page_len;
        self.relationship_scroll = self
            .relationship_scroll
            .min(self.relationships().len().saturating_sub(page_len));
        if changed {
            self.ensure_selected_relationship_visible();
        }
    }

    pub fn open_selected_issue(&mut self) {
        if let Some(issue) = self.selected_issue().cloned() {
            self.open_issue(issue);
        }
    }

    pub fn close_issue(&mut self) {
        self.detail_generation = self.detail_generation.wrapping_add(1);
        self.loading_detail = false;
        self.screen = Screen::Browser;
        self.relationship_index = 0;
        self.relationship_scroll = 0;
        self.issue_scroll = 0;
        self.loader.warm_details(Vec::new());
        self.select_detail();
        self.error = None;
    }

    pub fn reload_issue(&mut self) {
        if let Some(issue) = self.detail.clone() {
            self.detail_cache.remove(&issue.id);
            self.loader.warm_details(Vec::new());
            self.request_issue_detail(issue);
        }
    }

    pub fn select_next_relationship(&mut self) {
        let count = self.relationships().len();
        if count > 0 {
            self.relationship_index = (self.relationship_index + 1).min(count - 1);
            self.ensure_selected_relationship_visible();
            self.prioritize_selected_relationship();
        }
    }

    pub fn select_previous_relationship(&mut self) {
        self.relationship_index = self.relationship_index.saturating_sub(1);
        self.ensure_selected_relationship_visible();
        self.prioritize_selected_relationship();
    }

    pub fn open_selected_relationship(&mut self) {
        let related = self
            .relationships()
            .get(self.relationship_index)
            .map(|(_, issue)| (*issue).clone());
        if let Some(related) = related {
            self.open_issue(Self::related_preview(related));
        }
    }

    pub fn relationships(&self) -> Vec<(&'static str, &RelatedIssue)> {
        let Some(issue) = &self.detail else {
            return Vec::new();
        };
        issue
            .dependencies
            .iter()
            .map(|related| ("depends on", related))
            .chain(issue.dependents.iter().map(|related| ("blocks", related)))
            .collect()
    }

    fn open_issue(&mut self, preview: Issue) {
        self.screen = Screen::Issue;
        self.relationship_index = 0;
        self.relationship_scroll = 0;
        self.issue_scroll = 0;
        self.error = None;
        self.loader.warm_details(Vec::new());
        if let Some(cached) = self.detail_cache.get(&preview.id)
            && (preview.updated_at.is_empty() || cached.updated_at == preview.updated_at)
        {
            self.detail = Some(cached.clone());
            self.loading_detail = false;
            self.warm_relationship_cache();
            return;
        }
        self.detail = Some(preview.clone());
        self.request_issue_detail(preview);
    }

    fn request_issue_detail(&mut self, issue: Issue) {
        self.detail_generation = self.detail_generation.wrapping_add(1);
        self.loading_detail = true;
        if let Err(error) = self.loader.request_detail(self.detail_generation, issue.id) {
            self.loading_detail = false;
            self.error = Some(error);
        }
    }

    fn prioritize_selected_relationship(&self) {
        let id = self
            .relationships()
            .get(self.relationship_index)
            .map(|(_, issue)| issue.id.clone());
        if let Some(id) = id
            && !self.detail_cache.contains_key(&id)
        {
            self.loader.prioritize_detail(id);
        }
    }

    fn warm_relationship_cache(&self) {
        self.loader
            .warm_details(self.uncached_relationships_by_proximity());
    }

    fn uncached_relationships_by_proximity(&self) -> Vec<String> {
        let relationships = self.relationships();
        let mut ids = Vec::with_capacity(relationships.len());
        for distance in 0..relationships.len() {
            if let Some((_, issue)) = relationships.get(self.relationship_index + distance)
                && !self.detail_cache.contains_key(&issue.id)
            {
                ids.push(issue.id.clone());
            }
            if distance > 0
                && let Some(index) = self.relationship_index.checked_sub(distance)
                && let Some((_, issue)) = relationships.get(index)
                && !self.detail_cache.contains_key(&issue.id)
            {
                ids.push(issue.id.clone());
            }
        }
        ids
    }

    fn related_preview(related: RelatedIssue) -> Issue {
        Issue {
            id: related.id,
            title: related.title,
            status: related.status,
            priority: related.priority,
            issue_type: related.issue_type,
            description: String::new(),
            design: String::new(),
            acceptance_criteria: String::new(),
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

    fn rebuild_visible(&mut self) {
        let selected_id = self.selected_issue().map(|issue| issue.id.clone());
        self.rebuild_visible_preserving(selected_id);
    }

    fn rebuild_visible_preserving(&mut self, selected_id: Option<String>) {
        let needle = self.filter.to_lowercase();
        self.visible = self
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                needle.is_empty()
                    || issue.id.to_lowercase().contains(&needle)
                    || issue.title.to_lowercase().contains(&needle)
                    || issue.description.to_lowercase().contains(&needle)
                    || issue.assignee.to_lowercase().contains(&needle)
                    || issue.issue_type.to_lowercase().contains(&needle)
                    || issue.status.to_lowercase().contains(&needle)
                    || issue
                        .labels
                        .iter()
                        .any(|label| label.to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect();

        let selection = selected_id
            .and_then(|id| {
                self.visible
                    .iter()
                    .position(|index| self.issues[*index].id == id)
            })
            .or_else(|| (!self.visible.is_empty()).then_some(0));
        self.list_state.select(selection);
        self.list_scroll = selection.unwrap_or(0);
        self.ensure_selected_issue_visible();
        self.preview_scroll = 0;
        if self.screen == Screen::Browser {
            self.select_detail();
        }
    }

    fn refresh_open_issue_if_stale(&mut self) {
        if self.screen != Screen::Issue || self.loading_detail {
            return;
        }
        let Some(current) = self.detail.as_ref() else {
            return;
        };
        let preview = self
            .issues
            .iter()
            .find(|issue| issue.id == current.id)
            .cloned();
        if preview
            .as_ref()
            .is_none_or(|preview| preview.updated_at != current.updated_at)
        {
            let request = preview.unwrap_or_else(|| current.clone());
            self.detail_cache.remove(&request.id);
            self.request_issue_detail(request);
        }
    }

    fn select_detail(&mut self) {
        self.preview_scroll = 0;
        self.detail = self.selected_issue().cloned();
    }

    fn ensure_selected_issue_visible(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            self.list_scroll = 0;
            return;
        };
        if self.list_page_len == 0 {
            return;
        }
        if selected < self.list_scroll {
            self.list_scroll = selected;
        } else if selected >= self.list_scroll + self.list_page_len {
            self.list_scroll = selected + 1 - self.list_page_len;
        }
    }

    fn ensure_selected_relationship_visible(&mut self) {
        if self.relationship_page_len == 0 {
            return;
        }
        if self.relationship_index < self.relationship_scroll {
            self.relationship_scroll = self.relationship_index;
        } else if self.relationship_index >= self.relationship_scroll + self.relationship_page_len {
            self.relationship_scroll = self.relationship_index + 1 - self.relationship_page_len;
        }
    }

    fn selected_issue(&self) -> Option<&Issue> {
        self.list_state
            .selected()
            .and_then(|selected| self.visible.get(selected))
            .and_then(|index| self.issues.get(*index))
    }
}

fn add_signed_clamped(value: usize, delta: i16, max: usize) -> usize {
    value.saturating_add_signed(isize::from(delta)).min(max)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use anyhow::{Result, bail};

    use super::*;

    struct FakeSource {
        issues: Arc<Mutex<Vec<Issue>>>,
        list_calls: Arc<AtomicUsize>,
        show_calls: Arc<AtomicUsize>,
        active_shows: Arc<AtomicUsize>,
        max_active_shows: Arc<AtomicUsize>,
        requested_shows: Arc<Mutex<Vec<String>>>,
        requested: Arc<Mutex<Vec<ListOptions>>>,
        revision: Arc<Mutex<String>>,
        revision_calls: Arc<AtomicUsize>,
        revision_failures: Arc<AtomicUsize>,
        delay: Duration,
        fail: bool,
    }

    impl IssueSource for FakeSource {
        fn list(&self, options: ListOptions) -> Result<Vec<Issue>> {
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            self.requested.lock().unwrap().push(options);
            let snapshot = self
                .issues
                .lock()
                .unwrap()
                .iter()
                .filter(|issue| match options.view {
                    ListView::Active | ListView::Ready => issue.status != "closed",
                    ListView::Closed => issue.status == "closed",
                })
                .cloned()
                .collect();
            thread::sleep(self.delay);
            if self.fail {
                bail!("simulated bd failure");
            }
            Ok(snapshot)
        }

        fn show(&self, id: &str) -> Result<Issue> {
            self.show_calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active_shows.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active_shows.fetch_max(active, Ordering::SeqCst);
            self.requested_shows.lock().unwrap().push(id.to_owned());
            thread::sleep(self.delay);
            let issues = self.issues.lock().unwrap();
            let mut detail = issues
                .iter()
                .find(|issue| issue.id == id)
                .cloned()
                .unwrap_or_else(|| issue(id, "v1"));
            let related = issues
                .iter()
                .find(|issue| issue.id != id)
                .unwrap_or(&detail);
            detail.dependencies = vec![RelatedIssue {
                id: related.id.clone(),
                title: related.title.clone(),
                status: related.status.clone(),
                priority: related.priority,
                issue_type: related.issue_type.clone(),
                dependency_type: "blocks".to_owned(),
            }];
            detail.comments = vec![crate::bd::Comment {
                author: "Tester".to_owned(),
                text: "Extended context".to_owned(),
                created_at: "now".to_owned(),
            }];
            self.active_shows.fetch_sub(1, Ordering::SeqCst);
            Ok(detail)
        }

        fn revision(&self) -> Result<String> {
            self.revision_calls.fetch_add(1, Ordering::SeqCst);
            if self
                .revision_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |failures| {
                    failures.checked_sub(1)
                })
                .is_ok()
            {
                bail!("simulated revision failure");
            }
            Ok(self.revision.lock().unwrap().clone())
        }
    }

    struct TestHarness {
        app: App,
        issues: Arc<Mutex<Vec<Issue>>>,
        list_calls: Arc<AtomicUsize>,
        show_calls: Arc<AtomicUsize>,
        max_active_shows: Arc<AtomicUsize>,
        requested_shows: Arc<Mutex<Vec<String>>>,
        requested: Arc<Mutex<Vec<ListOptions>>>,
        revision: Arc<Mutex<String>>,
        revision_calls: Arc<AtomicUsize>,
        revision_failures: Arc<AtomicUsize>,
    }

    fn issue(id: &str, updated_at: &str) -> Issue {
        Issue {
            id: id.to_owned(),
            title: format!("Issue {id}"),
            description: format!("Detail for {id}"),
            design: String::new(),
            acceptance_criteria: String::new(),
            status: "open".to_owned(),
            priority: 1,
            issue_type: "task".to_owned(),
            assignee: String::new(),
            labels: Vec::new(),
            dependency_count: 0,
            dependent_count: 0,
            comment_count: 0,
            updated_at: updated_at.to_owned(),
            created_at: String::new(),
            closed_at: String::new(),
            owner: String::new(),
            notes: String::new(),
            dependencies: Vec::new(),
            dependents: Vec::new(),
            comments: Vec::new(),
        }
    }

    fn test_app(delay: Duration, fail: bool) -> TestHarness {
        let issues = Arc::new(Mutex::new(vec![
            issue("btui-1", "v1"),
            issue("btui-2", "v1"),
        ]));
        let list_calls = Arc::new(AtomicUsize::new(0));
        let show_calls = Arc::new(AtomicUsize::new(0));
        let active_shows = Arc::new(AtomicUsize::new(0));
        let max_active_shows = Arc::new(AtomicUsize::new(0));
        let requested_shows = Arc::new(Mutex::new(Vec::new()));
        let requested = Arc::new(Mutex::new(Vec::new()));
        let revision = Arc::new(Mutex::new("revision-1".to_owned()));
        let revision_calls = Arc::new(AtomicUsize::new(0));
        let revision_failures = Arc::new(AtomicUsize::new(0));
        let source = FakeSource {
            issues: Arc::clone(&issues),
            list_calls: Arc::clone(&list_calls),
            show_calls: Arc::clone(&show_calls),
            active_shows,
            max_active_shows: Arc::clone(&max_active_shows),
            requested_shows: Arc::clone(&requested_shows),
            requested: Arc::clone(&requested),
            revision: Arc::clone(&revision),
            revision_calls: Arc::clone(&revision_calls),
            revision_failures: Arc::clone(&revision_failures),
            delay,
            fail,
        };

        TestHarness {
            app: App::with_source(Box::new(source)),
            issues,
            list_calls,
            show_calls,
            max_active_shows,
            requested_shows,
            requested,
            revision,
            revision_calls,
            revision_failures,
        }
    }

    fn wait_until(app: &mut App, condition: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !condition(app) {
            assert!(Instant::now() < deadline, "timed out waiting for loader");
            app.poll();
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn delayed_initial_load_returns_without_blocking() {
        let started = Instant::now();
        let mut harness = test_app(Duration::from_millis(100), false);

        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(harness.app.loading_list);
        wait_until(&mut harness.app, |app| !app.loading_list);
        assert_eq!(harness.app.issues.len(), 2);
    }

    #[test]
    fn navigation_uses_pre_rendered_list_detail_and_never_calls_show() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);

        harness.app.select_next();
        harness.app.select_previous();
        harness.app.select_last();

        assert_eq!(harness.list_calls.load(Ordering::SeqCst), 1);
        assert_eq!(harness.show_calls.load(Ordering::SeqCst), 0);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
    }

    #[test]
    fn refresh_retains_selection_and_ignores_stale_response() {
        let mut harness = test_app(Duration::from_millis(50), false);
        harness.issues.lock().unwrap()[1].updated_at = "v2".to_owned();
        harness.app.refresh();

        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.select_last();
        harness.issues.lock().unwrap().reverse();
        harness.app.refresh();
        wait_until(&mut harness.app, |app| !app.loading_list);

        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
        assert_eq!(harness.app.detail.as_ref().unwrap().updated_at, "v2");
        assert_eq!(harness.list_calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn revision_change_refreshes_once_and_preserves_selection() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        wait_until(&mut harness.app, |app| app.revision.is_some());
        harness.app.select_last();
        harness
            .issues
            .lock()
            .unwrap()
            .insert(0, issue("btui-new", "v1"));
        *harness.revision.lock().unwrap() = "revision-2".to_owned();

        harness.app.tick(Instant::now() + REVISION_POLL_INTERVAL);
        wait_until(&mut harness.app, |app| {
            !app.loading_list && app.issues.len() == 3
        });

        assert_eq!(harness.list_calls.load(Ordering::SeqCst), 2);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
        assert!(!harness.app.showing_refresh);
    }

    #[test]
    fn unchanged_revision_does_not_run_another_list() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        wait_until(&mut harness.app, |app| app.revision.is_some());

        harness.app.tick(Instant::now() + REVISION_POLL_INTERVAL);
        wait_until(&mut harness.app, |_| {
            harness.revision_calls.load(Ordering::SeqCst) >= 2
        });
        harness.app.poll();

        assert_eq!(harness.list_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn revision_failure_is_visible_and_the_next_probe_recovers() {
        let mut harness = test_app(Duration::from_millis(20), false);
        harness.revision_failures.store(1, Ordering::SeqCst);
        wait_until(&mut harness.app, |app| !app.loading_list);
        wait_until(&mut harness.app, |app| app.auto_refresh_error.is_some());

        assert_eq!(
            harness.app.auto_refresh_error.as_deref(),
            Some("simulated revision failure")
        );

        harness.app.tick(Instant::now() + REVISION_POLL_INTERVAL);
        wait_until(&mut harness.app, |app| {
            app.auto_refresh_error.is_none() && app.revision.is_some()
        });

        assert_eq!(harness.list_calls.load(Ordering::SeqCst), 1);
        assert_eq!(harness.revision_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn background_failure_is_recoverable_ui_state() {
        let mut harness = test_app(Duration::ZERO, true);

        wait_until(&mut harness.app, |app| !app.loading_list);

        assert_eq!(harness.app.error.as_deref(), Some("simulated bd failure"));
    }

    #[test]
    fn view_and_sort_changes_request_matching_list_options() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);

        harness.app.set_view(ListView::Closed);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.cycle_sort();
        wait_until(&mut harness.app, |app| !app.loading_list);

        assert_eq!(
            *harness.requested.lock().unwrap(),
            [
                ListOptions::default(),
                ListOptions {
                    view: ListView::Closed,
                    sort: ListSort::Priority,
                },
                ListOptions {
                    view: ListView::Closed,
                    sort: ListSort::Updated,
                },
            ]
        );
    }

    #[test]
    fn filter_matches_human_visible_fields_and_can_be_cleared() {
        let mut harness = test_app(Duration::ZERO, false);
        harness.issues.lock().unwrap()[1].labels = vec!["frontend".to_owned()];
        harness.app.refresh();
        wait_until(&mut harness.app, |app| !app.loading_list);

        for character in "frontend".chars() {
            harness.app.push_filter(character);
        }
        assert_eq!(harness.app.visible.len(), 1);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");

        harness.app.clear_filter();
        assert_eq!(harness.app.visible.len(), 2);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
    }

    #[test]
    fn work_success_is_transient_but_errors_require_explicit_dismissal() {
        let mut harness = test_app(Duration::ZERO, false);
        harness
            .app
            .report_work_started("Started Codex".to_owned(), Some("focus warning".to_owned()));

        harness.app.clear_work_success();
        assert_eq!(harness.app.work_message, None);
        assert_eq!(harness.app.work_error.as_deref(), Some("focus warning"));

        assert!(harness.app.dismiss_work_error());
        assert_eq!(harness.app.work_error, None);
        assert!(!harness.app.dismiss_work_error());
    }

    #[test]
    fn pane_scrolling_is_bounded_and_does_not_change_issue_selection() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.update_list_viewport(1);

        harness.app.scroll(ScrollPane::IssueList, 50);
        assert_eq!(harness.app.list_scroll, 1);
        assert_eq!(harness.app.list_state.selected(), Some(0));
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-1");

        harness.app.update_preview_scroll_max(7);
        harness.app.scroll(ScrollPane::Preview, 50);
        assert_eq!(harness.app.preview_scroll, 7);
        harness.app.scroll(ScrollPane::Preview, -50);
        assert_eq!(harness.app.preview_scroll, 0);

        harness.app.update_issue_scroll_max(11);
        harness.app.scroll(ScrollPane::IssueBody, 50);
        assert_eq!(harness.app.issue_scroll, 11);
        harness.app.update_issue_scroll_max(4);
        assert_eq!(harness.app.issue_scroll, 4);
    }

    #[test]
    fn keyboard_selection_and_relationship_navigation_keep_targets_visible() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.update_list_viewport(1);
        harness.app.select_last();
        assert_eq!(harness.app.list_scroll, 1);

        harness.app.detail.as_mut().unwrap().dependencies = (0..4)
            .map(|index| RelatedIssue {
                id: format!("btui-related-{index}"),
                title: format!("Related {index}"),
                status: "open".to_owned(),
                priority: 2,
                issue_type: "task".to_owned(),
                dependency_type: "blocks".to_owned(),
            })
            .collect();
        harness.app.update_relationship_viewport(2);
        harness.app.select_next_relationship();
        harness.app.select_next_relationship();
        assert_eq!(harness.app.relationship_index, 2);
        assert_eq!(harness.app.relationship_scroll, 1);

        harness.app.scroll(ScrollPane::Relationships, 50);
        assert_eq!(harness.app.relationship_scroll, 2);
        assert_eq!(harness.app.relationship_index, 2);
    }

    #[test]
    fn issue_screen_loads_extended_detail_and_returns_to_preserved_browser() {
        let mut harness = test_app(Duration::from_millis(20), false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.select_last();

        harness.app.open_selected_issue();
        assert_eq!(harness.app.screen, Screen::Issue);
        assert!(harness.app.loading_detail);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
        wait_until(&mut harness.app, |app| !app.loading_detail);

        assert_eq!(harness.app.detail.as_ref().unwrap().comments.len(), 1);
        assert_eq!(harness.app.relationships()[0].1.id, "btui-1");
        wait_until(&mut harness.app, |app| {
            app.detail_cache.contains_key("btui-1")
        });
        assert_eq!(harness.show_calls.load(Ordering::SeqCst), 2);

        harness.app.close_issue();
        assert_eq!(harness.app.screen, Screen::Browser);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");

        harness.app.open_selected_issue();
        assert!(!harness.app.loading_detail);
        assert_eq!(harness.show_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn list_refresh_preserves_and_reloads_stale_open_issue_detail() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.open_selected_issue();
        wait_until(&mut harness.app, |app| !app.loading_detail);
        assert_eq!(harness.app.detail.as_ref().unwrap().comments.len(), 1);

        harness.issues.lock().unwrap()[0].updated_at = "v2".to_owned();
        harness.app.refresh();
        wait_until(&mut harness.app, |app| {
            !app.loading_list && !app.loading_detail
        });

        assert_eq!(harness.app.screen, Screen::Issue);
        assert_eq!(harness.app.detail.as_ref().unwrap().updated_at, "v2");
        assert_eq!(harness.app.detail.as_ref().unwrap().comments.len(), 1);
    }

    #[test]
    fn list_refresh_revalidates_open_detail_that_disappears_after_close() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.open_selected_issue();
        wait_until(&mut harness.app, |app| !app.loading_detail);
        let show_calls_before_refresh = harness.show_calls.load(Ordering::SeqCst);

        harness.issues.lock().unwrap()[0].status = "closed".to_owned();
        harness.app.refresh();
        wait_until(&mut harness.app, |app| {
            !app.loading_list && !app.loading_detail
        });

        assert_eq!(harness.app.screen, Screen::Issue);
        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-1");
        assert_eq!(harness.app.detail.as_ref().unwrap().status, "closed");
        assert_eq!(
            harness.show_calls.load(Ordering::SeqCst),
            show_calls_before_refresh + 1
        );
    }

    #[test]
    fn selected_relationship_opens_related_issue_without_blocking() {
        let mut harness = test_app(Duration::from_millis(20), false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.open_selected_issue();
        wait_until(&mut harness.app, |app| !app.loading_detail);
        wait_until(&mut harness.app, |app| {
            app.detail_cache.contains_key("btui-2")
        });

        harness.app.open_selected_relationship();

        assert_eq!(harness.app.detail.as_ref().unwrap().id, "btui-2");
        assert!(!harness.app.loading_detail);
        assert_eq!(harness.show_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn relationship_warming_deduplicates_and_bounds_concurrency() {
        let mut harness = test_app(Duration::from_millis(40), false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.detail.as_mut().unwrap().dependencies = [0, 1, 1, 2]
            .into_iter()
            .map(|index| RelatedIssue {
                id: format!("btui-related-{index}"),
                title: format!("Related {index}"),
                status: "open".to_owned(),
                priority: 2,
                issue_type: "task".to_owned(),
                dependency_type: "blocks".to_owned(),
            })
            .collect();

        harness.app.warm_relationship_cache();
        wait_until(&mut harness.app, |app| {
            [0, 1, 2].into_iter().all(|index| {
                app.detail_cache
                    .contains_key(&format!("btui-related-{index}"))
            })
        });

        let requested = harness.requested_shows.lock().unwrap();
        assert_eq!(requested.len(), 3);
        assert_eq!(requested.iter().collect::<HashSet<_>>().len(), 3);
        assert_eq!(harness.max_active_shows.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn relationship_warming_orders_uncached_ids_by_selection_proximity() {
        let mut harness = test_app(Duration::ZERO, false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.detail.as_mut().unwrap().dependencies = (0..5)
            .map(|index| RelatedIssue {
                id: format!("btui-related-{index}"),
                title: format!("Related {index}"),
                status: "open".to_owned(),
                priority: 2,
                issue_type: "task".to_owned(),
                dependency_type: "blocks".to_owned(),
            })
            .collect();
        harness.app.relationship_index = 2;
        harness
            .app
            .detail_cache
            .insert("btui-related-1".to_owned(), issue("btui-related-1", "v1"));

        assert_eq!(
            harness.app.uncached_relationships_by_proximity(),
            [
                "btui-related-2",
                "btui-related-3",
                "btui-related-4",
                "btui-related-0",
            ]
        );
    }

    #[test]
    fn failed_relationship_prefetch_does_not_replace_or_error_current_issue() {
        struct FailingPrefetchSource {
            calls: Arc<AtomicUsize>,
        }

        impl IssueSource for FailingPrefetchSource {
            fn list(&self, _options: ListOptions) -> Result<Vec<Issue>> {
                Ok(vec![issue("btui-1", "v1")])
            }

            fn show(&self, id: &str) -> Result<Issue> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if id == "btui-2" {
                    bail!("simulated prefetch failure");
                }
                let mut detail = issue("btui-1", "v1");
                detail.dependencies = vec![RelatedIssue {
                    id: "btui-2".to_owned(),
                    title: "Related issue".to_owned(),
                    status: "open".to_owned(),
                    priority: 2,
                    issue_type: "task".to_owned(),
                    dependency_type: "blocks".to_owned(),
                }];
                Ok(detail)
            }

            fn revision(&self) -> Result<String> {
                Ok("revision-1".to_owned())
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let mut app = App::with_source(Box::new(FailingPrefetchSource {
            calls: Arc::clone(&calls),
        }));
        wait_until(&mut app, |app| !app.loading_list);
        app.open_selected_issue();
        wait_until(&mut app, |app| !app.loading_detail);
        wait_until(&mut app, |_| calls.load(Ordering::SeqCst) == 2);
        thread::sleep(Duration::from_millis(5));
        app.poll();

        assert_eq!(app.detail.as_ref().unwrap().id, "btui-1");
        assert_eq!(app.error, None);
        assert!(!app.detail_cache.contains_key("btui-2"));
    }

    #[test]
    fn closing_issue_ignores_in_flight_detail_response() {
        let mut harness = test_app(Duration::from_millis(50), false);
        wait_until(&mut harness.app, |app| !app.loading_list);
        harness.app.open_selected_issue();

        harness.app.close_issue();
        thread::sleep(Duration::from_millis(60));
        harness.app.poll();

        assert_eq!(harness.app.screen, Screen::Browser);
        assert_eq!(harness.app.detail.as_ref().unwrap().comments.len(), 0);
    }

    #[test]
    fn issue_screen_keeps_preview_when_detail_loading_fails() {
        struct FailingDetailSource;

        impl IssueSource for FailingDetailSource {
            fn list(&self, _options: ListOptions) -> Result<Vec<Issue>> {
                Ok(vec![issue("btui-1", "v1")])
            }

            fn show(&self, _id: &str) -> Result<Issue> {
                bail!("simulated detail failure")
            }

            fn revision(&self) -> Result<String> {
                Ok("revision-1".to_owned())
            }
        }

        let mut app = App::with_source(Box::new(FailingDetailSource));
        wait_until(&mut app, |app| !app.loading_list);
        app.open_selected_issue();
        wait_until(&mut app, |app| !app.loading_detail);

        assert_eq!(app.screen, Screen::Issue);
        assert_eq!(app.detail.as_ref().unwrap().id, "btui-1");
        assert_eq!(app.error.as_deref(), Some("simulated detail failure"));
    }
}
