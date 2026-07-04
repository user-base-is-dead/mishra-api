//! request trace (Trace)persist
//!
//! recordeachtimes `/v1/messages` The full retry chain of the request, used for troubleshooting."interrupt"typeissue:
//! - aexternalrequest = 1 entry [`TraceRecord`] summarize + N entry [`TraceAttempt`] child record
//! - each hop records the hit credential,HTTP Status code, failure classification, upstream error body fragment, elapsed time.
//!
//! storage:SQLite(`traces.db`),WAL mode. the frontend query goes directly through SQL(index + WHERE + LIMIT),
//! Does not keep an in-memory buffer. A background task periodically cleans records older than the retention days (retention days and the enable switch can be changed at runtime).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{Connection, types::Type};
use serde::{Deserialize, Serialize};

/// trace record the default retention days
const DEFAULT_RETENTION_DAYS: u64 = 7;
/// Maximum length of the upstream error body fragment (bytes).
const ERROR_SNIPPET_MAX: usize = 2048;
/// the default number of entries returned by the query
pub const DEFAULT_QUERY_LIMIT: usize = 200;

/// the result of a single upstream attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceAttempt {
    /// numberseveraltimesattempt(0-based)
    pub attempt: u32,
    /// the hit upstream credential id;0 means no credential was obtained
    pub credential_id: u64,
    /// endpoint name(ide / cli)
    pub endpoint: String,
    /// upstream HTTP status code;None means a network layer failure (the request was not sent/noneresponse)
    pub http_status: Option<u16>,
    /// failedclassify,see [`Outcome`]
    pub outcome: String,
    /// Upstream error body fragment (truncated to [`ERROR_SNIPPET_MAX`])
    pub error_snippet: Option<String>,
    /// elapsed time of this hop (milliseconds)
    pub duration_ms: u64,
}

/// the entry point used by the caller Key type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TraceKeySource {
    /// adminAPIkey.
    MasterApiKey,
    /// Admin UI the client created and distributed in Key.
    ClientKey,
}

impl TraceKeySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MasterApiKey => "masterApiKey",
            Self::ClientKey => "clientKey",
        }
    }

    fn from_db(value: &str, column: usize) -> rusqlite::Result<Self> {
        match value {
            "masterApiKey" => Ok(Self::MasterApiKey),
            "clientKey" => Ok(Self::ClientKey),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown trace key_source: {other}"),
                )),
            )),
        }
    }
}

/// The complete chain of one external request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRecord {
    /// trace id(uuid v4),beforeend key
    pub trace_id: String,
    /// request start time (RFC3339)
    pub ts: String,
    /// client Key id;0 means master apiKey
    pub key_id: u64,
    /// entry Key type, distinguishing the administratorAPIthe key and the created client Key.
    pub key_source: TraceKeySource,
    /// model name
    pub model: String,
    /// iswhetherstreaming
    pub is_stream: bool,
    /// finalstate:success / error / interrupted
    pub final_status: String,
    /// The final hit (success) or the last attempted credential. id
    pub final_credential_id: u64,
    /// Failure classification (top level, convenient for filtering).
    pub error_type: Option<String>,
    /// concise error message for the user
    pub error_message: Option<String>,
    /// total attemptstimescount
    pub total_attempts: u32,
    /// end to end elapsed time (milliseconds)
    pub duration_ms: u64,
    /// Bytes already sent when a stream is interrupted (distinguishes a full failure). vs half cutinterrupt)
    pub interrupted_after_bytes: Option<u64>,
    /// input token(Anthropic basis)
    #[serde(default)]
    pub input_tokens: u64,
    /// output token
    #[serde(default)]
    pub output_tokens: u64,
    /// cachecreate token
    #[serde(default)]
    pub cache_creation_tokens: u64,
    /// cacheread token
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// feeuse(upstream meteringEvent cumulative credits)
    #[serde(default)]
    pub credits: f64,
    /// first Token Latency (milliseconds, only streaming has a value; non streaming is None)
    #[serde(default)]
    pub first_token_ms: Option<u64>,
    /// per hop detail
    pub attempts: Vec<TraceAttempt>,
}

