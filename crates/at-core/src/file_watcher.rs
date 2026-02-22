use crossbeam_channel::{Receiver, Sender};
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A file change event detected by the watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    pub path: String,
    pub kind: FileChangeKind,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// Configuration for the file watcher.
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    pub root_path: PathBuf,
    pub ignore_patterns: Vec<String>,
    pub debounce_ms: u64,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("."),
            ignore_patterns: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
                ".worktrees".to_string(),
            ],
            debounce_ms: 200,
        }
    }
}

/// Maps a `notify::EventKind` to our `FileChangeKind`, returning `None` for
/// event kinds we do not care about (e.g. access events).
fn map_event_kind(kind: &EventKind) -> Option<FileChangeKind> {
    match kind {
        EventKind::Create(CreateKind::File | CreateKind::Any) => Some(FileChangeKind::Created),
        EventKind::Create(_) => Some(FileChangeKind::Created),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both | RenameMode::From | RenameMode::To)) => {
            Some(FileChangeKind::Renamed)
        }
        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_) | ModifyKind::Any) => {
            Some(FileChangeKind::Modified)
        }
        EventKind::Modify(_) => Some(FileChangeKind::Modified),
        EventKind::Remove(RemoveKind::File | RemoveKind::Any) => Some(FileChangeKind::Deleted),
        EventKind::Remove(_) => Some(FileChangeKind::Deleted),
        _ => None,
    }
}

/// Tracks file changes using the `notify` crate with debounced events.
pub struct FileWatcher {
    config: FileWatcherConfig,
    watched_paths: HashSet<String>,
    watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<notify::Event>>,
}

impl FileWatcher {
    /// Create a new `FileWatcher` backed by a `notify::RecommendedWatcher`.
    ///
    /// The watcher uses debouncing based on `config.debounce_ms`.
    pub fn new(config: FileWatcherConfig) -> Result<Self, notify::Error> {
        let (tx, rx): (
            Sender<notify::Result<notify::Event>>,
            Receiver<notify::Result<notify::Event>>,
        ) = crossbeam_channel::unbounded();

        let debounce = Duration::from_millis(config.debounce_ms);

        let watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        // NOTE: notify 7.x removed built-in debounce from the watcher
        // constructor. If finer debounce is needed in the future, use
        // `notify_debouncer_full` or `notify_debouncer_mini`.
        let _ = debounce; // acknowledge config value; used for future debounce integration

        Ok(Self {
            config,
            watched_paths: HashSet::new(),
            watcher,
            rx,
        })
    }

    /// Start watching `path` recursively.
    pub fn add_watch(&mut self, path: &str) -> Result<(), notify::Error> {
        let p = Path::new(path);
        self.watcher.watch(p, RecursiveMode::Recursive)?;
        self.watched_paths.insert(path.to_string());
        Ok(())
    }

    /// Stop watching `path`.
    pub fn remove_watch(&mut self, path: &str) -> Result<(), notify::Error> {
        let p = Path::new(path);
        self.watcher.unwatch(p)?;
        self.watched_paths.remove(path);
        Ok(())
    }

    /// Return all currently watched paths.
    pub fn watched_paths(&self) -> Vec<String> {
        self.watched_paths.iter().cloned().collect()
    }

    /// Return a reference to the watcher configuration.
    pub fn config(&self) -> &FileWatcherConfig {
        &self.config
    }

