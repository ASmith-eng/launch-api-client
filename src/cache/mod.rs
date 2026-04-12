//! File-based cache system for app state, launch list, and launch details.
//!
//! All cache files include a `version` field checked on load — a mismatch is
//! treated as a cache miss (not an error) so the app gracefully re-fetches.
//! Writes are atomic (temp file then rename) to prevent corruption from crashes.
//!
//! Cache TTL is determined by [`CacheStrategy`], which selects a tier based on
//! launch status and proximity. The [`CacheManager`] accepts a [`Clock`]
//! implementation for testable time queries.

use std::path::{Path, PathBuf};

use tokio::io::ErrorKind;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{debug, warn};

use crate::clock::Clock;
use crate::config::CacheConfig;
use crate::error::AppError;
use crate::models::{AppState, LaunchDetailCache, LaunchListCache, CACHE_VERSION};

// ---------------------------------------------------------------------------
// Cache strategy & TTL
// ---------------------------------------------------------------------------

/// Cache TTL tier determined by launch status and proximity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStrategy {
    /// Past launches (status 3, 4, 7) — data is final, never expires.
    Permanent,
    /// Launches >7 days away — long TTL (default 24h).
    LongTerm,
    /// Launches 1–7 days away — medium TTL (default 3h).
    MediumTerm,
    /// Launches <24 hours away — short TTL (default 30m).
    ShortTerm,
    /// In-flight launches (status 6) — very short TTL (default 1m).
    RealTime,
}

/// Status IDs that indicate a completed launch (data won't change).
const PAST_STATUS_IDS: &[u32] = &[3, 4, 7];

/// Status ID for an in-flight launch.
const IN_FLIGHT_STATUS_ID: u32 = 6;

impl CacheStrategy {
    /// Determine the cache strategy for a launch detail based on its status
    /// and how far away the launch is from `now`.
    pub fn for_launch(status_id: u32, net: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        if PAST_STATUS_IDS.contains(&status_id) {
            return Self::Permanent;
        }
        if status_id == IN_FLIGHT_STATUS_ID {
            return Self::RealTime;
        }

        let until_launch = net - now;
        if until_launch > TimeDelta::days(7) {
            Self::LongTerm
        } else if until_launch > TimeDelta::hours(24) {
            Self::MediumTerm
        } else {
            Self::ShortTerm
        }
    }

    /// Return the TTL in minutes for this strategy, driven by config.
    ///
    /// Returns `None` for [`CacheStrategy::Permanent`] (never expires).
    pub fn ttl_minutes(self, config: &CacheConfig) -> Option<u64> {
        match self {
            Self::Permanent => None,
            Self::LongTerm => Some(config.ttl_far_future),
            Self::MediumTerm => Some(config.ttl_near_future),
            Self::ShortTerm => Some(config.ttl_imminent),
            Self::RealTime => Some(config.ttl_active),
        }
    }

}

/// Compute the `expires_at` timestamp for a given strategy.
///
/// For [`CacheStrategy::Permanent`], returns `DateTime::<Utc>::MAX_UTC`
/// so that `is_stale()` never returns true.
pub fn compute_expires_at(
    strategy: CacheStrategy,
    fetched_at: DateTime<Utc>,
    config: &CacheConfig,
) -> DateTime<Utc> {
    match strategy.ttl_minutes(config) {
        None => DateTime::<Utc>::MAX_UTC,
        Some(minutes) => fetched_at + TimeDelta::minutes(minutes as i64),
    }
}

/// Compute the `expires_at` timestamp for the launch list cache.
pub fn list_expires_at(fetched_at: DateTime<Utc>, config: &CacheConfig) -> DateTime<Utc> {
    fetched_at + TimeDelta::minutes(config.ttl_launch_list as i64)
}

// ---------------------------------------------------------------------------
// CacheManager
// ---------------------------------------------------------------------------

/// Manages reading and writing cache files on disk.
///
/// Generic over [`Clock`] so tests can use `FakeClock` to control time.
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
pub struct CacheManager<C: Clock> {
    cache_dir: PathBuf,
    clock: C,
}

impl<C: Clock> CacheManager<C> {
    pub fn new(cache_dir: PathBuf, clock: C) -> Self {
        Self { cache_dir, clock }
    }

    // -- App state --------------------------------------------------------

    pub async fn load_app_state(&self) -> Result<Option<AppState>, AppError> {
        load_versioned(&self.app_state_path()).await
    }

    pub async fn save_app_state(&self, state: &AppState) -> Result<(), AppError> {
        write_atomic(&self.app_state_path(), state).await
    }

    // -- Launch list ------------------------------------------------------

    pub async fn load_launch_list(&self) -> Result<Option<LaunchListCache>, AppError> {
        load_versioned(&self.launch_list_path()).await
    }