/// failedclassify(attempt.outcome / record.error_type value)
pub mod outcome {
    pub const SUCCESS: &str = "success";
    pub const QUOTA_EXHAUSTED: &str = "quota_exhausted";
    pub const ACCOUNT_THROTTLED: &str = "account_throttled";
    pub const AUTH_FAILED: &str = "auth_failed";
    pub const TRANSIENT: &str = "transient";
    pub const NETWORK_ERROR: &str = "network_error";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const UNKNOWN: &str = "unknown";
    /// only used as record.error_type: the streaming response began but upstream broke midway.
    pub const STREAM_INTERRUPTED: &str = "stream_interrupted";
}

/// Truncates the upstream error body to a safe length (by character boundary, avoiding cutting apart UTF-8)
pub fn truncate_snippet(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() <= ERROR_SNIPPET_MAX {
        return Some(trimmed.to_string());
    }
    let mut end = ERROR_SNIPPET_MAX;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}…(truncated)", &trimmed[..end]))
}

/// trace reporting receiver:provider call on each hop in the retry loop [`Self::on_attempt`]
pub trait TraceSink: Send + Sync {
    fn on_attempt(&self, attempt: TraceAttempt);
}

/// queryfilterentryitem
#[derive(Debug, Default, Clone)]
pub struct TraceQuery {
    /// final_status exactmatch(success/error/interrupted)
    pub status: Option<String>,
    /// error_type exactmatch
    pub error_type: Option<String>,
    /// finalcredential id
    pub credential_id: Option<u64>,
    /// client Key id(0 = master apiKey)
    pub key_id: Option<u64>,
    /// The credential failed on some hop (attempt level, across trace finalstate).
    /// used for"credentialfaileddetails":even the wholeentry trace Eventually succeeds; as long as the credential fails on some hop, it also matches.
    pub failed_attempt_credential_id: Option<u64>,
    /// model name
    pub model: Option<String>,
    /// onlyreturnnon success
    pub only_failed: bool,
    /// Filters by account group: returns only those whose final credential belongs to these. id of trace.
    /// by handler the layer before the query according to group convert the parameter into a credential id fill in whitelist.
    pub credential_ids: Option<Vec<u64>>,
    /// returnentrycountupper limit
    pub limit: usize,
    /// offset (used for pagination)
    pub offset: usize,
}

/// SQLite persiststore
pub struct TraceStore {
    conn: Mutex<Connection>,
    /// iswhetherenable trace written (changeable at runtime).false when insert directlyshort circuit.
    enabled: AtomicBool,
    /// Record retention days (changeable at runtime),cleanup whenread.
    retention_days: AtomicU64,
}

