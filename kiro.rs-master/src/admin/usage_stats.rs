//! requestusagerecord + whenorder aggregate
//!
//! recordeachtimes `/v1/messages` request token consumption and hit information:
//! - persist to disk:`usage_log.YYYY-MM-DD.jsonl`,each lineoneentry [`UsageRecord`], roll by local date
//! - memory:[`UsageAggregator`] maintain recent 31 daysmallwhenbucket + recent 31 the day bucket of days, query on demand
//!
//! scan history at startup JSONL The file rebuilds the aggregation, ensuring the trend chart does not lose data after restart.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// JSONL fileretaindaycount
const RETENTION_DAYS: i64 = 31;
/// hourbucketcount(31 days)
const HOUR_BUCKETS: usize = 24 * 31;
/// daybucketcount(31 days)
const DAY_BUCKETS: usize = 31;

/// Usage record of a single request (with JSONL each line corresponds one to one)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    /// request end time (RFC3339)
    pub ts: String,
    /// client Key id;0 means use master apiKey call
    pub key_id: u64,
    /// the actually hit upstream credential id;0 means the request did not reach the upstream
    pub credential_id: u64,
    /// Model name (declared in the request, may contain -thinking suffix)
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// upstream meteringEvent.usage reported credit billing amount (floating point)
    #[serde(default)]
    pub credits: f64,
    /// end to end elapsed time (milliseconds)
    #[serde(default)]
    pub duration_ms: u64,
    /// "success" or "error"
    pub status: String,
}

/// by day rotate of JSONL writer
pub struct UsageRecorder {
    inner: Mutex<RecorderState>,
    dir: PathBuf,
    /// Retention days (changeable at runtime),cleanup_old_logs whenread.
    retention_days: std::sync::atomic::AtomicI64,
}

struct RecorderState {
    /// currentopenof writer andcorresponddate
    current_date: Option<NaiveDate>,
    writer: Option<BufWriter<File>>,
}

impl UsageRecorder {
    /// construct with a specified initial retention days
    pub fn with_retention(dir: PathBuf, retention_days: i64) -> Self {
        // Fallback: when the caller passes an empty path, normalizes it to ".", avoid join produces a path with no directory prefix causing a write CWD
        let dir = if dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            dir
        };
        if !dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                tracing::warn!("create usage_log directoryfailed {}: {}", dir.display(), e);
            }
        }
        Self {
            inner: Mutex::new(RecorderState {
                current_date: None,
                writer: None,
            }),
            dir,
            retention_days: std::sync::atomic::AtomicI64::new(retention_days.max(1)),
        }
    }

    fn log_path(&self, date: NaiveDate) -> PathBuf {
        self.dir
            .join(format!("usage_log.{}.jsonl", date.format("%Y-%m-%d")))
    }

    /// Synchronously writes one record. Failure only warn, does not block the request.
    pub fn record(&self, rec: &UsageRecord) {
        let line = match serde_json::to_string(rec) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("usage_log serializefailed: {}", e);
                return;
            }
        };
        let today = Local::now().date_naive();
        let mut state = self.inner.lock();
        if state.current_date != Some(today) || state.writer.is_none() {
            // switch to the current day file
            let path = self.log_path(today);
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => {
                    state.writer = Some(BufWriter::new(file));
                    state.current_date = Some(today);
                }
                Err(e) => {
                    tracing::warn!("open usage_log {} failed: {}", path.display(), e);
                    return;
                }
            }
        }
        if let Some(w) = state.writer.as_mut() {
            if let Err(e) = writeln!(w, "{}", line) {
                tracing::warn!("write usage_log failed: {}", e);
                return;
            }
            // immediately flush, ensuring the most recent one is not lost on a crash.
            let _ = w.flush();
        }
    }

    /// fetchretaindaycount
    pub fn retention_days(&self) -> i64 {
        self.retention_days
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// set the retention days (>=1)
    pub fn set_retention_days(&self, days: i64) {
        self.retention_days
            .store(days.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    /// Cleans old files past the retention period.
    pub fn cleanup_old_logs(&self) {
        let cutoff = Local::now().date_naive() - Duration::days(self.retention_days());
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(date) = parse_usage_log_filename(&name) {
                if date < cutoff {
                    let _ = std::fs::remove_file(entry.path());
                    tracing::info!("cleanedexpired usage_log: {}", name);
                }
            }
        }
    }
}

fn parse_usage_log_filename(name: &str) -> Option<NaiveDate> {
    // like usage_log.2026-05-22.jsonl
    let body = name.strip_prefix("usage_log.")?.strip_suffix(".jsonl")?;
    NaiveDate::parse_from_str(body, "%Y-%m-%d").ok()
}

/// statistics of a single time bucket
#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub calls: u64,
    pub errors: u64,
    pub credits: f64,
}

