//! File-based cache system for app state, launch list, and launch details.
//!
//! All cache files include a `version` field checked on load — a mismatch is
//! treated as a cache miss (not an error) so the app gracefully re-fetches.
//! Writes are atomic (temp file then rename) to prevent corruption from crashes.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, warn};

use crate::error::AppError;
use crate::models::{AppState, LaunchDetailCache, LaunchListCache, CACHE_VERSION};

/// Manages reading and writing cache files on disk.
///
/// The cache directory layout is:
/// ```text
/// <cache_dir>/
/// ├── app_state.json
/// ├── cache.json
/// └── details/
///     └── {uuid}.json
/// ```
#[derive(Debug)]
pub struct CacheManager {
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    // -- App state --------------------------------------------------------

    pub fn load_app_state(&self) -> Result<Option<AppState>, AppError> {
        load_versioned(&self.app_state_path())
    }

    pub fn save_app_state(&self, state: &AppState) -> Result<(), AppError> {
        write_atomic(&self.app_state_path(), state)
    }

    // -- Launch list ------------------------------------------------------

    pub fn load_launch_list(&self) -> Result<Option<LaunchListCache>, AppError> {
        load_versioned(&self.launch_list_path())
    }

    pub fn save_launch_list(&self, cache: &LaunchListCache) -> Result<(), AppError> {
        write_atomic(&self.launch_list_path(), cache)
    }

    // -- Launch details ---------------------------------------------------

    pub fn load_launch_detail(
        &self,
        launch_id: &str,
    ) -> Result<Option<LaunchDetailCache>, AppError> {
        load_versioned(&self.detail_path(launch_id))
    }

    pub fn save_launch_detail(&self, detail: &LaunchDetailCache) -> Result<(), AppError> {
        write_atomic(&self.detail_path(&detail.launch_id), detail)
    }

    // -- Path helpers -----------------------------------------------------

    fn app_state_path(&self) -> PathBuf {
        self.cache_dir.join("app_state.json")
    }

    fn launch_list_path(&self) -> PathBuf {
        self.cache_dir.join("cache.json")
    }

    fn detail_path(&self, launch_id: &str) -> PathBuf {
        self.cache_dir.join("details").join(format!("{launch_id}.json"))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read and deserialize a versioned cache file.
///
/// Returns `Ok(None)` when:
/// - The file does not exist (normal cache miss)
/// - The file has a version mismatch (logged at DEBUG)
/// - The file contains invalid JSON (logged at WARN)
fn load_versioned<T: DeserializeOwned + HasVersion>(path: &Path) -> Result<Option<T>, AppError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            warn!(path = %path.display(), error = %e, "Cache read I/O error, treating as miss");
            return Ok(None);
        }
    };

    let value: T = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "Corrupt cache file, treating as miss");
            return Ok(None);
        }
    };

    if value.version() != CACHE_VERSION {
        debug!(
            path = %path.display(),
            found = value.version(),
            expected = CACHE_VERSION,
            "Cache version mismatch, treating as miss"
        );
        return Ok(None);
    }

    Ok(Some(value))
}

/// Write data to a file atomically: serialize → write temp → rename.
fn write_atomic<T: Serialize>(path: &Path, data: &T) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(data)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Trait for cache types that carry a `version` field.
trait HasVersion {
    fn version(&self) -> u32;
}

impl HasVersion for AppState {
    fn version(&self) -> u32 {
        self.version
    }
}

impl HasVersion for LaunchListCache {
    fn version(&self) -> u32 {
        self.version
    }
}