impl TraceStore {
    /// Opens (or creates) the database and creates tables. An empty path is normalized to one under the current directory. traces.db.
    pub fn open(path: PathBuf, enabled: bool, retention_days: u32) -> rusqlite::Result<Self> {
        let path = if path.as_os_str().is_empty() {
            PathBuf::from("traces.db")
        } else {
            path
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("create traces.db directoryfailed {}: {}", parent.display(), e);
                }
            }
        }
        let conn = Connection::open(&path)?;
        // WAL: concurrent reads do not block writes;synchronous=NORMAL: a balance between write throughput and crash safety.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            enabled: AtomicBool::new(enabled),
            retention_days: AtomicU64::new(retention_days.max(1) as u64),
        })
    }

    /// memorydatalibrary(traces.db Fallback when opening fails; lost on process exit, but guarantees Admin querynotcrash)
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            enabled: AtomicBool::new(true),
            retention_days: AtomicU64::new(DEFAULT_RETENTION_DAYS),
        })
    }

    /// olddatabase migration:as traces Fills new columns in the table (idempotent, adds whichever column is missing).
    /// oldversionof traces.db only base columns, the newly added token/credits/first_token_ms/key_source needed here ALTER.
    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut stmt = conn.prepare("PRAGMA table_info(traces)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for name in rows {
                existing.insert(name?);
            }
        }
        // (column name, define) —— and SCHEMA keep consistent with the newly added columns in
        // note key_source without NOT NULL: existing rows in the old database need first to NULL addagainbackfill(SQLite ALTER ADD COLUMN
        // NOT NULL withoutconstant DEFAULT cannot assign to an existing row). A new insert always writes a valid value.
        let columns: [(&str, &str); 7] = [
            ("input_tokens", "INTEGER NOT NULL DEFAULT 0"),
            ("output_tokens", "INTEGER NOT NULL DEFAULT 0"),
            ("cache_creation_tokens", "INTEGER NOT NULL DEFAULT 0"),
            ("cache_read_tokens", "INTEGER NOT NULL DEFAULT 0"),
            ("credits", "REAL NOT NULL DEFAULT 0"),
            ("first_token_ms", "INTEGER"),
            ("key_source", "TEXT"),
        ];
        let key_source_added = !existing.contains("key_source");
        for (name, def) in columns {
            if !existing.contains(name) {
                conn.execute_batch(&format!(
                    "ALTER TABLE traces ADD COLUMN {} {};",
                    name, def
                ))?;
            }
        }
        // old db key_source after the column is first added, by key_id semanticsbackfill:master apiKey (key_id=0) anything outside is treated as the client Key.
        if key_source_added {
            conn.execute_batch(
                "UPDATE traces SET key_source = CASE WHEN key_id = 0 \
                 THEN 'masterApiKey' ELSE 'clientKey' END WHERE key_source IS NULL;",
            )?;
        }
        Ok(())
    }

    /// iswhetherenable trace write
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// setenableswitch
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// fetchretaindaycount
    pub fn retention_days(&self) -> u64 {
        self.retention_days.load(Ordering::Relaxed)
    }

    /// set the retention days (>=1)
    pub fn set_retention_days(&self, days: u32) {
        self.retention_days
            .store(days.max(1) as u64, Ordering::Relaxed);
    }

    /// write a complete trace (traces + attempts within one transaction). Failure only warn, does not block the request.
    /// trace short circuit directly when closed.
    pub fn insert(&self, rec: &TraceRecord) {
        if !self.is_enabled() {
            return;
        }
        let mut conn = self.conn.lock();
        let tx = match conn.transaction() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("trace transactionopenfailed: {}", e);
                return;
            }
        };
        let ts_epoch = chrono::DateTime::parse_from_rfc3339(&rec.ts)
            .map(|d| d.timestamp())
            .unwrap_or_else(|_| Utc::now().timestamp());
        let res = (|| -> rusqlite::Result<()> {
            tx.execute(
                "INSERT OR REPLACE INTO traces (trace_id, ts, ts_epoch, key_id, key_source, model, \
                 is_stream, final_status, final_credential_id, error_type, error_message, \
                 total_attempts, duration_ms, interrupted_after_bytes, \
                 input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, \
                 credits, first_token_ms) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
                rusqlite::params![
                    rec.trace_id,
                    rec.ts,
                    ts_epoch,
                    rec.key_id as i64,
                    rec.key_source.as_str(),
                    rec.model,
                    rec.is_stream as i64,
                    rec.final_status,
                    rec.final_credential_id as i64,
                    rec.error_type,
                    rec.error_message,
                    rec.total_attempts as i64,
                    rec.duration_ms as i64,
                    rec.interrupted_after_bytes.map(|v| v as i64),
                    rec.input_tokens as i64,
                    rec.output_tokens as i64,
                    rec.cache_creation_tokens as i64,
                    rec.cache_read_tokens as i64,
                    rec.credits,
                    rec.first_token_ms.map(|v| v as i64),
                ],
            )?;
            for a in &rec.attempts {
                tx.execute(
                    "INSERT OR REPLACE INTO trace_attempts (trace_id, attempt, credential_id, \
                     endpoint, http_status, outcome, error_snippet, duration_ms) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    rusqlite::params![
                        rec.trace_id,
                        a.attempt as i64,
                        a.credential_id as i64,
                        a.endpoint,
                        a.http_status.map(|v| v as i64),
                        a.outcome,
                        a.error_snippet,
                        a.duration_ms as i64,
                    ],
                )?;
            }
            Ok(())
        })();
        match res {
            Ok(()) => {
                if let Err(e) = tx.commit() {
                    tracing::warn!("trace commitfailed: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!("trace writefailed: {}", e);
            }
        }
    }

    /// paginated query: return (currentpagerecord, the total count matching the condition). only warn failed, return (empty, 0).
    pub fn query_paged(&self, q: &TraceQuery) -> (Vec<TraceRecord>, usize) {
        let conn = self.conn.lock();
        match Self::query_inner(&conn, q) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("trace queryfailed: {}", e);
                (Vec::new(), 0)
            }
        }
    }

    /// Test helper: takes only records, ignores the total count.
    #[cfg(test)]
    fn query(&self, q: &TraceQuery) -> Vec<TraceRecord> {
        self.query_paged(q).0
    }

    /// take [`TraceQuery`] assemble the filter condition into WHERE clause + parameters (all values parameterized and bound).
    fn build_where(q: &TraceQuery) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(s) = &q.status {
            clauses.push("final_status = ?".to_string());
            params.push(Box::new(s.clone()));
        }
        if let Some(t) = &q.error_type {
            clauses.push("error_type = ?".to_string());
            params.push(Box::new(t.clone()));
        }
        if let Some(c) = q.credential_id {
            clauses.push("final_credential_id = ?".to_string());
            params.push(Box::new(c as i64));
        }
        if let Some(k) = q.key_id {
            clauses.push("key_id = ?".to_string());
            params.push(Box::new(k as i64));
        }
        if let Some(c) = q.failed_attempt_credential_id {
            // The credential failed on some hop (regardless of trace whether it finally succeeded)
            clauses.push(
                "EXISTS (SELECT 1 FROM trace_attempts a \
                 WHERE a.trace_id = traces.trace_id \
                 AND a.credential_id = ? AND a.outcome != 'success')"
                    .to_string(),
            );
            params.push(Box::new(c as i64));
        }
        if let Some(m) = &q.model {
            clauses.push("model = ?".to_string());
            params.push(Box::new(m.clone()));
        }
        if let Some(ids) = &q.credential_ids {
            if ids.is_empty() {
                // emptywhitelist = no credentials under this group → force zero match
                clauses.push("1=0".to_string());
            } else {
                let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
                clauses.push(format!(
                    "final_credential_id IN ({})",
                    placeholders.join(",")
                ));
                for id in ids {
                    params.push(Box::new(*id as i64));
                }
            }
        }
        if q.only_failed {
            clauses.push("final_status != 'success'".to_string());
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        (where_sql, params)
    }

    fn query_inner(
        conn: &Connection,
        q: &TraceQuery,
    ) -> rusqlite::Result<(Vec<TraceRecord>, usize)> {
        let (where_sql, params) = Self::build_where(q);
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

        // total count (used for frontend pagination)
        let count_sql = format!("SELECT COUNT(*) FROM traces {}", where_sql);
        let total: i64 = conn.query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))?;

        let limit = if q.limit == 0 {
            DEFAULT_QUERY_LIMIT
        } else {
            q.limit
        };
        let sql = format!(
            "SELECT trace_id, ts, key_id, key_source, model, is_stream, final_status, final_credential_id, \
             error_type, error_message, total_attempts, duration_ms, interrupted_after_bytes, \
             input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens, credits, first_token_ms \
             FROM traces {} ORDER BY ts_epoch DESC LIMIT {} OFFSET {}",
            where_sql, limit, q.offset
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(TraceRecord {
                trace_id: row.get(0)?,
                ts: row.get(1)?,
                key_id: row.get::<_, i64>(2)? as u64,
                key_source: TraceKeySource::from_db(row.get::<_, String>(3)?.as_str(), 3)?,
                model: row.get(4)?,
                is_stream: row.get::<_, i64>(5)? != 0,
                final_status: row.get(6)?,
                final_credential_id: row.get::<_, i64>(7)? as u64,
                error_type: row.get(8)?,
                error_message: row.get(9)?,
                total_attempts: row.get::<_, i64>(10)? as u32,
                duration_ms: row.get::<_, i64>(11)? as u64,
                interrupted_after_bytes: row.get::<_, Option<i64>>(12)?.map(|v| v as u64),
                input_tokens: row.get::<_, i64>(13)? as u64,
                output_tokens: row.get::<_, i64>(14)? as u64,
                cache_creation_tokens: row.get::<_, i64>(15)? as u64,
                cache_read_tokens: row.get::<_, i64>(16)? as u64,
                credits: row.get::<_, f64>(17)?,
                first_token_ms: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
                attempts: Vec::new(),
            })
        })?;
        let mut records: Vec<TraceRecord> = rows.collect::<rusqlite::Result<_>>()?;

        // batchtakeeachentry trace of attempts
        let mut attempt_stmt = conn.prepare(
            "SELECT attempt, credential_id, endpoint, http_status, outcome, error_snippet, \
             duration_ms FROM trace_attempts WHERE trace_id = ? ORDER BY attempt ASC",
        )?;
        for rec in &mut records {
            let attempts = attempt_stmt.query_map([&rec.trace_id], |row| {
                Ok(TraceAttempt {
                    attempt: row.get::<_, i64>(0)? as u32,
                    credential_id: row.get::<_, i64>(1)? as u64,
                    endpoint: row.get(2)?,
                    http_status: row.get::<_, Option<i64>>(3)?.map(|v| v as u16),
                    outcome: row.get(4)?,
                    error_snippet: row.get(5)?,
                    duration_ms: row.get::<_, i64>(6)? as u64,
                })
            })?;
            rec.attempts = attempts.collect::<rusqlite::Result<_>>()?;
        }
        Ok((records, total as usize))
    }

    /// Deletes records past the retention period (traces + associate attempts). only warn failed.
    pub fn cleanup(&self) {
        let cutoff =
            (Utc::now() - chrono::Duration::days(self.retention_days() as i64)).timestamp();
        let mut conn = self.conn.lock();
        let tx = match conn.transaction() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("trace cleanup transactionfailed: {}", e);
                return;
            }
        };
        let res = (|| -> rusqlite::Result<usize> {
            tx.execute(
                "DELETE FROM trace_attempts WHERE trace_id IN \
                 (SELECT trace_id FROM traces WHERE ts_epoch < ?1)",
                [cutoff],
            )?;
            let n = tx.execute("DELETE FROM traces WHERE ts_epoch < ?1", [cutoff])?;
            Ok(n)
        })();
        match res {
            Ok(n) => {
                if let Err(e) = tx.commit() {
                    tracing::warn!("trace cleanupcommitfailed: {}", e);
                } else if n > 0 {
                    tracing::info!("cleaned {} expired entries trace record", n);
                }
            }
            Err(e) => tracing::warn!("trace cleanupfailed: {}", e),
        }
    }

    /// delete the ones associated with the specified credential trace record, avoiding a new account reusing the same one after an account is deleted. credential_id
    /// inherits the old account failure statistics.
    pub fn delete_for_credential(&self, credential_id: u64) {
        if credential_id == 0 {
            return;
        }
        let mut conn = self.conn.lock();
        let tx = match conn.transaction() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("trace credential cleanup transaction failed: {}", e);
                return;
            }
        };
        let res = (|| -> rusqlite::Result<usize> {
            tx.execute(
                "DELETE FROM trace_attempts WHERE credential_id = ?1 \
                 OR trace_id IN (SELECT trace_id FROM traces WHERE final_credential_id = ?1)",
                [credential_id],
            )?;
            let n = tx.execute(
                "DELETE FROM traces WHERE final_credential_id = ?1",
                [credential_id],
            )?;
            Ok(n)
        })();
        match res {
            Ok(n) => {
                if let Err(e) = tx.commit() {
                    tracing::warn!("trace credential cleanup commit failed: {}", e);
                } else if n > 0 {
                    tracing::info!("cleanedcredential #{} of {} entry trace record", credential_id, n);
                }
            }
            Err(e) => tracing::warn!("trace credentialcleanupfailed: {}", e),
        }
    }

    /// Aggregates failed hops by credential, merging into three types: auth, / accountthrottle / other.
    /// statistics trace_attempts in outcome != 'success' ofhop,by credential_id + outcome group.
    /// return credential_id → (auth, throttle, other). only warn failed, return empty.
    pub fn failure_stats(&self) -> std::collections::HashMap<u64, FailureStats> {
        let conn = self.conn.lock();
        let mut out: std::collections::HashMap<u64, FailureStats> =
            std::collections::HashMap::new();
        let mut stmt = match conn.prepare(
            "SELECT credential_id, outcome, COUNT(*) FROM trace_attempts \
             WHERE outcome != 'success' AND credential_id != 0 \
             GROUP BY credential_id, outcome",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("trace failure_stats prepare failed: {}", e);
                return out;
            }
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
            ))
        });
        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("trace failure_stats queryfailed: {}", e);
                return out;
            }
        };
        for r in rows.flatten() {
            let (cred, outcome_str, cnt) = r;
            let s = out.entry(cred).or_default();
            match outcome_str.as_str() {
                "auth_failed" => s.auth += cnt,
                "account_throttled" => s.throttle += cnt,
                _ => s.other += cnt,
            }
        }
        out
    }
}