impl BucketStats {
    fn add(&mut self, rec: &UsageRecord) {
        self.input_tokens += rec.input_tokens;
        self.output_tokens += rec.output_tokens;
        self.cache_creation_tokens += rec.cache_creation_tokens;
        self.cache_read_tokens += rec.cache_read_tokens;
        self.credits += rec.credits;
        self.calls += 1;
        if rec.status != "success" {
            self.errors += 1;
        }
    }

    /// takeanothera stats accumulate onto itself (used for group re aggregate after filtering)
    fn add_stats(&mut self, other: &BucketStats) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.credits += other.credits;
        self.calls += other.calls;
        self.errors += other.errors;
    }
}

/// single time bucket containing group data
#[derive(Debug, Default, Clone)]
struct BucketEntry {
    /// Bucket start timestamp (an hour bucket is on the hour Unix seconds; the day bucket is local 0 point Unix seconds)
    ts: i64,
    overall: BucketStats,
    by_key: HashMap<u64, BucketStats>,
    by_model: HashMap<String, BucketStats>,
    by_credential: HashMap<u64, BucketStats>,
    by_key_model: HashMap<u64, HashMap<String, BucketStats>>,
    by_key_credential: HashMap<u64, HashMap<u64, BucketStats>>,
}

/// time dimension aggregator
pub struct UsageAggregator {
    inner: parking_lot::RwLock<AggregatorInner>,
}

struct AggregatorInner {
    /// Hour buckets (ring array indexed by bucket start time), the most recent 31 day
    hour_buckets: Vec<BucketEntry>,
    /// Day buckets (by local date), the most recent 31 day
    day_buckets: Vec<BucketEntry>,
}

/// preset aggregation query time range
#[derive(Debug, Clone, Copy)]
pub enum Range {
    Last24h,
    Last7d,
    Last30d,
}

