//! Executes capability effects without owning the overlay or its write lock.

use crate::settings::MinterFactory;
use hermes_client::{
    CallError, CallOptions, Client, capabilities_api as api,
    rest::{RestClient, RestError, RestOptions},
};
use hermes_contract::{
    methods::legacy,
    native::NativeMethod,
    schemas::{
        PluginCatalogLookupParams, PluginsAction, PluginsManageParams, SkillsAction,
        SkillsManageParams, SkillsReloadParams,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt, time::Duration};
use talaria_core::capabilities::*;

// The local response projection tolerates omitted display metadata and drops
// filesystem fields before they can enter application state.
struct PluginCall;
impl hermes_contract::method::Method for PluginCall {
    const NAME: &'static str =
        <legacy::plugins_manage::Marker as hermes_contract::method::Method>::NAME;
    type Params = PluginsManageParams;
    type Result = PluginReply;
}
impl NativeMethod for PluginCall {}
struct LookupCall;
impl hermes_contract::method::Method for LookupCall {
    const NAME: &'static str =
        <legacy::plugins_catalog_lookup::Marker as hermes_contract::method::Method>::NAME;
    type Params = PluginCatalogLookupParams;
    type Result = LookupReply;
}
impl NativeMethod for LookupCall {}

#[derive(Deserialize, Serialize)]
struct PluginReply {
    plugins: Option<Vec<SafePlugin>>,
    plugin: Option<SafePlugin>,
    ok: Option<bool>,
    unchanged: Option<bool>,
    restart_required: Option<bool>,
    activation: Option<SafeActivation>,
    plugin_name: Option<String>,
    warnings: Option<Vec<String>>,
    missing_env: Option<Vec<String>>,
    known_issues: Option<Vec<String>>,
    enabled: Option<bool>,
    cleared_memory_provider: Option<bool>,
}
#[derive(Deserialize, Serialize)]
struct SafeActivation {
    live_now: Option<SafeLiveNow>,
    #[serde(default, deserialize_with = "deferred_kinds")]
    deferred: BTreeMap<String, Vec<String>>,
}
fn deferred_kinds<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Vec<String>>, D::Error> {
    Ok(Option::<BTreeMap<String, Vec<String>>>::deserialize(deserializer)?.unwrap_or_default())
}
#[derive(Deserialize, Serialize)]
struct SafeLiveNow {
    mcp_servers: Option<Vec<LiveName>>,
    skills: Option<Vec<LiveName>>,
}
#[derive(Deserialize, Serialize)]
struct LiveName {
    name: String,
}
#[derive(Deserialize, Serialize)]
struct SafePlugin {
    name: String,
    key: Option<String>,
    version: Option<String>,
    description: Option<String>,
    source: Option<String>,
    status: Option<String>,
    servers: Option<Vec<SafeServer>>,
    catalog_tier: Option<String>,
    update_available: Option<bool>,
    #[serde(default)]
    settings_schema: Option<Vec<SafeSetting>>,
}
#[derive(Deserialize, Serialize)]
struct SafeServer {
    name: String,
    state: Option<String>,
    sentence: Option<String>,
}
#[derive(Serialize)]
struct SafeSetting {
    key: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    has_value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}
impl<'de> Deserialize<'de> for SafeSetting {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct WireSetting {
            key: Option<String>,
            label: Option<String>,
            #[serde(rename = "type")]
            kind: Option<String>,
            has_value: Option<bool>,
            value: Option<Value>,
        }
        let record = Value::deserialize(deserializer)?;
        if !record.is_object() {
            return Err(serde::de::Error::custom("expected a plugin setting object"));
        }
        let record: WireSetting =
            serde_json::from_value(record).map_err(serde::de::Error::custom)?;
        let kind = record.kind.unwrap_or_default();
        let value = if kind == "secret" { None } else { record.value };
        Ok(Self {
            key: record.key.unwrap_or_default(),
            label: record.label.unwrap_or_default(),
            kind,
            has_value: record.has_value.unwrap_or(false),
            value,
        })
    }
}
#[derive(Deserialize, Serialize)]
struct LookupReply {
    status: String,
    entry: Option<LookupEntry>,
}
#[derive(Deserialize, Serialize)]
struct LookupEntry {
    name: String,
    repo: String,
    sha: Option<String>,
}

