mod recursive_file_watcher;

use recursive_file_watcher::{
    FilteredNativeRecursiveWatcher, ManualRecursiveWatcher, NativeRecursiveWatcher, WatcherMode,
    collect_files_recursive,
};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Get a subset of files for filtered watching (e.g., every 10th file)
fn get_filtered_files(all_files: &[PathBuf], filter_ratio: usize) -> Vec<PathBuf> {
    all_files
        .iter()
        .enumerate()
        .filter_map(|(i, path)| {
            if i % filter_ratio == 0 {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Benchmark different watcher modes
fn benchmark_watcher(dir: &Path, mode: WatcherMode) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Benchmarking {} Watcher ===", mode.display_name());
    println!("Directory: {}", dir.display());

    // First, count the files
    let start_count = Instant::now();
    let all_files = collect_files_recursive(dir);
    let count_duration = start_count.elapsed();
    println!("File enumeration: {} files in {:?}", all_files.len(), count_duration);

    // For filtered modes, select a subset of files (every 10th file)
    let filter_ratio = 10;
    let filtered_files = get_filtered_files(&all_files, filter_ratio);

    // Setup watcher based on mode
    let start_setup = Instant::now();

    let (setup_time, rx, watched_count) = match mode {
        WatcherMode::Manual => {
            println!("\nSetting up manual recursive watcher (individual file watches)...");
            let watcher = ManualRecursiveWatcher::new(dir)?;
            let setup_time = watcher.setup_time();
            let watched = watcher.files_watched();
            let (_watcher, rx) = watcher.into_parts();
            (setup_time, rx, watched)
        },
        WatcherMode::Native => {
            println!("\nSetting up native recursive watcher...");
            let watcher = NativeRecursiveWatcher::new(dir)?;
            let setup_time = watcher.setup_time();
            let (_watcher, rx) = watcher.into_parts();
            (setup_time, rx, all_files.len())
        },
        WatcherMode::ManualFiltered => {
            println!("\nSetting up manual filtered watcher...");
            println!("Filtering: watching every {}th file ({} out of {} files)",
                     filter_ratio, filtered_files.len(), all_files.len());
            let watcher = ManualRecursiveWatcher::new_with_files(filtered_files.clone())?;
            let setup_time = watcher.setup_time();
            let watched = watcher.files_watched();
            let (_watcher, rx) = watcher.into_parts();
            (setup_time, rx, watched)
        },
        WatcherMode::NativeFiltered => {
            println!("\nSetting up native filtered watcher...");
            println!("Filtering: watching directory but only notifying for {} out of {} files",
                     filtered_files.len(), all_files.len());
            let watcher = NativeRecursiveWatcher::new_with_filter(dir, filtered_files.clone())?;
            let setup_time = watcher.setup_time();
            let watched = watcher.files_filtered();
            let (_watcher, rx) = watcher.into_parts();
            (setup_time, rx, watched)
        },
    };

    let total_setup_time = start_setup.elapsed();

    println!("\n--- Setup Complete ---");
    println!("Watcher setup time: {:?}", setup_time);
    println!("Total setup time (including overhead): {:?}", total_setup_time);
    println!("Files being watched/filtered: {}", watched_count);
    if matches!(mode, WatcherMode::ManualFiltered | WatcherMode::NativeFiltered) {
        println!("Average time per filtered file: {:?}",
                 setup_time / watched_count.max(1) as u32);
    }

    // Keep the watcher alive for a bit to test event handling
    println!("\nWatcher is active. Waiting for events (5 seconds)...");
    println!("(Try modifying some files to see events)");

    // Try to receive events for 5 seconds
    let test_duration = Duration::from_secs(5);
    let test_start = Instant::now();
    let mut event_count = 0;

    while test_start.elapsed() < test_duration {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                event_count += 1;
                if event_count <= 5 {
                    println!("Event #{}: {:?} for {:?}",
                             event_count, event.kind, event.paths);
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {:?}", e);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No events, continue waiting
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("Watcher disconnected");
                break;
            }
        }
    }

    if event_count > 5 {
        println!("... and {} more events", event_count - 5);
    } else if event_count == 0 {
        println!("No events received (this is expected if no files were modified)");
    }

    println!("\n=== Benchmark Complete ===\n");

    Ok(())
}

/// Copy directory recursively to a temporary location
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    // Create destination directory
    fs::create_dir_all(dst)?;

    // Read the source directory
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(file_name);

        if path.is_dir() {
            // Recursively copy subdirectory
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            // Copy file
            fs::copy(&path, &dest_path)?;
        }
    }

    Ok(())
}

