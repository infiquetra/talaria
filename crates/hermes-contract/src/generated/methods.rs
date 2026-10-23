// Hermes v0.22.4 commit 4d04303818b61bf3aa09278af48356315515965b contract sha256 248ce12519dcc1e830a15c5819c93e6d27ef18404d19da38c960570710f83d00
pub mod legacy {
    pub mod agents_list {
        pub const NAME: &str = "agents.list";
        pub type Params = crate::schemas::AgentsListParams;
        pub type Result = crate::schemas::AgentsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod approval_pending {
        pub const NAME: &str = "approval.pending";
        pub type Params = crate::schemas::ApprovalPendingParams;
        pub type Result = crate::schemas::ApprovalPendingResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod approval_received {
        pub const NAME: &str = "approval.received";
        pub type Params = crate::schemas::ApprovalReceivedParams;
        pub type Result = crate::schemas::ApprovalReceivedResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod approval_respond {
        pub const NAME: &str = "approval.respond";
        pub type Params = crate::schemas::ApprovalRespondParams;
        pub type Result = crate::schemas::ApprovalRespondResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod billing_auto_reload {
        pub const NAME: &str = "billing.auto_reload";
        pub type Params = crate::schemas::BillingAutoReloadParams;
        pub type Result = crate::schemas::BillingMutationResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod billing_charge {
        pub const NAME: &str = "billing.charge";
        pub type Params = crate::schemas::BillingChargeParams;
        pub type Result = crate::schemas::BillingChargeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod billing_charge_status {
        pub const NAME: &str = "billing.charge_status";
        pub type Params = crate::schemas::BillingChargeStatusParams;
        pub type Result = crate::schemas::BillingChargeStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod billing_state {
        pub const NAME: &str = "billing.state";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::BillingStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod billing_step_up {
        pub const NAME: &str = "billing.step_up";
        pub type Params = crate::schemas::BillingStepUpParams;
        pub type Result = crate::schemas::BillingStepUpResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_deliver {
        pub const NAME: &str = "bot_relay.deliver";
        pub type Params = crate::schemas::BotRelayDeliverParams;
        pub type Result = crate::schemas::BotRelayDeliverResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_outbox_drain {
        pub const NAME: &str = "bot_relay.outbox.drain";
        pub type Params = crate::schemas::BotRelayOutboxDrainParams;
        pub type Result = crate::schemas::BotRelayOutboxDrainResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_reply {
        pub const NAME: &str = "bot_relay.reply";
        pub type Params = crate::schemas::BotRelayReplyParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_roster_sync {
        pub const NAME: &str = "bot_relay.roster.sync";
        pub type Params = crate::schemas::BotRelayRosterSyncParams;
        pub type Result = crate::schemas::BotRelayRosterSyncResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod browser_controller_detach {
        pub const NAME: &str = "browser.controller.detach";
        pub type Params = crate::schemas::BrowserControllerParams;
        pub type Result = crate::schemas::BrowserControllerDetachResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod browser_controller_heartbeat {
        pub const NAME: &str = "browser.controller.heartbeat";
        pub type Params = crate::schemas::BrowserControllerParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod browser_controller_register {
        pub const NAME: &str = "browser.controller.register";
        pub type Params = crate::schemas::BrowserControllerRegisterParams;
        pub type Result = crate::schemas::BrowserControllerRegisterResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod browser_controller_result {
        pub const NAME: &str = "browser.controller.result";
        pub type Params = crate::schemas::BrowserControllerResultParams;
        pub type Result = crate::schemas::BrowserControllerResultResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod browser_manage {
        pub const NAME: &str = "browser.manage";
        pub type Params = crate::schemas::BrowserManageParams;
        pub type Result = crate::schemas::BrowserManageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod clarify_lock {
        pub const NAME: &str = "clarify.lock";
        pub type Params = crate::schemas::ClarifyLockParams;
        pub type Result = crate::schemas::ClarifyLockResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cli_exec {
        pub const NAME: &str = "cli.exec";
        pub type Params = crate::schemas::CliExecParams;
        pub type Result = crate::schemas::CliExecResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod client_capabilities {
        pub const NAME: &str = "client.capabilities";
        pub type Params = crate::schemas::ClientCapabilitiesParams;
        pub type Result = crate::schemas::ClientCapabilitiesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod clipboard_paste {
        pub const NAME: &str = "clipboard.paste";
        pub type Params = crate::schemas::ClipboardPasteParams;
        pub type Result = crate::schemas::AttachedImageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod command_dispatch {
        pub const NAME: &str = "command.dispatch";
        pub type Params = crate::schemas::CommandDispatchParams;
        pub type Result = crate::schemas::CommandDispatchResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod command_resolve {
        pub const NAME: &str = "command.resolve";
        pub type Params = crate::schemas::CommandResolveParams;
        pub type Result = crate::schemas::CommandResolveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod commands_catalog {
        pub const NAME: &str = "commands.catalog";
        pub type Params = crate::schemas::CommandsCatalogParams;
        pub type Result = crate::schemas::CommandsCatalogResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod complete_path {
        pub const NAME: &str = "complete.path";
        pub type Params = crate::schemas::CompletePathParams;
        pub type Result = crate::schemas::CompletionItemsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod complete_slash {
        pub const NAME: &str = "complete.slash";
        pub type Params = crate::schemas::CompleteSlashParams;
        pub type Result = crate::schemas::CompleteSlashResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod config_get {
        pub const NAME: &str = "config.get";
        pub type Params = crate::schemas::ConfigGetParams;
        pub type Result = crate::schemas::ConfigGetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod config_set {
        pub const NAME: &str = "config.set";
        pub type Params = crate::schemas::ConfigSetParams;
        pub type Result = crate::schemas::ConfigSetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod config_show {
        pub const NAME: &str = "config.show";
        pub type Params = crate::schemas::ConfigShowParams;
        pub type Result = crate::schemas::ConfigShowResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connection_respond {
        pub const NAME: &str = "connection.respond";
        pub type Params = crate::schemas::ConnectionRespondParams;
        pub type Result = crate::schemas::ConnectionRespondResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_accounts {
        pub const NAME: &str = "connectors.accounts";
        pub type Params = crate::schemas::ConnectorAccountsParams;
        pub type Result = crate::schemas::ConnectorAccountsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_accounts_remove {
        pub const NAME: &str = "connectors.accounts.remove";
        pub type Params = crate::schemas::ConnectorAccountsRemoveParams;
        pub type Result = crate::schemas::ConnectorAccountsRemoveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_accounts_rename {
        pub const NAME: &str = "connectors.accounts.rename";
        pub type Params = crate::schemas::ConnectorAccountsRenameParams;
        pub type Result = crate::schemas::ConnectorAccountRow;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_catalog {
        pub const NAME: &str = "connectors.catalog";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::ConnectorsCatalogResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_connect {
        pub const NAME: &str = "connectors.connect";
        pub type Params = crate::schemas::ConnectorsConnectParams;
        pub type Result = crate::schemas::ConnectorsConnectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_list {
        pub const NAME: &str = "connectors.list";
        pub type Params = crate::schemas::ConnectorsListParams;
        pub type Result = crate::schemas::ConnectorsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_operation_status {
        pub const NAME: &str = "connectors.operation.status";
        pub type Params = crate::schemas::ConnectionOperationParams;
        pub type Result = crate::schemas::ConnectionOperationStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_operation_wake {
        pub const NAME: &str = "connectors.operation.wake";
        pub type Params = crate::schemas::ConnectionOperationParams;
        pub type Result = crate::schemas::ConnectionWakeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_policy_get {
        pub const NAME: &str = "connectors.policy.get";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::ConnectorPolicyGetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_policy_set {
        pub const NAME: &str = "connectors.policy.set";
        pub type Params = crate::schemas::ConnectorPolicySetParams;
        pub type Result = crate::schemas::ConnectorPolicySetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod connectors_tools {
        pub const NAME: &str = "connectors.tools";
        pub type Params = crate::schemas::ConnectorToolsParams;
        pub type Result = crate::schemas::ConnectorToolsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cron_manage {
        pub const NAME: &str = "cron.manage";
        pub type Params = crate::schemas::CronManageParams;
        pub type Result = crate::schemas::CronManageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod delegation_pause {
        pub const NAME: &str = "delegation.pause";
        pub type Params = crate::schemas::DelegationPauseParams;
        pub type Result = crate::schemas::DelegationPauseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod delegation_status {
        pub const NAME: &str = "delegation.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::DelegationStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod diagnostics_share_nous {
        pub const NAME: &str = "diagnostics.share_nous";
        pub type Params = crate::schemas::DiagnosticsShareNousParams;
        pub type Result = crate::schemas::DiagnosticsShareNousResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_install {
        pub const NAME: &str = "display.install";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::DisplayInstallResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_lease_acquire {
        pub const NAME: &str = "display.lease.acquire";
        pub type Params = crate::schemas::DisplayLeaseAcquireParams;
        pub type Result = crate::schemas::DisplayLeaseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_lease_release {
        pub const NAME: &str = "display.lease.release";
        pub type Params = crate::schemas::DisplayLeaseReleaseParams;
        pub type Result = crate::schemas::DisplayLeaseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_observe {
        pub const NAME: &str = "display.observe";
        pub type Params = crate::schemas::DisplayObserveParams;
        pub type Result = crate::schemas::DisplayObserveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_start {
        pub const NAME: &str = "display.start";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::DisplayStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_status {
        pub const NAME: &str = "display.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::DisplayStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_stop {
        pub const NAME: &str = "display.stop";
        pub type Params = crate::schemas::DisplayStopParams;
        pub type Result = crate::schemas::DisplayStopResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_switch_sandbox_image {
        pub const NAME: &str = "display.switchSandboxImage";
        pub type Params = crate::schemas::DisplaySwitchSandboxImageParams;
        pub type Result = crate::schemas::DisplaySwitchSandboxImageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod display_thumbnail {
        pub const NAME: &str = "display.thumbnail";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::DisplayThumbnailResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod file_attach {
        pub const NAME: &str = "file.attach";
        pub type Params = crate::schemas::FileAttachParams;
        pub type Result = crate::schemas::FileAttachResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod free_tier_ack_notice {
        pub const NAME: &str = "free_tier.ack_notice";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::FreeTierAckNoticeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod free_tier_challenge_result {
        pub const NAME: &str = "free_tier.challenge_result";
        pub type Params = crate::schemas::FreeTierChallengeResultParams;
        pub type Result = crate::schemas::FreeTierChallengeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod free_tier_claim_nudge {
        pub const NAME: &str = "free_tier.claim_nudge";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::FreeTierClaimNudgeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod free_tier_provision {
        pub const NAME: &str = "free_tier.provision";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::FreeTierProvisionResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod free_tier_status {
        pub const NAME: &str = "free_tier.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::FreeTierStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod gateway_capabilities {
        pub const NAME: &str = "gateway.capabilities";
        pub type Params = crate::schemas::PingParams;
        pub type Result = crate::schemas::GatewayCapabilitiesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_approve {
        pub const NAME: &str = "groups.approve";
        pub type Params = crate::schemas::GroupsApproveParams;
        pub type Result = crate::schemas::GroupsApproveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_capabilities {
        pub const NAME: &str = "groups.capabilities";
        pub type Params = crate::schemas::GroupsCapabilitiesParams;
        pub type Result = crate::schemas::GroupsCapabilitiesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_create {
        pub const NAME: &str = "groups.create";
        pub type Params = crate::schemas::GroupsCreateParams;
        pub type Result = crate::schemas::GroupsCreateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_demote {
        pub const NAME: &str = "groups.demote";
        pub type Params = crate::schemas::GroupsDemoteParams;
        pub type Result = crate::schemas::GroupsDemoteResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_disband {
        pub const NAME: &str = "groups.disband";
        pub type Params = crate::schemas::GroupsDisbandParams;
        pub type Result = crate::schemas::GroupsDisbandResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_list {
        pub const NAME: &str = "groups.list";
        pub type Params = crate::schemas::GroupsListParams;
        pub type Result = crate::schemas::GroupsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_log {
        pub const NAME: &str = "groups.log";
        pub type Params = crate::schemas::GroupsLogParams;
        pub type Result = crate::schemas::GroupsLogResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_peer_invite {
        pub const NAME: &str = "groups.peer.invite";
        pub type Params = crate::schemas::GroupsPeerInviteParams;
        pub type Result = crate::schemas::GroupsPeerInviteResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_peer_register {
        pub const NAME: &str = "groups.peer.register";
        pub type Params = crate::schemas::GroupsPeerRegisterParams;
        pub type Result = crate::schemas::GroupsPeerRegisterResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_peer_revoke {
        pub const NAME: &str = "groups.peer.revoke";
        pub type Params = crate::schemas::GroupsPeerRevokeParams;
        pub type Result = crate::schemas::GroupsPeerRevokeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_promote {
        pub const NAME: &str = "groups.promote";
        pub type Params = crate::schemas::GroupsPromoteParams;
        pub type Result = crate::schemas::GroupsPromoteResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_rename {
        pub const NAME: &str = "groups.rename";
        pub type Params = crate::schemas::GroupsRenameParams;
        pub type Result = crate::schemas::GroupsRenameResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_replica_state {
        pub const NAME: &str = "groups.replica_state";
        pub type Params = crate::schemas::GroupsReplicaStateParams;
        pub type Result = crate::schemas::GroupsReplicaStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_replicate {
        pub const NAME: &str = "groups.replicate";
        pub type Params = crate::schemas::GroupsReplicateParams;
        pub type Result = crate::schemas::GroupsReplicateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_retry {
        pub const NAME: &str = "groups.retry";
        pub type Params = crate::schemas::GroupsRetryParams;
        pub type Result = crate::schemas::GroupsRetryResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_send {
        pub const NAME: &str = "groups.send";
        pub type Params = crate::schemas::GroupsSendParams;
        pub type Result = crate::schemas::GroupsSendResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_state {
        pub const NAME: &str = "groups.state";
        pub type Params = crate::schemas::GroupsStateParams;
        pub type Result = crate::schemas::GroupsStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_stop {
        pub const NAME: &str = "groups.stop";
        pub type Params = crate::schemas::GroupsStopParams;
        pub type Result = crate::schemas::GroupsStopResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod handoff_fail {
        pub const NAME: &str = "handoff.fail";
        pub type Params = crate::schemas::HandoffFailParams;
        pub type Result = crate::schemas::HandoffFailResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod handoff_request {
        pub const NAME: &str = "handoff.request";
        pub type Params = crate::schemas::HandoffRequestParams;
        pub type Result = crate::schemas::HandoffRequestResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod handoff_state {
        pub const NAME: &str = "handoff.state";
        pub type Params = crate::schemas::SessionParams;
        pub type Result = crate::schemas::HandoffStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod i18n_catalog {
        pub const NAME: &str = "i18n.catalog";
        pub type Params = crate::schemas::I18nCatalogParams;
        pub type Result = crate::schemas::I18nCatalogResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod i18n_languages {
        pub const NAME: &str = "i18n.languages";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::I18nLanguagesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod image_attach {
        pub const NAME: &str = "image.attach";
        pub type Params = crate::schemas::ImageAttachParams;
        pub type Result = crate::schemas::AttachedImageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod image_attach_bytes {
        pub const NAME: &str = "image.attach_bytes";
        pub type Params = crate::schemas::ImageAttachBytesParams;
        pub type Result = crate::schemas::AttachedImageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod image_detach {
        pub const NAME: &str = "image.detach";
        pub type Params = crate::schemas::ImageDetachParams;
        pub type Result = crate::schemas::ImageDetachResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod image_generate {
        pub const NAME: &str = "image.generate";
        pub type Params = crate::schemas::ImageGenerateParams;
        pub type Result = crate::schemas::ImageGenerateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod input_detect_drop {
        pub const NAME: &str = "input.detect_drop";
        pub type Params = crate::schemas::InputDetectDropParams;
        pub type Result = crate::schemas::InputDetectDropResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod insights_get {
        pub const NAME: &str = "insights.get";
        pub type Params = crate::schemas::InsightsGetParams;
        pub type Result = crate::schemas::InsightsGetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod learning_delete {
        pub const NAME: &str = "learning.delete";
        pub type Params = crate::schemas::LearningNodeParams;
        pub type Result = crate::schemas::LearningMutationResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod learning_detail {
        pub const NAME: &str = "learning.detail";
        pub type Params = crate::schemas::LearningNodeParams;
        pub type Result = crate::schemas::LearningDetailResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod learning_edit {
        pub const NAME: &str = "learning.edit";
        pub type Params = crate::schemas::LearningEditParams;
        pub type Result = crate::schemas::LearningMutationResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod learning_frames {
        pub const NAME: &str = "learning.frames";
        pub type Params = crate::schemas::LearningFramesParams;
        pub type Result = crate::schemas::LearningFramesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod llm_oneshot {
        pub const NAME: &str = "llm.oneshot";
        pub type Params = crate::schemas::LlmOneshotParams;
        pub type Result = crate::schemas::LlmOneshotResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod machine_facts {
        pub const NAME: &str = "machine.facts";
        pub type Params = crate::schemas::Params;
        pub type Result = crate::schemas::MachineFactsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_catalog {
        pub const NAME: &str = "mcp.catalog";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::McpCatalogResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_add {
        pub const NAME: &str = "mcp.servers.add";
        pub type Params = crate::schemas::McpServersAddParams;
        pub type Result = crate::schemas::McpServersAddResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_list {
        pub const NAME: &str = "mcp.servers.list";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::McpServersListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_oauth_callback {
        pub const NAME: &str = "mcp.servers.oauth.callback";
        pub type Params = crate::schemas::McpOauthCallbackParams;
        pub type Result = crate::schemas::McpOauthCallbackResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_oauth_cancel {
        pub const NAME: &str = "mcp.servers.oauth.cancel";
        pub type Params = crate::schemas::McpOauthFlowParams;
        pub type Result = crate::schemas::McpOauthCancelResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_oauth_poll {
        pub const NAME: &str = "mcp.servers.oauth.poll";
        pub type Params = crate::schemas::McpOauthFlowParams;
        pub type Result = crate::schemas::McpOauthPollResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_oauth_start {
        pub const NAME: &str = "mcp.servers.oauth.start";
        pub type Params = crate::schemas::McpOauthStartParams;
        pub type Result = crate::schemas::McpOauthStartResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_remove {
        pub const NAME: &str = "mcp.servers.remove";
        pub type Params = crate::schemas::McpServerNameParams;
        pub type Result = crate::schemas::McpServersRemoveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_set_api_key {
        pub const NAME: &str = "mcp.servers.set_api_key";
        pub type Params = crate::schemas::McpServersSetApiKeyParams;
        pub type Result = crate::schemas::McpServersSetApiKeyResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_status {
        pub const NAME: &str = "mcp.servers.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::McpServersStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod mcp_servers_test {
        pub const NAME: &str = "mcp.servers.test";
        pub type Params = crate::schemas::McpServerNameParams;
        pub type Result = crate::schemas::McpServersTestResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod message_react {
        pub const NAME: &str = "message.react";
        pub type Params = crate::schemas::MessageReactParams;
        pub type Result = crate::schemas::MessageReactResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod model_disconnect {
        pub const NAME: &str = "model.disconnect";
        pub type Params = crate::schemas::ModelDisconnectParams;
        pub type Result = crate::schemas::ModelDisconnectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod model_options {
        pub const NAME: &str = "model.options";
        pub type Params = crate::schemas::ModelOptionsParams;
        pub type Result = crate::schemas::ModelOptionsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod model_save_key {
        pub const NAME: &str = "model.save_key";
        pub type Params = crate::schemas::ModelSaveKeyParams;
        pub type Result = crate::schemas::ModelSaveKeyResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod onboarding_set_run {
        pub const NAME: &str = "onboarding.set_run";
        pub type Params = crate::schemas::OnboardingSetRunParams;
        pub type Result = crate::schemas::OnboardingRunStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod onboarding_skip {
        pub const NAME: &str = "onboarding.skip";
        pub type Params = crate::schemas::OnboardingSkipParams;
        pub type Result = crate::schemas::OnboardingSkipResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod onboarding_start {
        pub const NAME: &str = "onboarding.start";
        pub type Params = crate::schemas::OnboardingStartParams;
        pub type Result = crate::schemas::OnboardingStartResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod onboarding_state {
        pub const NAME: &str = "onboarding.state";
        pub type Params = crate::schemas::OnboardingStateParams;
        pub type Result = crate::schemas::OnboardingRunStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod paste_collapse {
        pub const NAME: &str = "paste.collapse";
        pub type Params = crate::schemas::PasteCollapseParams;
        pub type Result = crate::schemas::PasteCollapseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pdf_attach {
        pub const NAME: &str = "pdf.attach";
        pub type Params = crate::schemas::PdfAttachParams;
        pub type Result = crate::schemas::PdfAttachResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_cancel {
        pub const NAME: &str = "pet.cancel";
        pub type Params = crate::schemas::PetCancelParams;
        pub type Result = crate::schemas::PetCancelResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_cells {
        pub const NAME: &str = "pet.cells";
        pub type Params = crate::schemas::PetCellsParams;
        pub type Result = crate::schemas::PetCellsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_disable {
        pub const NAME: &str = "pet.disable";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_export {
        pub const NAME: &str = "pet.export";
        pub type Params = crate::schemas::PetSlugParams;
        pub type Result = crate::schemas::PetExportResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_gallery {
        pub const NAME: &str = "pet.gallery";
        pub type Params = crate::schemas::PetGalleryParams;
        pub type Result = crate::schemas::PetGalleryResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_generate {
        pub const NAME: &str = "pet.generate";
        pub type Params = crate::schemas::PetGenerateParams;
        pub type Result = crate::schemas::PetGenerateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_generate_status {
        pub const NAME: &str = "pet.generate.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::PetGenerateStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_hatch {
        pub const NAME: &str = "pet.hatch";
        pub type Params = crate::schemas::PetHatchParams;
        pub type Result = crate::schemas::PetHatchResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_info {
        pub const NAME: &str = "pet.info";
        pub type Params = crate::schemas::PetInfoParams;
        pub type Result = crate::schemas::PetInfoResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_info_meta {
        pub const NAME: &str = "pet.info.meta";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::PetInfoMetaResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_remove {
        pub const NAME: &str = "pet.remove";
        pub type Params = crate::schemas::PetSlugParams;
        pub type Result = crate::schemas::PetSlugResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_rename {
        pub const NAME: &str = "pet.rename";
        pub type Params = crate::schemas::PetRenameParams;
        pub type Result = crate::schemas::PetSlugResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_scale {
        pub const NAME: &str = "pet.scale";
        pub type Params = crate::schemas::PetScaleParams;
        pub type Result = crate::schemas::PetScaleResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_select {
        pub const NAME: &str = "pet.select";
        pub type Params = crate::schemas::PetSlugParams;
        pub type Result = crate::schemas::PetSlugResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod pet_thumb {
        pub const NAME: &str = "pet.thumb";
        pub type Params = crate::schemas::PetThumbParams;
        pub type Result = crate::schemas::PetThumbResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod ping {
        pub const NAME: &str = "ping";
        pub type Params = crate::schemas::PingParams;
        pub type Result = crate::schemas::PingResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod plugins_catalog_lookup {
        pub const NAME: &str = "plugins.catalog.lookup";
        pub type Params = crate::schemas::PluginCatalogLookupParams;
        pub type Result = crate::schemas::PluginCatalogLookupResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod plugins_list {
        pub const NAME: &str = "plugins.list";
        pub type Params = crate::schemas::PluginsListParams;
        pub type Result = crate::schemas::PluginsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod plugins_manage {
        pub const NAME: &str = "plugins.manage";
        pub type Params = crate::schemas::PluginsManageParams;
        pub type Result = crate::schemas::PluginsManageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod preview_restart {
        pub const NAME: &str = "preview.restart";
        pub type Params = crate::schemas::PreviewRestartParams;
        pub type Result = crate::schemas::TaskIdResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod process_kill {
        pub const NAME: &str = "process.kill";
        pub type Params = crate::schemas::ProcessKillParams;
        pub type Result = crate::schemas::ProcessKillResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod process_list {
        pub const NAME: &str = "process.list";
        pub type Params = crate::schemas::ProcessListParams;
        pub type Result = crate::schemas::ProcessListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod process_stop {
        pub const NAME: &str = "process.stop";
        pub type Params = crate::schemas::ProcessStopParams;
        pub type Result = crate::schemas::ProcessStopResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_configure {
        pub const NAME: &str = "profiles.configure";
        pub type Params = crate::schemas::ProfilesConfigureParams;
        pub type Result = crate::schemas::ProfilesConfigureResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_create {
        pub const NAME: &str = "profiles.create";
        pub type Params = crate::schemas::ProfilesCreateParams;
        pub type Result = crate::schemas::ProfilesCreateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_describe {
        pub const NAME: &str = "profiles.describe";
        pub type Params = crate::schemas::ProfileNameParams;
        pub type Result = crate::schemas::ProfilesDescribeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_get_asset {
        pub const NAME: &str = "profiles.get_asset";
        pub type Params = crate::schemas::ProfilesGetAssetParams;
        pub type Result = crate::schemas::ProfilesGetAssetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_list {
        pub const NAME: &str = "profiles.list";
        pub type Params = crate::schemas::ProfilesListParams;
        pub type Result = crate::schemas::ProfilesListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_set_asset {
        pub const NAME: &str = "profiles.set_asset";
        pub type Params = crate::schemas::ProfilesSetAssetParams;
        pub type Result = crate::schemas::ProfilesSetAssetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod project_facts {
        pub const NAME: &str = "project.facts";
        pub type Params = crate::schemas::ProjectFactsParams;
        pub type Result = crate::schemas::ProjectFactsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_add_folder {
        pub const NAME: &str = "projects.add_folder";
        pub type Params = crate::schemas::ProjectsAddFolderParams;
        pub type Result = crate::schemas::ProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_archive {
        pub const NAME: &str = "projects.archive";
        pub type Params = crate::schemas::ProjectsArchiveParams;
        pub type Result = crate::schemas::ProjectsPayload;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_create {
        pub const NAME: &str = "projects.create";
        pub type Params = crate::schemas::ProjectsCreateParams;
        pub type Result = crate::schemas::OptionalProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_delete {
        pub const NAME: &str = "projects.delete";
        pub type Params = crate::schemas::ProjectIdParams;
        pub type Result = crate::schemas::ProjectsPayload;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_discover_repos {
        pub const NAME: &str = "projects.discover_repos";
        pub type Params = crate::schemas::ProjectsDiscoverReposParams;
        pub type Result = crate::schemas::ProjectsDiscoverReposResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_for_cwd {
        pub const NAME: &str = "projects.for_cwd";
        pub type Params = crate::schemas::ProjectsForCwdParams;
        pub type Result = crate::schemas::ProjectsForCwdResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_get {
        pub const NAME: &str = "projects.get";
        pub type Params = crate::schemas::ProjectIdParams;
        pub type Result = crate::schemas::ProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_list {
        pub const NAME: &str = "projects.list";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::ProjectsPayload;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_project_sessions {
        pub const NAME: &str = "projects.project_sessions";
        pub type Params = crate::schemas::ProjectsProjectSessionsParams;
        pub type Result = crate::schemas::ProjectsProjectSessionsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_record_repos {
        pub const NAME: &str = "projects.record_repos";
        pub type Params = crate::schemas::ProjectsRecordReposParams;
        pub type Result = crate::schemas::ProjectsRecordReposResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_remove_folder {
        pub const NAME: &str = "projects.remove_folder";
        pub type Params = crate::schemas::ProjectFolderParams;
        pub type Result = crate::schemas::ProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_set_active {
        pub const NAME: &str = "projects.set_active";
        pub type Params = crate::schemas::ProjectsSetActiveParams;
        pub type Result = crate::schemas::ActiveIdResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_set_primary {
        pub const NAME: &str = "projects.set_primary";
        pub type Params = crate::schemas::ProjectFolderParams;
        pub type Result = crate::schemas::ProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_tree {
        pub const NAME: &str = "projects.tree";
        pub type Params = crate::schemas::ProjectsTreeParams;
        pub type Result = crate::schemas::ProjectsTreeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod projects_update {
        pub const NAME: &str = "projects.update";
        pub type Params = crate::schemas::ProjectsUpdateParams;
        pub type Result = crate::schemas::ProjectResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_background {
        pub const NAME: &str = "prompt.background";
        pub type Params = crate::schemas::SideAgentParams;
        pub type Result = crate::schemas::TaskIdResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_btw {
        pub const NAME: &str = "prompt.btw";
        pub type Params = crate::schemas::SideAgentParams;
        pub type Result = crate::schemas::TaskIdResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_submit {
        pub const NAME: &str = "prompt.submit";
        pub type Params = crate::schemas::PromptSubmitParams;
        pub type Result = crate::schemas::PromptSubmitResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod reload_env {
        pub const NAME: &str = "reload.env";
        pub type Params = crate::schemas::ReloadEnvParams;
        pub type Result = crate::schemas::ReloadEnvResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod reload_mcp {
        pub const NAME: &str = "reload.mcp";
        pub type Params = crate::schemas::ReloadMcpParams;
        pub type Result = crate::schemas::ReloadMcpResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod request_answer {
        pub const NAME: &str = "request.answer";
        pub type Params = crate::schemas::RequestAnswerParams;
        pub type Result = crate::schemas::RequestAnswerResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod rollback_diff {
        pub const NAME: &str = "rollback.diff";
        pub type Params = crate::schemas::RollbackDiffParams;
        pub type Result = crate::schemas::RollbackDiffResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod rollback_list {
        pub const NAME: &str = "rollback.list";
        pub type Params = crate::schemas::RollbackListParams;
        pub type Result = crate::schemas::RollbackListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod rollback_restore {
        pub const NAME: &str = "rollback.restore";
        pub type Params = crate::schemas::RollbackRestoreParams;
        pub type Result = crate::schemas::RollbackRestoreResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_activate {
        pub const NAME: &str = "session.activate";
        pub type Params = crate::schemas::SessionActivateParams;
        pub type Result = crate::schemas::SessionActivateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_active_list {
        pub const NAME: &str = "session.active_list";
        pub type Params = crate::schemas::SessionActiveListParams;
        pub type Result = crate::schemas::SessionActiveListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_archive {
        pub const NAME: &str = "session.archive";
        pub type Params = crate::schemas::SessionArchiveParams;
        pub type Result = crate::schemas::SessionArchiveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_branch {
        pub const NAME: &str = "session.branch";
        pub type Params = crate::schemas::SessionBranchParams;
        pub type Result = crate::schemas::SessionBranchResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_branch_stored {
        pub const NAME: &str = "session.branch_stored";
        pub type Params = crate::schemas::SessionBranchStoredParams;
        pub type Result = crate::schemas::SessionBranchStoredResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_branch_whole {
        pub const NAME: &str = "session.branch_whole";
        pub type Params = crate::schemas::SessionBranchWholeParams;
        pub type Result = crate::schemas::SessionBranchWholeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_close {
        pub const NAME: &str = "session.close";
        pub type Params = crate::schemas::SessionCloseParams;
        pub type Result = crate::schemas::SessionCloseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_compress {
        pub const NAME: &str = "session.compress";
        pub type Params = crate::schemas::SessionCompressParams;
        pub type Result = crate::schemas::SessionCompressResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_context_breakdown {
        pub const NAME: &str = "session.context_breakdown";
        pub type Params = crate::schemas::SessionContextBreakdownParams;
        pub type Result = crate::schemas::SessionContextBreakdownResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_control {
        pub const NAME: &str = "session.control";
        pub type Params = crate::schemas::SessionControlParams;
        pub type Result = crate::schemas::SessionControlResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_control_read {
        pub const NAME: &str = "session.control.read";
        pub type Params = crate::schemas::SessionControlReadParams;
        pub type Result = crate::schemas::SessionControlReadResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_create {
        pub const NAME: &str = "session.create";
        pub type Params = crate::schemas::SessionCreateParams;
        pub type Result = crate::schemas::SessionCreateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_cwd_set {
        pub const NAME: &str = "session.cwd.set";
        pub type Params = crate::schemas::SessionCwdSetParams;
        pub type Result = crate::schemas::SessionCwdSetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_delete {
        pub const NAME: &str = "session.delete";
        pub type Params = crate::schemas::SessionDeleteParams;
        pub type Result = crate::schemas::SessionDeleteResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_events_since {
        pub const NAME: &str = "session.events.since";
        pub type Params = crate::schemas::SessionEventsSinceParams;
        pub type Result = crate::schemas::SessionEventsSinceResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_events_stats {
        pub const NAME: &str = "session.events.stats";
        pub type Params = crate::schemas::SessionEventsStatsParams;
        pub type Result = crate::schemas::SessionEventsStatsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_foreign_import {
        pub const NAME: &str = "session.foreign.import";
        pub type Params = crate::schemas::SessionForeignIdParams;
        pub type Result = crate::schemas::SessionForeignImportResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_foreign_list {
        pub const NAME: &str = "session.foreign.list";
        pub type Params = crate::schemas::SessionForeignListParams;
        pub type Result = crate::schemas::SessionForeignListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_foreign_preview {
        pub const NAME: &str = "session.foreign.preview";
        pub type Params = crate::schemas::SessionForeignIdParams;
        pub type Result = crate::schemas::SessionForeignPreviewResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_history {
        pub const NAME: &str = "session.history";
        pub type Params = crate::schemas::SessionHistoryParams;
        pub type Result = crate::schemas::SessionHistoryResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_interrupt {
        pub const NAME: &str = "session.interrupt";
        pub type Params = crate::schemas::SessionInterruptParams;
        pub type Result = crate::schemas::SessionInterruptResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_list {
        pub const NAME: &str = "session.list";
        pub type Params = crate::schemas::SessionListParams;
        pub type Result = crate::schemas::SessionListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_most_recent {
        pub const NAME: &str = "session.most_recent";
        pub type Params = crate::schemas::SessionMostRecentParams;
        pub type Result = crate::schemas::SessionMostRecentResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_redirect {
        pub const NAME: &str = "session.redirect";
        pub type Params = crate::schemas::SessionCorrectionParams;
        pub type Result = crate::schemas::SessionCorrectionResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_resume {
        pub const NAME: &str = "session.resume";
        pub type Params = crate::schemas::SessionResumeParams;
        pub type Result = crate::schemas::SessionResumeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_save {
        pub const NAME: &str = "session.save";
        pub type Params = crate::schemas::SessionSaveParams;
        pub type Result = crate::schemas::SessionSaveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_set_hidden {
        pub const NAME: &str = "session.set_hidden";
        pub type Params = crate::schemas::SessionSetHiddenParams;
        pub type Result = crate::schemas::SessionSetHiddenResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_status {
        pub const NAME: &str = "session.status";
        pub type Params = crate::schemas::SessionStatusParams;
        pub type Result = crate::schemas::SessionStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_steer {
        pub const NAME: &str = "session.steer";
        pub type Params = crate::schemas::SessionCorrectionParams;
        pub type Result = crate::schemas::SessionCorrectionResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_title {
        pub const NAME: &str = "session.title";
        pub type Params = crate::schemas::SessionTitleParams;
        pub type Result = crate::schemas::SessionTitleResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_undo {
        pub const NAME: &str = "session.undo";
        pub type Params = crate::schemas::SessionUndoParams;
        pub type Result = crate::schemas::SessionUndoResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_usage {
        pub const NAME: &str = "session.usage";
        pub type Params = crate::schemas::SessionUsageParams;
        pub type Result = crate::schemas::SessionUsageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_workspace_move {
        pub const NAME: &str = "session.workspace.move";
        pub type Params = crate::schemas::SessionWorkspaceMoveParams;
        pub type Result = crate::schemas::SessionWorkspaceMoveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod setup_runtime_check {
        pub const NAME: &str = "setup.runtime_check";
        pub type Params = crate::schemas::SetupRuntimeCheckParams;
        pub type Result = crate::schemas::SetupRuntimeCheckResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod setup_status {
        pub const NAME: &str = "setup.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::SetupStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_desktop_daily {
        pub const NAME: &str = "shared_metrics.desktop_daily";
        pub type Params = crate::schemas::SharedMetricsDesktopDailyParams;
        pub type Result = crate::schemas::SharedMetricsDesktopDailyResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_desktop_dislike {
        pub const NAME: &str = "shared_metrics.desktop_dislike";
        pub type Params = crate::schemas::SharedMetricsDesktopDislikeParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_desktop_feature_use {
        pub const NAME: &str = "shared_metrics.desktop_feature_use";
        pub type Params = crate::schemas::SharedMetricsDesktopFeatureUseParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_desktop_friction {
        pub const NAME: &str = "shared_metrics.desktop_friction";
        pub type Params = crate::schemas::SharedMetricsDesktopFrictionParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_desktop_onboarding {
        pub const NAME: &str = "shared_metrics.desktop_onboarding";
        pub type Params = crate::schemas::SharedMetricsDesktopOnboardingParams;
        pub type Result = crate::schemas::OkResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_set {
        pub const NAME: &str = "shared_metrics.set";
        pub type Params = crate::schemas::SharedMetricsSetParams;
        pub type Result = crate::schemas::SharedMetricsConsentResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_slash_command {
        pub const NAME: &str = "shared_metrics.slash_command";
        pub type Params = crate::schemas::SharedMetricsSlashCommandParams;
        pub type Result = crate::schemas::SharedMetricsSlashCommandResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_startup_latency {
        pub const NAME: &str = "shared_metrics.startup_latency";
        pub type Params = crate::schemas::SharedMetricsStartupLatencyParams;
        pub type Result = crate::schemas::SharedMetricsStartupLatencyResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_status {
        pub const NAME: &str = "shared_metrics.status";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::SharedMetricsConsentResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shared_metrics_update_run {
        pub const NAME: &str = "shared_metrics.update_run";
        pub type Params = crate::schemas::SharedMetricsUpdateRunParams;
        pub type Result = crate::schemas::SharedMetricsUpdateRunResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shell_exec {
        pub const NAME: &str = "shell.exec";
        pub type Params = crate::schemas::ShellExecParams;
        pub type Result = crate::schemas::ShellExecResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod skills_manage {
        pub const NAME: &str = "skills.manage";
        pub type Params = crate::schemas::SkillsManageParams;
        pub type Result = crate::schemas::SkillsManageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod skills_reload {
        pub const NAME: &str = "skills.reload";
        pub type Params = crate::schemas::SkillsReloadParams;
        pub type Result = crate::schemas::SkillsReloadResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod slash_exec {
        pub const NAME: &str = "slash.exec";
        pub type Params = crate::schemas::SlashExecParams;
        pub type Result = crate::schemas::SlashExecResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod spawn_tree_list {
        pub const NAME: &str = "spawn_tree.list";
        pub type Params = crate::schemas::SpawnTreeListParams;
        pub type Result = crate::schemas::SpawnTreeListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod spawn_tree_load {
        pub const NAME: &str = "spawn_tree.load";
        pub type Params = crate::schemas::SpawnTreeLoadParams;
        pub type Result = crate::schemas::SpawnTreeLoadResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod spawn_tree_save {
        pub const NAME: &str = "spawn_tree.save";
        pub type Params = crate::schemas::SpawnTreeSaveParams;
        pub type Result = crate::schemas::SpawnTreeSaveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subagent_interrupt {
        pub const NAME: &str = "subagent.interrupt";
        pub type Params = crate::schemas::SubagentIdParams;
        pub type Result = crate::schemas::SubagentInterruptResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subagent_list {
        pub const NAME: &str = "subagent.list";
        pub type Params = crate::schemas::SessionParams;
        pub type Result = crate::schemas::SubagentListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subagent_steer {
        pub const NAME: &str = "subagent.steer";
        pub type Params = crate::schemas::SubagentSteerParams;
        pub type Result = crate::schemas::SubagentSteerResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subagent_tail {
        pub const NAME: &str = "subagent.tail";
        pub type Params = crate::schemas::SubagentIdParams;
        pub type Result = crate::schemas::SubagentTailResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subscription_change {
        pub const NAME: &str = "subscription.change";
        pub type Params = crate::schemas::SubscriptionChangeParams;
        pub type Result = crate::schemas::BillingPendingChangeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subscription_preview {
        pub const NAME: &str = "subscription.preview";
        pub type Params = crate::schemas::SubscriptionPreviewParams;
        pub type Result = crate::schemas::SubscriptionPreviewResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subscription_resume {
        pub const NAME: &str = "subscription.resume";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::BillingPendingChangeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subscription_state {
        pub const NAME: &str = "subscription.state";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::SubscriptionStateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod subscription_upgrade {
        pub const NAME: &str = "subscription.upgrade";
        pub type Params = crate::schemas::SubscriptionUpgradeParams;
        pub type Result = crate::schemas::SubscriptionUpgradeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod system_battery {
        pub const NAME: &str = "system.battery";
        pub type Params = crate::schemas::SystemBatteryParams;
        pub type Result = crate::schemas::SystemBatteryResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod terminal_resize {
        pub const NAME: &str = "terminal.resize";
        pub type Params = crate::schemas::TerminalResizeParams;
        pub type Result = crate::schemas::TerminalResizeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod tools_configure {
        pub const NAME: &str = "tools.configure";
        pub type Params = crate::schemas::ToolsConfigureParams;
        pub type Result = crate::schemas::ToolsConfigureResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod tools_list {
        pub const NAME: &str = "tools.list";
        pub type Params = crate::schemas::SessionScoped;
        pub type Result = crate::schemas::ToolsetsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod tools_show {
        pub const NAME: &str = "tools.show";
        pub type Params = crate::schemas::SessionScoped;
        pub type Result = crate::schemas::ToolsShowResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod toolsets_list {
        pub const NAME: &str = "toolsets.list";
        pub type Params = crate::schemas::SessionScoped;
        pub type Result = crate::schemas::ToolsetsListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod usage_bars {
        pub const NAME: &str = "usage.bars";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::UsageModel;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_add {
        pub const NAME: &str = "vault.add";
        pub type Params = crate::schemas::VaultAddParams;
        pub type Result = crate::schemas::VaultAddResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_list {
        pub const NAME: &str = "vault.list";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::VaultListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_lock {
        pub const NAME: &str = "vault.lock";
        pub type Params = crate::schemas::VaultLockParams;
        pub type Result = crate::schemas::VaultLockResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_remove {
        pub const NAME: &str = "vault.remove";
        pub type Params = crate::schemas::VaultRemoveParams;
        pub type Result = crate::schemas::VaultRemoveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_source_set {
        pub const NAME: &str = "vault.source.set";
        pub type Params = crate::schemas::VaultSourceSetParams;
        pub type Result = crate::schemas::VaultSourceSetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_sources {
        pub const NAME: &str = "vault.sources";
        pub type Params = crate::schemas::ProfileParams;
        pub type Result = crate::schemas::VaultSourcesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod vault_unlock {
        pub const NAME: &str = "vault.unlock";
        pub type Params = crate::schemas::VaultUnlockParams;
        pub type Result = crate::schemas::VaultUnlockResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod verification_status {
        pub const NAME: &str = "verification.status";
        pub type Params = crate::schemas::VerificationStatusParams;
        pub type Result = crate::schemas::VerificationStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod voice_record {
        pub const NAME: &str = "voice.record";
        pub type Params = crate::schemas::VoiceRecordParams;
        pub type Result = crate::schemas::VoiceRecordResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod voice_toggle {
        pub const NAME: &str = "voice.toggle";
        pub type Params = crate::schemas::VoiceToggleParams;
        pub type Result = crate::schemas::VoiceToggleResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod voice_tts {
        pub const NAME: &str = "voice.tts";
        pub type Params = crate::schemas::VoiceTtsParams;
        pub type Result = crate::schemas::VoiceTtsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_feed {
        pub const NAME: &str = "wake.feed";
        pub type Params = crate::schemas::WakeFeedParams;
        pub type Result = crate::schemas::WakeFeedResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_pause {
        pub const NAME: &str = "wake.pause";
        pub type Params = crate::schemas::WakeControlParams;
        pub type Result = crate::schemas::WakePauseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_resume {
        pub const NAME: &str = "wake.resume";
        pub type Params = crate::schemas::WakeControlParams;
        pub type Result = crate::schemas::WakeResumeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_start {
        pub const NAME: &str = "wake.start";
        pub type Params = crate::schemas::WakeStartParams;
        pub type Result = crate::schemas::WakeStartResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_status {
        pub const NAME: &str = "wake.status";
        pub type Params = crate::schemas::WakeStatusParams;
        pub type Result = crate::schemas::WakeStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod wake_stop {
        pub const NAME: &str = "wake.stop";
        pub type Params = crate::schemas::WakeStopParams;
        pub type Result = crate::schemas::WakeStopResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
}
pub mod canonical {
    pub mod a2a_forward {
        pub const NAME: &str = "a2a.forward";
        pub type Params = crate::schemas::A2aForwardParams;
        pub type Result = crate::schemas::A2aForwardResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod approval_respond {
        pub const NAME: &str = "approval.respond";
        pub type Params = crate::schemas::CanonicalApprovalParams;
        pub type Result = crate::schemas::PromptResponseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_deliver {
        pub const NAME: &str = "bot_relay.deliver";
        pub type Params = crate::schemas::CanonicalBotDeliverParams;
        pub type Result = crate::schemas::CanonicalBotDeliverResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod bot_relay_settle {
        pub const NAME: &str = "bot_relay.settle";
        pub type Params = crate::schemas::CanonicalBotDeliverParams;
        pub type Result = crate::schemas::CanonicalBotDeliverResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod clarify_lock {
        pub const NAME: &str = "clarify.lock";
        pub type Params = crate::schemas::ClarifyLockParams;
        pub type Result = crate::schemas::ClarifyLockResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod clarify_respond {
        pub const NAME: &str = "clarify.respond";
        pub type Params = crate::schemas::ClarifyRespondParams;
        pub type Result = crate::schemas::PromptResponseResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod command_observe {
        pub const NAME: &str = "command.observe";
        pub type Params = crate::schemas::CommandObserveParams;
        pub type Result = crate::schemas::CommandObserveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod config_get {
        pub const NAME: &str = "config.get";
        pub type Params = crate::schemas::ConfigGetParams;
        pub type Result = crate::schemas::CanonicalConfigGetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod config_set {
        pub const NAME: &str = "config.set";
        pub type Params = crate::schemas::CanonicalConfigSetParams;
        pub type Result = crate::schemas::ConfigSetResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cron_cancel {
        pub const NAME: &str = "cron.cancel";
        pub type Params = crate::schemas::CronAdmissionParams;
        pub type Result = crate::schemas::CronCancelResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cron_recover {
        pub const NAME: &str = "cron.recover";
        pub type Params = crate::schemas::CronRunParams;
        pub type Result = crate::schemas::CronStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cron_status {
        pub const NAME: &str = "cron.status";
        pub type Params = crate::schemas::CronAdmissionParams;
        pub type Result = crate::schemas::CronStatusResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod cron_submit {
        pub const NAME: &str = "cron.submit";
        pub type Params = crate::schemas::CronRunParams;
        pub type Result = crate::schemas::CronSubmitResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_approve {
        pub const NAME: &str = "groups.approve";
        pub type Params = crate::schemas::CanonicalRoomApproveParams;
        pub type Result = crate::schemas::GroupsApproveResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_attachment_download {
        pub const NAME: &str = "groups.attachment.download";
        pub type Params = crate::schemas::GroupsAttachmentDownloadParams;
        pub type Result = crate::schemas::RoomAttachmentBytes;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_attachment_upload {
        pub const NAME: &str = "groups.attachment.upload";
        pub type Params = crate::schemas::GroupsAttachmentUploadParams;
        pub type Result = crate::schemas::RoomAttachment;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_create {
        pub const NAME: &str = "groups.create";
        pub type Params = crate::schemas::CanonicalRoomCreateParams;
        pub type Result = crate::schemas::GroupsCreateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_discard {
        pub const NAME: &str = "groups.discard";
        pub type Params = crate::schemas::GroupsDiscardParams;
        pub type Result = crate::schemas::GroupsDiscardResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod groups_retry {
        pub const NAME: &str = "groups.retry";
        pub type Params = crate::schemas::CanonicalRoomAttemptParams;
        pub type Result = crate::schemas::GroupsRetryResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod image_attach_bytes {
        pub const NAME: &str = "image.attach_bytes";
        pub type Params = crate::schemas::ImageAttachBytesParams;
        pub type Result = crate::schemas::CanonicalAttachedImageResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod kanban_run {
        pub const NAME: &str = "kanban.run";
        pub type Params = crate::schemas::KanbanRunParams;
        pub type Result = crate::schemas::KanbanRunResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod profiles_list {
        pub const NAME: &str = "profiles.list";
        pub type Params = crate::schemas::CanonicalProfilesParams;
        pub type Result = crate::schemas::CanonicalProfilesResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_cancel {
        pub const NAME: &str = "prompt.cancel";
        pub type Params = crate::schemas::PromptCancelParams;
        pub type Result = crate::schemas::AdmissionStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_receipt {
        pub const NAME: &str = "prompt.receipt";
        pub type Params = crate::schemas::PromptReceiptParams;
        pub type Result = crate::schemas::PromptReceiptResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_replace_queued {
        pub const NAME: &str = "prompt.replace_queued";
        pub type Params = crate::schemas::CanonicalReplaceQueuedParams;
        pub type Result = crate::schemas::AdmissionStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_resolve_unknown {
        pub const NAME: &str = "prompt.resolve_unknown";
        pub type Params = crate::schemas::PromptResolveUnknownParams;
        pub type Result = crate::schemas::AdmissionStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_steer_queued {
        pub const NAME: &str = "prompt.steer_queued";
        pub type Params = crate::schemas::CanonicalSteerQueuedParams;
        pub type Result = crate::schemas::AdmissionStatus;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod prompt_submit {
        pub const NAME: &str = "prompt.submit";
        pub type Params = crate::schemas::CanonicalSubmitParams;
        pub type Result = crate::schemas::CanonicalSubmitResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod rollback_session_diff {
        pub const NAME: &str = "rollback.session_diff";
        pub type Params = crate::schemas::RollbackSessionDiffParams;
        pub type Result = crate::schemas::RollbackSessionDiffResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod runtime_describe {
        pub const NAME: &str = "runtime.describe";
        pub type Params = crate::schemas::RuntimeDescribeParams;
        pub type Result = crate::schemas::RuntimeDescribeResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_automation {
        pub const NAME: &str = "session.automation";
        pub type Params = crate::schemas::SessionAutomationParams;
        pub type Result = crate::schemas::SessionAutomationResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_create {
        pub const NAME: &str = "session.create";
        pub type Params = crate::schemas::CanonicalCreateParams;
        pub type Result = crate::schemas::CanonicalSnapshot;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_detach {
        pub const NAME: &str = "session.detach";
        pub type Params = crate::schemas::SessionDetachParams;
        pub type Result = crate::schemas::SessionDetachResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_events_since {
        pub const NAME: &str = "session.events.since";
        pub type Params = crate::schemas::CanonicalEventsParams;
        pub type Result = crate::schemas::CanonicalEventsResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_export {
        pub const NAME: &str = "session.export";
        pub type Params = crate::schemas::SessionExportParams;
        pub type Result = crate::schemas::SessionExportResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_info {
        pub const NAME: &str = "session.info";
        pub type Params = crate::schemas::CanonicalSessionInfoParams;
        pub type Result = crate::schemas::CanonicalSessionInfo;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_interrupt {
        pub const NAME: &str = "session.interrupt";
        pub type Params = crate::schemas::CanonicalControlParams;
        pub type Result = crate::schemas::CanonicalSessionHandle;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_list {
        pub const NAME: &str = "session.list";
        pub type Params = crate::schemas::CanonicalListParams;
        pub type Result = crate::schemas::CanonicalListResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_mutate {
        pub const NAME: &str = "session.mutate";
        pub type Params = crate::schemas::SessionMutateParams;
        pub type Result = crate::schemas::SessionMutateResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_prune {
        pub const NAME: &str = "session.prune";
        pub type Params = crate::schemas::SessionPruneParams;
        pub type Result = crate::schemas::SessionPruneResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_redirect {
        pub const NAME: &str = "session.redirect";
        pub type Params = crate::schemas::CanonicalCorrectionParams;
        pub type Result = crate::schemas::CanonicalCorrectionResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_resume {
        pub const NAME: &str = "session.resume";
        pub type Params = crate::schemas::CanonicalResumeParams;
        pub type Result = crate::schemas::CanonicalSnapshot;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_stats {
        pub const NAME: &str = "session.stats";
        pub type Params = crate::schemas::SessionStatsParams;
        pub type Result = crate::schemas::SessionStats;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod session_steer {
        pub const NAME: &str = "session.steer";
        pub type Params = crate::schemas::CanonicalCorrectionParams;
        pub type Result = crate::schemas::CanonicalCorrectionResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod shell_exec {
        pub const NAME: &str = "shell.exec";
        pub type Params = crate::schemas::CanonicalShellExecParams;
        pub type Result = crate::schemas::ShellExecResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod worker_adopt {
        pub const NAME: &str = "worker.adopt";
        pub type Params = crate::schemas::WorkerScopeParams;
        pub type Result = crate::schemas::WorkerExecution;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod worker_persist {
        pub const NAME: &str = "worker.persist";
        pub type Params = crate::schemas::WorkerPersistParams;
        pub type Result = crate::schemas::WorkerPersistResult;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
    pub mod worker_register {
        pub const NAME: &str = "worker.register";
        pub type Params = crate::schemas::WorkerRegisterParams;
        pub type Result = crate::schemas::WorkerExecution;
        pub struct Marker;
        impl crate::method::Method for Marker {
            const NAME: &'static str = NAME;
            type Params = Params;
            type Result = Result;
        }
    }
}