fn activation_deferred(value: &Value) -> Vec<(String, Vec<String>)> {
    value
        .as_object()
        .map(|kinds| {
            kinds
                .iter()
                .map(|(kind, names)| (kind.clone(), strings(names)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn plugin_optional_metadata_accepts_missing_and_null() {
        for row in [
            json!({"name":"plugin"}),
            json!({"name":"plugin","key":null,"version":null,"description":null,"source":null,"status":null,"servers":null,"catalog_tier":null,"update_available":null,"settings_schema":null}),
        ] {
            let plugin: SafePlugin = serde_json::from_value(row).unwrap();
            let projected = serde_json::to_value(plugin).unwrap();
            assert!(projected["source"].is_null());
            assert!(projected["servers"].is_null());
        }
        for row in [json!({}), json!({"name":null}), json!({"name":3})] {
            assert!(serde_json::from_value::<SafePlugin>(row).is_err());
        }
        for key in [
            "key",
            "version",
            "description",
            "source",
            "status",
            "servers",
            "catalog_tier",
            "update_available",
            "settings_schema",
        ] {
            let mut row = json!({"name":"plugin"});
            row[key] = json!(5);
            assert!(
                serde_json::from_value::<SafePlugin>(row).is_err(),
                "field {key}"
            );
        }
    }

    #[test]
    fn setting_rejects_malformed_fields_and_discards_secret_content() {
        for value in [
            json!(null),
            json!([]),
            json!("text"),
            json!({"key":true}),
            json!({"label":4}),
            json!({"type":[]}),
            json!({"has_value":"yes"}),
        ] {
            assert!(serde_json::from_value::<SafeSetting>(value).is_err());
        }
        let setting: SafeSetting = serde_json::from_value(json!({"key":null,"label":null,"type":"secret","has_value":true,"value":"private-setting-content"})).unwrap();
        let projected = serde_json::to_value(setting).unwrap();
        assert!(projected.get("value").is_none());
        assert_eq!(projected["has_value"], true);
        assert!(!projected.to_string().contains("private-setting-content"));
        let setting: SafeSetting =
            serde_json::from_value(json!({"key":null,"label":null,"type":null,"has_value":null}))
                .unwrap();
        assert_eq!(setting.key, "");
        assert!(!setting.has_value);
    }

    #[test]
    fn deferred_activation_has_deterministic_kind_order_through_projection() {
        let reply: PluginReply = serde_json::from_str(
            r#"{"activation":{"deferred":{"z_tools":["tool"],"a_commands":["command"]}}}"#,
        )
        .unwrap();
        let projected = serde_json::to_value(reply).unwrap();
        assert_eq!(
            activation_deferred(&projected["activation"]["deferred"]),
            vec![
                ("a_commands".into(), vec!["command".into()]),
                ("z_tools".into(), vec!["tool".into()])
            ]
        );
        for value in [json!({}), json!({"deferred":null})] {
            let activation: SafeActivation = serde_json::from_value(value).unwrap();
            assert!(activation.deferred.is_empty());
        }
        for value in [
            json!({"deferred":[]}),
            json!({"deferred":{"tools":false}}),
            json!({"deferred":{"tools":[1]}}),
        ] {
            assert!(serde_json::from_value::<SafeActivation>(value).is_err());
        }
    }
}

pub struct CapabilitiesService {
    api_origin: Option<String>,
    served_profiles: Vec<String>,
    make_minter: MinterFactory,
    socket: Client,
    hermes_home: String,
    user_home: String,
}
impl fmt::Debug for CapabilitiesService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapabilitiesService")
            .field("connected", &self.api_origin.is_some())
            .field("served_count", &self.served_profiles.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeOutcome {
    Applied,
    Unchanged,
    Refused(String),
    Unknown,
    ReadBackMismatch,
}

impl CapabilitiesService {
    pub fn new(
        api_origin: Option<String>,
        served_profiles: Vec<String>,
        make_minter: MinterFactory,
        socket: Client,
        hermes_home: String,
        user_home: String,
    ) -> Self {
        Self {
            api_origin,
            served_profiles,
            make_minter,
            socket,
            hermes_home,
            user_home,
        }
    }
    fn check_profile(&self, profile: &str) -> Result<(), CapError> {
        if self.served_profiles.iter().any(|p| p == profile) {
            Ok(())
        } else {
            Err(CapError::Forbidden {
                detail: String::new(),
            })
        }
    }
    fn rest(&self, profile: &str) -> Result<RestClient, CapError> {
        self.check_profile(profile)?;
        RestClient::new(
            self.api_origin.as_deref().ok_or(CapError::NotConnected)?,
            (self.make_minter)(profile),
            RestOptions::default(),
        )
        .map_err(|e| self.rest_error(e))
    }
    async fn call<M: NativeMethod>(
        &self,
        params: M::Params,
        seconds: u64,
    ) -> Result<Value, CapError>
    where
        M::Result: Serialize,
    {
        let reply = self
            .socket
            .call_with::<M>(
                params,
                CallOptions {
                    timeout: Some(Duration::from_secs(seconds)),
                },
            )
            .await
            .map_err(|e| self.call_error(e))?;
        serde_json::to_value(reply).map_err(|_| CapError::BadResult)
    }
    async fn plugins(
        &self,
        profile: &str,
        mut params: PluginsManageParams,
        seconds: u64,
    ) -> Result<Value, CapError> {
        self.check_profile(profile)?;
        params.profile = Some(profile.to_owned());
        self.call::<PluginCall>(params, seconds).await
    }
    async fn load(&self, profile: &str, tab: CapTab) -> Result<CapData, CapError> {
        let value = match tab {
            CapTab::Skills => serde_json::to_value(
                api::skills_list(&self.rest(profile)?, profile)
                    .await
                    .map_err(|e| self.rest_error(e))?,
            )
            .map_err(|_| CapError::BadBody)?,
            CapTab::Toolsets => serde_json::to_value(
                api::toolsets_list(&self.rest(profile)?, profile)
                    .await
                    .map_err(|e| self.rest_error(e))?,
            )
            .map_err(|_| CapError::BadBody)?,
            CapTab::Plugins => {
                self.plugins(profile, PluginsManageParams::default(), 30)
                    .await?
            }
            CapTab::Hub => return self.browse(profile, 1).await,
        };
        self.data(tab, value)
    }
    fn data(&self, tab: CapTab, mut value: Value) -> Result<CapData, CapError> {
        self.clean_json(&mut value, "");
        CapData::from_json(tab, &value)
    }
    async fn browse(&self, profile: &str, page: u64) -> Result<CapData, CapError> {
        self.check_profile(profile)?;
        let page = i64::try_from(page).map_err(|_| CapError::Encode)?;
        let reply = self
            .call::<legacy::skills_manage::Marker>(
                SkillsManageParams {
                    action: SkillsAction::Browse,
                    profile: Some(profile.to_owned()),
                    page: Some(page),
                    page_size: Some(20),
                    ..Default::default()
                },
                45,
            )
            .await?;
        self.data(CapTab::Hub, reply)
    }
    pub async fn execute(&self, effect: CapEffect) -> Vec<CapInput> {
        let input = match effect {
            CapEffect::Fetch {
                profile,
                tab,
                generation,
            } => CapInput::Loaded {
                tab,
                generation,
                result: self.load(&profile, tab).await,
            },
            CapEffect::ReadBack {
                profile,
                tab,
                op_id,
            } => CapInput::ReadBack {
                op_id,
                result: self.load(&profile, tab).await,
            },
            CapEffect::FetchDetail {
                profile,
                tab,
                key,
                generation,
            } => CapInput::DetailLoaded {
                tab,
                generation,
                result: self.detail(&profile, tab, &key).await,
            },
            CapEffect::HubBrowse {
                profile,
                page,
                generation,
            } => CapInput::Loaded {
                tab: CapTab::Hub,
                generation,
                result: self.browse(&profile, page).await,
            },
            CapEffect::HubSearch {
                profile,
                q,
                generation,
            } => CapInput::Loaded {
                tab: CapTab::Hub,
                generation,
                result: self.search(&profile, &q).await,
            },
            CapEffect::HubPreview {
                profile,
                identifier,
            } => CapInput::PreviewDone(self.preview(&profile, &identifier).await),
            CapEffect::HubScan {
                profile,
                identifier,
            } => CapInput::ScanDone(self.scan(&profile, &identifier).await),
            CapEffect::ToggleSkill {
                profile,
                op_id,
                name,
                enabled,
            } => CapInput::WriteDone {
                op_id,
                result: self
                    .toggle(&profile, CapabilityChange::Skill { name, enabled })
                    .await,
            },
            CapEffect::ToggleToolset {
                profile,
                op_id,
                name,
                enabled,
            } => CapInput::WriteDone {
                op_id,
                result: self
                    .toggle(&profile, CapabilityChange::Toolset { name, enabled })
                    .await,
            },
            CapEffect::TogglePlugin {
                profile,
                op_id,
                key,
                enable,
            } => CapInput::WriteDone {
                op_id,
                result: self
                    .toggle(
                        &profile,
                        CapabilityChange::Plugin {
                            key,
                            enabled: enable,
                        },
                    )
                    .await,
            },
            CapEffect::InstallSkill {
                profile,
                op_id,
                install_key,
            } => CapInput::WriteDone {
                op_id,
                result: self.install_skill(&profile, install_key).await,
            },
            CapEffect::ReloadSkills { session_id } => CapInput::ReloadDone(
                self.call::<legacy::skills_reload::Marker>(
                    SkillsReloadParams {
                        session_id: Some(session_id),
                        ..Default::default()
                    },
                    30,
                )
                .await
                .map(|value| self.clean(value["output"].as_str().unwrap_or_default())),
            ),
            CapEffect::LookupPlugin { profile, name } => {
                CapInput::LookupDone(self.lookup(&profile, name).await)
            }
            CapEffect::InstallPlugin {
                profile,
                op_id,
                target,
            } => CapInput::WriteDone {
                op_id,
                result: self.install_plugin(&profile, target).await,
            },
            CapEffect::RemovePlugin {
                profile,
                op_id,
                name,
            } => CapInput::WriteDone {
                op_id,
                result: self
                    .plugins(
                        &profile,
                        PluginsManageParams {
                            action: PluginsAction::Remove,
                            name: Some(name),
                            ..Default::default()
                        },
                        30,
                    )
                    .await
                    .and_then(|v| self.receipt(v, true)),
            },
            CapEffect::FetchCatalog {
                profile,
                generation,
            } => CapInput::CatalogLoaded {
                generation,
                result: self.catalog(&profile).await,
            },
            CapEffect::ScheduleTick { .. } | CapEffect::Notice(_) | CapEffect::Dispose => {
                return Vec::new();
            }
        };
        vec![input]
    }
    async fn toggle(
        &self,
        profile: &str,
        change: CapabilityChange,
    ) -> Result<WriteReceipt, CapError> {
        match change {
            CapabilityChange::Skill { name, enabled } => {
                api::skill_toggle(&self.rest(profile)?, profile, &name, enabled)
                    .await
                    .map_err(|e| self.rest_error(e))?;
                Ok(WriteReceipt::default())
            }
            CapabilityChange::Toolset { name, enabled } => {
                let reply = api::toolset_toggle(&self.rest(profile)?, profile, &name, enabled)
                    .await
                    .map_err(|e| self.rest_error(e))?;
                Ok(WriteReceipt {
                    post_setup_started: reply.post_setup_started.map(|s| self.clean(&s)),
                    ..Default::default()
                })
            }
            CapabilityChange::Plugin { key, enabled } => {
                let value = self
                    .plugins(
                        profile,
                        PluginsManageParams {
                            action: PluginsAction::Toggle,
                            key: Some(key),
                            enable: Some(enabled),
                            ..Default::default()
                        },
                        30,
                    )
                    .await?;
                self.receipt(value, true)
            }
        }
    }
    async fn install_skill(&self, profile: &str, key: String) -> Result<WriteReceipt, CapError> {
        self.check_profile(profile)?;
        let reply = self
            .call::<legacy::skills_manage::Marker>(
                SkillsManageParams {
                    action: SkillsAction::Install,
                    query: Some(key),
                    profile: Some(profile.to_owned()),
                    ..Default::default()
                },
                120,
            )
            .await?;
        if reply["installed"].as_bool() != Some(true) {
            return Err(CapError::BadResult);
        }
        Ok(WriteReceipt::default())
    }
    async fn install_plugin(
        &self,
        profile: &str,
        target: InstallTarget,
    ) -> Result<WriteReceipt, CapError> {
        let mut params = PluginsManageParams {
            action: PluginsAction::Install,
            enable: Some(true),
            ..Default::default()
        };
        match target {
            InstallTarget::CatalogName(name) => params.catalog_name = Some(name),
            InstallTarget::Repository(address) => params.identifier = Some(address),
        }
        let reply = self.plugins(profile, params, 120).await?;
        self.receipt(reply, false)
    }
    fn receipt(&self, mut value: Value, requires_ok: bool) -> Result<WriteReceipt, CapError> {
        if requires_ok && value["ok"].as_bool() != Some(true) {
            return Err(CapError::BadResult);
        }
        if !requires_ok && value["plugin_name"].as_str().is_none() {
            return Err(CapError::BadResult);
        }
        self.clean_json(&mut value, "");
        let plugin = if value["plugin"].is_object() {
            match self.data(
                CapTab::Plugins,
                json!({"plugins":[value["plugin"].clone()]}),
            )? {
                CapData::Plugins(mut rows) => rows.pop(),
                _ => None,
            }
        } else {
            None
        };
        let activation = &value["activation"];
        let live_names = |key: &str| {
            activation["live_now"][key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v["name"].as_str().map(|s| self.clean(s)))
                        .collect()
                })
                .unwrap_or_default()
        };
        let deferred = activation_deferred(&activation["deferred"])
            .into_iter()
            .map(|(kind, names)| (self.clean(&kind), names))
            .collect();
        Ok(WriteReceipt {
            unchanged: value["unchanged"].as_bool().unwrap_or(false),
            plugin,
            post_setup_started: value["post_setup_started"].as_str().map(|s| self.clean(s)),
            restart_required: value["restart_required"].as_bool().unwrap_or(false),
            activation: Activation {
                live_servers: live_names("mcp_servers"),
                live_skills: live_names("skills"),
                deferred,
            },
            plugin_name: value["plugin_name"].as_str().map(|s| self.clean(s)),
            warnings: strings(&value["warnings"]),
            missing_env: strings(&value["missing_env"]),
            known_issues: strings(&value["known_issues"]),
            enabled: value["enabled"].as_bool().unwrap_or(false),
            cleared_memory_provider: value["cleared_memory_provider"].as_bool().unwrap_or(false),
        })
    }
    async fn search(&self, profile: &str, q: &str) -> Result<CapData, CapError> {
        let reply = api::hub_search(&self.rest(profile)?, profile, q)
            .await
            .map_err(|e| self.rest_error(e))?;
        let value = serde_json::to_value(reply).map_err(|_| CapError::BadBody)?;
        let mut data = self.data(CapTab::Hub, value)?;
        if let CapData::Hub(page) = &mut data {
            page.query = Some(q.to_owned());
        }
        Ok(data)
    }
    async fn preview(&self, profile: &str, identifier: &str) -> Result<String, CapError> {
        let reply = api::hub_preview(&self.rest(profile)?, profile, identifier)
            .await
            .map_err(|e| self.rest_error(e))?;
        Ok(self.content(&reply.skill_md))
    }
    async fn scan(&self, profile: &str, identifier: &str) -> Result<ScanReport, CapError> {
        let reply = api::hub_scan(&self.rest(profile)?, profile, identifier)
            .await
            .map_err(|e| self.rest_error(e))?;
        let policy = match reply.policy.as_str() {
            "allow" => ScanPolicy::Allow,
            "ask" => ScanPolicy::Ask,
            "block" => ScanPolicy::Block,
            other => ScanPolicy::Other(self.clean(other)),
        };
        let findings = reply
            .findings
            .into_iter()
            .map(|f| Finding {
                severity: self.clean(text(&f, "severity")),
                category: self.clean(text(&f, "category")),
                file: self.clean(text(&f, "file")),
                line: f["line"].as_u64().unwrap_or(0),
                description: self.clean(text(&f, "description")),
            })
            .collect();
        let count = |key| {
            reply.severity_counts[key]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        };
        Ok(ScanReport {
            verdict: self.clean(&reply.verdict),
            policy,
            policy_reason: self.clean(&reply.policy_reason),
            source: self.clean(&reply.source),
            trust: self.clean(reply.trust_level.as_deref().unwrap_or_default()),
            counts: [
                count("critical"),
                count("high"),
                count("medium"),
                count("low"),
            ],
            findings,
        })
    }
    async fn detail(&self, profile: &str, tab: CapTab, key: &str) -> Result<String, CapError> {
        match tab {
            CapTab::Skills => Ok(self.content(
                &api::skill_content(&self.rest(profile)?, profile, key)
                    .await
                    .map_err(|e| self.rest_error(e))?,
            )),
            CapTab::Hub => self.preview(profile, key).await,
            CapTab::Toolsets => {
                let reply = api::toolset_config(&self.rest(profile)?, profile, key)
                    .await
                    .map_err(|e| self.rest_error(e))?;
                let mut lines = Vec::new();
                for provider in reply.providers {
                    lines.push(format!(
                        "{}: {}",
                        self.clean(text(&provider, "name")),
                        self.clean(text(&provider, "status"))
                    ));
                    if let Some(keys) = provider["env_vars"].as_array() {
                        for key in keys {
                            lines.push(format!(
                                "{}: {}",
                                self.clean(text(key, "key")),
                                if key["is_set"].as_bool().unwrap_or(false) {
                                    "set"
                                } else {
                                    "not set"
                                }
                            ));
                        }
                    }
                }
                Ok(lines.join("\n"))
            }
            CapTab::Plugins => Ok(String::new()),
        }
    }
    async fn lookup(&self, profile: &str, name: String) -> Result<PluginLookup, CapError> {
        self.check_profile(profile)?;
        let value = self
            .call::<LookupCall>(
                PluginCatalogLookupParams {
                    name,
                    profile: Some(profile.to_owned()),
                },
                20,
            )
            .await?;
        Ok(match value["status"].as_str() {
            Some("found") => PluginLookup::Found {
                name: self.clean(text(&value["entry"], "name")),
                repository: self.clean(text(&value["entry"], "repo")),
                sha: value["entry"]["sha"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| self.clean(s)),
            },
            Some("unknown") => PluginLookup::Unknown,
            Some("invalid_name") => PluginLookup::InvalidName,
            Some("unavailable") => PluginLookup::Unavailable,
            _ => return Err(CapError::BadResult),
        })
    }
    async fn catalog(&self, profile: &str) -> Result<Vec<CatalogEntry>, CapError> {
        let reply = api::plugin_catalog(&self.rest(profile)?)
            .await
            .map_err(|e| self.rest_error(e))?;
        Ok(reply
            .entries
            .into_iter()
            .map(|v| CatalogEntry {
                name: self.clean(text(&v, "name")),
                title: self.clean(text(&v, "title")),
                description: self.clean(text(&v, "description")),
                tier: self.clean(text(&v, "tier")),
                sha_short: self.clean(text(&v, "sha_short")),
                installed: v["installed"].as_bool().unwrap_or(false),
            })
            .collect())
    }
    pub async fn apply(&self, profile: &str, change: CapabilityChange) -> ChangeOutcome {
        let (tab, key, enabled) = match &change {
            CapabilityChange::Skill { name, enabled } => (CapTab::Skills, name, *enabled),
            CapabilityChange::Toolset { name, enabled } => (CapTab::Toolsets, name, *enabled),
            CapabilityChange::Plugin { key, enabled } => (CapTab::Plugins, key, *enabled),
        };
        let before = match self.load(profile, tab).await {
            Ok(data) => data,
            Err(e) => {
                return if lost(&e) {
                    ChangeOutcome::Unknown
                } else {
                    ChangeOutcome::Refused(self.error_text(&e, profile, tab))
                };
            }
        };
        let state = match current(&before, key) {
            Some(Ok(state)) => state,
            Some(Err(name)) => {
                return ChangeOutcome::Refused(format_text(
                    CapText::ReadOnly,
                    &TextArgs {
                        name: self.clean(&name),
                        ..Default::default()
                    },
                ));
            }
            None => {
                return ChangeOutcome::Refused(format!(
                    "{} is not listed in profile {}.",
                    self.clean(key),
                    self.clean(profile)
                ));
            }
        };
        if state == enabled {
            return ChangeOutcome::Unchanged;
        }
        let receipt = match self.toggle(profile, change.clone()).await {
            Ok(r) => r,
            Err(e) => {
                return if unknown(&e) {
                    ChangeOutcome::Unknown
                } else {
                    ChangeOutcome::Refused(self.error_text(&e, profile, tab))
                };
            }
        };
        if receipt.unchanged {
            return ChangeOutcome::Unchanged;
        }
        let after = if let Some(plugin) = receipt.plugin {
            CapData::Plugins(vec![plugin])
        } else {
            match self.load(profile, tab).await {
                Ok(data) => data,
                Err(_) => return ChangeOutcome::Unknown,
            }
        };
        if current(&after, key) == Some(Ok(enabled)) {
            ChangeOutcome::Applied
        } else {
            ChangeOutcome::ReadBackMismatch
        }
    }
    pub fn read_error_text(&self, error: &CapError, tab: CapTab, profile: &str) -> String {
        if matches!(
            error,
            CapError::Timeout { sent: true }
                | CapError::Dropped { sent: true }
                | CapError::TimedOut
                | CapError::Unknown
        ) {
            return format_text(
                if tab == CapTab::Hub {
                    CapText::HubUnavailable
                } else {
                    CapText::ReadTimeout
                },
                &TextArgs::default(),
            );
        }
        if tab == CapTab::Hub
            && matches!(
                error,
                CapError::Server { .. } | CapError::Rpc { code: 5024, .. }
            )
        {
            return format_text(CapText::HubUnavailable, &TextArgs::default());
        }
        self.error_text(error, profile, tab)
    }
    fn error_text(&self, error: &CapError, profile: &str, tab: CapTab) -> String {
        let args = TextArgs {
            profile: self.clean(profile),
            tab,
            ..Default::default()
        };
        let kind = match error {
            CapError::Forbidden { .. } | CapError::Rpc { code: 4064, .. } => {
                CapText::ProfileUnavailable
            }
            CapError::Rpc {
                code: 4001,
                reason: Some(reason),
                ..
            } if reason == "profile_mismatch" => CapText::ProfileUnavailable,
            CapError::Connect
            | CapError::Ticket
            | CapError::NotLoopback
            | CapError::TlsNotSupported
            | CapError::Timeout { sent: false }
            | CapError::Dropped { sent: false }
            | CapError::NotConnected
            | CapError::Encode
            | CapError::InvalidTimeout => CapText::Unreachable,
            CapError::Unauthorized => CapText::Unauthorized,
            CapError::Rpc { code: -32601, .. } => {
                return format_text(
                    CapText::Unsupported,
                    &TextArgs {
                        key: if matches!(tab, CapTab::Skills | CapTab::Hub) {
                            "skills.manage"
                        } else {
                            "plugins.manage"
                        }
                        .into(),
                        ..args
                    },
                );
            }
            CapError::NotFound { route } => {
                return format_text(
                    CapText::Unsupported,
                    &TextArgs {
                        key: self.clean(route),
                        ..args
                    },
                );
            }
            _ => {
                let detail = match error {
                    CapError::Rpc {
                        code: 4001,
                        reason: Some(reason),
                        ..
                    } => reason.clone(),
                    CapError::Rpc { message, .. } => message.clone(),
                    CapError::Refused { status, detail } => {
                        detail.clone().unwrap_or_else(|| format!("HTTP {status}"))
                    }
                    CapError::Server { status } => format!("HTTP {status}"),
                    _ => "unreadable reply".into(),
                };
                return format_text(
                    CapText::Refused,
                    &TextArgs {
                        message: self.clean(&detail),
                        ..args
                    },
                );
            }
        };
        format_text(kind, &args)
    }
    fn rest_error(&self, error: RestError) -> CapError {
        match error {
            RestError::NotLoopback => CapError::NotLoopback,
            RestError::TlsNotSupported => CapError::TlsNotSupported,
            RestError::Ticket(_) => CapError::Ticket,
            RestError::Connect(_) => CapError::Connect,
            RestError::Timeout { sent } => CapError::Timeout { sent },
            RestError::Dropped { sent } => CapError::Dropped { sent },
            RestError::Unauthorized => CapError::Unauthorized,
            RestError::Forbidden { detail } => CapError::Forbidden {
                detail: self.clean(&detail),
            },
            RestError::NotFound { route } => CapError::NotFound {
                route: self.clean(&route),
            },
            RestError::Refused { status, detail } => CapError::Refused {
                status,
                detail: detail.map(|s| self.clean(&s)),
            },
            RestError::Server { status } => CapError::Server { status },
            RestError::BodyTooLarge => CapError::BodyTooLarge,
            RestError::BadBody(_) => CapError::BadBody,
        }
    }
    fn call_error(&self, error: CallError) -> CapError {
        match error {
            CallError::NotConnected => CapError::NotConnected,
            CallError::Encode => CapError::Encode,
            CallError::InvalidTimeout => CapError::InvalidTimeout,
            CallError::TimedOut { .. } => CapError::TimedOut,
            CallError::Unknown { .. } => CapError::Unknown,
            CallError::BadResult { .. } => CapError::BadResult,
            CallError::Rpc(error) => CapError::Rpc {
                code: error.code,
                message: self.clean(&error.message),
                reason: error.reason.map(|s| self.clean(&s)),
                log: error
                    .data
                    .as_ref()
                    .and_then(|v| v["log"].as_str())
                    .map(|s| self.content(s)),
            },
        }
    }
    fn clean(&self, text: &str) -> String {
        let text = self.safe(text);
        if text.chars().count() > 300 {
            format!("{}…", text.chars().take(299).collect::<String>())
        } else {
            text
        }
    }
    fn content(&self, text: &str) -> String {
        api::cap_content(
            &text
                .split('\n')
                .map(|line| self.safe(line))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
    fn safe(&self, text: &str) -> String {
        let mut out = String::new();
        for c in text.chars() {
            match c {
                '\t' => out.push_str("    "),
                '\u{1b}' => out.push('␛'),
                '\u{7f}' => out.push('␡'),
                '\u{0}'..='\u{1f}' => {
                    if let Some(c) = char::from_u32(0x2400 + u32::from(c)) {
                        out.push(c);
                    }
                }
                '\u{80}'..='\u{9f}'
                | '\u{61c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{e0000}'..='\u{e007f}' => {
                    use std::fmt::Write;
                    let _ = write!(out, "‹U+{:04X}›", u32::from(c));
                }
                _ => out.push(c),
            }
        }
        redact_paths(&out, &self.hermes_home, &self.user_home)
    }
    fn clean_json(&self, value: &mut Value, key: &str) {
        match value {
            Value::Object(object) => {
                object.remove("path");
                object.remove("install_dir");
                object.remove("after_install_path");
                if object.get("type").and_then(Value::as_str) == Some("secret") {
                    object.remove("value");
                    object.remove("default");
                }
                for (key, value) in object {
                    self.clean_json(value, key);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.clean_json(item, key);
                }
            }
            Value::String(text)
                if !matches!(
                    key,
                    "name"
                        | "key"
                        | "identifier"
                        | "source"
                        | "status"
                        | "provenance"
                        | "category"
                        | "platform"
                ) =>
            {
                *text = self.clean(text)
            }
            _ => {}
        }
    }
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn unknown(error: &CapError) -> bool {
    matches!(
        error,
        CapError::Timeout { sent: true }
            | CapError::Dropped { sent: true }
            | CapError::TimedOut
            | CapError::Unknown
            | CapError::BadBody
            | CapError::BodyTooLarge
            | CapError::BadResult
    )
}
fn lost(error: &CapError) -> bool {
    matches!(
        error,
        CapError::Timeout { sent: true }
            | CapError::Dropped { sent: true }
            | CapError::TimedOut
            | CapError::Unknown
    )
}
fn current(data: &CapData, key: &str) -> Option<Result<bool, String>> {
    match data {
        CapData::Skills(rows) => rows.iter().find(|r| r.name == key).map(|r| Ok(r.enabled)),
        CapData::Toolsets(rows) => rows.iter().find(|r| r.name == key).map(|r| Ok(r.enabled)),
        CapData::Plugins(rows) => rows
            .iter()
            .find(|r| r.key.as_deref() == Some(key))
            .or_else(|| rows.iter().find(|r| r.key.is_none() && r.name == key))
            .map(|r| match r.status {
                PluginStatus::Enabled if r.key.is_some() => Ok(true),
                PluginStatus::Disabled | PluginStatus::NotEnabled if r.key.is_some() => Ok(false),
                _ => Err(r.name.clone()),
            }),
        CapData::Hub(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_key_takes_precedence_over_a_read_only_display_name() {
        let data = CapData::from_json(
            CapTab::Plugins,
            &json!({ "plugins": [
            { "name": "fixture-key", "status": "enabled" },
            { "name": "fixture-plugin", "key": "fixture-key", "status": "disabled" }
        ] }),
        )
        .unwrap();
        assert_eq!(current(&data, "fixture-key"), Some(Ok(false)));
    }
}