/// Run watch test with temporary directory
fn run_watch_test(dir: &Path, mode: WatcherMode) -> Result<(), Box<dyn std::error::Error>> {
    // Get the directory name for the temp path
    let dir_name = dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("test");

    let tmp_dir = PathBuf::from("./tmp").join(dir_name);

    println!("\n=== Watch Test for {} ===", mode.display_name());
    println!("Source directory: {}", dir.display());
    println!("Temporary directory: {}", tmp_dir.display());

    // Step 1: Copy files to temporary directory
    println!("\n1. Copying files to temporary directory...");
    let copy_start = Instant::now();

    // Remove temp dir if it exists
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)?;
    }

    copy_dir_recursive(dir, &tmp_dir)?;
    let copy_duration = copy_start.elapsed();

    let file_count = collect_files_recursive(&tmp_dir).len();
    println!("   Copied {} files in {:?}", file_count, copy_duration);

    // Step 2: Set up watcher
    println!("\n2. Setting up {} watcher...", mode.display_name());
    let setup_start = Instant::now();

    let (_watcher, rx) = match mode {
        WatcherMode::Manual => {
            let watcher = ManualRecursiveWatcher::new(&tmp_dir)?;
            println!("   Setup time: {:?}", watcher.setup_time());
            println!("   Files watched: {}", watcher.files_watched());
            watcher.into_parts()
        },
        WatcherMode::Native => {
            let watcher = NativeRecursiveWatcher::new(&tmp_dir)?;
            println!("   Setup time: {:?}", watcher.setup_time());
            watcher.into_parts()
        },
        WatcherMode::ManualFiltered => {
            let all_files = collect_files_recursive(&tmp_dir);
            let filtered_files = get_filtered_files(&all_files, 10);
            let watcher = ManualRecursiveWatcher::new_with_files(filtered_files)?;
            println!("   Setup time: {:?}", watcher.setup_time());
            println!("   Files watched: {}", watcher.files_watched());
            watcher.into_parts()
        },
        WatcherMode::NativeFiltered => {
            let all_files = collect_files_recursive(&tmp_dir);
            let filtered_files = get_filtered_files(&all_files, 10);
            let watcher = NativeRecursiveWatcher::new_with_filter(&tmp_dir, filtered_files)?;
            println!("   Setup time: {:?}", watcher.setup_time());
            println!("   Files filtered: {}", watcher.files_filtered());
            watcher.into_parts()
        },
    };

    let setup_duration = setup_start.elapsed();
    println!("   Total setup time: {:?}", setup_duration);

    // Step 3: Run tests (modify files and observe events)
    println!("\n3. Running file modification tests...");

    // Get some files to modify
    let test_files = collect_files_recursive(&tmp_dir);
    let files_to_modify: Vec<_> = test_files.iter()
        .take(5.min(test_files.len()))
        .collect();

    if files_to_modify.is_empty() {
        println!("   No files to modify for testing");
    } else {
        println!("   Modifying {} test files...", files_to_modify.len());

        // Start event collection thread
        let (event_tx, event_rx) = mpsc::channel();
        let test_duration = Duration::from_secs(3);

        std::thread::spawn(move || {
            let start = Instant::now();
            let mut events = Vec::new();

            while start.elapsed() < test_duration {
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(Ok(event)) => {
                        events.push(event);
                    }
                    Ok(Err(e)) => {
                        eprintln!("Watch error: {:?}", e);
                    }
                    Err(_) => {
                        // Timeout or disconnected
                    }
                }
            }

            event_tx.send(events).unwrap();
        });

        // Give watcher time to stabilize
        std::thread::sleep(Duration::from_millis(100));

        // Modify files
        let modify_start = Instant::now();
        for (i, file_path) in files_to_modify.iter().enumerate() {
            // Append to file
            if let Ok(mut content) = fs::read_to_string(file_path) {
                content.push_str(&format!("\n// Modified by test {}", i));
                if let Err(e) = fs::write(file_path, content) {
                    eprintln!("   Failed to modify {}: {}", file_path.display(), e);
                }
            }
            // Small delay between modifications
            std::thread::sleep(Duration::from_millis(10));
        }
        let modify_duration = modify_start.elapsed();

        println!("   Modified {} files in {:?}", files_to_modify.len(), modify_duration);

        // Wait for events
        println!("   Collecting events for {:?}...", test_duration);

        // Get collected events
        if let Ok(events) = event_rx.recv_timeout(test_duration + Duration::from_secs(1)) {
            println!("   Received {} events", events.len());

            // Show first few events
            for (i, event) in events.iter().take(3).enumerate() {
                println!("   Event {}: {:?}", i + 1, event.kind);
            }

            if events.len() > 3 {
                println!("   ... and {} more events", events.len() - 3);
            }
        }
    }

    // Step 4: Cleanup
    println!("\n4. Cleaning up temporary directory...");
    let cleanup_start = Instant::now();
    fs::remove_dir_all(&tmp_dir)?;
    let cleanup_duration = cleanup_start.elapsed();
    println!("   Cleanup completed in {:?}", cleanup_duration);

    println!("\n=== Watch Test Complete ===\n");

    Ok(())
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <directory> <mode>", program);
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  manual           - Manually recursive: watch each file individually");
    eprintln!("  native           - Native recursive: use built-in recursive watching");
    eprintln!("  manual-filtered  - Manual with subset: watch only every 10th file");
    eprintln!("  native-filtered  - Native with filter: watch dir but filter events");
    eprintln!("  compare          - Compare manual vs native modes");
    eprintln!("  compare-filtered - Compare filtered manual vs filtered native");
    eprintln!();
    eprintln!("Test Modes (with file modifications):");
    eprintln!("  test-manual      - Test manual watcher with file modifications");
    eprintln!("  test-native      - Test native watcher with file modifications");
    eprintln!("  test-filtered    - Test both filtered watchers");
    eprintln!("  test-all         - Run all watch tests");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} ./test-tree manual", program);
    eprintln!("  {} ./test-tree native", program);
    eprintln!("  {} ./test-tree test-manual", program);
    eprintln!("  {} ./test-tree test-all", program);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let dir_path = Path::new(&args[1]);
    let mode_str = &args[2];

    if !dir_path.exists() {
        eprintln!("Error: Directory '{}' does not exist", dir_path.display());
        eprintln!();
        eprintln!("Hint: First create a test directory with the JavaScript generator:");
        eprintln!("  node ./scripts/generate-tree.js 2 ./test-tree");
        eprintln!();
        eprintln!("Then run this benchmark:");
        eprintln!("  cargo run --release ./test-tree manual");
        std::process::exit(1);
    }

    if !dir_path.is_dir() {
        eprintln!("Error: '{}' is not a directory", dir_path.display());
        std::process::exit(1);
    }

    // Run benchmark based on mode
    let result = match mode_str.as_str() {
        "compare" => {
            // Run both modes and compare
            println!("Comparing manual vs native recursive watching");
            println!();
            println!("Test directory: {}", dir_path.display());

            let files = collect_files_recursive(dir_path);
            println!("Total files in directory: {}", files.len());

            println!("\n{}", "=".repeat(60));

            // Store results for comparison
            let mut manual_time = Duration::default();
            let mut native_time = Duration::default();

            // Run manual mode
            match ManualRecursiveWatcher::new(dir_path) {
                Ok(watcher) => {
                    manual_time = watcher.setup_time();
                    println!("\nManual Recursive Watcher:");
                    println!("  Setup time: {:?}", manual_time);
                    println!("  Files watched: {}", watcher.files_watched());
                },
                Err(e) => eprintln!("Manual watcher failed: {}", e),
            }

            println!("\n{}", "=".repeat(60));

            // Run native mode
            match NativeRecursiveWatcher::new(dir_path) {
                Ok(watcher) => {
                    native_time = watcher.setup_time();
                    println!("\nNative Recursive Watcher:");
                    println!("  Setup time: {:?}", native_time);
                },
                Err(e) => eprintln!("Native watcher failed: {}", e),
            }

            println!("\n{}", "=".repeat(60));
            println!("\n📊 Comparison Results:");
            println!("  Manual setup time: {:?}", manual_time);
            println!("  Native setup time: {:?}", native_time);

            if native_time < manual_time {
                let speedup = manual_time.as_nanos() as f64 / native_time.as_nanos() as f64;
                println!("  Native is {:.2}x faster", speedup);
            } else {
                let speedup = native_time.as_nanos() as f64 / manual_time.as_nanos() as f64;
                println!("  Manual is {:.2}x faster", speedup);
            }

            Ok(())
        },
        "compare-filtered" => {
            // Compare filtered modes
            println!("Comparing filtered manual vs filtered native watching");
            println!();
            println!("Test directory: {}", dir_path.display());

            let all_files = collect_files_recursive(dir_path);
            let filtered_files = get_filtered_files(&all_files, 10);
            println!("Total files: {}, Filtered to: {} files", all_files.len(), filtered_files.len());

            println!("\n{}", "=".repeat(60));

            // Store results for comparison
            let mut manual_time = Duration::default();
            let mut native_time = Duration::default();

            // Run manual filtered mode
            match ManualRecursiveWatcher::new_with_files(filtered_files.clone()) {
                Ok(watcher) => {
                    manual_time = watcher.setup_time();
                    println!("\nManual Filtered Watcher:");
                    println!("  Setup time: {:?}", manual_time);
                    println!("  Files watched: {}", watcher.files_watched());
                },
                Err(e) => eprintln!("Manual filtered watcher failed: {}", e),
            }

            println!("\n{}", "=".repeat(60));

            // Run native filtered mode
            match NativeRecursiveWatcher::new_with_filter(dir_path, filtered_files.clone()) {
                Ok(watcher) => {
                    native_time = watcher.setup_time();
                    println!("\nNative Filtered Watcher:");
                    println!("  Setup time: {:?}", native_time);
                    println!("  Files filtered: {}", watcher.files_filtered());
                },
                Err(e) => eprintln!("Native filtered watcher failed: {}", e),
            }

            println!("\n{}", "=".repeat(60));
            println!("\n📊 Filtered Comparison Results:");
            println!("  Manual filtered setup time: {:?}", manual_time);
            println!("  Native filtered setup time: {:?}", native_time);

            if native_time < manual_time {
                let speedup = manual_time.as_nanos() as f64 / native_time.as_nanos() as f64;
                println!("  Native filtered is {:.2}x faster", speedup);
            } else {
                let speedup = native_time.as_nanos() as f64 / manual_time.as_nanos() as f64;
                println!("  Manual filtered is {:.2}x faster", speedup);
            }

            Ok(())
        },
        "test-manual" => {
            println!("Running watch test for manual mode");
            run_watch_test(dir_path, WatcherMode::Manual)
        },
        "test-native" => {
            println!("Running watch test for native mode");
            run_watch_test(dir_path, WatcherMode::Native)
        },
        "test-filtered" => {
            println!("Running watch tests for filtered modes");
            println!("\n{}", "=".repeat(60));

            if let Err(e) = run_watch_test(dir_path, WatcherMode::ManualFiltered) {
                eprintln!("Manual filtered test failed: {}", e);
            }

            println!("\n{}", "=".repeat(60));

            if let Err(e) = run_watch_test(dir_path, WatcherMode::NativeFiltered) {
                eprintln!("Native filtered test failed: {}", e);
            }

            Ok(())
        },
        "test-all" => {
            println!("Running all watch tests");

            let modes = [
                WatcherMode::Manual,
                WatcherMode::Native,
                WatcherMode::ManualFiltered,
                WatcherMode::NativeFiltered,
            ];

            for mode in &modes {
                println!("\n{}", "=".repeat(60));
                if let Err(e) = run_watch_test(dir_path, *mode) {
                    eprintln!("{} test failed: {}", mode.display_name(), e);
                }
            }

            Ok(())
        },
        mode_str => {
            // Try to parse as a specific mode
            match WatcherMode::from_str(mode_str) {
                Some(mode) => benchmark_watcher(dir_path, mode),
                None => {
                    eprintln!("Unknown mode: {}", mode_str);
                    print_usage(&args[0]);
                    std::process::exit(1);
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    #[test]
    fn test_benchmark_with_temp_dir() {
        // Create a temporary test directory
        let test_dir = Path::new("test_benchmark_dir");
        fs::create_dir_all(test_dir).unwrap();

        // Create some test files
        for i in 0..5 {
            File::create(test_dir.join(format!("file{}.txt", i))).unwrap();
        }

        // Create a subdirectory with files
        let sub_dir = test_dir.join("subdir");
        fs::create_dir_all(&sub_dir).unwrap();
        for i in 0..3 {
            File::create(sub_dir.join(format!("subfile{}.txt", i))).unwrap();
        }

        // Test both watcher modes
        assert!(benchmark_watcher(test_dir, WatcherMode::Manual).is_ok());
        assert!(benchmark_watcher(test_dir, WatcherMode::Native).is_ok());
        assert!(benchmark_watcher(test_dir, WatcherMode::ManualFiltered).is_ok());
        assert!(benchmark_watcher(test_dir, WatcherMode::NativeFiltered).is_ok());

        // Clean up
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn test_get_filtered_files() {
        let files: Vec<PathBuf> = (0..100)
            .map(|i| PathBuf::from(format!("file{}.txt", i)))
            .collect();

        let filtered = get_filtered_files(&files, 10);
        assert_eq!(filtered.len(), 10); // Should get every 10th file

        let filtered = get_filtered_files(&files, 5);
        assert_eq!(filtered.len(), 20); // Should get every 5th file
    }
}