impl HasVersion for LaunchDetailCache {
    fn version(&self) -> u32 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        LaunchStatus, LaunchSummary, LocationInfo, MissionSummary, PadInfo, Provider, RateLimitState,
    };
    use chrono::{TimeDelta, Utc};
    use std::fs;
    use tempfile::TempDir;

    /// Helper: build a CacheManager backed by a temporary directory.
    fn setup() -> (CacheManager, TempDir) {
        let dir = TempDir::new().unwrap();
        // Create the details/ subdirectory
        fs::create_dir_all(dir.path().join("details")).unwrap();
        let mgr = CacheManager::new(dir.path().to_path_buf());
        (mgr, dir)
    }

    fn sample_app_state() -> AppState {
        let now = Utc::now();
        AppState {
            version: CACHE_VERSION,
            rate_limit: RateLimitState {
                limit: 15,
                requests: vec![now - TimeDelta::minutes(5), now - TimeDelta::minutes(2)],
                last_sync: now - TimeDelta::minutes(10),
                authenticated: false,
            },
            last_startup: now,
            startup_count: 42,
            last_prune: Some(now - TimeDelta::days(2)),
        }
    }

    fn sample_launch_list() -> LaunchListCache {
        let now = Utc::now();
        LaunchListCache {
            version: CACHE_VERSION,
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(30),
            total_count: 1,
            launches: vec![LaunchSummary {
                id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
                name: "Starship IFT-7".into(),
                net: now + TimeDelta::days(6),
                net_precision: None,
                window_start: None,
                window_end: None,
                status: LaunchStatus {
                    id: 1,
                    name: "Go for Launch".into(),
                    abbrev: "Go".into(),
                },
                launch_service_provider: Provider {
                    name: "SpaceX".into(),
                    provider_type: None,
                },
                pad: PadInfo {
                    name: Some("Launch Pad 39A".into()),
                    location: LocationInfo {
                        name: "Starbase, Texas".into(),
                        timezone_name: Some("America/Chicago".into()),
                        country: None,
                    },
                },
                mission: Some(MissionSummary {
                    name: "Starship IFT-7".into(),
                    mission_type: "Test Flight".into(),
                    description: None,
                    orbit: None,
                }),
            }],
        }
    }

    fn sample_launch_detail() -> LaunchDetailCache {
        let now = Utc::now();
        LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(30),
            ttl_strategy: "imminent".into(),
            data: serde_json::json!({
                "name": "Starship IFT-7",
                "status": { "id": 1, "name": "Go for Launch" }
            }),
        }
    }

    // -- Round-trip tests -------------------------------------------------

    #[test]
    fn app_state_write_then_read() {
        let (mgr, _dir) = setup();
        let state = sample_app_state();

        mgr.save_app_state(&state).unwrap();
        let loaded = mgr.load_app_state().unwrap().expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.startup_count, 42);
        assert_eq!(loaded.rate_limit.limit, 15);
        assert_eq!(loaded.rate_limit.requests.len(), 2);
        assert!(!loaded.rate_limit.authenticated);
        assert!(loaded.last_prune.is_some());
    }

    #[test]
    fn launch_list_write_then_read() {
        let (mgr, _dir) = setup();
        let list = sample_launch_list();

        mgr.save_launch_list(&list).unwrap();
        let loaded = mgr.load_launch_list().unwrap().expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.total_count, 1);
        assert_eq!(loaded.launches.len(), 1);
        assert_eq!(loaded.launches[0].name, "Starship IFT-7");
        assert_eq!(loaded.launches[0].status.abbrev, "Go");
    }

    #[test]
    fn launch_detail_write_then_read() {
        let (mgr, _dir) = setup();
        let detail = sample_launch_detail();

        mgr.save_launch_detail(&detail).unwrap();
        let loaded = mgr
            .load_launch_detail("e3df2ecd-c239-472f-95e4-2b89b4f75800")
            .unwrap()
            .expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.launch_id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(loaded.ttl_strategy, "imminent");
        assert_eq!(loaded.data["name"], "Starship IFT-7");
    }

    // -- Missing file tests -----------------------------------------------

    #[test]
    fn missing_app_state_returns_none() {
        let (mgr, _dir) = setup();
        assert!(mgr.load_app_state().unwrap().is_none());
    }

    #[test]
    fn missing_launch_list_returns_none() {
        let (mgr, _dir) = setup();
        assert!(mgr.load_launch_list().unwrap().is_none());
    }

    #[test]
    fn missing_launch_detail_returns_none() {
        let (mgr, _dir) = setup();
        assert!(mgr.load_launch_detail("nonexistent-uuid").unwrap().is_none());
    }

    // -- Corrupt file tests -----------------------------------------------

    #[test]
    fn corrupt_app_state_returns_none() {
        let (mgr, dir) = setup();
        fs::write(dir.path().join("app_state.json"), "not json {{").unwrap();
        assert!(mgr.load_app_state().unwrap().is_none());
    }

    #[test]
    fn corrupt_launch_list_returns_none() {
        let (mgr, dir) = setup();
        fs::write(dir.path().join("cache.json"), "definitely not valid").unwrap();
        assert!(mgr.load_launch_list().unwrap().is_none());
    }

    #[test]
    fn corrupt_launch_detail_returns_none() {
        let (mgr, dir) = setup();
        fs::write(
            dir.path().join("details/some-uuid.json"),
            "{{garbage",
        )
        .unwrap();
        assert!(mgr.load_launch_detail("some-uuid").unwrap().is_none());
    }

    // -- Version mismatch tests -------------------------------------------

    #[test]
    fn app_state_version_mismatch_returns_none() {
        let (mgr, _dir) = setup();
        let mut state = sample_app_state();
        state.version = 999;
        mgr.save_app_state(&state).unwrap();

        assert!(mgr.load_app_state().unwrap().is_none());
    }

    #[test]
    fn launch_list_version_mismatch_returns_none() {
        let (mgr, _dir) = setup();
        let mut list = sample_launch_list();
        list.version = 0;
        mgr.save_launch_list(&list).unwrap();

        assert!(mgr.load_launch_list().unwrap().is_none());
    }

    #[test]
    fn launch_detail_version_mismatch_returns_none() {
        let (mgr, _dir) = setup();
        let mut detail = sample_launch_detail();
        detail.version = 42;
        mgr.save_launch_detail(&detail).unwrap();

        assert!(
            mgr.load_launch_detail("e3df2ecd-c239-472f-95e4-2b89b4f75800")
                .unwrap()
                .is_none()
        );
    }

    // -- Atomic write tests -----------------------------------------------

    #[test]
    fn atomic_write_does_not_leave_tmp_file() {
        let (mgr, dir) = setup();
        mgr.save_app_state(&sample_app_state()).unwrap();

        // The .tmp file should not remain after a successful write
        assert!(!dir.path().join("app_state.tmp").exists());
        // The final file should exist
        assert!(dir.path().join("app_state.json").exists());
    }

    #[test]
    fn overwrite_preserves_latest_data() {
        let (mgr, _dir) = setup();
        let mut state = sample_app_state();
        state.startup_count = 1;
        mgr.save_app_state(&state).unwrap();

        state.startup_count = 99;
        mgr.save_app_state(&state).unwrap();

        let loaded = mgr.load_app_state().unwrap().expect("should load");
        assert_eq!(loaded.startup_count, 99);
    }

    // -- Edge cases -------------------------------------------------------

    #[test]
    fn empty_file_returns_none() {
        let (mgr, dir) = setup();
        fs::write(dir.path().join("app_state.json"), "").unwrap();
        assert!(mgr.load_app_state().unwrap().is_none());
    }

    #[test]
    fn valid_json_but_wrong_shape_returns_none() {
        let (mgr, dir) = setup();
        fs::write(
            dir.path().join("cache.json"),
            r#"{"some_other": "structure"}"#,
        )
        .unwrap();
        assert!(mgr.load_launch_list().unwrap().is_none());
    }

    #[test]
    fn detail_files_are_keyed_by_launch_id() {
        let (mgr, dir) = setup();
        let detail = sample_launch_detail();
        mgr.save_launch_detail(&detail).unwrap();

        let expected_path = dir
            .path()
            .join("details/e3df2ecd-c239-472f-95e4-2b89b4f75800.json");
        assert!(expected_path.exists());
    }

    #[test]
    fn multiple_details_coexist() {
        let (mgr, _dir) = setup();
        let mut d1 = sample_launch_detail();
        d1.launch_id = "aaaa-1111".into();

        let mut d2 = sample_launch_detail();
        d2.launch_id = "bbbb-2222".into();
        d2.data = serde_json::json!({"name": "Falcon 9"});

        mgr.save_launch_detail(&d1).unwrap();
        mgr.save_launch_detail(&d2).unwrap();

        let loaded1 = mgr.load_launch_detail("aaaa-1111").unwrap().expect("d1");
        let loaded2 = mgr.load_launch_detail("bbbb-2222").unwrap().expect("d2");

        assert_eq!(loaded1.data["name"], "Starship IFT-7");
        assert_eq!(loaded2.data["name"], "Falcon 9");
    }
}
