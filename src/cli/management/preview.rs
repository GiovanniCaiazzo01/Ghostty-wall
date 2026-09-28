//! One in-flight preparation and one replaceable pending selection; no persisted cache.

use super::*;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
struct Request {
    generation: u64,
    profile: IntentId,
    seed: ResolutionSeed,
    area: ratatui::layout::Rect,
}

#[derive(Default)]
struct Mailbox {
    pending: Option<Request>,
    completed: Option<(u64, ratatui::layout::Rect, Result<Sample, String>)>,
    closed: bool,
}

pub(super) struct Previews {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    generation: u64,
    pub seed: ResolutionSeed,
    pub area: ratatui::layout::Rect,
}

impl Previews {
    pub fn new(seed: Option<ResolutionSeed>) -> Result<Self, CliError> {
        let seed = match seed {
            Some(seed) => seed,
            None => ResolutionSeed::from_str(&editor::random_seed()?.to_string())?,
        };
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let worker = Arc::clone(&mailbox);
        std::thread::Builder::new()
            .name("profile-preview".into())
            .spawn(move || {
                let mut previous: Option<(Request, Prepared)> = None;
                loop {
                    let request = {
                        let (lock, wake) = &*worker;
                        let mut state = lock.lock().unwrap();
                        while state.pending.is_none() && !state.closed {
                            let (next, timeout) = wake
                                .wait_timeout(state, std::time::Duration::from_millis(250))
                                .unwrap();
                            state = next;
                            if timeout.timed_out() {
                                // Stat only on the worker, without holding the input thread's mailbox.
                                drop(state);
                                let changed = previous.as_ref().is_some_and(|(_, p)| p.changed());
                                state = lock.lock().unwrap();
                                if changed && state.pending.is_none() {
                                    let (request, _) = previous.take().unwrap();
                                    state.completed = Some((
                                        request.generation,
                                        request.area,
                                        Err("Profile/image changed; loading...".into()),
                                    ));
                                    state.pending = Some(request);
                                }
                            }
                        }
                        if state.closed {
                            return;
                        }
                        state.pending.take().unwrap()
                    };
                    let mut result = match &previous {
                        Some((last, prepared))
                            if last.generation == request.generation && !prepared.changed() =>
                        {
                            Ok(prepared.clone())
                        }
                        _ => prepare(&request.profile, request.seed).map_err(|e| e.to_string()),
                    };
                    if let Ok(prepared) = &mut result
                        && let Err(error) = prepared.sample.prepare_graphics(request.area)
                    {
                        result = Err(error.to_string());
                    }
                    previous = result.as_ref().ok().map(|p| (request.clone(), p.clone()));
                    let mut state = worker.0.lock().unwrap();
                    if state.closed {
                        return;
                    }
                    state.completed =
                        Some((request.generation, request.area, result.map(|p| p.sample)));
                }
            })?;
        Ok(Self {
            mailbox,
            generation: 0,
            seed,
            area: ratatui::layout::Rect::default(),
        })
    }

    pub fn request(&mut self, browser: &TerminalBrowser, view: &mut tui::View) {
        self.generation += 1;
        self.resize(browser, view, self.area);
    }

    pub fn resize(
        &mut self,
        browser: &TerminalBrowser,
        view: &mut tui::View,
        area: ratatui::layout::Rect,
    ) {
        self.area = area;
        view.sample = None;
        view.preview_error = "Loading selected Profile...".into();
        let mut state = self.mailbox.0.lock().unwrap();
        state.completed = None;
        state.pending = browser.selected_profile().cloned().map(|profile| Request {
            generation: self.generation,
            profile,
            seed: self.seed,
            area: self.area,
        });
        if state.pending.is_none() {
            view.preview_error = "No Profiles; choose Create.".into();
        }
        self.mailbox.1.notify_one();
    }

