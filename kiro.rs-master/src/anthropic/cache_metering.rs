//! relay layer prompt cache(no external dependency)
//!
//! Kiro upstreamdo not dispatch cache_creation / cache_read token charactersegment (measured meteringEvent
//! only give credit billing amount), so here the relay layer simulates it by itself."promptcache", reproduce Anthropic
//! The longest common prefix hit semantics of the sliding window cache:
//!
//! - take prompt ofstableprefixby message Splits the boundary into an increasing chain of prefix segments:
//!   `[tools+system] → [+msg0] → [+msg1] → ... → [+msg(n-2)]`, each segment hash is
//!   the fingerprint accumulated from the start to that boundary,token is the cumulative estimate of this prefix.
//! - mostafteroneentry message(the new input of the current round) not segmented.——itisthis round cache_creation oftail.
//! - lookup take deepesthitsegment = the longest cached prefix = `cache_read_input_tokens`;itsafterto
//!   last segment = `cache_creation_input_tokens`; fully miss → cache_read = 0.
//!
//! Key to hitting across rounds: history messages are byte for byte unchanged, so Turn N+1 ofhistoryprefixsegment hash inevitablyetc.at
//! Turn N the same segment written. Session isolation: the hash chain starts with an isolation seed (preferably metadata
//! session, otherwiseclient Key id), so that different sessions / Key the identical prefixes do not hit each other.
//!
//! memory + JSON persist to disk: written once per minute to `cache_dir/cache_metering.json`,startwhenread
//! expired records will be discarded.**does not depend on Redis or anyexternal KV**.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Default entry limit (prevents unbounded memory growth).
const DEFAULT_CAPACITY: usize = 4096;
/// longest TTL(1h, with Anthropic ttl="1h" aligned)
const MAX_TTL_SECS: i64 = 3600;
/// default TTL(5min,ephemeral defaultvalue)
const DEFAULT_TTL_SECS: i64 = 5 * 60;

/// singlecacheentryentry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// the cumulative estimate of this prefix segment token count
    pub tokens: u32,
    /// expiry timestamp(unix seconds)
    pub expires_at: i64,
    /// last hit time (used for LRU evicted)
    pub last_hit_at: i64,
}

/// The result of one query (one per segment).
#[derive(Debug, Clone, Copy)]
pub struct SegmentResult {
    /// thissegmentiswhetherhit
    pub hit: bool,
    /// thissegmentcumulative tokens(retainprovidedebug / callside extension,dead_code suppressed)
    #[allow(dead_code)]
    pub tokens: u32,
}

/// `compute_cache_usage` the result: cache billing amount + what is needed for proportional apportionment estimate basisbaseline.
///
/// `cache_creation` / `cache_read` is by `estimate_tokens` the cache covered part computed by the basis
/// prefix split; but the final report must convert to**real total basis**(contextUsage truthy or
/// `count_tokens` estimate); the two estimators use different scales, so here it additionally returns two estimate basis
/// the baseline amount, for the caller to use**dimensionless proportional apportionment**:
///   - `cache_covered_est` = the prefix covered by the cache estimate token(= creation + read)
///   - `prompt_total_est`  = whole prompt(including the uncached tail after the deepest breakpoint) estimate token
///
/// callthe side computes based on this `prefix_ratio = cache_covered_est / prompt_total_est`,againmultiplytoreal
/// total obtains the cache covered part; the remainder is the uncached one. `input_tokens`, the three add up mutually exclusively == total.
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheUsage {
    /// cacheread token(estimate basis, deepest hit segment accumulation).
    /// creation part = `cache_covered_est − cache_read`, no need to store separately.
    pub cache_read: i32,
    /// the prefix covered by the cache estimate token total (read + creation).
    pub cache_covered_est: i32,
    /// whole prompt of estimate token The total (the denominator for proportional apportionment).
    pub prompt_total_est: i32,
}

impl CacheUsage {
    /// by real total use the measure for mutually exclusive apportionment, return `(input_tokens, cache_creation, cache_read)`.
    ///
    /// `total_real` is the full amount of the final reported measure prompt token(contextUsage truthypriority,
    /// otherwise `count_tokens` estimate). the three satisfy `input + creation + read == total_real`.
    ///
    /// nonecache override(`cache_covered_est == 0`) or when the baseline is missing, returns directly.
    /// `(total_real, 0, 0)`——allcount in input, do not fabricate cache counts.
    pub fn split_against_total(&self, total_real: i32) -> (i32, i32, i32) {
        let total = total_real.max(0);
        if self.cache_covered_est <= 0 || self.prompt_total_est <= 0 {
            return (total, 0, 0);
        }
        // The ratio is dimensionless and holds across estimators;clamp to [0, total] prevent estimate deviation out of bounds.
        let ratio = (self.cache_covered_est as f64 / self.prompt_total_est as f64).clamp(0.0, 1.0);
        let cache_total = ((total as f64) * ratio).round() as i32;
        let cache_total = cache_total.min(total);
        // Inside the cache covered part, by estimate basis read/creation split the proportion a second time.
        let read = if self.cache_covered_est > 0 {
            ((cache_total as f64) * (self.cache_read as f64 / self.cache_covered_est as f64)).round()
                as i32
        } else {
            0
        };
        let read = read.clamp(0, cache_total);
        let creation = cache_total - read;
        let input = total - cache_total;
        (input, creation, read)
    }
}

/// in process prompt cache
pub struct CacheMeter {
    inner: Mutex<Inner>,
    persist_path: Option<PathBuf>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<u64, CacheEntry>,
    /// Whether there has been a change since the last disk write.
    dirty: bool,
}

