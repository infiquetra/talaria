use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Proposal(Proposal),
    Need(Need),
    Answer(String),
    Cannot(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub summary: String,
    pub profile: Option<String>,
    pub changes: Vec<Change>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Need {
    pub sections: Vec<String>,
    pub why: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyError {
    NoBlock,
    SeveralBlocks(usize),
    TooLarge(usize),
    Json(String),
    NotAnObject,
    UnknownKind(String),
    MissingField(&'static str),
    TooManyChanges(usize),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Set {
        key: String,
        value: Value,
        why: Option<String>,
    },
    McpAdd {
        name: String,
        source: McpSource,
        why: Option<String>,
    },
    McpEnable {
        name: String,
        enabled: bool,
        why: Option<String>,
    },
    McpRemove {
        name: String,
        why: Option<String>,
    },
    Toggle {
        kind: CapKind,
        name: String,
        enabled: bool,
        why: Option<String>,
    },
    CronAdd {
        name: String,
        schedule: String,
        prompt: String,
        repeat: Option<u32>,
        deliver: Option<String>,
        why: Option<String>,
    },
    CronPause {
        job_id: String,
        why: Option<String>,
    },
    CronResume {
        job_id: String,
        why: Option<String>,
    },
    CronRemove {
        job_id: String,
        why: Option<String>,
    },
    MainModel {
        provider: String,
        model: String,
        why: Option<String>,
    },
    Open {
        screen: Screen,
        target: Option<String>,
        why: Option<String>,
    },
    Malformed {
        op: String,
        field: &'static str,
    },
    Unknown {
        op: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum McpSource {
    Preset(String),
    Command { command: String, args: Vec<String> },
    Url { url: String, oauth: bool },
    Forbidden(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapKind {
    Skill,
    Toolset,
    Plugin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Providers,
    Mcp,
    Capabilities,
    Cron,
    Profiles,
    Connectors,
    Settings,
}

pub fn parse_reply(_text: &str) -> Result<Reply, ReplyError> {
    todo!("configure stub")
}
pub fn prose_without_block(text: &str) -> String {
    let _ = text;
    todo!("configure stub")
}
