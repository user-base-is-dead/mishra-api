//! device fingerprint generator
//!

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;

/// fallback machineId cache(by credential id bucketed, stable within the process lifetime)
///
/// key as `credentials.id`; none id credentials share the same fallback value (does not occur in the normal flow).
static FALLBACK_MACHINE_IDS: OnceLock<Mutex<HashMap<Option<u64>, String>>> = OnceLock::new();

/// standardize machineId format
///
/// supports the following formats:
/// - 64 character hexadecimal string (returned directly).
/// - UUID format (such as "2582956e-cc88-4669-b546-07adbffcb894", after removing hyphens pad to 64 characters)
fn normalize_machine_id(machine_id: &str) -> Option<String> {
    let trimmed = machine_id.trim();

    // ifalreadyalreadyis 64 characters, return directly
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(trimmed.to_string());
    }

    // try to parse UUID format (remove hyphens)
    let without_dashes: String = trimmed.chars().filter(|c| *c != '-').collect();

    // UUID after removing hyphens it is 32 character
    if without_dashes.len() == 32 && without_dashes.chars().all(|c| c.is_ascii_hexdigit()) {
        // pad to 64 characters (repeat once)
        return Some(format!("{}{}", without_dashes, without_dashes));
    }

    // unrecognizable format
    None
}

/// Generates a unique one from the credential info. Machine ID
///
/// priority:
/// 1. credential level `machineId`(if configured and the format is valid)
/// 2. global `config.machineId`(if configured and the format is valid)
/// 3. Derived from the credential type (mutually exclusive, by [`KiroCredentials::is_api_key_credential`] split routing):
///    - API Key credential:based on `kiroApiKey` derive
///    - OAuth credential:based on `refreshToken` derive
/// 4. Fallback: derived from a random seed, by `credentials.id` Cached in process (first trigger warn log)
pub fn generate_from_credentials(credentials: &KiroCredentials, config: &Config) -> String {
    // if a credential level is configured machineId,prefer use
    if let Some(ref machine_id) = credentials.machine_id {
        if let Some(normalized) = normalize_machine_id(machine_id) {
            return normalized;
        }
    }

    // if a global one is configured machineId,actas defaultvalue
    if let Some(ref machine_id) = config.machine_id {
        if let Some(normalized) = normalize_machine_id(machine_id) {
            return normalized;
        }
    }

    // derive by credential type (API Key and refreshToken The two paths are mutually exclusive, no fallback)
    if credentials.is_api_key_credential() {
        // API Key credential:based on kiroApiKey derive
        if let Some(ref api_key) = credentials.kiro_api_key {
            if !api_key.is_empty() {
                return sha256_hex(&format!("KiroAPIKey/{}", api_key));
            }
        }
    } else if let Some(ref refresh_token) = credentials.refresh_token {
        // OAuth credential:based on refreshToken derive
        if !refresh_token.is_empty() {
            return sha256_hex(&format!("KotlinNativeAPI/{}", refresh_token));
        }
    }

    // Fallback: goes through the derivation flow to generate a random machineId, by credential id enterprocessinsidestable
    fallback_machine_id(credentials)
}

/// Generates a fallback for a credential missing derivation material. machineId
///
/// - still through `sha256("KiroFallback/<uuid>")` derived; the output format is consistent with the normal path (64 character hexadecimal)
/// - by `credentials.id` Cached in process; multiple calls for the same credential return the same value.
/// - A process restart re-randomizes it; not persisted.
/// - when each credential is first generated warn once
fn fallback_machine_id(credentials: &KiroCredentials) -> String {
    let cache = FALLBACK_MACHINE_IDS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock();
    if let Some(existing) = map.get(&credentials.id) {
        return existing.clone();
    }

    let seed = Uuid::new_v4();
    let derived = sha256_hex(&format!("KiroFallback/{}", seed));
    tracing::warn!(
        credential_id = ?credentials.id,
        "the credential lacks derivation material (kiroApiKey/refreshToken are all unavailable), uses a random fallback. machineId(stable in process)"
    );
    map.insert(credentials.id, derived.clone());
    derived
}