impl Range {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "24h" => Some(Range::Last24h),
            "7d" => Some(Range::Last7d),
            "30d" => Some(Range::Last30d),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsGranularity {
    Hour,
    Day,
}

impl StatsGranularity {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hour" => Some(StatsGranularity::Hour),
            "day" => Some(StatsGranularity::Day),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StatsQueryWindow {
    pub start_ts: i64,
    pub end_ts: i64,
    pub granularity: StatsGranularity,
}

impl StatsQueryWindow {
    pub fn preset(range: Range, granularity: StatsGranularity) -> Self {
        let now = Utc::now().timestamp();
        let start_ts = match range {
            Range::Last24h => now - 24 * 3600,
            Range::Last7d => now - 7 * 24 * 3600,
            Range::Last30d => now - 30 * 24 * 3600,
        };
        Self {
            start_ts,
            end_ts: now,
            granularity,
        }
    }
}

/// time series point (exported to the frontend)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesPoint {
    /// bucket starttime(RFC3339)
    pub ts: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub calls: u64,
    pub errors: u64,
    pub credits: f64,
}

/// modeldistribution
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDistribution {
    pub model: String,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// upstreamcredentialdistribution
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDistribution {
    pub credential_id: u64,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub errors: u64,
}

/// overview:today + cumulative
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewStats {
    /// today(local 0 the call count starting from the point)
    pub today_calls: u64,
    pub today_input_tokens: u64,
    pub today_output_tokens: u64,
    pub today_errors: u64,
    pub today_credits: f64,
    /// recent 7 daily cumulative
    pub week_calls: u64,
    pub week_input_tokens: u64,
    pub week_output_tokens: u64,
    pub week_credits: f64,
}

impl UsageAggregator {
    pub fn new() -> Self {
        Self {
            inner: parking_lot::RwLock::new(AggregatorInner {
                hour_buckets: Vec::new(),
                day_buckets: Vec::new(),
            }),
        }
    }

    /// startwhenfromhistory JSONL rebuild aggregation
    pub fn rebuild_from_logs(&self, dir: &Path) {
        // fallback: normalize the empty path to ".", otherwise read_dir("") will fail causing a rebuild to 0
        let dir_buf;
        let dir = if dir.as_os_str().is_empty() {
            dir_buf = PathBuf::from(".");
            dir_buf.as_path()
        } else {
            dir
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(it) => it,
            Err(e) => {
                tracing::warn!("read usage_log directoryfailed {}: {}", dir.display(), e);
                return;
            }
        };
        let cutoff = Local::now().date_naive() - Duration::days(RETENTION_DAYS);
        let mut count = 0u64;
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let Some(date) = parse_usage_log_filename(&name) else {
                continue;
            };
            if date < cutoff {
                continue;
            }
            let file = match File::open(entry.path()) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
                    self.ingest(&rec);
                    count += 1;
                }
            }
        }
        tracing::info!(
            "UsageAggregator rebuild finishedinto:from {} load {} entryhistoryrecord",
            dir.display(),
            count
        );
    }

    /// Receives one record and places it into its bucket.
    pub fn ingest(&self, rec: &UsageRecord) {
        let dt: DateTime<Utc> = match DateTime::parse_from_rfc3339(&rec.ts) {
            Ok(d) => d.with_timezone(&Utc),
            Err(_) => Utc::now(),
        };
        let local = dt.with_timezone(&Local);

        // Hour bucket start: the local hour on the hour. → convert back UTC unix second
        let hour_start = Local
            .with_ymd_and_hms(local.year(), local.month(), local.day(), local.hour(), 0, 0)
            .single();
        // day bucket start: local 0 point → convert back UTC unix second
        let day_start = Local
            .with_ymd_and_hms(local.year(), local.month(), local.day(), 0, 0, 0)
            .single();

        let hour_ts = hour_start.map(|d| d.timestamp()).unwrap_or(0);
        let day_ts = day_start.map(|d| d.timestamp()).unwrap_or(0);

        let mut inner = self.inner.write();

        upsert_bucket(&mut inner.hour_buckets, hour_ts, rec, HOUR_BUCKETS);
        upsert_bucket(&mut inner.day_buckets, day_ts, rec, DAY_BUCKETS);
    }

    /// whenorderdataquery
    pub fn query_timeseries(
        &self,
        window: StatsQueryWindow,
        key_id: Option<u64>,
        cred_filter: Option<&std::collections::HashSet<u64>>,
    ) -> Vec<TimeSeriesPoint> {
        let inner = self.inner.read();
        let buckets = select_buckets(&inner, window.granularity);

        let mut points: Vec<TimeSeriesPoint> = buckets
            .iter()
            .filter(|b| bucket_in_window(b, window))
            .filter(|b| bucket_matches_key(b, key_id))
            .map(|b| {
                // without group filter → takes the old logic (faster, hits the pre-aggregation). by_key/overall bucket)
                let stats = match cred_filter {
                    None => stats_for_key(b, key_id),
                    Some(allow) => credential_group_for_key(b, key_id)
                        .map(|group| {
                            let mut s = BucketStats::default();
                            for (cid, cs) in group {
                                if allow.contains(cid) {
                                    s.add_stats(cs);
                                }
                            }
                            s
                        })
                        .unwrap_or_default(),
                };
                TimeSeriesPoint {
                    ts: ts_to_rfc3339(b.ts),
                    input_tokens: stats.input_tokens,
                    output_tokens: stats.output_tokens,
                    cache_creation_tokens: stats.cache_creation_tokens,
                    cache_read_tokens: stats.cache_read_tokens,
                    calls: stats.calls,
                    errors: stats.errors,
                    credits: stats.credits,
                }
            })
            .collect();
        points.sort_by_key(|p| p.ts.clone());
        points
    }

    /// modeldistribution
    pub fn query_by_model(
        &self,
        window: StatsQueryWindow,
        key_id: Option<u64>,
    ) -> Vec<ModelDistribution> {
        let inner = self.inner.read();
        let buckets = select_buckets(&inner, window.granularity);
        let mut acc: HashMap<String, BucketStats> = HashMap::new();
        for b in buckets.iter().filter(|b| bucket_in_window(b, window)) {
            let Some(group) = model_group_for_key(b, key_id) else {
                continue;
            };
            for (model, stats) in group {
                let entry = acc.entry(model.clone()).or_default();
                entry.input_tokens += stats.input_tokens;
                entry.output_tokens += stats.output_tokens;
                entry.calls += stats.calls;
            }
        }
        let mut out: Vec<ModelDistribution> = acc
            .into_iter()
            .map(|(model, stats)| ModelDistribution {
                model,
                calls: stats.calls,
                input_tokens: stats.input_tokens,
                output_tokens: stats.output_tokens,
            })
            .collect();
        out.sort_by(|a, b| b.calls.cmp(&a.calls));
        out
    }

    /// upstreamcredentialdistribution
    pub fn query_by_credential(
        &self,
        window: StatsQueryWindow,
        key_id: Option<u64>,
        cred_filter: Option<&std::collections::HashSet<u64>>,
    ) -> Vec<CredentialDistribution> {
        let inner = self.inner.read();
        let buckets = select_buckets(&inner, window.granularity);
        let mut acc: HashMap<u64, BucketStats> = HashMap::new();
        for b in buckets.iter().filter(|b| bucket_in_window(b, window)) {
            let Some(group) = credential_group_for_key(b, key_id) else {
                continue;
            };
            for (id, stats) in group {
                if let Some(allow) = cred_filter {
                    if !allow.contains(id) {
                        continue;
                    }
                }
                let entry = acc.entry(*id).or_default();
                entry.input_tokens += stats.input_tokens;
                entry.output_tokens += stats.output_tokens;
                entry.calls += stats.calls;
                entry.errors += stats.errors;
            }
        }
        let mut out: Vec<CredentialDistribution> = acc
            .into_iter()
            .map(|(id, stats)| CredentialDistribution {
                credential_id: id,
                calls: stats.calls,
                input_tokens: stats.input_tokens,
                output_tokens: stats.output_tokens,
                errors: stats.errors,
            })
            .collect();
        out.sort_by(|a, b| b.calls.cmp(&a.calls));
        out
    }

    /// overview(today + recent 7 days)
    pub fn overview(&self) -> OverviewStats {
        let inner = self.inner.read();
        let today_start = Local
            .with_ymd_and_hms(
                Local::now().year(),
                Local::now().month(),
                Local::now().day(),
                0,
                0,
                0,
            )
            .single()
            .map(|d| d.timestamp())
            .unwrap_or(0);

        let mut today = BucketStats::default();
        for b in inner.hour_buckets.iter().filter(|b| b.ts >= today_start) {
            today.input_tokens += b.overall.input_tokens;
            today.output_tokens += b.overall.output_tokens;
            today.calls += b.overall.calls;
            today.errors += b.overall.errors;
            today.credits += b.overall.credits;
        }

        let week_cutoff = Utc::now().timestamp() - 7 * 24 * 3600;
        let mut week = BucketStats::default();
        for b in inner.hour_buckets.iter().filter(|b| b.ts >= week_cutoff) {
            week.input_tokens += b.overall.input_tokens;
            week.output_tokens += b.overall.output_tokens;
            week.calls += b.overall.calls;
            week.credits += b.overall.credits;
        }

        OverviewStats {
            today_calls: today.calls,
            today_input_tokens: today.input_tokens,
            today_output_tokens: today.output_tokens,
            today_errors: today.errors,
            today_credits: today.credits,
            week_calls: week.calls,
            week_input_tokens: week.input_tokens,
            week_output_tokens: week.output_tokens,
            week_credits: week.credits,
        }
    }
}