    pub async fn save_launch_list(&self, cache: &LaunchListCache) -> Result<(), AppError> {
        write_atomic(&self.launch_list_path(), cache).await
    }

    // -- Launch details ---------------------------------------------------

    pub async fn load_launch_detail(
        &self,
        launch_id: &str,
    ) -> Result<Option<LaunchDetailCache>, AppError> {
        load_versioned(&self.detail_path(launch_id)).await
    }

    pub async fn save_launch_detail(&self, detail: &LaunchDetailCache) -> Result<(), AppError> {
        write_atomic(&self.detail_path(&detail.launch_id), detail).await
    }

    // -- Pruning ----------------------------------------------------------

    /// Prune old detail cache files using age-based then count-based deletion.
    ///
    /// 1. Delete files with `fetched_at` older than `max_detail_age_days`.
    ///    Corrupt/unreadable files are also deleted.
    /// 2. If remaining files exceed `max_detail_files`, delete the oldest
    ///    (by `fetched_at`) until within the limit.
    ///
    /// Returns the number of files deleted.
    pub async fn prune_details(&self, config: &CacheConfig) -> Result<u32, AppError> {
        let details_dir = self.details_dir();
        let mut entries = match tokio::fs::read_dir(&details_dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(AppError::CacheIo(e)),
        };

        let now = self.clock.now();
        let max_age = TimeDelta::days(config.max_detail_age_days as i64);
        let mut deleted = 0u32;

        // Collect surviving files with their fetched_at for count-based pass.
        let mut survivors: Vec<(PathBuf, DateTime<Utc>)> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip non-.json files (e.g. leftover .tmp)
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Try to read fetched_at from the file.
            let fetched_at = match read_fetched_at(&path).await {
                Some(ts) => ts,
                None => {
                    // Corrupt or unreadable — delete it
                    warn!(path = %path.display(), "Deleting unreadable detail cache file");
                    let _ = tokio::fs::remove_file(&path).await;
                    deleted += 1;
                    continue;
                }
            };

            // Age-based: delete if older than max_detail_age_days
            if now - fetched_at > max_age {
                debug!(
                    path = %path.display(),
                    age_days = (now - fetched_at).num_days(),
                    "Pruning old detail cache file"
                );
                let _ = tokio::fs::remove_file(&path).await;
                deleted += 1;
                continue;
            }

            survivors.push((path, fetched_at));
        }

        // Count-based: if too many remain, delete oldest first
        let max_files = config.max_detail_files as usize;
        if survivors.len() > max_files {
            // Sort by fetched_at ascending (oldest first)
            survivors.sort_by_key(|(_, ts)| *ts);
            let to_remove = survivors.len() - max_files;
            for (path, _) in survivors.iter().take(to_remove) {
                debug!(path = %path.display(), "Pruning excess detail cache file");
                let _ = tokio::fs::remove_file(path).await;
                deleted += 1;
            }
        }

        if deleted > 0 {
            tracing::info!(deleted, "Detail cache pruning complete");
        }

