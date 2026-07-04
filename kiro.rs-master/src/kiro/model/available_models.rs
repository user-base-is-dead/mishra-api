//! available model query data model
//!
//! contains ListAvailableModels API the response type definition.
//!
//! upstreaminterface:`GET https://q.{api_region}.amazonaws.com/ListAvailableModels?origin=AI_EDITOR`
//! Returns the truly available model list for the credential right now (by subscription tier).

use serde::Deserialize;

/// ListAvailableModels API response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAvailableModelsResponse {
    /// availablemodellist
    #[serde(default)]
    pub models: Vec<UpstreamModel>,
}

/// singleupstreammodel
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamModel {
    /// model ID(such as "claude-sonnet-4.5")
    pub model_id: String,

    /// Model display name (may not exist).
    #[serde(default)]
    pub model_name: Option<String>,

    /// Model description (may not exist).
    #[serde(default)]
    pub description: Option<String>,

    /// Token Limit information (may not exist).
    #[serde(default)]
    pub token_limits: Option<TokenLimits>,
}

/// model Token quota limit
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenLimits {
    /// maximuminput Token count
    #[serde(default)]
    pub max_input_tokens: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_full_response() {
        let json = r#"{
            "models": [
                {
                    "modelId": "claude-sonnet-4.5",
                    "modelName": "Claude Sonnet 4.5",
                    "description": "balanced model",
                    "tokenLimits": { "maxInputTokens": 200000 }
                },
                {
                    "modelId": "claude-opus-4.6"
                }
            ]
        }"#;
        let resp: ListAvailableModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);

        let first = &resp.models[0];
        assert_eq!(first.model_id, "claude-sonnet-4.5");
        assert_eq!(first.model_name.as_deref(), Some("Claude Sonnet 4.5"));
        assert_eq!(
            first.token_limits.as_ref().unwrap().max_input_tokens,
            Some(200000)
        );

        // only modelId the minimal object: the remaining fields default to None
        let second = &resp.models[1];
        assert_eq!(second.model_id, "claude-opus-4.6");
        assert!(second.model_name.is_none());
        assert!(second.token_limits.is_none());
    }

    #[test]
    fn test_deserialize_empty_models() {
        let resp: ListAvailableModelsResponse =
            serde_json::from_str(r#"{"models":[]}"#).unwrap();
        assert!(resp.models.is_empty());
    }

    #[test]
    fn test_deserialize_missing_models_field() {
        // missing models fall back to an empty array for the field
        let resp: ListAvailableModelsResponse = serde_json::from_str(r#"{}"#).unwrap();
        assert!(resp.models.is_empty());
    }
}
