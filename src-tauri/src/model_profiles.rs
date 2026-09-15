use serde::Serialize;

/// Compatibility with AIIDE's current agent protocol, not general model quality.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ProfileStatus { Unknown, Experimental, Compatible, Limited }

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability { Unknown, Supported, Limited }

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfile {
    pub label: &'static str,
    pub status: ProfileStatus,
    pub chat: Capability,
    pub repository_inspection: Capability,
    pub structured_edits: Capability,
    pub resource_class: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelAdaptation {
    pub protocol_hint: Option<&'static str>,
    pub protocol_temperature: Option<f32>,
}

const UNKNOWN: ModelProfile = ModelProfile {
    label: "Unknown", status: ProfileStatus::Unknown, chat: Capability::Unknown,
    repository_inspection: Capability::Unknown, structured_edits: Capability::Unknown,
    resource_class: None,
};

pub fn resolve(model: &str) -> ModelProfile {
    let id = model.trim().to_ascii_lowercase();
    if id == "qwen2.5-coder:7b" {
        ModelProfile { label: "Limited", status: ProfileStatus::Limited, chat: Capability::Supported,
            repository_inspection: Capability::Supported, structured_edits: Capability::Limited,
            resource_class: Some("7B") }
    } else if id == "qwen3:4b" {
        ModelProfile { label: "Experimental", status: ProfileStatus::Experimental, chat: Capability::Unknown,
            repository_inspection: Capability::Unknown, structured_edits: Capability::Limited,
            resource_class: Some("4B") }
    } else if id == "phi4-mini" || id.starts_with("phi4-mini:") {
        ModelProfile { label: "Limited", status: ProfileStatus::Limited, chat: Capability::Unknown,
            repository_inspection: Capability::Limited, structured_edits: Capability::Unknown,
            resource_class: Some("mini") }
    } else { UNKNOWN }
}

/// Prompt/configuration aids only. Invalid paths and proposals still reach normal validation.
pub fn adaptation(model: &str) -> ModelAdaptation {
    let id = model.trim().to_ascii_lowercase();
    if id == "phi4-mini" || id.starts_with("phi4-mini:") {
        ModelAdaptation {
            protocol_hint: Some("Repository paths must be copied exactly from tool results and must remain project-relative. Never add a leading slash or project directory name."),
            protocol_temperature: None,
        }
    } else { ModelAdaptation::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_models_case_insensitively() {
        assert_eq!(resolve("QWEN2.5-CODER:7B").status, ProfileStatus::Limited);
        assert_eq!(resolve("qwen3:4b").status, ProfileStatus::Experimental);
        assert_eq!(resolve("phi4-mini:latest").repository_inspection, Capability::Limited);
    }

    #[test]
    fn unknown_models_get_a_neutral_profile_and_no_adapter() {
        assert_eq!(resolve("future-model:9b"), UNKNOWN);
        assert_eq!(adaptation("future-model:9b"), ModelAdaptation::default());
    }

    #[test]
    fn adapters_only_supply_prompt_configuration() {
        let value = adaptation("phi4-mini:latest");
        assert!(value.protocol_hint.unwrap().contains("project-relative"));
        assert_eq!(value.protocol_temperature, None);
    }
}