        Ok(deleted)
    }

    // -- Path helpers -----------------------------------------------------

    fn app_state_path(&self) -> PathBuf {
        self.cache_dir.join("app_state.json")
    }

    fn launch_list_path(&self) -> PathBuf {
        self.cache_dir.join("cache.json")
    }

    fn detail_path(&self, launch_id: &str) -> PathBuf {
        self.details_dir().join(format!("{launch_id}.json"))
    }

    fn details_dir(&self) -> PathBuf {
        self.cache_dir.join("details")
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
async fn load_versioned<T: DeserializeOwned + HasVersion>(path: &Path) -> Result<Option<T>, AppError> {
    let bytes = match tokio::fs::read(path).await {
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

/// Whether cache pruning should run this startup.
///
/// Returns `true` when `startup_count` is a multiple of
/// `prune_every_n_startups`. The config sanitizer guarantees this is >= 1.
pub fn should_prune(startup_count: u32, config: &CacheConfig) -> bool {
    startup_count.is_multiple_of(config.prune_every_n_startups)
}

/// Read just the `fetched_at` field from a detail cache JSON file.
///
/// Returns `None` if the file can't be read or parsed.
async fn read_fetched_at(path: &Path) -> Option<DateTime<Utc>> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let ts_str = value.get("fetched_at")?.as_str()?;
    ts_str.parse::<DateTime<Utc>>().ok()
}

/// Write data to a file atomically: serialize → write temp → rename.
async fn write_atomic<T: Serialize>(path: &Path, data: &T) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(data)?;
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, json.as_bytes()).await?;
    tokio::fs::rename(&tmp, path).await?;
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
    use crate::clock::testing::FakeClock;
    use crate::models::{
        tests::dummy_launch_detail, ActiveFilters, LaunchStatus, LaunchSummary, LocationInfo,
        MissionSummary, PadInfo, Provider, RateLimitState,
    };
    use chrono::{TimeDelta, Utc};
    use std::fs;
    use tempfile::TempDir;

    fn base_time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-03-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// Helper: build a CacheManager backed by a temporary directory.
    fn setup() -> (CacheManager<FakeClock>, TempDir, FakeClock) {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("details")).unwrap();
        let clock = FakeClock::new(base_time());
        let mgr = CacheManager::new(dir.path().to_path_buf(), clock.clone());
        (mgr, dir, clock)
    }

    fn sample_app_state() -> AppState {
        let now = base_time();
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
        let now = base_time();
        LaunchListCache {
            version: CACHE_VERSION,
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(30),
            total_count: 1,
            page_offset: 0,
            active_filters: ActiveFilters::default(),
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
        let now = base_time();
        LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "e3df2ecd-c239-472f-95e4-2b89b4f75800".into(),
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(30),
            ttl_strategy: CacheStrategy::ShortTerm,
            data: dummy_launch_detail(),
        }
    }

    fn default_cache_config() -> CacheConfig {
        CacheConfig::default()
    }

    // =====================================================================
    // CacheManager read/write tests (from step 2.1)
    // =====================================================================

    #[tokio::test]
    async fn app_state_write_then_read() {
        let (mgr, _dir, _clock) = setup();
        let state = sample_app_state();

        mgr.save_app_state(&state).await.unwrap();
        let loaded = mgr.load_app_state().await.unwrap().expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.startup_count, 42);
        assert_eq!(loaded.rate_limit.limit, 15);
        assert_eq!(loaded.rate_limit.requests.len(), 2);
        assert!(!loaded.rate_limit.authenticated);
        assert!(loaded.last_prune.is_some());
    }

    #[tokio::test]
    async fn launch_list_write_then_read() {
        let (mgr, _dir, _clock) = setup();
        let list = sample_launch_list();

        mgr.save_launch_list(&list).await.unwrap();
        let loaded = mgr.load_launch_list().await.unwrap().expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.total_count, 1);
        assert_eq!(loaded.launches.len(), 1);
        assert_eq!(loaded.launches[0].name, "Starship IFT-7");
        assert_eq!(loaded.launches[0].status.abbrev, "Go");
    }

    #[tokio::test]
    async fn launch_detail_write_then_read() {
        let (mgr, _dir, _clock) = setup();
        let detail = sample_launch_detail();

        mgr.save_launch_detail(&detail).await.unwrap();
        let loaded = mgr
            .load_launch_detail("e3df2ecd-c239-472f-95e4-2b89b4f75800")
            .await
            .unwrap()
            .expect("should load");

        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.launch_id, "e3df2ecd-c239-472f-95e4-2b89b4f75800");
        assert_eq!(loaded.ttl_strategy, CacheStrategy::ShortTerm);
        assert_eq!(loaded.data.name, "Starship IFT-7");
    }

    #[tokio::test]
    async fn missing_app_state_returns_none() {
        let (mgr, _dir, _clock) = setup();
        assert!(mgr.load_app_state().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn missing_launch_list_returns_none() {
        let (mgr, _dir, _clock) = setup();
        assert!(mgr.load_launch_list().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn missing_launch_detail_returns_none() {
        let (mgr, _dir, _clock) = setup();
        assert!(mgr.load_launch_detail("nonexistent-uuid").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_app_state_returns_none() {
        let (mgr, dir, _clock) = setup();
        fs::write(dir.path().join("app_state.json"), "not json {{").unwrap();
        assert!(mgr.load_app_state().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_launch_list_returns_none() {
        let (mgr, dir, _clock) = setup();
        fs::write(dir.path().join("cache.json"), "definitely not valid").unwrap();
        assert!(mgr.load_launch_list().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_launch_detail_returns_none() {
        let (mgr, dir, _clock) = setup();
        fs::write(dir.path().join("details/some-uuid.json"), "{{garbage").unwrap();
        assert!(mgr.load_launch_detail("some-uuid").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn app_state_version_mismatch_returns_none() {
        let (mgr, _dir, _clock) = setup();
        let mut state = sample_app_state();
        state.version = 999;
        mgr.save_app_state(&state).await.unwrap();
        assert!(mgr.load_app_state().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn launch_list_version_mismatch_returns_none() {
        let (mgr, _dir, _clock) = setup();
        let mut list = sample_launch_list();
        list.version = 0;
        mgr.save_launch_list(&list).await.unwrap();
        assert!(mgr.load_launch_list().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn launch_detail_version_mismatch_returns_none() {
        let (mgr, _dir, _clock) = setup();
        let mut detail = sample_launch_detail();
        detail.version = 42;
        mgr.save_launch_detail(&detail).await.unwrap();
        assert!(
            mgr.load_launch_detail("e3df2ecd-c239-472f-95e4-2b89b4f75800")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn atomic_write_does_not_leave_tmp_file() {
        let (mgr, dir, _clock) = setup();
        mgr.save_app_state(&sample_app_state()).await.unwrap();
        assert!(!dir.path().join("app_state.tmp").exists());
        assert!(dir.path().join("app_state.json").exists());
    }

    #[tokio::test]
    async fn overwrite_preserves_latest_data() {
        let (mgr, _dir, _clock) = setup();
        let mut state = sample_app_state();
        state.startup_count = 1;
        mgr.save_app_state(&state).await.unwrap();

        state.startup_count = 99;
        mgr.save_app_state(&state).await.unwrap();

        let loaded = mgr.load_app_state().await.unwrap().expect("should load");
        assert_eq!(loaded.startup_count, 99);
    }

    #[tokio::test]
    async fn empty_file_returns_none() {
        let (mgr, dir, _clock) = setup();
        fs::write(dir.path().join("app_state.json"), "").unwrap();
        assert!(mgr.load_app_state().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn valid_json_but_wrong_shape_returns_none() {
        let (mgr, dir, _clock) = setup();
        fs::write(
            dir.path().join("cache.json"),
            r#"{"some_other": "structure"}"#,
        )
        .unwrap();
        assert!(mgr.load_launch_list().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn detail_files_are_keyed_by_launch_id() {
        let (mgr, dir, _clock) = setup();
        let detail = sample_launch_detail();
        mgr.save_launch_detail(&detail).await.unwrap();
        let expected = dir
            .path()
            .join("details/e3df2ecd-c239-472f-95e4-2b89b4f75800.json");
        assert!(expected.exists());
    }

    #[tokio::test]
    async fn multiple_details_coexist() {
        let (mgr, _dir, _clock) = setup();
        let mut d1 = sample_launch_detail();
        d1.launch_id = "aaaa-1111".into();

        let mut d2 = sample_launch_detail();
        d2.launch_id = "bbbb-2222".into();
        d2.data.name = "Falcon 9".into();

        mgr.save_launch_detail(&d1).await.unwrap();
        mgr.save_launch_detail(&d2).await.unwrap();

        let loaded1 = mgr.load_launch_detail("aaaa-1111").await.unwrap().expect("d1");
        let loaded2 = mgr.load_launch_detail("bbbb-2222").await.unwrap().expect("d2");
        assert_eq!(loaded1.data.name, "Starship IFT-7");
        assert_eq!(loaded2.data.name, "Falcon 9");
    }

    // =====================================================================
    // CacheStrategy determination tests (step 2.2)
    // =====================================================================

    #[test]
    fn strategy_past_launch_success() {
        let now = base_time();
        // Status 3 = Launch Successful
        assert_eq!(
            CacheStrategy::for_launch(3, now - TimeDelta::days(1), now),
            CacheStrategy::Permanent
        );
    }

    #[test]
    fn strategy_past_launch_failure() {
        let now = base_time();
        // Status 4 = Launch Failure
        assert_eq!(
            CacheStrategy::for_launch(4, now - TimeDelta::days(1), now),
            CacheStrategy::Permanent
        );
    }

    #[test]
    fn strategy_past_launch_partial_failure() {
        let now = base_time();
        // Status 7 = Partial Failure
        assert_eq!(
            CacheStrategy::for_launch(7, now + TimeDelta::days(30), now),
            CacheStrategy::Permanent
        );
    }

    #[test]
    fn strategy_in_flight() {
        let now = base_time();
        // Status 6 = In Flight — overrides proximity
        assert_eq!(
            CacheStrategy::for_launch(6, now + TimeDelta::days(1), now),
            CacheStrategy::RealTime
        );
    }

    #[test]
    fn strategy_far_future() {
        let now = base_time();
        // Status 1 = Go, >7 days away
        let net = now + TimeDelta::days(8);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::LongTerm
        );
    }

    #[test]
    fn strategy_medium_term() {
        let now = base_time();
        // Status 2 = TBD, 3 days away
        let net = now + TimeDelta::days(3);
        assert_eq!(
            CacheStrategy::for_launch(2, net, now),
            CacheStrategy::MediumTerm
        );
    }

    #[test]
    fn strategy_short_term() {
        let now = base_time();
        // Status 1 = Go, 12 hours away
        let net = now + TimeDelta::hours(12);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
    }

    // -- Boundary tests ---------------------------------------------------

    #[test]
    fn strategy_exactly_7_days_boundary() {
        let now = base_time();
        // Exactly 7 days: not strictly >7 days, so MediumTerm
        let net = now + TimeDelta::days(7);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::MediumTerm
        );
    }

    #[test]
    fn strategy_just_over_7_days() {
        let now = base_time();
        let net = now + TimeDelta::days(7) + TimeDelta::seconds(1);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::LongTerm
        );
    }

    #[test]
    fn strategy_exactly_24_hours_boundary() {
        let now = base_time();
        // Exactly 24 hours: not strictly >24h, so ShortTerm
        let net = now + TimeDelta::hours(24);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
    }

    #[test]
    fn strategy_just_over_24_hours() {
        let now = base_time();
        let net = now + TimeDelta::hours(24) + TimeDelta::seconds(1);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::MediumTerm
        );
    }

    #[test]
    fn strategy_past_net_but_not_past_status() {
        let now = base_time();
        // Launch NET has passed but status is still "Go" (not yet updated)
        // Negative time-to-launch → ShortTerm (treated as imminent)
        let net = now - TimeDelta::hours(1);
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
    }

    #[test]
    fn strategy_in_flight_overrides_proximity() {
        let now = base_time();
        // In-flight even though NET is far away (unusual but possible)
        let net = now + TimeDelta::days(30);
        assert_eq!(
            CacheStrategy::for_launch(6, net, now),
            CacheStrategy::RealTime
        );
    }

    // =====================================================================
    // TTL computation tests (step 2.2)
    // =====================================================================

    #[test]
    fn ttl_permanent_returns_none() {
        let config = default_cache_config();
        assert_eq!(CacheStrategy::Permanent.ttl_minutes(&config), None);
    }

    #[test]
    fn ttl_long_term_uses_config() {
        let config = default_cache_config();
        assert_eq!(
            CacheStrategy::LongTerm.ttl_minutes(&config),
            Some(1440) // 24 hours
        );
    }

    #[test]
    fn ttl_medium_term_uses_config() {
        let config = default_cache_config();
        assert_eq!(
            CacheStrategy::MediumTerm.ttl_minutes(&config),
            Some(180) // 3 hours
        );
    }

    #[test]
    fn ttl_short_term_uses_config() {
        let config = default_cache_config();
        assert_eq!(
            CacheStrategy::ShortTerm.ttl_minutes(&config),
            Some(30) // 30 minutes
        );
    }

    #[test]
    fn ttl_real_time_uses_config() {
        let config = default_cache_config();
        assert_eq!(
            CacheStrategy::RealTime.ttl_minutes(&config),
            Some(1) // 1 minute
        );
    }

    #[test]
    fn ttl_custom_config_values() {
        let mut config = default_cache_config();
        config.ttl_far_future = 720;
        config.ttl_near_future = 60;
        config.ttl_imminent = 10;
        config.ttl_active = 2;

        assert_eq!(CacheStrategy::LongTerm.ttl_minutes(&config), Some(720));
        assert_eq!(CacheStrategy::MediumTerm.ttl_minutes(&config), Some(60));
        assert_eq!(CacheStrategy::ShortTerm.ttl_minutes(&config), Some(10));
        assert_eq!(CacheStrategy::RealTime.ttl_minutes(&config), Some(2));
    }

    // =====================================================================
    // compute_expires_at tests (step 2.2)
    // =====================================================================

    #[test]
    fn expires_at_permanent_is_max() {
        let config = default_cache_config();
        let fetched = base_time();
        let expires = compute_expires_at(CacheStrategy::Permanent, fetched, &config);
        assert_eq!(expires, DateTime::<Utc>::MAX_UTC);
    }

    #[test]
    fn expires_at_long_term() {
        let config = default_cache_config();
        let fetched = base_time();
        let expires = compute_expires_at(CacheStrategy::LongTerm, fetched, &config);
        assert_eq!(expires, fetched + TimeDelta::minutes(1440));
    }

    #[test]
    fn expires_at_short_term() {
        let config = default_cache_config();
        let fetched = base_time();
        let expires = compute_expires_at(CacheStrategy::ShortTerm, fetched, &config);
        assert_eq!(expires, fetched + TimeDelta::minutes(30));
    }

    #[test]
    fn expires_at_real_time() {
        let config = default_cache_config();
        let fetched = base_time();
        let expires = compute_expires_at(CacheStrategy::RealTime, fetched, &config);
        assert_eq!(expires, fetched + TimeDelta::minutes(1));
    }

    #[test]
    fn list_expires_at_uses_config() {
        let config = default_cache_config();
        let fetched = base_time();
        let expires = list_expires_at(fetched, &config);
        assert_eq!(expires, fetched + TimeDelta::minutes(30));
    }

    // =====================================================================
    // is_stale() tests (step 2.2)
    // =====================================================================

    #[test]
    fn launch_list_not_stale_before_expiry() {
        let list = sample_launch_list(); // expires_at = base_time() + 30min
        assert!(!list.is_stale(base_time() + TimeDelta::minutes(29)));
    }

    #[test]
    fn launch_list_stale_at_expiry() {
        let list = sample_launch_list(); // expires_at = base_time() + 30min
        assert!(list.is_stale(base_time() + TimeDelta::minutes(30)));
    }

    #[test]
    fn launch_list_stale_after_expiry() {
        let list = sample_launch_list(); // expires_at = base_time() + 30min
        assert!(list.is_stale(base_time() + TimeDelta::hours(1)));
    }

    #[test]
    fn launch_detail_not_stale_before_expiry() {
        let now = base_time();
        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(1),
            ttl_strategy: CacheStrategy::RealTime,
            data: dummy_launch_detail(),
        };
        assert!(!detail.is_stale(now + TimeDelta::seconds(59)));
    }

    #[test]
    fn launch_detail_stale_at_expiry() {
        let now = base_time();
        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(1),
            ttl_strategy: CacheStrategy::RealTime,
            data: dummy_launch_detail(),
        };
        assert!(detail.is_stale(now + TimeDelta::minutes(1)));
    }

    #[test]
    fn permanent_cache_never_stale() {
        let now = base_time();
        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: DateTime::<Utc>::MAX_UTC,
            ttl_strategy: CacheStrategy::Permanent,
            data: dummy_launch_detail(),
        };
        // Even far in the future, permanent cache is never stale
        assert!(!detail.is_stale(now + TimeDelta::days(365 * 100)));
    }

    // =====================================================================
    // Integrated: strategy → expires_at → is_stale with FakeClock
    // =====================================================================

    #[test]
    fn integrated_short_term_becomes_stale() {
        let now = base_time();
        let config = default_cache_config();
        let net = now + TimeDelta::hours(12); // <24h → ShortTerm

        let strategy = CacheStrategy::for_launch(1, net, now);
        assert_eq!(strategy, CacheStrategy::ShortTerm);

        let expires = compute_expires_at(strategy, now, &config);
        assert_eq!(expires, now + TimeDelta::minutes(30));

        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: expires,
            ttl_strategy: strategy,
            data: dummy_launch_detail(),
        };

        // Fresh at 29 minutes
        assert!(!detail.is_stale(now + TimeDelta::minutes(29)));
        // Stale at 30 minutes
        assert!(detail.is_stale(now + TimeDelta::minutes(30)));
    }

    #[test]
    fn integrated_real_time_becomes_stale_quickly() {
        let now = base_time();
        let config = default_cache_config();

        let strategy = CacheStrategy::for_launch(6, now, now); // In-flight
        assert_eq!(strategy, CacheStrategy::RealTime);

        let expires = compute_expires_at(strategy, now, &config);
        assert_eq!(expires, now + TimeDelta::minutes(1));

        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: expires,
            ttl_strategy: strategy,
            data: dummy_launch_detail(),
        };

        assert!(!detail.is_stale(now + TimeDelta::seconds(59)));
        assert!(detail.is_stale(now + TimeDelta::minutes(1)));
    }

    #[test]
    fn integrated_with_fake_clock_advancing() {
        let (_mgr, _dir, clock) = setup();
        let config = default_cache_config();

        let now = clock.now();
        let net = now + TimeDelta::days(3); // MediumTerm
        let strategy = CacheStrategy::for_launch(2, net, now);
        assert_eq!(strategy, CacheStrategy::MediumTerm);

        let expires = compute_expires_at(strategy, now, &config);
        let list = LaunchListCache {
            fetched_at: now,
            expires_at: expires,
            ..sample_launch_list()
        };

        // Advance clock by 2 hours — still fresh (TTL = 3h)
        clock.advance(TimeDelta::hours(2));
        assert!(!list.is_stale(clock.now()));

        // Advance another 1 hour — now stale
        clock.advance(TimeDelta::hours(1));
        assert!(list.is_stale(clock.now()));
    }

    // =====================================================================
    // CacheStrategy serde tests
    // =====================================================================

    #[test]
    fn strategy_serde_round_trip() {
        for (strategy, expected_str) in [
            (CacheStrategy::Permanent, "\"permanent\""),
            (CacheStrategy::LongTerm, "\"long_term\""),
            (CacheStrategy::MediumTerm, "\"medium_term\""),
            (CacheStrategy::ShortTerm, "\"short_term\""),
            (CacheStrategy::RealTime, "\"real_time\""),
        ] {
            let json = serde_json::to_string(&strategy).unwrap();
            assert_eq!(json, expected_str);
            let deserialized: CacheStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, strategy);
        }
    }

    // =====================================================================
    // Cache pruning tests (step 2.3)
    // =====================================================================

    /// Helper: write a detail cache file with a specific fetched_at time.
    async fn write_detail_at(mgr: &CacheManager<FakeClock>, id: &str, fetched_at: DateTime<Utc>) {
        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: id.into(),
            fetched_at,
            expires_at: fetched_at + TimeDelta::hours(1),
            ttl_strategy: CacheStrategy::LongTerm,
            data: dummy_launch_detail(),
        };
        mgr.save_launch_detail(&detail).await.unwrap();
    }

    // -- should_prune tests -----------------------------------------------

    #[test]
    fn should_prune_at_multiples() {
        let config = default_cache_config(); // prune_every_n_startups = 5
        assert!(should_prune(5, &config));
        assert!(should_prune(10, &config));
        assert!(should_prune(100, &config));
    }

    #[test]
    fn should_not_prune_between_multiples() {
        let config = default_cache_config();
        assert!(!should_prune(1, &config));
        assert!(!should_prune(3, &config));
        assert!(!should_prune(7, &config));
    }

    #[test]
    fn should_prune_every_startup_when_one() {
        let mut config = default_cache_config();
        config.prune_every_n_startups = 1;
        assert!(should_prune(1, &config));
        assert!(should_prune(2, &config));
        assert!(should_prune(99, &config));
    }

    // -- Age-based pruning ------------------------------------------------

    #[tokio::test]
    async fn prune_deletes_old_files() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let config = default_cache_config(); // max_detail_age_days = 30

        // Write one old file (40 days ago) and one fresh file
        write_detail_at(&mgr, "old-launch", now - TimeDelta::days(40)).await;
        write_detail_at(&mgr, "fresh-launch", now - TimeDelta::days(1)).await;

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 1);

        // Old file gone, fresh file remains
        assert!(mgr.load_launch_detail("fresh-launch").await.unwrap().is_some());
        assert!(!mgr.detail_path("old-launch").exists());
    }

    #[tokio::test]
    async fn prune_keeps_files_at_max_age_boundary() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let config = default_cache_config(); // max_detail_age_days = 30

        // Exactly 30 days old: now - fetched_at == 30 days, not > 30 days
        write_detail_at(&mgr, "boundary", now - TimeDelta::days(30)).await;

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 0);
        assert!(mgr.detail_path("boundary").exists());
    }

    #[tokio::test]
    async fn prune_deletes_just_over_max_age() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let config = default_cache_config();

        // 30 days + 1 second: just over the limit
        write_detail_at(
            &mgr,
            "just-over",
            now - TimeDelta::days(30) - TimeDelta::seconds(1),
        ).await;

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 1);
    }

    // -- Count-based pruning ----------------------------------------------

    #[tokio::test]
    async fn prune_count_based_removes_oldest() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let mut config = default_cache_config();
        config.max_detail_files = 3;

        // Create 5 files with staggered fetched_at (all recent enough to survive age check)
        for i in 0..5 {
            write_detail_at(
                &mgr,
                &format!("launch-{i}"),
                now - TimeDelta::hours(5 - i), // 0 is oldest, 4 is newest
            ).await;
        }

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 2); // 5 - 3 = 2 removed

        // Oldest two should be gone
        assert!(!mgr.detail_path("launch-0").exists());
        assert!(!mgr.detail_path("launch-1").exists());
        // Newest three remain
        assert!(mgr.detail_path("launch-2").exists());
        assert!(mgr.detail_path("launch-3").exists());
        assert!(mgr.detail_path("launch-4").exists());
    }

    #[tokio::test]
    async fn prune_count_at_limit_deletes_nothing() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let mut config = default_cache_config();
        config.max_detail_files = 3;

        for i in 0..3 {
            write_detail_at(&mgr, &format!("launch-{i}"), now - TimeDelta::hours(i)).await;
        }

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 0);
    }

    // -- Combined age + count pruning -------------------------------------

    #[tokio::test]
    async fn prune_age_then_count() {
        let (mgr, _dir, _clock) = setup();
        let now = base_time();
        let mut config = default_cache_config();
        config.max_detail_age_days = 30;
        config.max_detail_files = 2;

        // 1 old file (deleted by age), 4 recent files (2 deleted by count)
        write_detail_at(&mgr, "ancient", now - TimeDelta::days(60)).await;
        for i in 0..4 {
            write_detail_at(
                &mgr,
                &format!("recent-{i}"),
                now - TimeDelta::hours(4 - i),
            ).await;
        }

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 3); // 1 age + 2 count

        // ancient gone (age), recent-0 and recent-1 gone (count)
        assert!(!mgr.detail_path("ancient").exists());
        assert!(!mgr.detail_path("recent-0").exists());
        assert!(!mgr.detail_path("recent-1").exists());
        // Newest 2 survive
        assert!(mgr.detail_path("recent-2").exists());
        assert!(mgr.detail_path("recent-3").exists());
    }

    // -- Edge cases -------------------------------------------------------

    #[tokio::test]
    async fn prune_empty_details_dir() {
        let (mgr, _dir, _clock) = setup();
        let config = default_cache_config();
        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn prune_missing_details_dir() {
        let dir = TempDir::new().unwrap();
        // Don't create the details/ subdirectory
        let clock = FakeClock::new(base_time());
        let mgr = CacheManager::new(dir.path().to_path_buf(), clock);
        let config = default_cache_config();

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn prune_deletes_corrupt_files() {
        let (mgr, dir, _clock) = setup();
        let now = base_time();
        let config = default_cache_config();

        // Write one valid file and one corrupt file
        write_detail_at(&mgr, "valid", now).await;
        fs::write(dir.path().join("details/corrupt.json"), "{{not json").unwrap();

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 1); // corrupt file deleted

        assert!(mgr.detail_path("valid").exists());
        assert!(!dir.path().join("details/corrupt.json").exists());
    }

    #[tokio::test]
    async fn prune_skips_non_json_files() {
        let (mgr, dir, _clock) = setup();
        let config = default_cache_config();

        // Write a .tmp file — should be ignored, not deleted
        fs::write(dir.path().join("details/leftover.tmp"), "temp data").unwrap();

        let deleted = mgr.prune_details(&config).await.unwrap();
        assert_eq!(deleted, 0);
        assert!(dir.path().join("details/leftover.tmp").exists());
    }

    #[tokio::test]
    async fn prune_with_clock_advancement() {
        let (mgr, _dir, clock) = setup();
        let now = base_time();
        let mut config = default_cache_config();
        config.max_detail_age_days = 10;

        // Write files that are currently fresh
        write_detail_at(&mgr, "will-expire", now).await;
        write_detail_at(&mgr, "also-expires", now - TimeDelta::days(1)).await;

        // No pruning yet — both are within 10 days
        assert_eq!(mgr.prune_details(&config).await.unwrap(), 0);

        // Advance clock 11 days — both files now exceed max age
        clock.advance(TimeDelta::days(11));
        assert_eq!(mgr.prune_details(&config).await.unwrap(), 2);
    }

    // =====================================================================
    // Clock skew edge cases (step 8.2)
    // =====================================================================

    #[test]
    fn clock_backward_jump_stale_cache_handled_correctly() {
        let list = sample_launch_list(); // expires_at = base_time() + 30min
        // Clock jumps backward 1 hour — expires_at is now in the future
        // relative to the jumped-back time.
        let jumped_back = base_time() - TimeDelta::hours(1);
        assert!(!list.is_stale(jumped_back));
    }

    #[test]
    fn clock_backward_jump_detail_cache_handled_correctly() {
        let now = base_time();
        let detail = LaunchDetailCache {
            version: CACHE_VERSION,
            launch_id: "test".into(),
            fetched_at: now,
            expires_at: now + TimeDelta::minutes(5),
            ttl_strategy: CacheStrategy::ShortTerm,
            data: dummy_launch_detail(),
        };
        // Clock jumps backward — detail should not be considered stale.
        let jumped_back = now - TimeDelta::minutes(30);
        assert!(!detail.is_stale(jumped_back));
    }

    #[test]
    fn clock_backward_jump_strategy_treats_as_imminent() {
        let now = base_time();
        let net = now + TimeDelta::days(2);
        // Clock jumps backward by 10 days — net is now >7 days away.
        let jumped_back = now - TimeDelta::days(10);
        let strategy = CacheStrategy::for_launch(1, net, jumped_back);
        assert_eq!(strategy, CacheStrategy::LongTerm);
    }

    // =====================================================================
    // Cache expiry boundary: in-flight status transitions (step 8.2)
    // =====================================================================

    #[test]
    fn strategy_transitions_from_go_to_success() {
        let now = base_time();
        let net = now - TimeDelta::hours(2); // NET was 2 hours ago

        // Before launch: Go (status 1) → ShortTerm (past NET, still active)
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
        // After success: status 3 → Permanent
        assert_eq!(
            CacheStrategy::for_launch(3, net, now),
            CacheStrategy::Permanent
        );
    }

    #[test]
    fn strategy_transitions_from_go_to_failure() {
        let now = base_time();
        let net = now - TimeDelta::hours(1);

        // Before: Go → ShortTerm
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
        // After failure: status 4 → Permanent
        assert_eq!(
            CacheStrategy::for_launch(4, net, now),
            CacheStrategy::Permanent
        );
    }

    #[test]
    fn strategy_transitions_from_go_to_in_flight() {
        let now = base_time();
        let net = now - TimeDelta::minutes(10);

        // Go → ShortTerm
        assert_eq!(
            CacheStrategy::for_launch(1, net, now),
            CacheStrategy::ShortTerm
        );
        // In-flight (status 6) → RealTime
        assert_eq!(
            CacheStrategy::for_launch(6, net, now),
            CacheStrategy::RealTime
        );
    }
}