impl Default for UsageAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Writes the record into its bucket; if absent, inserts and sorts by time, and removes the oldest when capacity is exceeded.
fn upsert_bucket(buckets: &mut Vec<BucketEntry>, ts: i64, rec: &UsageRecord, max: usize) {
    if let Some(b) = buckets.iter_mut().find(|b| b.ts == ts) {
        add_record_to_bucket(b, rec);
        return;
    }
    let mut entry = BucketEntry {
        ts,
        ..Default::default()
    };
    add_record_to_bucket(&mut entry, rec);
    buckets.push(entry);
    buckets.sort_by_key(|b| b.ts);
    while buckets.len() > max {
        buckets.remove(0);
    }
}

fn add_record_to_bucket(bucket: &mut BucketEntry, rec: &UsageRecord) {
    bucket.overall.add(rec);
    bucket.by_key.entry(rec.key_id).or_default().add(rec);
    bucket
        .by_model
        .entry(rec.model.clone())
        .or_default()
        .add(rec);
    bucket
        .by_key_model
        .entry(rec.key_id)
        .or_default()
        .entry(rec.model.clone())
        .or_default()
        .add(rec);
    if rec.credential_id == 0 {
        return;
    }
    bucket
        .by_credential
        .entry(rec.credential_id)
        .or_default()
        .add(rec);
    bucket
        .by_key_credential
        .entry(rec.key_id)
        .or_default()
        .entry(rec.credential_id)
        .or_default()
        .add(rec);
}