    pub fn poll(&mut self, view: &mut tui::View) -> bool {
        let completed = self.mailbox.0.lock().unwrap().completed.take();
        if let Some((generation, area, result)) = completed {
            if generation != self.generation || area != self.area {
                return false;
            }
            match result {
                Ok(sample) => {
                    view.sample = Some(sample);
                    view.preview_error.clear();
                }
                Err(error) => {
                    view.sample = None;
                    view.preview_error = error;
                }
            }
            return true;
        }
        false
    }
}

impl Drop for Previews {
    fn drop(&mut self) {
        let mut state = self.mailbox.0.lock().unwrap();
        state.closed = true;
        state.pending = None;
        self.mailbox.1.notify_one();
        // A blocked filesystem/network read must never delay terminal restoration.
        // The detached read-only worker exits after that read (or process exit).
    }
}

pub(super) fn set_seed(
    application: &mut Application,
    id: &IntentId,
    seed: ResolutionSeed,
) -> Result<ProfileIntent, CliError> {
    let intent = application.load_profile(id)?;
    application.seed = if matches!(
        intent.wallpaper,
        Some(WallpaperIntent::Source {
            selection: WallpaperSelection::Random,
            ..
        })
    ) {
        Some(seed)
    } else {
        None
    };
    Ok(intent)
}

#[derive(Clone)]
struct Prepared {
    sample: Sample,
    files: Vec<FileVersion>,
}

impl Prepared {
    fn changed(&self) -> bool {
        self.files.iter().any(FileVersion::changed)
    }
}

#[derive(Clone, PartialEq)]
struct FileVersion {
    path: PathBuf,
    stamp: Option<(u64, std::time::SystemTime, i64, i64)>,
}

impl FileVersion {
    fn capture(path: PathBuf) -> Self {
        let stamp = fs::metadata(&path).ok().and_then(|m| {
            #[cfg(unix)]
            let change = {
                use std::os::unix::fs::MetadataExt;
                (m.ctime(), m.ctime_nsec())
            };
            #[cfg(not(unix))]
            let change = (0, 0);
            Some((m.len(), m.modified().ok()?, change.0, change.1))
        });
        Self { path, stamp }
    }
    fn changed(&self) -> bool {
        *self != Self::capture(self.path.clone())
    }
}

fn prepare(id: &IntentId, seed: ResolutionSeed) -> Result<Prepared, CliError> {
    let root = process_paths()?.managed_root();
    let mut files = vec![
        FileVersion::capture(root.join("config.toml")),
        FileVersion::capture(root.join("profiles").join(format!("{id}.toml"))),
    ];
    let mut application = Application::load(None)?;
    let intent = set_seed(&mut application, id, seed)?;
    let plan = crate::plan::plan_profile_preview_json(
        &root,
        &application.paths.home,
        &root,
        id,
        &application.config,
        &intent,
        application.seed.as_ref(),
        &application.github,
        &application.themes,
    )
    .map_err(CliError::Plan)?;
    if let Some(root) = plan
        .pointer("/source/resolved_root")
        .and_then(Value::as_str)
    {
        let root = PathBuf::from(root);
        files.push(FileVersion::capture(root.clone()));
        if let Some(candidate) = plan.pointer("/selection/candidate").and_then(Value::as_str) {
            files.push(FileVersion::capture(root.join(candidate)));
        }
    }
    let image = if uses_github_plan(&plan) {
        planned_github_asset_bytes(&plan, &application.github)
    } else {
        planned_local_asset_bytes(&plan)
    }
    .map_err(CliError::Plan)?;
    let manifest =
        crate::codec::manifest::decode(&serde_json::to_vec(&plan["environment"]["manifest"])?)
            .map_err(|e| CliError::Intent(e.to_string()))?;
    let sample = Sample::for_preview(manifest, image.as_deref())
        .map_err(|e| CliError::Input(e.to_string()))?;
    if files.iter().any(FileVersion::changed) {
        return Err(CliError::Input(
            "Profile/image changed during preparation; select again.".into(),
        ));
    }
    Ok(Prepared { sample, files })
}
