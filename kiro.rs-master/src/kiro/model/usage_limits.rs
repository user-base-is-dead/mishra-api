//! usage quota query data model
//!
//! contains getUsageLimits API the response type definition

use serde::Deserialize;

/// usage quota query response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitsResponse {
    /// downtimesresetdate (Unix timestamp)
    #[serde(default)]
    pub next_date_reset: Option<f64>,

    /// subscription info
    #[serde(default)]
    pub subscription_info: Option<SubscriptionInfo>,

    /// usage detail list
    #[serde(default)]
    pub usage_breakdown_list: Vec<UsageBreakdown>,

    /// Overage config (whether the user currently enabled overage; may not exist).
    #[serde(default)]
    pub overage_configuration: Option<OverageConfiguration>,

    /// user information (the request carries isEmailRequired=true whenupstreamreturn)
    #[serde(default)]
    pub user_info: Option<UserInfo>,
}

/// user info
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    /// account email
    #[serde(default)]
    pub email: Option<String>,
}

/// subscription info
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    /// subscription title (KIRO PRO+ / KIRO FREE etc.)
    #[serde(default)]
    pub subscription_title: Option<String>,

    /// whether overage can be enabled ("ENABLED" / "DISABLED" / "NOT_AVAILABLE" etc.)
    /// thismeansaccount"whether can"openoverage,FREE such subscriptions usually return NOT_AVAILABLE
    #[serde(default)]
    pub overage_capability: Option<String>,
}

/// overage config
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverageConfiguration {
    /// Whether the user currently enabled overage (compatibility field).
    #[serde(default)]
    pub overage_enabled: Option<bool>,

    /// The user current overage state string ("ENABLED" / "DISABLED")
    #[serde(default)]
    pub overage_status: Option<String>,
}

/// usageline item detail
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UsageBreakdown {
    /// currentusage
    #[serde(default)]
    pub current_usage: i64,

    /// current usage (exact value)
    #[serde(default)]
    pub current_usage_with_precision: f64,

    /// bonus quotalist
    #[serde(default)]
    pub bonuses: Vec<Bonus>,

    /// free trialuseinfo
    #[serde(default)]
    pub free_trial_info: Option<FreeTrialInfo>,

    /// downtimesresetdate (Unix timestamp)
    #[serde(default)]
    pub next_date_reset: Option<f64>,

    /// usequota limit
    #[serde(default)]
    pub usage_limit: i64,

    /// usage limit (exact value)
    #[serde(default)]
    pub usage_limit_with_precision: f64,
}

/// bonus quota
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bonus {
    /// currentusage
    #[serde(default)]
    pub current_usage: f64,

    /// usequota limit
    #[serde(default)]
    pub usage_limit: f64,

    /// state (ACTIVE / EXPIRED)
    #[serde(default)]
    pub status: Option<String>,
}

impl Bonus {
    /// check bonus whether it is in an active state
    pub fn is_active(&self) -> bool {
        self.status
            .as_deref()
            .map(|s| s == "ACTIVE")
            .unwrap_or(false)
    }
}

/// free trialuseinfo
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct FreeTrialInfo {
    /// currentusage
    #[serde(default)]
    pub current_usage: i64,

    /// current usage (exact value)
    #[serde(default)]
    pub current_usage_with_precision: f64,

    /// free trial expiry time (Unix timestamp)
    #[serde(default)]
    pub free_trial_expiry: Option<f64>,

    /// free trialusestate (ACTIVE / EXPIRED)
    #[serde(default)]
    pub free_trial_status: Option<String>,

    /// usequota limit
    #[serde(default)]
    pub usage_limit: i64,

    /// usage limit (exact value)
    #[serde(default)]
    pub usage_limit_with_precision: f64,
}

// ============ convenientmethodimplement ============

impl FreeTrialInfo {
    /// Checks whether the free trial is active.
    pub fn is_active(&self) -> bool {
        self.free_trial_status
            .as_deref()
            .map(|s| s == "ACTIVE")
            .unwrap_or(false)
    }
}

