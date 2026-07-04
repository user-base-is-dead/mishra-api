//! available Profile querydata model
//!
//! corresponding upstream `ListAvailableProfiles`(AWS JSON 1.0,target
//! `AmazonCodeWhispererService.ListAvailableProfiles`) the response type.
//!
//! Enterprise / IAM Identity Center (IdC) the account needs a real `profileArn` in order to call
//! streaming endpoint `generateAssistantResponse`——without profileArn will beupstreamto
//! `400 {"message":"profileArn is required for this request."}` reject; with BuilderID
//! placeholderthen will because of token Rejected due to identity mismatch. The real profileArn can only be obtained through this interface.

use serde::Deserialize;

/// `ListAvailableProfiles` response
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListAvailableProfilesResponse {
    /// thiscredentialavailableof profile list
    #[serde(default)]
    pub profiles: Vec<AvailableProfile>,

    /// pagination token(this project only takes the first profile, usually no pagination needed)
    #[serde(default)]
    #[allow(dead_code)]
    pub next_token: Option<String>,
}

/// single available profile
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AvailableProfile {
    /// Profile ARN(realavailableof profileArn)
    #[serde(default)]
    pub arn: Option<String>,

    /// Profile name (such as `KiroProfile-us-east-1`)
    #[serde(default)]
    #[allow(dead_code)]
    pub profile_name: Option<String>,
}

impl ListAvailableProfilesResponse {
    /// return the first non empty real profileArn(ifhas).
    pub fn first_arn(&self) -> Option<&str> {
        self.profiles
            .iter()
            .filter_map(|p| p.arn.as_deref())
            .find(|arn| !arn.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_profiles_and_first_arn() {
        let json = r#"{
            "profiles": [
                {
                    "arn": "arn:aws:codewhisperer:us-east-1:610548660232:profile/VNECVYCYYAWN",
                    "profileName": "KiroProfile-us-east-1",
                    "identityDetails": { "ssoIdentityDetails": { "ssoRegion": "us-east-1" } }
                }
            ]
        }"#;
        let resp: ListAvailableProfilesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.first_arn(),
            Some("arn:aws:codewhisperer:us-east-1:610548660232:profile/VNECVYCYYAWN")
        );
    }

    #[test]
    fn test_first_arn_none_when_empty() {
        let resp: ListAvailableProfilesResponse =
            serde_json::from_str(r#"{"profiles":[]}"#).unwrap();
        assert_eq!(resp.first_arn(), None);
    }

    #[test]
    fn test_first_arn_skips_blank() {
        let json = r#"{"profiles":[{"arn":""},{"arn":"arn:real"}]}"#;
        let resp: ListAvailableProfilesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.first_arn(), Some("arn:real"));
    }
}