impl CacheMeter {
    /// create aempty cache.`persist_path` as `Some` auto loads history from this file.
    pub fn new(persist_path: Option<PathBuf>) -> Self {
        let mut inner = Inner::default();
        if let Some(path) = persist_path.as_ref() {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(entries) = serde_json::from_slice::<HashMap<u64, CacheEntry>>(&bytes) {
                    let now = now_secs();
                    for (k, v) in entries {
                        if v.expires_at > now {
                            inner.entries.insert(k, v);
                        }
                    }
                    tracing::info!(
                        "CacheMeter rebuild:from {} load {} entryhaseffectrecord",
                        path.display(),
                        inner.entries.len()
                    );
                }
            }
        }
        Self {
            inner: Mutex::new(inner),
            persist_path,
        }
    }

    /// Queries a set of prefix segment hashes and returns the hit status of each segment; hit segments are refreshed. last_hit_at.
    ///
    /// `segment_hashes` the order must match the request cache_control the breakpoint order is consistent;
    /// `segment_tokens` iseachsegmentcumulative tokens(namely segment_hashes[i] the corresponding whole segment cumulative value).
    pub fn lookup(&self, segment_hashes: &[u64], segment_tokens: &[u32]) -> Vec<SegmentResult> {
        debug_assert_eq!(segment_hashes.len(), segment_tokens.len());
        let now = now_secs();
        let mut inner = self.inner.lock();
        let mut out = Vec::with_capacity(segment_hashes.len());
        for (h, t) in segment_hashes.iter().zip(segment_tokens.iter()) {
            let hit = match inner.entries.get_mut(h) {
                Some(entry) if entry.expires_at > now => {
                    entry.last_hit_at = now;
                    true
                }
                _ => false,
            };
            out.push(SegmentResult { hit, tokens: *t });
        }
        out
    }

    /// Writes a set of prefix segments into the cache (used for miss register after / renew).`ttl_secs` clip to [60, MAX_TTL_SECS].
    pub fn record(&self, segment_hashes: &[u64], segment_tokens: &[u32], ttl_secs: i64) {
        debug_assert_eq!(segment_hashes.len(), segment_tokens.len());
        let ttl = ttl_secs.clamp(60, MAX_TTL_SECS);
        let now = now_secs();
        let expires_at = now + ttl;
        let mut inner = self.inner.lock();
        for (h, t) in segment_hashes.iter().zip(segment_tokens.iter()) {
            inner.entries.insert(
                *h,
                CacheEntry {
                    tokens: *t,
                    expires_at,
                    last_hit_at: now,
                },
            );
        }
        inner.dirty = true;
        // capacity over limit:by last_hit_at evict the oldest several entries
        if inner.entries.len() > DEFAULT_CAPACITY {
            let drop_n = inner.entries.len() - DEFAULT_CAPACITY;
            let mut victims: Vec<(u64, i64)> = inner
                .entries
                .iter()
                .map(|(k, v)| (*k, v.last_hit_at))
                .collect();
            victims.sort_by_key(|x| x.1);
            for (k, _) in victims.into_iter().take(drop_n) {
                inner.entries.remove(&k);
            }
        }
    }

    /// write the current snapshot to persist_path(only in dirty whenactualpersist to disk)
    pub fn flush_to_disk(&self) {
        let path = match self.persist_path.clone() {
            Some(p) => p,
            None => return,
        };
        let snapshot = {
            let mut inner = self.inner.lock();
            if !inner.dirty {
                return;
            }
            inner.dirty = false;
            inner.entries.clone()
        };
        let json = match serde_json::to_vec(&snapshot) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("CacheMeter serializefailed: {}", e);
                return;
            }
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!("CacheMeter persist to diskfailed {}: {}", path.display(), e);
        }
    }

    /// Starts a background periodic task: periodically flush + clean up expired entriesentryentry
    pub fn spawn_background(self: Arc<Self>) {
        let weak = Arc::downgrade(&self);
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(60);
            loop {
                tokio::time::sleep(interval).await;
                let Some(cache) = weak.upgrade() else { return };
                cache.evict_expired();
                cache.flush_to_disk();
            }
        });
    }

    /// delete expired entries (lookup on a miss due to expiry it just returns miss, will not clean up along the way;
    /// here it is cleaned once per background cycle to avoid memory bloat).
    pub fn evict_expired(&self) {
        let now = now_secs();
        let mut inner = self.inner.lock();
        let before = inner.entries.len();
        inner.entries.retain(|_, v| v.expires_at > now);
        if inner.entries.len() != before {
            inner.dirty = true;
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().entries.len()
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// parse cache_control of ttl string("5m" / "1h")→ second
pub fn parse_ttl(ttl: Option<&str>) -> i64 {
    match ttl {
        Some(s) if s.eq_ignore_ascii_case("1h") => 3600,
        Some(s) if s.eq_ignore_ascii_case("5m") => 300,
        _ => DEFAULT_TTL_SECS,
    }
}

/// `Arc<CacheMeter>` alias
pub type SharedCacheMeter = Arc<CacheMeter>;

// ============================================================================
// the wiring with the request body protocol layer
// ============================================================================

use super::stream::estimate_tokens;
use super::types::{CacheControl, MessagesRequest, SystemMessage, Tool};

/// one extracted by the protocol layer"segment"(segment): all content accumulated from the request start up to this breakpoint.
///
/// `tokens` isthisprefix**cumulative**estimate token count;`hash` from the accumulation of the prefix text SHA-256
/// obtained by folding (take the lower 64 bit as key, with CacheMeter of u64 key compatible).
#[derive(Debug, Clone, Copy)]
struct Segment {
    hash: u64,
    cumulative_tokens: u32,
    /// thissegmentseparateof ttl(seconds)
    ttl_secs: i64,
}

/// call CacheMeter Computes this request cache coverage and records all breakpoints (including hit segments) back.
/// cache, refresh TTL. returns [`CacheUsage`], by the caller when obtaining the real total then do mutually exclusive apportionment.
///
/// **fully by Anthropic protocol**: take the segment index of the deepest hit i*,then(estimate basis)
/// - `cache_read = segments[i*].cumulative_tokens`
/// - `cache_creation = segments.last().cumulative_tokens - segments[i*].cumulative_tokens`
///
/// all miss when cache_read = 0,cache_creation = deepestsegmentcumulative tokens.
///
/// note `cache_creation` onlycumulativeto**deepest breakpoint**until; after the deepest breakpoint the prompt tail
/// (not yetbyany cache_control override)belongattrue input,notcount incache——this is exactly `prompt_total_est`
/// and `cache_covered_est` ofdifferencevalue.
///
/// noneany cache_control at a breakpoint, return all zero `CacheUsage`(`split_against_total`
/// will total allcount in input)andnotwrite.
///
/// `key_id` isclient Key id, used for session isolation: the prefix hash mixes in an isolation seed (preferably taken
/// request metadata inside session, otherwisereturn back key_id), so that different sessions / differentclient Key of
/// cachemutualnothit——The same prefix is reused only within the same session.
pub fn compute_cache_usage(cache: &CacheMeter, req: &MessagesRequest, key_id: u64) -> CacheUsage {
    let (segments, prompt_total_est) = extract_segments(req, key_id);
    if segments.is_empty() {
        // no breakpoint: still bring out prompt_total_est so the caller can extend in the future, but covered=0 → all in input.
        return CacheUsage {
            prompt_total_est: prompt_total_est as i32,
            ..Default::default()
        };
    }

    let hashes: Vec<u64> = segments.iter().map(|s| s.hash).collect();
    let cum_tokens: Vec<u32> = segments.iter().map(|s| s.cumulative_tokens).collect();
    let results = cache.lookup(&hashes, &cum_tokens);

    // diagnostic (DEBUG level): print each segment hash / cumulative token / hit situation, troubleshoot across rounds miss.
    if tracing::enabled!(tracing::Level::DEBUG) {
        let dump: Vec<String> = segments
            .iter()
            .zip(results.iter())
            .enumerate()
            .map(|(i, (s, r))| {
                format!("[{i}] hash={} cum={} hit={}", s.hash, s.cumulative_tokens, r.hit)
            })
            .collect();
        tracing::debug!(
            "CacheMeter: {} segment, msgs={} | {}",
            segments.len(),
            req.messages.len(),
            dump.join(", ")
        );
    }

    let deepest_hit = results.iter().rposition(|r| r.hit);
    // the prefix covered by the cache = Deepest breakpoint accumulation (the tail after the deepest breakpoint is the truly uncached input).
    // on hit read = hitsegmentcumulative,creation = covered − read; all miss when read = 0.
    let covered = *cum_tokens.last().unwrap();
    let cache_read = match deepest_hit {
        Some(i) => cum_tokens[i],
        None => 0u32,
    };

    // Writes all segments back at once (hit segments are refreshed). last_hit_at; missed segments are inserted). All segments share the same
    // ttl(detect_max_ttl the single value), single locking + A single capacity check to avoid repeated per segment overhead.
    cache.record(&hashes, &cum_tokens, segments[0].ttl_secs);

    CacheUsage {
        cache_read: cache_read as i32,
        cache_covered_est: covered as i32,
        prompt_total_est: prompt_total_est as i32,
    }
}

/// Extracts breakpoint segments in order from the request body:tools → system → messages
///
/// thisitemorderand Anthropic concatenate prompt oforderforalign:tools inmostbefore,system secondary,
/// soafteronly thenis messages.each time it meetstoa cache_control a breakpoint produces one Segment.
/// cumulative token The count accumulates in processing order and is always the current position"prefixtotal amount".
///
/// return `(segments, prompt_total_est)`, where `prompt_total_est` isfinished feedingwhole prompt
/// (including the tail after the deepest breakpoint) of estimate token accumulation, used as the denominator for proportional apportionment.
///
/// `key_id` Used for session isolation: the hash starts with an isolation seed (preferably using metadata session, otherwise
/// key_id), the seed is not counted token, only letting the same prefix in different sessions produce different hash → mutualnothit.
fn extract_segments(req: &MessagesRequest, key_id: u64) -> (Vec<Segment>, u32) {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut cum_tokens: u32 = 0;
    let mut segments: Vec<Segment> = Vec::new();

    // Session isolation seed: as the frontmost input of the hash chain, does not enter token estimate. The prefix is stable within the same session.
    // reuse;acrosssession / acrossclient Key the same prefix differs due to a different seed. hash different, do not hit each other.
    hasher.update(isolation_seed(req, key_id).as_bytes());

    // feed decouple hashand token estimate:`hash_text` into the hash chain (determines the hit),`token_text`
    // enter token accumulation (determines the numeric basis). Separating the two is to let token countcountattachrecent**original text**,
    // not by the signature prefix ("block:"/"tool:"),separator("|"),role names and other noise pollution; while the hash
    // Still uses the structured signature to keep the hit judgment stable.token_text passing an empty string means hash only, do not count. token.
    let feed = |hasher: &mut Sha256, hash_text: &str, token_text: &str, cum: &mut u32| {
        hasher.update(hash_text.as_bytes());
        if !token_text.is_empty() {
            *cum = cum.saturating_add(estimate_tokens(token_text).max(0) as u32);
        }
    };

    let commit = |hasher: &Sha256, cum: u32, segments: &mut Vec<Segment>, ttl_secs: i64| {
        let digest = hasher.clone().finalize();
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&digest[..8]);
        let hash = u64::from_be_bytes(buf);
        segments.push(Segment {
            hash,
            cumulative_tokens: cum,
            ttl_secs,
        });
    };

    // prefix chain matching model (reproduce Anthropic of the sliding window cache"longest common prefix hit"semantics):
    //
    // take prompt ofstableprefixby message boundarycutintooneentry**incrementprefixsegmentchain**:
    //   [tools+system] → [+msg0] → [+msg1] → ... → [+msg(n-2)]
    // eachitemsegmentof hash is the fingerprint accumulated from the start to that boundary,token is the cumulative estimate of this prefix.
    // mostafteroneentry message(the new input of the current round) only fed into the hash prompt_total_est,**do not split segment**
    // ——itisthis round cache_creation the tail, and it should not be treated as a reusable prefix.
    //
    // Why this hits across rounds: history messages are byte for byte identical across rounds, so Turn N+1 of
    // [+msg_k] segment hash inevitablyetc.at Turn N writeofthe same [+msg_k] segment.lookup take deepest
    // The hit segment is the longest cached prefix.= cache_read;itsaftertolast segment = cache_creation.
    //
    // oldstrategy("reversecountnumbertwoitem user"anchor) fatal flaw: with tool_result offorwordsin
    // tool_result is also role=user, the anchor points to a different physical message each round, so the prefix never aligns,
    // cause cache_read always 0,allrecordinto creation.

    // unify ttl: probes the largest that ever appeared in the whole request. cache_control.ttl, otherwisedefault 5m.
    let ttl = detect_max_ttl(req);

    // 1. tools(all fed in as part of the prefix base; tool definitions are stable across rounds).
    if let Some(tools) = req.tools.as_ref() {
        for t in tools {
            feed(&mut hasher, &tool_signature(t), &tool_token_text(t), &mut cum_tokens);
        }
    }

    // 2. system —— skipopen bracketfirstitemcarry cache_control of block the dynamic header before.
    //
    // Claude Code in system inject one at the front of the array**changes each round**small block(such ascurrent
    // time / session marker), and deliberately**do not print cache_control**; the truly stable large segment
    // (tool description, rules) carries it. cache_control. If hash accumulation starts from that dynamic header, the whole prefix
    // The chain would be polluted by it every round, all miss——This is exactly the measured root cause of create but never hit.
    //
    // therefore:when system there exists at least one with cache_control of block when, skip what precedes it
    // all block,fromfirstitem cache_control boundary begins accumulation (aligned with the client stable cache intent).
    // ifnoneany cache_control, then include all (unable to judge the dynamic boundary, keep as is).
    if let Some(systems) = req.system.as_ref() {
        let skip_until = systems
            .iter()
            .position(|s| s.cache_control.is_some())
            .unwrap_or(0);
        for sys in systems.iter().skip(skip_until) {
            feed(&mut hasher, &system_signature(sys), &sys.text, &mut cum_tokens);
        }
    }

    // tools+system prefix as the first segment of the chain (only when there is truly content).
    if cum_tokens > 0 {
        commit(&hasher, cum_tokens, &mut segments, ttl);
    }

    // 3. messages: except the last one, each message Splits an increasing prefix segment at the boundary.
    let last_idx = req.messages.len().saturating_sub(1);
    for (idx, msg) in req.messages.iter().enumerate() {
        // role enterhash(distinguish user/assistant boundary), but not counted token.
        feed(&mut hasher, &msg.role, "", &mut cum_tokens);
        match &msg.content {
            serde_json::Value::String(s) => {
                feed(&mut hasher, s, s, &mut cum_tokens);
            }
            serde_json::Value::Array(arr) => {
                // per block handling: text block hashes use the structured signature,token computes the original; image block hashes are included
                // Image data fingerprint (distinguishes different images),token use Anthropic basisestimate((w×h)/750).
                // do not deserialize the whole block, not clone Value: save overhead, and avoid a certain block
                // prefix drift caused by a deserialization failure being skipped.
                for v in arr {
                    if v.get("type").and_then(|t| t.as_str()) == Some("image") {
                        // image:hash feed media_type + data (ensure different images hash different, stable for the same image),
                        // token Estimates by real size then accumulates directly (base64 notentertext estimate).
                        let (media_type, data) = image_source_parts(v);
                        hasher.update(b"block:image|");
                        hasher.update(media_type.as_bytes());
                        hasher.update(b"|");
                        hasher.update(data.as_bytes());
                        let img_tokens = crate::image_resize::estimate_image_tokens(media_type, data);
                        cum_tokens = cum_tokens.saturating_add(img_tokens);
                    } else {
                        feed(
                            &mut hasher,
                            &block_signature_value(v),
                            &block_token_text(v),
                            &mut cum_tokens,
                        );
                    }
                }
            }
            _ => {}
        }
        // The last one is not segmented (the new input of the current round, belonging to cache_creation tail).
        if idx != last_idx {
            commit(&hasher, cum_tokens, &mut segments, ttl);
        }
    }

    (segments, cum_tokens)
}

/// Generates a session isolation seed as the frontmost input of the prefix hash chain.
///
/// priority:
///   1. metadata.user_id inside session segment (Claude Code format contains `_session_<uuid>`)
///      —— The most precise session dimension: shared across rounds within the same session, isolated across sessions.
///   2. return backclient Key id —— at least ensure different clients Key betweenisolate.
///
/// The seed participates only in the hash, not counted. token estimate, therefore does not affect cache_creation/read ofcountvaluebasis.
fn isolation_seed(req: &MessagesRequest, key_id: u64) -> String {
    if let Some(session) = req
        .metadata
        .as_ref()
        .and_then(|m| m.user_id.as_deref())
        .and_then(extract_session_id)
    {
        return format!("sess:{session}");
    }
    format!("key:{key_id}")
}

/// from Claude Code of user_id extract from session identifier.
///
/// formatlike `user_<hash>_account__session_<uuid>`, take `_session_` afterofpart.
/// return when this marker is not included None(let the caller return key_id).
fn extract_session_id(user_id: &str) -> Option<String> {
    user_id
        .split_once("_session_")
        .map(|(_, sid)| sid.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Probes the largest that appeared in the request. cache_control.ttl("1h" takes priority over "5m");
/// without any cache_control return whendefault 5m. Determines the lifetime of the written cache segment.
fn detect_max_ttl(req: &MessagesRequest) -> i64 {
    let mut ttl = DEFAULT_TTL_SECS;
    let mut bump = |cc: Option<&CacheControl>| {
        if let Some(cc) = cc {
            ttl = ttl.max(parse_ttl(cc.ttl.as_deref()));
        }
    };
    if let Some(tools) = req.tools.as_ref() {
        for t in tools {
            bump(t.cache_control.as_ref());
        }
    }
    if let Some(systems) = req.system.as_ref() {
        for sys in systems {
            bump(sys.cache_control.as_ref());
        }
    }
    for msg in &req.messages {
        if let serde_json::Value::Array(arr) = &msg.content {
            for v in arr {
                if let Some(t) = v
                    .get("cache_control")
                    .and_then(|cc| cc.get("ttl"))
                    .and_then(|t| t.as_str())
                {
                    ttl = ttl.max(parse_ttl(Some(t)));
                }
            }
        }
    }
    ttl
}

fn tool_signature(t: &Tool) -> String {
    // take name + description + input_schema serialize into stable text
    let schema = serde_json::to_string(&t.input_schema).unwrap_or_default();
    format!("tool:{}|{}|{}", t.name, t.description, schema)
}

/// tool token estimateoriginal text:name + description + schema concatenate, excluding the signature prefix/separator.
/// and [`tool_signature`] separate,let token The count stays close to the real content and is not polluted by structural markers.
fn tool_token_text(t: &Tool) -> String {
    let schema = serde_json::to_string(&t.input_schema).unwrap_or_default();
    format!("{} {} {}", t.name, t.description, schema)
}

fn system_signature(s: &SystemMessage) -> String {
    format!("sys:{}", s.text)
}

/// directly from content block of JSON compute the signature by value, take only type/text/thinking threeitemfield.
///
/// do not deserialize the whole ContentBlock, not clone:image of base64,tool_use of input,
/// tool_result of content Large or volatile fields do not participate in the signature, keeping the prefix fingerprint stable and cheap.
fn block_signature_value(v: &serde_json::Value) -> String {
    let s = |key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or("");
    format!("block:{}|{}|{}", s("type"), s("text"), s("thinking"))
}

/// content block of token estimateoriginal text: only text + thinking plain text, without signature structural markers.
fn block_token_text(v: &serde_json::Value) -> String {
    let s = |key: &str| v.get(key).and_then(|x| x.as_str()).unwrap_or("");
    let text = s("text");
    let thinking = s("thinking");
    if thinking.is_empty() {
        text.to_string()
    } else if text.is_empty() {
        thinking.to_string()
    } else {
        format!("{text} {thinking}")
    }
}

/// from image content block of JSON value take `(media_type, base64_data)`.
///
/// compatible base64 source(`source.type == "base64"`); when a field is missing returns an empty string, left to the caller.
/// image token the estimate goes through the fallback logic.url typeimagenone data, returnempty data(estimate fallback).
fn image_source_parts(v: &serde_json::Value) -> (&str, &str) {
    let src = v.get("source");
    let media_type = src
        .and_then(|s| s.get("media_type"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let data = src
        .and_then(|s| s.get("data"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    (media_type, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_miss_then_record_then_hit() {
        let cache = CacheMeter::new(None);
        let hashes = [1u64, 2u64];
        let tokens = [10u32, 25u32];
        let r1 = cache.lookup(&hashes, &tokens);
        assert!(r1.iter().all(|s| !s.hit));

        cache.record(&hashes, &tokens, 300);
        let r2 = cache.lookup(&hashes, &tokens);
        assert!(r2.iter().all(|s| s.hit));
    }

    #[test]
    fn ttl_expiry_makes_entry_miss() {
        let cache = CacheMeter::new(None);
        cache.record(&[42], &[100], 60);
        // manually expire the entry
        {
            let mut inner = cache.inner.lock();
            if let Some(e) = inner.entries.get_mut(&42) {
                e.expires_at = now_secs() - 1;
            }
        }
        let r = cache.lookup(&[42], &[100]);
        assert!(!r[0].hit);
    }

    #[test]
    fn evict_expired_removes_dead_entries() {
        let cache = CacheMeter::new(None);
        cache.record(&[1, 2], &[5, 5], 60);
        {
            let mut inner = cache.inner.lock();
            for (_, v) in inner.entries.iter_mut() {
                v.expires_at = now_secs() - 1;
            }
        }
        cache.evict_expired();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn parse_ttl_handles_known_values() {
        assert_eq!(parse_ttl(Some("1h")), 3600);
        assert_eq!(parse_ttl(Some("5m")), 300);
        assert_eq!(parse_ttl(None), 300);
        assert_eq!(parse_ttl(Some("garbage")), 300);
    }

    #[test]
    fn flush_and_reload_round_trip() {
        let tmp = std::env::temp_dir().join(format!("kiro-pc-{}.json", now_secs()));
        let cache = CacheMeter::new(Some(tmp.clone()));
        cache.record(&[7], &[42], 600);
        cache.flush_to_disk();

        let cache2 = CacheMeter::new(Some(tmp.clone()));
        let r = cache2.lookup(&[7], &[42]);
        assert!(r[0].hit);

        let _ = std::fs::remove_file(&tmp);
    }

    fn build_request_with_system_breakpoint() -> super::super::types::MessagesRequest {
        use super::super::types::{CacheControl, Message, MessagesRequest, SystemMessage};
        MessagesRequest {
            model: "claude-sonnet-4-5-20250929".to_string(),
            max_tokens: 32,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::Value::String("Hello".to_string()),
            }],
            stream: false,
            system: Some(vec![SystemMessage {
                text: "You are a helpful assistant. ".repeat(100),
                cache_control: Some(CacheControl {
                    cache_type: "ephemeral".to_string(),
                    ttl: None,
                }),
            }]),
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    #[test]
    fn compute_cache_usage_first_miss_then_hit() {
        let cache = CacheMeter::new(None);
        let req = build_request_with_system_breakpoint();

        // the first time: all segments miss → the covered prefix is all counted creation(read == 0).
        let u1 = compute_cache_usage(&cache, &req, 1);
        assert!(u1.cache_covered_est > 0, "first call should cover prefix");
        assert_eq!(u1.cache_read, 0, "first call has nothing cached to read");
        // use real total allocate:allenter creation,input = total − covered.
        let total = u1.prompt_total_est; // take estimate total asopen bracketreal totalthen closing bracketatassertion
        let (in1, cc1, cr1) = u1.split_against_total(total);
        assert!(cc1 > 0, "first call creation>0, cc={}", cc1);
        assert_eq!(cr1, 0);
        assert_eq!(in1 + cc1 + cr1, total, "the mutually exclusive measure must be self consistent");

        // the second time: the same request → hit, the covered prefix is all counted read(creation == 0).
        let u2 = compute_cache_usage(&cache, &req, 1);
        assert!(u2.cache_read > 0, "second call should hit");
        let (in2, cc2, cr2) = u2.split_against_total(total);
        assert_eq!(cc2, 0, "second call creation should be 0, got {}", cc2);
        assert!(cr2 > 0, "second call read>0, cr={}", cr2);
        assert_eq!(in2 + cc2 + cr2, total, "the mutually exclusive measure must be self consistent");
        // The cache covered part of the two splits is consistent: the first creation == numbertwotimesof read.
        assert_eq!(cc1, cr2);
    }

    #[test]
    fn split_against_total_is_mutually_exclusive() {
        // input + creation + read must be constantetc.at total, and the cache coverage ratio is apportioned correctly.
        let u = CacheUsage {
            cache_read: 30,
            cache_covered_est: 80, // creation part = 50
            prompt_total_est: 100,
        };
        // covered occupy prompt of 80% → real total=1000 whencache override 800.
        let (input, creation, read) = u.split_against_total(1000);
        assert_eq!(input + creation + read, 1000);
        assert_eq!(input, 200, "tail 20% isuncached input");
        // overridepart 800 sort by within read:creation = 30:50 split → read=300, creation=500.
        assert_eq!(read, 300);
        assert_eq!(creation, 500);
    }

    #[test]
    fn split_against_total_no_cache_all_input() {
        let u = CacheUsage {
            cache_read: 0,
            cache_covered_est: 0,
            prompt_total_est: 100,
        };
        assert_eq!(u.split_against_total(500), (500, 0, 0));
    }

    #[test]
    fn compute_cache_usage_single_message_no_prefix() {
        // single user message,none system/tools: there is no cacheable history prefix (the last one is not segmented).
        // → covered=0,total all in input.
        use super::super::types::{Message, MessagesRequest};
        let cache = CacheMeter::new(None);
        let req = MessagesRequest {
            model: "x".to_string(),
            max_tokens: 8,
            messages: vec![Message {
                role: "user".to_string(),
                content: serde_json::Value::String("Hello".to_string()),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };
        let u = compute_cache_usage(&cache, &req, 1);
        assert_eq!(u.cache_covered_est, 0);
        assert_eq!(u.split_against_total(123), (123, 0, 0));
    }

    /// construct an ordinary tool,input_schema top level key insert in the given order.
    /// Used to verify: regardless of insertion order,tool_signature all stable(BTreeMap guarantee).
    fn build_tool_with_schema_order(insert_required_first: bool) -> super::super::types::Tool {
        use super::super::types::Tool;
        let mut schema = std::collections::BTreeMap::new();
        // Deliberately uses a different insertion order to simulate upstream. JSON the nondeterministic iteration order of parsing.
        if insert_required_first {
            schema.insert("required".to_string(), serde_json::json!([]));
            schema.insert("properties".to_string(), serde_json::json!({}));
            schema.insert("type".to_string(), serde_json::json!("object"));
        } else {
            schema.insert("type".to_string(), serde_json::json!("object"));
            schema.insert("properties".to_string(), serde_json::json!({}));
            schema.insert("required".to_string(), serde_json::json!([]));
        }
        Tool {
            tool_type: None,
            name: "my_tool".to_string(),
            description: "desc".to_string(),
            input_schema: schema,
            max_uses: None,
            cache_control: None,
        }
    }

    #[test]
    fn tool_signature_stable_across_insert_order() {
        let a = build_tool_with_schema_order(true);
        let b = build_tool_with_schema_order(false);
        // logically equivalent, with a different insertion order. schema must produce the same signature,
        // otherwise tools segment hash jitter willletsubsequent system/messages breakpointchain miss.
        assert_eq!(tool_signature(&a), tool_signature(&b));
    }

    #[test]
    fn compute_cache_usage_tools_hit_regardless_of_schema_order() {        use super::super::types::{CacheControl, Message, MessagesRequest};

        let make_req = |insert_required_first: bool| {
            let mut tool = build_tool_with_schema_order(insert_required_first);
            tool.cache_control = Some(CacheControl {
                cache_type: "ephemeral".to_string(),
                ttl: None,
            });
            MessagesRequest {
                model: "claude-sonnet-4-5-20250929".to_string(),
                max_tokens: 32,
                messages: vec![Message {
                    role: "user".to_string(),
                    content: serde_json::Value::String("Hello".to_string()),
                }],
                stream: false,
                system: None,
                tools: Some(vec![tool]),
                tool_choice: None,
                thinking: None,
                output_config: None,
                metadata: None,
            }
        };

        let cache = CacheMeter::new(None);
        // First time: with one insertion order, should write the cache (miss → read==0).
        let u1 = compute_cache_usage(&cache, &make_req(false), 1);
        assert!(u1.cache_covered_est > 0, "first call should cover prefix");
        assert_eq!(u1.cache_read, 0);

        // Second time: a different insertion order but logically equivalent, should hit the cache (read equals the first covered prefix).
        let u2 = compute_cache_usage(&cache, &make_req(true), 1);
        assert_eq!(
            u2.cache_read, u1.cache_covered_est,
            "schema order should not affect the hit:second read should equal first covered"
        );
    }

    /// constructoneentrycarry cache_control of user/assistant textmessage.
    fn msg_with_cc(role: &str, text: &str, with_cc: bool) -> super::super::types::Message {
        use super::super::types::Message;
        let block = if with_cc {
            serde_json::json!({
                "type": "text",
                "text": text,
                "cache_control": {"type": "ephemeral"}
            })
        } else {
            serde_json::json!({"type": "text", "text": text})
        };
        Message {
            role: role.to_string(),
            content: serde_json::Value::Array(vec![block]),
        }
    }

    fn req_with_messages(messages: Vec<super::super::types::Message>) -> super::super::types::MessagesRequest {
        use super::super::types::MessagesRequest;
        MessagesRequest {
            model: "claude-sonnet-4-5-20250929".to_string(),
            max_tokens: 32,
            messages,
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    /// simulate Claude Code real tool call sequence:tool_use(assistant) / tool_result(user)
    /// The block carries a newly generated one each round when returned. id. validate the prefix chain against with id a drifting tool block can still hit.
    #[test]
    fn tool_call_history_still_hits_despite_id_drift() {
        let body = "analyze the repository structure carefully ".repeat(15);
        // assistant round:a tool_use block,input istoolparameter,id may differ each round.
        let assistant_tool = |id: &str| {
            use super::super::types::Message;
            Message {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "text", "text": body},
                    {"type": "tool_use", "id": id, "name": "bash", "input": {"cmd": "ls"}}
                ]),
            }
        };
        // user round:tool_result block,tool_use_id correspondaboveof id.
        let user_result = |id: &str| {
            use super::super::types::Message;
            Message {
                role: "user".to_string(),
                content: serde_json::json!([
                    {"type": "tool_result", "tool_use_id": id, "content": body}
                ]),
            }
        };
        let user_text = |t: &str| msg_with_cc("user", t, false);

        let cache = CacheMeter::new(None);
        // Turn 1: user → assistant(tool_use #a) → user(tool_result #a) → assistant(text) → user(new question)
        let turn1 = req_with_messages(vec![
            user_text(&body),
            assistant_tool("toolu_aaa"),
            user_result("toolu_aaa"),
            msg_with_cc("assistant", &body, false),
            user_text("next question one"),
        ]);
        let u1 = compute_cache_usage(&cache, &turn1, 1);
        assert!(u1.cache_covered_est > 0);
        assert_eq!(u1.cache_read, 0, "turn1 nonehistory canhit");

        // Turn 2: append assistant(text) + user(new question). before 5 entries of history are byte for byte unchanged.
        let turn2 = req_with_messages(vec![
            user_text(&body),
            assistant_tool("toolu_aaa"),
            user_result("toolu_aaa"),
            msg_with_cc("assistant", &body, false),
            user_text("next question one"),
            msg_with_cc("assistant", &body, false),
            user_text("next question two"),
        ]);
        let u2 = compute_cache_usage(&cache, &turn2, 1);
        assert!(
            u2.cache_read > 0,
            "turn2 should hit turn1 the history prefix (even if the tool block carries id)"
        );
        assert_eq!(
            u2.cache_read, u1.cache_covered_est,
            "The deepest prefix hit should equal the previous round. covered"
        );
    }

    #[test]
    fn multi_turn_prefix_chain_produces_read_hit() {
        // prefixchainmodel:turn4 in turn3 on the basis ofappend a/u a pair, the history prefix is byte for byte unchanged,
        // so turn4 should hit turn3 The deepest history prefix segment written (cache_read > 0).
        let cache = CacheMeter::new(None);
        let body = "the quick brown fox jumps over the lazy dog ".repeat(20);

        // number 3 round:u,a,u,a,u(5 entries). Segmenting: except the last one, each entry message aprefixsegment
        // → idx 0,1,2,3 total 4 itemsegment (none system/tools).
        let turn3 = req_with_messages(vec![
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, true),
        ]);
        let u3 = compute_cache_usage(&cache, &turn3, 1);
        assert!(u3.cache_covered_est > 0, "turn3 should create cache");
        assert_eq!(u3.cache_read, 0, "turn3 has no prior cache to read");

        // number 4 round:append a3,u4(7 entry).history idx 0..=5 split into segments, the last one idx6 do not split.
        // turn3 ofdeepestsegmentin idx3(itsprefix=u,a,u,a),turn4 of idx3 the segment prefix is byte for byte identical
        // → hit.turn4 also add idx4,5 two deeper history prefix segments.
        let turn4 = req_with_messages(vec![
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, true),
        ]);
        let u4 = compute_cache_usage(&cache, &turn4, 1);
        assert!(u4.cache_read > 0, "turn4 should hit a prior-turn prefix");
        // turn4 the deepest hit prefix = turn3 ofdeepestsegment (idx3 prefix,that is turn3 of covered).
        assert_eq!(
            u4.cache_read, u3.cache_covered_est,
            "read should equal the deepest history prefix written in the previous round."
        );
        // turn4 The covered prefix is deeper (a new history segment).→ creation part > 0.
        assert!(
            u4.cache_covered_est > u4.cache_read,
            "turn4 Still creates a cache for the newly added history prefix."
        );
    }

    #[test]
    fn prefix_chain_works_without_any_cache_control() {
        // newmodeldoes not depend on cache_control: as long as there is a history prefix stable across rounds, it can hit.
        // this reproduces Anthropic Auto prefix cache semantics, consistent with the old"must have cache_control"strategydifferent.
        let cache = CacheMeter::new(None);
        let body = "lorem ipsum dolor sit amet ".repeat(20);
        let turn1 = req_with_messages(vec![
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
        ]);
        let u1 = compute_cache_usage(&cache, &turn1, 1);
        assert!(u1.cache_covered_est > 0, "Should create a cache segment for the history prefix.");
        assert_eq!(u1.cache_read, 0);

        let turn2 = req_with_messages(vec![
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
            msg_with_cc("assistant", &body, false),
            msg_with_cc("user", &body, false),
        ]);
        let u2 = compute_cache_usage(&cache, &turn2, 1);
        assert!(u2.cache_read > 0, "none cache_control should also hit the history prefix across rounds");
    }

    /// reproduce the measured root cause:system[0] is the dynamic header that changes each round (no cache_control),
    /// itsafteriscarry cache_control stable large block. After skipping the dynamic header, the stable prefix should hit across rounds.
    #[test]
    fn dynamic_system_header_does_not_break_cache_hit() {
        use super::super::types::{CacheControl, Message, MessagesRequest, SystemMessage};
        let stable_sys = "You are a coding assistant. ".repeat(200);
        let body = "implement the feature step by step ".repeat(15);

        let make_req = |dyn_header: &str, msgs: Vec<Message>| MessagesRequest {
            model: "claude-opus-4-8".to_string(),
            max_tokens: 64,
            messages: msgs,
            stream: false,
            system: Some(vec![
                // sys[0]: the dynamic header that changes each round (such as the current time), no cache_control.
                SystemMessage {
                    text: dyn_header.to_string(),
                    cache_control: None,
                },
                // sys[1]: a stable large block, with cache_control.
                SystemMessage {
                    text: stable_sys.clone(),
                    cache_control: Some(CacheControl {
                        cache_type: "ephemeral".to_string(),
                        ttl: None,
                    }),
                },
            ]),
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let cache = CacheMeter::new(None);
        // Turn 1:dynamichead = "now=1001",3 entrymessage.
        let u1 = compute_cache_usage(
            &cache,
            &make_req(
                "now=1001",
                vec![
                    msg_with_cc("user", &body, false),
                    msg_with_cc("assistant", &body, false),
                    msg_with_cc("user", &body, false),
                ],
            ),
            1,
        );
        assert!(u1.cache_covered_est > 0);
        assert_eq!(u1.cache_read, 0, "turn1 nonehistory canhit");

        // Turn 2:dynamicheader changeinto "now=2002"(different!), append a pair a/u.
        // after skipping the dynamic header,sys[1]+the history prefix is byte for byte unchanged → musthit.
        let u2 = compute_cache_usage(
            &cache,
            &make_req(
                "now=2002",
                vec![
                    msg_with_cc("user", &body, false),
                    msg_with_cc("assistant", &body, false),
                    msg_with_cc("user", &body, false),
                    msg_with_cc("assistant", &body, false),
                    msg_with_cc("user", &body, false),
                ],
            ),
            1,
        );
        assert!(
            u2.cache_read > 0,
            "dynamic system Header changes should not break stable prefix hits (the measured root cause)."
        );
    }

    /// Session isolation: same prefix content, different clients. Key(key_id) should not hit each other.
    #[test]
    fn different_key_id_does_not_cross_hit() {
        let cache = CacheMeter::new(None);
        let body = "shared system prompt and history ".repeat(20);
        let msgs = || {
            vec![
                msg_with_cc("user", &body, false),
                msg_with_cc("assistant", &body, false),
                msg_with_cc("user", &body, false),
            ]
        };
        // Key=1 establishcache.
        let a = compute_cache_usage(&cache, &req_with_messages(msgs()), 1);
        assert!(a.cache_covered_est > 0);
        assert_eq!(a.cache_read, 0);
        // Key=2 same content, but the isolation seed differs. → no hit (treated as newly created).
        let b = compute_cache_usage(&cache, &req_with_messages(msgs()), 2);
        assert_eq!(b.cache_read, 0, "different key_id should not hit each other prefixes");
        // Key=1 do the same content once more → hits what it wrote last time.
        let c = compute_cache_usage(&cache, &req_with_messages(msgs()), 1);
        assert!(c.cache_read > 0, "same key_id should hit its own prefix");
    }

    /// sessionisolate:metadata.user_id in session different → nothit;session identical → hit.
    #[test]
    fn metadata_session_scopes_cache() {
        use super::super::types::{Message, MessagesRequest, Metadata};
        let body = "conversation prefix that stays stable ".repeat(20);
        let make = |session: &str| MessagesRequest {
            model: "claude-opus-4-8".to_string(),
            max_tokens: 64,
            messages: vec![
                Message { role: "user".into(), content: serde_json::json!([{"type":"text","text":body}]) },
                Message { role: "assistant".into(), content: serde_json::json!([{"type":"text","text":body}]) },
                Message { role: "user".into(), content: serde_json::json!([{"type":"text","text":body}]) },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: Some(Metadata {
                user_id: Some(format!("user_abc_account__session_{session}")),
            }),
        };
        let cache = CacheMeter::new(None);
        // same key_id(both are 0), only session different——rely on metadata session isolate.
        let s1a = compute_cache_usage(&cache, &make("aaa"), 0);
        assert_eq!(s1a.cache_read, 0);
        let s2 = compute_cache_usage(&cache, &make("bbb"), 0);
        assert_eq!(s2.cache_read, 0, "different session notshould hit");
        let s1b = compute_cache_usage(&cache, &make("aaa"), 0);
        assert!(s1b.cache_read > 0, "identical session should hit");
    }

    #[test]
    fn extract_session_id_parses_claude_code_format() {
        assert_eq!(
            extract_session_id("user_xxx_account__session_0b4445e1-uuid"),
            Some("0b4445e1-uuid".to_string())
        );
        assert_eq!(extract_session_id("no-session-here"), None);
        assert_eq!(extract_session_id("trailing_session_"), None);
    }

    /// token basispurity:cum_tokens count only the original text, excluding role / signatureprefix / separator noise.
    #[test]
    fn token_count_excludes_signature_noise() {
        use super::super::types::{Message, MessagesRequest};
        // Two messages: the first is history (segmented), content is known plain text; the last is a placeholder (not segmented).
        let history_text = "the quick brown fox jumps over the lazy dog";
        let req = MessagesRequest {
            model: "m".to_string(),
            max_tokens: 8,
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: serde_json::json!([{"type": "text", "text": history_text}]),
                },
                Message {
                    role: "assistant".to_string(),
                    content: serde_json::Value::String("ok".to_string()),
                },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };
        let u = compute_cache_usage(&CacheMeter::new(None), &req, 1);
        // the history segment (the first) of covered should be strictly equal to plain text estimate——
        // excludes "user" role,"block:" prefix,"|" separatorofany token.
        let pure = estimate_tokens(history_text) as i32;
        assert_eq!(
            u.cache_covered_est, pure,
            "covered shouldonlycomputeoriginal text token, measured {} vs plain text {}",
            u.cache_covered_est, pure
        );
    }

    /// history segment containing an image:covered shouldcount inimageof Anthropic basis token, and hits stably across rounds.
    #[test]
    fn image_block_contributes_tokens_and_hits() {
        use super::super::types::{Message, MessagesRequest};
        // use image_resize the same PNG generatethe component builds a table 750×750(≈750 token)truediagram.
        let png = make_test_png(750, 750);
        let img_tokens = crate::image_resize::estimate_image_tokens("image/png", &png) as i32;
        assert!(img_tokens > 100, "premise: the test image should have a considerable token, measured {img_tokens}");

        let make = |trailing: &str| MessagesRequest {
            model: "m".to_string(),
            max_tokens: 8,
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: serde_json::json!([
                        {"type":"image","source":{"type":"base64","media_type":"image/png","data": png}},
                        {"type":"text","text":"describe"}
                    ]),
                },
                Message { role: "assistant".to_string(), content: serde_json::json!("a pixel") },
                Message { role: "user".to_string(), content: serde_json::json!(trailing) },
            ],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        };

        let cache = CacheMeter::new(None);
        // Turn 1:contains imageof user is the first history segment, its covered must includecontains image token.
        let u1 = compute_cache_usage(&cache, &make("q1"), 1);
        let text_only = estimate_tokens("describe") as i32;
        // the deepest history segment covers at least to [contains imageuser] segment,covered should ≥ image token(far larger than plain text).
        assert!(
            u1.cache_covered_est >= img_tokens + text_only - 5,
            "covered({}) shouldcontains image token({})", u1.cache_covered_est, img_tokens
        );
        assert_eq!(u1.cache_read, 0);

        // Turn 2: appends a round, history with images is byte for byte unchanged. → hit (read contains image token).
        let u2 = compute_cache_usage(&cache, &make("q2"), 1);
        assert!(
            u2.cache_read >= img_tokens,
            "history with images should hit across rounds and read({}) contains image token({})", u2.cache_read, img_tokens
        );
    }

    /// for testing PNG generatecomponent(with image_resize same as the test, gradient fill is closer to the real compression ratio).
    fn make_test_png(w: u32, h: u32) -> String {
        use base64::{Engine, engine::general_purpose::STANDARD as B64};
        use image::{ImageFormat, Rgb, RgbImage};
        use std::io::Cursor;
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
            }
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        B64.encode(&buf)
    }
}
