use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

/// Recursively collect all files in a directory
/// Returns a vector of PathBuf for all files found
pub fn collect_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive_impl(dir, &mut files);
    files
}

/// Helper function to recursively collect files
fn collect_files_recursive_impl(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                // Recurse into subdirectory
                collect_files_recursive_impl(&path, files);
            } else if path.is_file() {
                // Add file to the collection
                files.push(path);
            }
        }
    }
}

/// Manual recursive file watcher that watches each file individually
pub struct ManualRecursiveWatcher {
    watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<Event>>,
    files_watched: usize,
    setup_time: std::time::Duration,
}

impl ManualRecursiveWatcher {
    /// Create a new manual recursive watcher for the specified directory
    pub fn new(dir: &Path) -> notify::Result<Self> {
        // Collect all files recursively
        let files = collect_files_recursive(dir);
        Self::new_with_files(files)
    }

    /// Create a new manual recursive watcher for specific files
    pub fn new_with_files<I>(files_to_watch: I) -> notify::Result<Self>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        // Create a channel for receiving events
        let (tx, rx) = mpsc::channel();

        // Create the watcher with a custom config
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = tx.send(res);  // Ignore send errors when receiver is dropped
            },
            Config::default(),
        )?;

        // Collect the files from the iterator
        let files: Vec<PathBuf> = files_to_watch.into_iter().collect();
        let files_count = files.len();

        println!(
            "ManualRecursiveWatcher: Watching {} specific files",
            files_count
        );

        // Add watch for each file individually (non-recursive mode)
        let start_watch = Instant::now();
        for file_path in &files {
            watcher.watch(file_path, RecursiveMode::NonRecursive)?;
        }
        let watch_duration = start_watch.elapsed();

        println!(
            "ManualRecursiveWatcher: Added watches for {} files in {:?}",
            files_count, watch_duration
        );
        if files_count > 0 {
            println!(
                "ManualRecursiveWatcher: Average time per watch: {:?}",
                watch_duration / files_count as u32
            );
        }

        Ok(Self {
            watcher,
            receiver: rx,
            files_watched: files_count,
            setup_time: watch_duration,
        })
    }

    /// Get the number of files being watched
    pub fn files_watched(&self) -> usize {
        self.files_watched
    }

    /// Get the setup time for adding all watches
    pub fn setup_time(&self) -> std::time::Duration {
        self.setup_time
    }

    /// Get the event receiver
    pub fn receiver(&self) -> &mpsc::Receiver<notify::Result<Event>> {
        &self.receiver
    }

    /// Consume self and return the watcher and receiver
    pub fn into_parts(self) -> (RecommendedWatcher, mpsc::Receiver<notify::Result<Event>>) {
        (self.watcher, self.receiver)
    }
}

/// Native recursive watcher that uses the OS's native recursive watching
pub struct NativeRecursiveWatcher {
    watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<Event>>,
    setup_time: std::time::Duration,
}

/// Native recursive watcher with filtering
pub struct FilteredNativeRecursiveWatcher {
    watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<Event>>,
    filter_files: HashSet<PathBuf>,
    setup_time: std::time::Duration,
}