fn bucket_matches_key(bucket: &BucketEntry, key_id: Option<u64>) -> bool {
    key_id
        .map(|id| bucket.by_key.contains_key(&id))
        .unwrap_or(true)
}

fn credential_group_for_key(
    bucket: &BucketEntry,
    key_id: Option<u64>,
) -> Option<&HashMap<u64, BucketStats>> {
    match key_id {
        Some(id) => bucket.by_key_credential.get(&id),
        None => Some(&bucket.by_credential),
    }
}

fn model_group_for_key(
    bucket: &BucketEntry,
    key_id: Option<u64>,
) -> Option<&HashMap<String, BucketStats>> {
    match key_id {
        Some(id) => bucket.by_key_model.get(&id),
        None => Some(&bucket.by_model),
    }
}

fn bucket_in_window(bucket: &BucketEntry, window: StatsQueryWindow) -> bool {
    bucket.ts >= window.start_ts && bucket.ts < window.end_ts
}

fn select_buckets(inner: &AggregatorInner, granularity: StatsGranularity) -> &[BucketEntry] {
    match granularity {
        StatsGranularity::Hour => &inner.hour_buckets,
        StatsGranularity::Day => &inner.day_buckets,
    }
}

fn stats_for_key(bucket: &BucketEntry, key_id: Option<u64>) -> BucketStats {
    match key_id {
        Some(id) => bucket.by_key.get(&id).copied().unwrap_or_default(),
        None => bucket.overall,
    }
}

