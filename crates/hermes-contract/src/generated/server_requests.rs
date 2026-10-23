// Hermes v0.22.4 commit 4d04303818b61bf3aa09278af48356315515965b contract sha256 248ce12519dcc1e830a15c5819c93e6d27ef18404d19da38c960570710f83d00
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum ServerRequest {
    Approval(crate::schemas::ApprovalRequestParams),
    Clarify(crate::schemas::ClarifyRequestParams),
    DisplayInstallSudo(crate::schemas::DisplayInstallSudoParams),
    PreviewAct(crate::schemas::PreviewActRequestParams),
    PreviewRead(crate::schemas::ReadRangeRequestParams),
    Secret(crate::schemas::SecretRequestParams),
    SetupBeat(crate::schemas::SetupBeatRequestParams),
    Sudo(crate::schemas::SudoRequestParams),
    TerminalRead(crate::schemas::ReadRangeRequestParams),
    ThemeApply(crate::schemas::ThemeApplyRequestParams),
    ThemePick(crate::schemas::ThemePickRequestParams),
    Tour(crate::schemas::TourRequestParams),
    VaultCode(crate::schemas::VaultCodeRequestParams),
    VaultSaveLogin(crate::schemas::VaultSaveLoginRequestParams),
    VaultUnlockPrompt(crate::schemas::VaultUnlockRequestParams),
    WindowRead(crate::schemas::EmptyRequestParams),
    Unknown(serde_json::Value),
}
pub const REQUIRED_FIELDS: &[(&str, &[&str])] = &[
    ("approval", &["request_id", "session_id"]),
    ("clarify", &["questions", "session_id"]),
    ("display.install.sudo", &["profile_key", "session_id"]),
    ("preview.act", &["action", "session_id"]),
    ("preview.read", &["session_id"]),
    ("secret", &["env_var", "prompt", "session_id"]),
    ("setup_beat", &["beat", "card", "session_id"]),
    ("sudo", &["session_id"]),
    ("terminal.read", &["session_id"]),
    ("theme.apply", &["session_id"]),
    ("theme.pick", &["picker", "session_id"]),
    ("tour", &["action", "session_id"]),
    ("vault.code", &["session_id"]),
    ("vault.save_login", &["origin", "session_id", "site"]),
    (
        "vault.unlock_prompt",
        &["backend", "display_name", "session_id"],
    ),
    ("window.read", &["session_id"]),
];
pub mod approval {
    pub const NAME: &str = "approval";
    pub type Params = crate::schemas::ApprovalRequestParams;
    pub type Result = crate::schemas::ApprovalResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod clarify {
    pub const NAME: &str = "clarify";
    pub type Params = crate::schemas::ClarifyRequestParams;
    pub type Result = crate::schemas::ClarifyResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod display_install_sudo {
    pub const NAME: &str = "display.install.sudo";
    pub type Params = crate::schemas::DisplayInstallSudoParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod preview_act {
    pub const NAME: &str = "preview.act";
    pub type Params = crate::schemas::PreviewActRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod preview_read {
    pub const NAME: &str = "preview.read";
    pub type Params = crate::schemas::ReadRangeRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod secret {
    pub const NAME: &str = "secret";
    pub type Params = crate::schemas::SecretRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod setup_beat {
    pub const NAME: &str = "setup_beat";
    pub type Params = crate::schemas::SetupBeatRequestParams;
    pub type Result = crate::schemas::SetupBeatResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod sudo {
    pub const NAME: &str = "sudo";
    pub type Params = crate::schemas::SudoRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod terminal_read {
    pub const NAME: &str = "terminal.read";
    pub type Params = crate::schemas::ReadRangeRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod theme_apply {
    pub const NAME: &str = "theme.apply";
    pub type Params = crate::schemas::ThemeApplyRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod theme_pick {
    pub const NAME: &str = "theme.pick";
    pub type Params = crate::schemas::ThemePickRequestParams;
    pub type Result = crate::schemas::ThemePickResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod tour {
    pub const NAME: &str = "tour";
    pub type Params = crate::schemas::TourRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod vault_code {
    pub const NAME: &str = "vault.code";
    pub type Params = crate::schemas::VaultCodeRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod vault_save_login {
    pub const NAME: &str = "vault.save_login";
    pub type Params = crate::schemas::VaultSaveLoginRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod vault_unlock_prompt {
    pub const NAME: &str = "vault.unlock_prompt";
    pub type Params = crate::schemas::VaultUnlockRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
pub mod window_read {
    pub const NAME: &str = "window.read";
    pub type Params = crate::schemas::EmptyRequestParams;
    pub type Result = crate::schemas::ValueResult;
    pub struct Marker;
    impl crate::method::Method for Marker {
        const NAME: &'static str = NAME;
        type Params = Params;
        type Result = Result;
    }
}