impl NativeRecursiveWatcher {
    /// Create a new native recursive watcher for the specified directory
    pub fn new(dir: &Path) -> notify::Result<Self> {
        // Create a channel for receiving events
        let (tx, rx) = mpsc::channel();

        // Create the watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = tx.send(res);  // Ignore send errors when receiver is dropped
            },
            Config::default(),
        )?;

        // Watch the directory recursively using native recursive mode
        let start_watch = Instant::now();
        watcher.watch(dir, RecursiveMode::Recursive)?;
        let watch_duration = start_watch.elapsed();

        println!(
            "NativeRecursiveWatcher: Setup native recursive watch in {:?}",
            watch_duration
        );

        Ok(Self {
            watcher,
            receiver: rx,
            setup_time: watch_duration,
        })
    }

    /// Create a new native recursive watcher with file filtering
    pub fn new_with_filter<I>(
        dir: &Path,
        files_to_watch: I,
    ) -> notify::Result<FilteredNativeRecursiveWatcher>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        // Collect files into a HashSet for fast lookup
        let filter_files: HashSet<PathBuf> = files_to_watch
            .into_iter()
            .filter(|p| p.exists() && p.is_file())
            .collect();

        let files_count = filter_files.len();

        // Create a channel for receiving events
        let (tx, rx) = mpsc::channel();

        // Clone the filter_files for the closure
        let filter_files_clone = filter_files.clone();

        // Create the watcher with filtering
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                // Filter events to only include files in our filter set
                if let Ok(event) = &res {
                    // Check if any of the paths in the event are in our filter set
                    let should_send = event
                        .paths
                        .iter()
                        .any(|path| filter_files_clone.contains(path));

                    if should_send {
                        let _ = tx.send(res);  // Ignore send errors when receiver is dropped
                    }
                }
            },
            Config::default(),
        )?;

        // Watch the directory recursively using native recursive mode
        let start_watch = Instant::now();
        watcher.watch(dir, RecursiveMode::Recursive)?;
        let watch_duration = start_watch.elapsed();

        println!(
            "FilteredNativeRecursiveWatcher: Setup native recursive watch with {} file filters in {:?}",
            files_count, watch_duration
        );

        Ok(FilteredNativeRecursiveWatcher {
            watcher,
            receiver: rx,
            filter_files,
            setup_time: watch_duration,
        })
    }

    /// Get the setup time for the native recursive watch
    pub fn setup_time(&self) -> std::time::Duration {
        self.setup_time
    }

    /// Get the event receiver
    pub fn receiver(&self) -> &mpsc::Receiver<notify::Result<Event>> {
        &self.receiver
    }

    /// Consume self and return the watcher and receiver
    pub fn into_parts(self) -> (RecommendedWatcher, mpsc::Receiver<notify::Result<Event>>) {
        (self.watcher, self.receiver)
    }
}

impl FilteredNativeRecursiveWatcher {
    /// Get the number of files being filtered
    pub fn files_filtered(&self) -> usize {
        self.filter_files.len()
    }

    /// Get the setup time for the native recursive watch
    pub fn setup_time(&self) -> std::time::Duration {
        self.setup_time
    }

    /// Get the event receiver
    pub fn receiver(&self) -> &mpsc::Receiver<notify::Result<Event>> {
        &self.receiver
    }

    /// Consume self and return the watcher and receiver
    pub fn into_parts(self) -> (RecommendedWatcher, mpsc::Receiver<notify::Result<Event>>) {
        (self.watcher, self.receiver)
    }
}

/// Watcher mode enum for selecting which type of watcher to use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherMode {
    /// Manual recursive: watch each file individually
    Manual,
    /// Native recursive: use OS's built-in recursive watching
    Native,
    /// Manual with filtered files: watch only specific files
    ManualFiltered,
    /// Native with filtered files: watch directory but filter events
    NativeFiltered,
}

impl WatcherMode {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "native" => Some(Self::Native),
            "manual-filtered" => Some(Self::ManualFiltered),
            "native-filtered" => Some(Self::NativeFiltered),
            _ => None,
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &str {
        match self {
            Self::Manual => "Manual Recursive",
            Self::Native => "Native Recursive",
            Self::ManualFiltered => "Manual Filtered",
            Self::NativeFiltered => "Native Filtered",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    #[test]
    fn test_collect_files_recursive() {
        // Create a temporary test directory
        let test_dir = Path::new("test_temp_watcher_dir");
        fs::create_dir_all(test_dir).unwrap();

        // Create some test files
        File::create(test_dir.join("file1.txt")).unwrap();
        File::create(test_dir.join("file2.txt")).unwrap();

        // Create a subdirectory with files
        let sub_dir = test_dir.join("subdir");
        fs::create_dir_all(&sub_dir).unwrap();
        File::create(sub_dir.join("file3.txt")).unwrap();

        // Test file collection
        let files = collect_files_recursive(test_dir);
        assert_eq!(files.len(), 3);

        // Clean up
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn test_watcher_mode_parsing() {
        assert_eq!(WatcherMode::from_str("manual"), Some(WatcherMode::Manual));
        assert_eq!(WatcherMode::from_str("MANUAL"), Some(WatcherMode::Manual));
        assert_eq!(WatcherMode::from_str("native"), Some(WatcherMode::Native));
        assert_eq!(WatcherMode::from_str("NATIVE"), Some(WatcherMode::Native));
        assert_eq!(WatcherMode::from_str("invalid"), None);
    }
}