/// Counts by the credential failure classification (auth, / accountthrottle / other)
#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureStats {
    pub auth: u64,
    pub throttle: u64,
    pub other: u64,
}

/// sharestorehandle
pub type SharedTraceStore = Arc<TraceStore>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS traces (
    trace_id          TEXT PRIMARY KEY,
    ts                TEXT NOT NULL,
    ts_epoch          INTEGER NOT NULL,
    key_id            INTEGER NOT NULL,
    key_source        TEXT,
    model             TEXT NOT NULL,
    is_stream         INTEGER NOT NULL,
    final_status      TEXT NOT NULL,
    final_credential_id INTEGER NOT NULL,
    error_type        TEXT,
    error_message     TEXT,
    total_attempts    INTEGER NOT NULL,
    duration_ms       INTEGER NOT NULL,
    interrupted_after_bytes INTEGER,
    input_tokens      INTEGER NOT NULL DEFAULT 0,
    output_tokens     INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    credits           REAL NOT NULL DEFAULT 0,
    first_token_ms    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_traces_ts ON traces(ts_epoch DESC);
CREATE INDEX IF NOT EXISTS idx_traces_status ON traces(final_status);
CREATE INDEX IF NOT EXISTS idx_traces_cred ON traces(final_credential_id);

CREATE TABLE IF NOT EXISTS trace_attempts (
    trace_id      TEXT NOT NULL,
    attempt       INTEGER NOT NULL,
    credential_id INTEGER NOT NULL,
    endpoint      TEXT NOT NULL,
    http_status   INTEGER,
    outcome       TEXT NOT NULL,
    error_snippet TEXT,
    duration_ms   INTEGER NOT NULL,
    PRIMARY KEY (trace_id, attempt)
);
CREATE INDEX IF NOT EXISTS idx_attempts_trace ON trace_attempts(trace_id);
";

#[cfg(test)]
mod tests {
    use super::*;

    struct TraceSample<'a> {
        trace_id: &'a str,
        status: &'a str,
        credential_id: u64,
        model: &'a str,
    }

    fn sample(input: TraceSample<'_>) -> TraceRecord {
        TraceRecord {
            trace_id: input.trace_id.to_string(),
            ts: Utc::now().to_rfc3339(),
            key_id: 1,
            key_source: TraceKeySource::ClientKey,
            model: input.model.to_string(),
            is_stream: true,
            final_status: input.status.to_string(),
            final_credential_id: input.credential_id,
            error_type: if input.status == "success" {
                None
            } else {
                Some(outcome::ACCOUNT_THROTTLED.to_string())
            },
            error_message: if input.status == "success" {
                None
            } else {
                Some("blocked".to_string())
            },
            total_attempts: 2,
            duration_ms: 1200,
            interrupted_after_bytes: None,
            input_tokens: 1093,
            output_tokens: 779,
            cache_creation_tokens: 0,
            cache_read_tokens: 101760,
            credits: 0.0,
            first_token_ms: None,
            attempts: vec![
                TraceAttempt {
                    attempt: 0,
                    credential_id: 9,
                    endpoint: "ide".to_string(),
                    http_status: Some(429),
                    outcome: outcome::ACCOUNT_THROTTLED.to_string(),
                    error_snippet: Some("suspicious activity".to_string()),
                    duration_ms: 400,
                },
                TraceAttempt {
                    attempt: 1,
                    credential_id: input.credential_id,
                    endpoint: "ide".to_string(),
                    http_status: if input.status == "success" {
                        Some(200)
                    } else {
                        None
                    },
                    outcome: input.status.to_string(),
                    error_snippet: None,
                    duration_ms: 800,
                },
            ],
        }
    }

    fn mem_store() -> TraceStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        TraceStore {
            conn: Mutex::new(conn),
            enabled: AtomicBool::new(true),
            retention_days: AtomicU64::new(DEFAULT_RETENTION_DAYS),
        }
    }

    #[test]
    fn insert_and_query_roundtrip() {
        let store = mem_store();
        store.insert(&sample(TraceSample {
            trace_id: "t1",
            status: "success",
            credential_id: 5,
            model: "claude-opus-4-7",
        }));
        let out = store.query(&TraceQuery {
            limit: 50,
            ..Default::default()
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].trace_id, "t1");
        assert_eq!(out[0].attempts.len(), 2);
        assert_eq!(out[0].attempts[0].outcome, outcome::ACCOUNT_THROTTLED);
        assert_eq!(out[0].key_source, TraceKeySource::ClientKey);
        // token itemized round trip
        assert_eq!(out[0].input_tokens, 1093);
        assert_eq!(out[0].output_tokens, 779);
        assert_eq!(out[0].cache_read_tokens, 101760);
        assert_eq!(out[0].cache_creation_tokens, 0);
    }

    #[test]
    fn disabled_skips_insert() {
        let store = mem_store();
        store.set_enabled(false);
        store.insert(&sample(TraceSample {
            trace_id: "t1",
            status: "success",
            credential_id: 5,
            model: "m1",
        }));
        let out = store.query(&TraceQuery {
            limit: 50,
            ..Default::default()
        });
        assert_eq!(out.len(), 0, "trace should not write when closed");
        // writing resumes after re enabling
        store.set_enabled(true);
        store.insert(&sample(TraceSample {
            trace_id: "t2",
            status: "success",
            credential_id: 5,
            model: "m1",
        }));
        assert_eq!(
            store
                .query(&TraceQuery {
                    limit: 50,
                    ..Default::default()
                })
                .len(),
            1
        );
    }

    #[test]
    fn delete_for_credential_removes_failure_stats() {
        let store = mem_store();
        store.insert(&sample(TraceSample {
            trace_id: "old",
            status: "error",
            credential_id: 5,
            model: "m1",
        }));
        store.insert(&sample(TraceSample {
            trace_id: "keep",
            status: "error",
            credential_id: 6,
            model: "m1",
        }));

        assert!(store.failure_stats().contains_key(&5));
        store.delete_for_credential(5);

        let stats = store.failure_stats();
        assert!(!stats.contains_key(&5));
        assert!(stats.contains_key(&6));
        assert!(
            store
                .query(&TraceQuery {
                    credential_id: Some(5),
                    limit: 50,
                    ..Default::default()
                })
                .is_empty(),
            "deleted credential traces should not attach to a future account with the same id"
        );
    }

    #[test]
    fn filter_only_failed_and_status() {
        let store = mem_store();
        store.insert(&sample(TraceSample {
            trace_id: "ok",
            status: "success",
            credential_id: 5,
            model: "m1",
        }));
        store.insert(&sample(TraceSample {
            trace_id: "bad",
            status: "error",
            credential_id: 6,
            model: "m1",
        }));
        store.insert(&sample(TraceSample {
            trace_id: "cut",
            status: "interrupted",
            credential_id: 7,
            model: "m2",
        }));

        let failed = store.query(&TraceQuery {
            only_failed: true,
            limit: 50,
            ..Default::default()
        });
        assert_eq!(failed.len(), 2);
        assert!(failed.iter().all(|r| r.final_status != "success"));

        let by_status = store.query(&TraceQuery {
            status: Some("interrupted".to_string()),
            limit: 50,
            ..Default::default()
        });
        assert_eq!(by_status.len(), 1);
        assert_eq!(by_status[0].trace_id, "cut");

        let by_model = store.query(&TraceQuery {
            model: Some("m2".to_string()),
            limit: 50,
            ..Default::default()
        });
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model[0].trace_id, "cut");
    }

    #[test]
    fn cleanup_removes_old() {
        let store = mem_store();
        store.insert(&sample(TraceSample {
            trace_id: "recent",
            status: "success",
            credential_id: 5,
            model: "m1",
        }));
        // manually insert oneentry 8 daybeforeofrecord
        {
            let conn = store.conn.lock();
            let old = (Utc::now() - chrono::Duration::days(8)).timestamp();
            conn.execute(
                "INSERT INTO traces (trace_id, ts, ts_epoch, key_id, key_source, model, is_stream, \
                 final_status, final_credential_id, total_attempts, duration_ms) \
                 VALUES ('old','2020',?1,1,'clientKey','m',1,'success',1,1,1)",
                [old],
            )
            .unwrap();
        }
        store.cleanup();
        let out = store.query(&TraceQuery {
            limit: 50,
            ..Default::default()
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].trace_id, "recent");
    }

    #[test]
    fn query_inner_rejects_unknown_key_source() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO traces (trace_id, ts, ts_epoch, key_id, key_source, model, is_stream, \
             final_status, final_credential_id, total_attempts, duration_ms) \
             VALUES ('bad-source','2020',1,1,'unknown','m',1,'success',1,1,1)",
            [],
        )
        .unwrap();

        let result = TraceStore::query_inner(
            &conn,
            &TraceQuery {
                limit: 50,
                ..Default::default()
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn truncate_snippet_respects_limit() {
        assert_eq!(truncate_snippet("  "), None);
        assert_eq!(truncate_snippet("hi"), Some("hi".to_string()));
        let long = "x".repeat(ERROR_SNIPPET_MAX + 100);
        let out = truncate_snippet(&long).unwrap();
        assert!(out.ends_with("…(truncated)"));
        assert!(out.len() <= ERROR_SNIPPET_MAX + 20);
    }
}