/// SHA256 Hash implementation (returns a hexadecimal string).
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let result = sha256_hex("test");
        assert_eq!(result.len(), 64);
        assert_eq!(
            result,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_generate_with_custom_machine_id() {
        let credentials = KiroCredentials::default();
        let mut config = Config::default();
        config.machine_id = Some("a".repeat(64));

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, "a".repeat(64));
    }

    #[test]
    fn test_generate_with_credential_machine_id_overrides_config() {
        let mut credentials = KiroCredentials::default();
        credentials.machine_id = Some("b".repeat(64));

        let mut config = Config::default();
        config.machine_id = Some("a".repeat(64));

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, "b".repeat(64));
    }

    #[test]
    fn test_generate_with_refresh_token() {
        let mut credentials = KiroCredentials::default();
        credentials.refresh_token = Some("test_refresh_token".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_generate_without_credentials_uses_fallback() {
        // A completely empty credential takes the fallback branch and returns a derived random machineId
        let credentials = KiroCredentials::default();
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_with_api_key() {
        let mut credentials = KiroCredentials::default();
        credentials.kiro_api_key = Some("ksk_test_api_key".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
        // should match KiroAPIKey/<api_key> ofhash consistent
        assert_eq!(result, sha256_hex("KiroAPIKey/ksk_test_api_key"));
    }

    #[test]
    fn test_api_key_and_refresh_token_are_mutually_exclusive() {
        // coexist kiroApiKey and refreshToken when, should go API Key branch
        let mut credentials = KiroCredentials::default();
        credentials.kiro_api_key = Some("ksk_test".to_string());
        credentials.refresh_token = Some("should_not_be_used".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, sha256_hex("KiroAPIKey/ksk_test"));
    }

    #[test]
    fn test_api_key_auth_method_empty_uses_fallback_not_refresh_token() {
        // auth_method=api_key but kiro_api_key empty: do not fall back to refreshToken,gofallbackbranch
        let mut credentials = KiroCredentials::default();
        credentials.id = Some(u64::MAX - 1);
        credentials.auth_method = Some("api_key".to_string());
        credentials.refresh_token = Some("should_not_be_used".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
        // mustis notbased on refresh_token The derived value (mutual exclusivity verification).
        assert_ne!(result, sha256_hex("KotlinNativeAPI/should_not_be_used"));
    }

    #[test]
    fn test_fallback_is_stable_per_credential() {
        // samecredential (by id distinguishes) multiple fallback calls should return the same value.
        let mut credentials = KiroCredentials::default();
        credentials.id = Some(u64::MAX - 10);
        let config = Config::default();

        let first = generate_from_credentials(&credentials, &config);
        let second = generate_from_credentials(&credentials, &config);
        assert_eq!(first, second);
    }

    #[test]
    fn test_fallback_differs_across_credentials() {
        // different credential (different id) the fallback values should differ from each other
        let mut cred_a = KiroCredentials::default();
        cred_a.id = Some(u64::MAX - 20);
        let mut cred_b = KiroCredentials::default();
        cred_b.id = Some(u64::MAX - 21);
        let config = Config::default();

        let id_a = generate_from_credentials(&cred_a, &config);
        let id_b = generate_from_credentials(&cred_b, &config);
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn test_normalize_uuid_format() {
        // UUID the format should be converted to 64 character
        let uuid = "2582956e-cc88-4669-b546-07adbffcb894";
        let result = normalize_machine_id(uuid);
        assert!(result.is_some());
        let normalized = result.unwrap();
        assert_eq!(normalized.len(), 64);
        // UUID after removing hyphens repeat once
        assert_eq!(
            normalized,
            "2582956ecc884669b54607adbffcb8942582956ecc884669b54607adbffcb894"
        );
    }

    #[test]
    fn test_normalize_64_char_hex() {
        // 64 A hexadecimal of this many characters should be returned directly.
        let hex64 = "a".repeat(64);
        let result = normalize_machine_id(&hex64);
        assert_eq!(result, Some(hex64));
    }

    #[test]
    fn test_normalize_invalid_format() {
        // an invalid format should return None
        assert!(normalize_machine_id("invalid").is_none());
        assert!(normalize_machine_id("too-short").is_none());
        assert!(normalize_machine_id(&"g".repeat(64)).is_none()); // nonhexadecimalentermechanism
    }

    #[test]
    fn test_generate_with_uuid_machine_id() {
        let mut credentials = KiroCredentials::default();
        credentials.machine_id = Some("2582956e-cc88-4669-b546-07adbffcb894".to_string());

        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
    }
}