fn ts_to_rfc3339(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

pub type SharedRecorder = Arc<UsageRecorder>;
pub type SharedAggregator = Arc<UsageAggregator>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_filename() {
        assert!(parse_usage_log_filename("usage_log.2026-05-22.jsonl").is_some());
        assert!(parse_usage_log_filename("foo.bar").is_none());
    }

    #[test]
    fn aggregator_basic_ingest_and_overview() {
        let agg = UsageAggregator::new();
        let now = Utc::now();
        let rec = UsageRecord {
            ts: now.to_rfc3339(),
            key_id: 1,
            credential_id: 5,
            model: "claude-opus-4-7".to_string(),
            input_tokens: 1000,
            output_tokens: 200,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.05,
            duration_ms: 1500,
            status: "success".to_string(),
        };
        agg.ingest(&rec);
        agg.ingest(&rec);

        let ov = agg.overview();
        assert_eq!(ov.today_calls, 2);
        assert_eq!(ov.today_input_tokens, 2000);

        let window = StatsQueryWindow::preset(Range::Last24h, StatsGranularity::Hour);
        let series = agg.query_timeseries(window, None, None);
        assert!(!series.is_empty());

        let by_model = agg.query_by_model(window, None);
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model[0].model, "claude-opus-4-7");
        assert_eq!(by_model[0].calls, 2);

        let by_cred = agg.query_by_credential(window, None, None);
        assert_eq!(by_cred.len(), 1);
        assert_eq!(by_cred[0].credential_id, 5);
    }

    #[test]
    fn aggregator_filters_by_client_key() {
        let agg = UsageAggregator::new();
        let now = Utc::now().to_rfc3339();
        let rec_a = UsageRecord {
            ts: now.clone(),
            key_id: 1,
            credential_id: 5,
            model: "m-a".to_string(),
            input_tokens: 100,
            output_tokens: 20,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.01,
            duration_ms: 100,
            status: "success".to_string(),
        };
        let rec_b = UsageRecord {
            ts: now,
            key_id: 2,
            credential_id: 6,
            model: "m-b".to_string(),
            input_tokens: 300,
            output_tokens: 40,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.02,
            duration_ms: 200,
            status: "error".to_string(),
        };
        agg.ingest(&rec_a);
        agg.ingest(&rec_b);

        let window = StatsQueryWindow::preset(Range::Last24h, StatsGranularity::Hour);
        let series = agg.query_timeseries(window, Some(1), None);
        assert_eq!(series.iter().map(|p| p.calls).sum::<u64>(), 1);
        assert_eq!(series.iter().map(|p| p.input_tokens).sum::<u64>(), 100);

        let by_model = agg.query_by_model(window, Some(1));
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model[0].model, "m-a");

        let by_cred = agg.query_by_credential(window, Some(1), None);
        assert_eq!(by_cred.len(), 1);
        assert_eq!(by_cred[0].credential_id, 5);
    }

    #[test]
    fn aggregator_filters_by_custom_window_and_granularity() {
        let agg = UsageAggregator::new();
        let today = Local::now().date_naive();
        let yesterday = today - Duration::days(1);
        let yesterday_noon = Local
            .with_ymd_and_hms(
                yesterday.year(),
                yesterday.month(),
                yesterday.day(),
                12,
                0,
                0,
            )
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        let today_noon = Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 12, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339();
        let rec_yesterday = UsageRecord {
            ts: yesterday_noon,
            key_id: 0,
            credential_id: 5,
            model: "m-yesterday".to_string(),
            input_tokens: 100,
            output_tokens: 20,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.01,
            duration_ms: 100,
            status: "success".to_string(),
        };
        let rec_today = UsageRecord {
            ts: today_noon,
            key_id: 0,
            credential_id: 5,
            model: "m-today".to_string(),
            input_tokens: 300,
            output_tokens: 40,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.02,
            duration_ms: 100,
            status: "success".to_string(),
        };
        agg.ingest(&rec_yesterday);
        agg.ingest(&rec_today);

        let start_ts = Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let end_ts = Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 23, 59, 59)
            .single()
            .unwrap()
            .timestamp();
        let hour_window = StatsQueryWindow {
            start_ts,
            end_ts,
            granularity: StatsGranularity::Hour,
        };
        let day_window = StatsQueryWindow {
            start_ts,
            end_ts,
            granularity: StatsGranularity::Day,
        };

        let hourly = agg.query_timeseries(hour_window, None, None);
        assert_eq!(hourly.iter().map(|p| p.calls).sum::<u64>(), 1);
        assert_eq!(hourly.iter().map(|p| p.input_tokens).sum::<u64>(), 300);

        let daily = agg.query_timeseries(day_window, None, None);
        assert_eq!(daily.iter().map(|p| p.calls).sum::<u64>(), 1);
        assert_eq!(daily.iter().map(|p| p.output_tokens).sum::<u64>(), 40);
    }

    #[test]
    fn error_record_increments_errors() {
        let agg = UsageAggregator::new();
        let rec = UsageRecord {
            ts: Utc::now().to_rfc3339(),
            key_id: 0,
            credential_id: 0,
            model: "claude-opus-4-7".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            credits: 0.0,
            duration_ms: 100,
            status: "error".to_string(),
        };
        agg.ingest(&rec);
        let ov = agg.overview();
        assert_eq!(ov.today_errors, 1);
    }
}