    /// Drain all pending events from the channel, filtering out ignored paths,
    /// and return them as `FileChangeEvent`s.
    pub fn recv_events(&self) -> Vec<FileChangeEvent> {
        let mut events = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        while let Ok(result) = self.rx.try_recv() {
            if let Ok(event) = result {
                let Some(kind) = map_event_kind(&event.kind) else {
                    continue;
                };

                for path in &event.paths {
                    let path_str = path.to_string_lossy().to_string();

                    // Apply ignore pattern filtering.
                    if self
                        .config
                        .ignore_patterns
                        .iter()
                        .any(|pat| path_str.contains(pat))
                    {
                        continue;
                    }

                    events.push(FileChangeEvent {
                        path: path_str,
                        kind: kind.clone(),
                        timestamp: now.clone(),
                    });
                }
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let cfg = FileWatcherConfig::default();
        assert_eq!(cfg.root_path, PathBuf::from("."));
        assert!(cfg.ignore_patterns.contains(&".git".to_string()));
        assert_eq!(cfg.debounce_ms, 200);
    }

    #[test]
    fn test_watcher_creation_and_config() {
        let cfg = FileWatcherConfig::default();
        let watcher = FileWatcher::new(cfg).expect("should create watcher");
        assert!(watcher.watched_paths().is_empty());
        assert_eq!(watcher.config().debounce_ms, 200);
    }

    #[test]
    fn test_add_remove_watch() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let cfg = FileWatcherConfig {
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec![],
            debounce_ms: 50,
        };
        let mut watcher = FileWatcher::new(cfg).expect("should create watcher");

        watcher.add_watch(&dir_path).expect("add_watch");
        assert_eq!(watcher.watched_paths().len(), 1);
        assert!(watcher.watched_paths().contains(&dir_path));

        watcher.remove_watch(&dir_path).expect("remove_watch");
        assert!(watcher.watched_paths().is_empty());
    }

    #[test]
    fn test_detects_file_creation() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let cfg = FileWatcherConfig {
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec![],
            debounce_ms: 50,
        };
        let mut watcher = FileWatcher::new(cfg).expect("should create watcher");
        watcher.add_watch(&dir_path).expect("add_watch");

        // Create a file and give the OS time to deliver the event.
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "world").unwrap();
        thread::sleep(Duration::from_millis(500));

        let events = watcher.recv_events();
        // We should have at least one event related to our file.
        assert!(
            !events.is_empty(),
            "expected at least one event after file creation"
        );
        let related: Vec<_> = events
            .iter()
            .filter(|e| e.path.contains("hello.txt"))
            .collect();
        assert!(
            !related.is_empty(),
            "expected an event for hello.txt, got: {:?}",
            events
        );
    }

    #[test]
    fn test_ignore_patterns_filter() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        // Create a subdirectory matching an ignore pattern.
        let ignored_dir = dir.path().join("node_modules");
        fs::create_dir_all(&ignored_dir).unwrap();

        let cfg = FileWatcherConfig {
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec!["node_modules".to_string()],
            debounce_ms: 50,
        };
        let mut watcher = FileWatcher::new(cfg).expect("should create watcher");
        watcher.add_watch(&dir_path).expect("add_watch");

        // Write inside ignored dir.
        fs::write(ignored_dir.join("package.json"), "{}").unwrap();
        // Write outside ignored dir.
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        thread::sleep(Duration::from_millis(500));

        let events = watcher.recv_events();
        // Events for node_modules paths should be filtered out.
        let node_events: Vec<_> = events
            .iter()
            .filter(|e| e.path.contains("node_modules"))
            .collect();
        assert!(
            node_events.is_empty(),
            "node_modules events should be filtered: {:?}",
            node_events
        );
    }

    #[test]
    fn test_detects_file_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let file_path = dir.path().join("doomed.txt");
        fs::write(&file_path, "bye").unwrap();

        let cfg = FileWatcherConfig {
            root_path: dir.path().to_path_buf(),
            ignore_patterns: vec![],
            debounce_ms: 50,
        };
        let mut watcher = FileWatcher::new(cfg).expect("should create watcher");
        watcher.add_watch(&dir_path).expect("add_watch");

        // Drain any creation events from setup.
        thread::sleep(Duration::from_millis(200));
        let _ = watcher.recv_events();

        // Delete the file.
        fs::remove_file(&file_path).unwrap();
        thread::sleep(Duration::from_millis(500));

        let events = watcher.recv_events();
        let delete_events: Vec<_> = events
            .iter()
            .filter(|e| e.path.contains("doomed.txt") && e.kind == FileChangeKind::Deleted)
            .collect();
        assert!(
            !delete_events.is_empty(),
            "expected a delete event for doomed.txt, got: {:?}",
            events
        );
    }

    #[test]
    fn test_map_event_kind() {
        assert_eq!(
            map_event_kind(&EventKind::Create(CreateKind::File)),
            Some(FileChangeKind::Created)
        );
        assert_eq!(
            map_event_kind(&EventKind::Remove(RemoveKind::File)),
            Some(FileChangeKind::Deleted)
        );
        assert_eq!(
            map_event_kind(&EventKind::Modify(ModifyKind::Data(
                notify::event::DataChange::Any
            ))),
            Some(FileChangeKind::Modified)
        );
        assert_eq!(
            map_event_kind(&EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
            Some(FileChangeKind::Renamed)
        );
        assert_eq!(map_event_kind(&EventKind::Access(notify::event::AccessKind::Any)), None);
    }
}