impl UsageLimitsResponse {
    /// fetchsubscription title
    pub fn subscription_title(&self) -> Option<&str> {
        self.subscription_info
            .as_ref()
            .and_then(|info| info.subscription_title.as_deref())
    }

    /// get the account email (upstream userInfo.email,cancanis empty)
    pub fn email(&self) -> Option<&str> {
        self.user_info
            .as_ref()
            .and_then(|info| info.email.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// Whether the user currently enabled overage (compatibility overageEnabled / overageStatus)
    pub fn overage_enabled(&self) -> Option<bool> {
        let cfg = self.overage_configuration.as_ref()?;
        if let Some(enabled) = cfg.overage_enabled {
            return Some(enabled);
        }
        cfg.overage_status
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("ENABLED"))
    }

    /// whether account"can"enable overage (based on subscriptionInfo.overageCapability)
    /// `Some(true)` = can enable (OVERAGE_CAPABLE);`Some(false)` = this subscription explicitly does not support
    /// (NOT_OVERAGE_CAPABLE / NOT_AVAILABLE);`None` = Upstream did not provide the field or the value is unrecognized.
    pub fn overage_capable(&self) -> Option<bool> {
        let cap = self
            .subscription_info
            .as_ref()
            .and_then(|s| s.overage_capability.as_deref())?;
        let normalized = cap.trim().to_uppercase();
        if normalized == "OVERAGE_CAPABLE" {
            return Some(true);
        }
        if normalized == "NOT_OVERAGE_CAPABLE" || normalized == "NOT_AVAILABLE" {
            return Some(false);
        }
        // Do not hard classify an unrecognized value as"not supported", return None letbeforeendshow"unknown"
        None
    }

    /// get the first usage detail
    fn primary_breakdown(&self) -> Option<&UsageBreakdown> {
        self.usage_breakdown_list.first()
    }

    /// Gets the total usage limit (exact value).
    ///
    /// Sums the base quota, the activated free trial quota, and the activated bonus quota.
    pub fn usage_limit(&self) -> f64 {
        let Some(breakdown) = self.primary_breakdown() else {
            return 0.0;
        };

        let mut total = breakdown.usage_limit_with_precision;

        // accumulate activationof free trial quota
        if let Some(trial) = &breakdown.free_trial_info {
            if trial.is_active() {
                total += trial.usage_limit_with_precision;
            }
        }

        // accumulate activationof bonus quota
        for bonus in &breakdown.bonuses {
            if bonus.is_active() {
                total += bonus.usage_limit;
            }
        }

        total
    }

    /// Gets the total current usage (exact value).
    ///
    /// Sums the base usage, the activated free trial usage, and the activated bonus usage.
    pub fn current_usage(&self) -> f64 {
        let Some(breakdown) = self.primary_breakdown() else {
            return 0.0;
        };

        let mut total = breakdown.current_usage_with_precision;

        // accumulate activationof free trial usage
        if let Some(trial) = &breakdown.free_trial_info {
            if trial.is_active() {
                total += trial.current_usage_with_precision;
            }
        }

        // accumulate activationof bonus usage
        for bonus in &breakdown.bonuses {
            if bonus.is_active() {
                total += bonus.current_usage;
            }
        }

        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_email_from_user_info() {
        let json = r#"{
            "subscriptionInfo": { "subscriptionTitle": "KIRO PRO+" },
            "userInfo": { "email": "alice@example.com", "userId": "u-123" }
        }"#;
        let resp: UsageLimitsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.subscription_title(), Some("KIRO PRO+"));
        assert_eq!(resp.email(), Some("alice@example.com"));
    }

    #[test]
    fn test_email_none_when_user_info_absent() {
        let json = r#"{ "subscriptionInfo": { "subscriptionTitle": "KIRO FREE" } }"#;
        let resp: UsageLimitsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.email(), None);
    }

    #[test]
    fn test_email_none_when_empty_string() {
        // Upstream may return an empty string email, which should be treated as no email.
        let json = r#"{ "userInfo": { "email": "" } }"#;
        let resp: UsageLimitsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.email(), None);
    }
}
