//! Closed value sets for `/config` keys, built into Talaria.
//!
//! The gateway's schema types most settings as plain strings and carries select lists only for a
//! few keys, some of them wrong. This table adds the sets Hermes accepts, with a one-line
//! description of each value, and corrects the schema lists that name values the runtime does not
//! read. The live schema stays the base: a key without an entry keeps the schema's editor.
//!
//! Every list here is proved against the pinned Hermes source by
//! `tools/contract-gen/config-choices.json`, which `contract_gen.py config-choices` writes and a
//! parity test compares with this table.
use serde_json::Value;

/// One key's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChoiceSpec {
    pub key: &'static str,
    pub pick: Pick,
    /// In display order.
    pub items: &'static [ChoiceValue],
    /// The hint on a last "Type a value…" row, when Hermes also takes values outside the list.
    pub custom: Option<&'static str>,
    /// For a [`Pick::Mixed`] key, what an own entry is, finishing "Enter …" when one is refused.
    pub ask: &'static str,
    pub schema: SchemaList,
    pub save: SaveAs,
    /// How Hermes compares a stored word with the listed values before it reads it.
    pub fold: Fold,
    /// A list that differs per install or profile, fetched when the chooser opens and merged
    /// after the built-in rows.
    pub runtime: Option<RuntimeList>,
}

/// A list of values that differs per install or profile, read from the gateway while settings are
/// open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeList {
    Skins,
    Personalities,
    TerminalBackends,
    DashboardThemes,
    ContextEngines,
    /// The profiles the gateway serves.
    Profiles,
    /// The toolsets the profile offers.
    Toolsets,
    /// The skills installed for the profile.
    Skills,
}

/// One value a fetched list offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeChoice {
    pub value: String,
    pub description: String,
}

/// How many values the editor picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    One,
    /// Any number of the listed values, ticked one by one.
    Many {
        /// The order of the picks is meaningful, so a new pick goes last and the order is kept.
        ordered: bool,
        /// A first row that clears the picks, named for what an empty list means.
        empty_label: Option<&'static str>,
        store: ManyStore,
    },
    /// One of the listed words, or an entry of the user's own: the last row opens an editor.
    Mixed {
        own_label: &'static str,
        own: OwnKind,
    },
}

/// How the picks of a [`Pick::Many`] key are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManyStore {
    /// A list of strings.
    List,
    /// One string with the picks separated by a comma, because Hermes types the key as a string.
    CommaString,
}

/// What a [`Pick::Mixed`] key takes beside its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnKind {
    /// A comma separated list, saved as a list of strings.
    List,
    /// A whole number, saved as a number.
    Number,
}

/// What happens to the options the gateway's schema sends for the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaList {
    /// Schema options the table lacks are added after the table's rows.
    Merge,
    /// The schema's list names values the runtime does not read; only the table's rows show.
    Replace,
}

/// How a chosen value is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAs {
    /// As the text of the value.
    Text,
    /// A key whose default is null: blank saves null, `true` and `false` save booleans.
    Json,
    /// A key that holds a list: one word saves as a list of that word and blank as an empty list.
    List,
}

/// How Hermes reads a stored word before comparing it with the values it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Exactly as stored.
    Exact,
    /// Trimmed and lowercased.
    Word,
    /// Trimmed, lowercased, underscores as hyphens, and a regional tag such as `de-be` read as
    /// its base language when no alias names it.
    Language,
}

/// One row of a key's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChoiceValue {
    /// The exact spelling saved.
    pub value: &'static str,
    /// Shown instead of the value; used for the blank value.
    pub label: Option<&'static str>,
    pub description: &'static str,
    /// Other spellings the runtime reads as this value; a stored alias selects this row.
    pub aliases: &'static [&'static str],
}

impl ChoiceSpec {
    /// The value written when `text` is chosen or typed.
    #[must_use]
    pub fn saved(&self, text: &str) -> Value {
        match (self.save, text) {
            (SaveAs::Json, "") => Value::Null,
            (SaveAs::Json, "true") => Value::Bool(true),
            (SaveAs::Json, "false") => Value::Bool(false),
            (SaveAs::List, "") => Value::Array(Vec::new()),
            (SaveAs::List, word) => Value::Array(vec![Value::String(word.into())]),
            _ => Value::String(text.into()),
        }
    }

    const fn custom(self, hint: &'static str) -> Self {
        Self {
            custom: Some(hint),
            ..self
        }
    }

    const fn replace(self) -> Self {
        Self {
            schema: SchemaList::Replace,
            ..self
        }
    }

    const fn json(self) -> Self {
        Self {
            save: SaveAs::Json,
            ..self
        }
    }

    const fn as_list(self) -> Self {
        Self {
            save: SaveAs::List,
            ..self
        }
    }

    const fn fold(self, fold: Fold) -> Self {
        Self { fold, ..self }
    }

    const fn runtime(self, list: RuntimeList) -> Self {
        Self {
            runtime: Some(list),
            ..self
        }
    }
}

impl ChoiceValue {
    const fn label(self, label: &'static str) -> Self {
        Self {
            label: Some(label),
            ..self
        }
    }

    const fn aliases(self, aliases: &'static [&'static str]) -> Self {
        Self { aliases, ..self }
    }
}

const fn one(key: &'static str, items: &'static [ChoiceValue]) -> ChoiceSpec {
    ChoiceSpec {
        key,
        pick: Pick::One,
        items,
        custom: None,
        ask: "",
        schema: SchemaList::Merge,
        save: SaveAs::Text,
        fold: Fold::Exact,
        runtime: None,
    }
}

const fn many(
    key: &'static str,
    ordered: bool,
    empty_label: Option<&'static str>,
    store: ManyStore,
    items: &'static [ChoiceValue],
) -> ChoiceSpec {
    ChoiceSpec {
        pick: Pick::Many {
            ordered,
            empty_label,
            store,
        },
        ..one(key, items)
    }
}

/// A word list that is also saved as JSON, with an own entry of the given kind on the last row.
const fn mixed(
    key: &'static str,
    own_label: &'static str,
    own: OwnKind,
    hint: &'static str,
    ask: &'static str,
    items: &'static [ChoiceValue],
) -> ChoiceSpec {
    ChoiceSpec {
        pick: Pick::Mixed { own_label, own },
        custom: Some(hint),
        ask,
        save: SaveAs::Json,
        ..one(key, items)
    }
}

const fn v(value: &'static str, description: &'static str) -> ChoiceValue {
    ChoiceValue {
        value,
        label: None,
        description,
        aliases: &[],
    }
}

/// The spec for `key`, when Talaria knows its values.
#[must_use]
pub fn choice_spec(key: &str) -> Option<&'static ChoiceSpec> {
    SPECS.iter().find(|spec| spec.key == key)
}

/// Every spec, sorted by key.
#[must_use]
pub fn choice_specs() -> &'static [ChoiceSpec] {
    SPECS
}

/// The range, step and unit of a number, as far as the Hermes source states them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberSpec {
    pub key: &'static str,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Both bounds are left out: the value lies strictly between them.
    pub exclusive: bool,
    pub step: f64,
    pub unit: &'static str,
    /// What a blank value means, for a key where blank saves null.
    pub blank: Option<&'static str>,
}

impl NumberSpec {
    /// Whether the value is a whole number.
    #[must_use]
    pub fn whole(&self) -> bool {
        self.step.fract() == 0.0
    }

    /// Whether the source states a bound.
    #[must_use]
    pub fn ranged(&self) -> bool {
        self.min.is_some() || self.max.is_some()
    }

    #[must_use]
    pub fn allows(&self, number: f64) -> bool {
        let (low, high) = if self.exclusive {
            (
                self.min.is_none_or(|min| number > min),
                self.max.is_none_or(|max| number < max),
            )
        } else {
            (
                self.min.is_none_or(|min| number >= min),
                self.max.is_none_or(|max| number <= max),
            )
        };
        low && high
    }

    /// A bound or step as the user types it: `1` for a whole number, `0.05` otherwise.
    #[must_use]
    pub fn text(&self, number: f64) -> String {
        if self.whole() {
            format!("{}", number.round() as i64)
        } else {
            format!("{:?}", (number * 1e6).round() / 1e6)
        }
    }

    /// What to say when a value is refused, ending in how to leave the key unset when it may be.
    #[must_use]
    pub fn refusal(&self) -> String {
        let ask = match (self.min, self.max) {
            (Some(min), Some(max)) if self.exclusive => format!(
                "a number above {} and below {}",
                self.text(min),
                self.text(max)
            ),
            (Some(min), Some(max)) => {
                format!("a number from {} to {}", self.text(min), self.text(max))
            }
            (Some(min), None) => format!("a number of {} or more", self.text(min)),
            (None, Some(max)) => format!("a number of {} or less", self.text(max)),
            (None, None) if self.whole() => "a whole number".to_owned(),
            (None, None) => "a number".to_owned(),
        };
        if self.blank.is_some() {
            format!("Enter {ask}, or leave it blank.")
        } else {
            format!("Enter {ask}.")
        }
    }

    /// What to say when the text is not a number at all: a whole number key asks for one, even
    /// where a range follows.
    #[must_use]
    pub fn unreadable(&self) -> String {
        if !self.whole() {
            self.refusal()
        } else if self.blank.is_some() {
            "Enter a whole number, or leave it blank.".to_owned()
        } else {
            "Enter a whole number.".to_owned()
        }
    }

    /// The line shown above the number editor.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        match (self.min, self.max) {
            (Some(min), Some(max)) if self.exclusive => parts.push(format!(
                "range above {} and below {}",
                self.text(min),
                self.text(max)
            )),
            (Some(min), Some(max)) => {
                parts.push(format!("range {} to {}", self.text(min), self.text(max)));
            }
            (Some(min), None) => parts.push(format!("at least {}", self.text(min))),
            (None, Some(max)) => parts.push(format!("at most {}", self.text(max))),
            (None, None) => {}
        }
        if self.ranged() {
            parts.push(format!("step {}", self.text(self.step)));
            parts.push(self.unit.to_owned());
        } else {
            let kind = if self.whole() {
                "whole number"
            } else {
                "number"
            };
            parts.push(format!("{kind} of {}", self.unit));
        }
        if let Some(blank) = self.blank {
            parts.push(format!("blank = {blank}"));
        }
        parts.join(" · ")
    }

    /// The text of the number one step above or below `text`, kept inside the range. Blank or
    /// unreadable text counts as zero. None when the source states no range to step within.
    #[must_use]
    pub fn nudge(&self, text: &str, up: bool) -> Option<String> {
        if !self.ranged() {
            return None;
        }
        let low = self
            .min
            .map(|min| if self.exclusive { min + self.step } else { min });
        let high = self
            .max
            .map(|max| if self.exclusive { max - self.step } else { max });
        let base = text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .unwrap_or(0.0);
        let mut next = if up {
            base + self.step
        } else {
            base - self.step
        };
        if let Some(low) = low {
            next = next.max(low);
        }
        if let Some(high) = high {
            next = next.min(high);
        }
        Some(self.text(next))
    }
}

/// The number spec for `key`, when Hermes states a range or a blank meaning for it.
#[must_use]
pub fn number_spec(key: &str) -> Option<&'static NumberSpec> {
    NUMBERS.iter().find(|spec| spec.key == key)
}

/// Every number spec, sorted by key.
#[must_use]
pub fn number_specs() -> &'static [NumberSpec] {
    NUMBERS
}

const fn num(key: &'static str, step: f64, unit: &'static str) -> NumberSpec {
    NumberSpec {
        key,
        min: None,
        max: None,
        exclusive: false,
        step,
        unit,
        blank: None,
    }
}

impl NumberSpec {
    const fn range(self, min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..self
        }
    }

    const fn at_least(self, min: f64) -> Self {
        Self {
            min: Some(min),
            ..self
        }
    }

    const fn between(self, min: f64, max: f64) -> Self {
        Self {
            exclusive: true,
            ..self.range(min, max)
        }
    }

    const fn blank(self, meaning: &'static str) -> Self {
        Self {
            blank: Some(meaning),
            ..self
        }
    }
}

static NUMBERS: &[NumberSpec] = &[
    num("agent.budget_warning_ratio", 0.05, "ratio")
        .between(0.0, 1.0)
        .blank("off"),
    num("agent.max_turns", 1.0, "turns")
        .at_least(1.0)
        .blank("no limit"),
    num("agent.run_budget_seconds", 1.0, "seconds").blank("off"),
    num("approvals.discord_prompt_timeout", 1.0, "seconds").at_least(30.0),
    num(
        "compression.codex_responses_compact_threshold",
        1.0,
        "tokens",
    )
    .blank("follow the local trigger"),
    num("compression.max_attempts", 1.0, "rounds").range(1.0, 10.0),
    num("compression.threshold", 0.05, "ratio of the context window").range(0.0, 1.0),
    num("compression.threshold_tokens", 1.0, "tokens").blank("off"),
    num("context_file_max_chars", 1.0, "characters").blank("scale with the model window"),
    num("cron.max_parallel_jobs", 1.0, "jobs").blank("no limit"),
    num("database.journal_size_limit", 1.0, "bytes").blank("no limit"),
    num("database.wal_autocheckpoint", 1.0, "pages").blank("SQLite default"),
    num("display.pet.scale", 0.05, "scale").range(0.1, 3.0),
    num("kanban.max_in_progress", 1.0, "tasks").blank("derive from memory"),
    num("kanban.max_in_progress_per_profile", 1.0, "workers").blank("no cap"),
    num("max_concurrent_sessions", 1.0, "sessions")
        .at_least(0.0)
        .blank("no limit"),
    num("openrouter.min_coding_score", 0.05, "score").range(0.0, 1.0),
    num("openrouter.response_cache_ttl", 1.0, "seconds").range(1.0, 86400.0),
    num("plugins.hook_callback_timeout", 1.0, "seconds").range(0.0, 600.0),
    num("plugins.load_timeout_seconds", 1.0, "seconds").range(0.0, 600.0),
    num("tools.tool_search.listing_max_tokens", 100.0, "tokens").range(200.0, 60000.0),
    num("tools.tool_search.max_search_limit", 1.0, "hits").range(1.0, 50.0),
    num("tools.tool_search.search_default_limit", 1.0, "hits").at_least(1.0),
    num("tools.tool_search.threshold_pct", 1.0, "percent").range(0.0, 100.0),
    num("vision.embed_target_bytes", 1024.0, "bytes").range(65536.0, 4194304.0),
    num("vision.max_calls_per_image", 1.0, "calls").blank("3 in subagents, no limit otherwise"),
    num("voice.beep_volume", 0.05, "amplitude").range(0.0, 1.0),
    num("wake_word.sensitivity", 0.05, "threshold").range(0.0, 1.0),
];

static SPECS: &[ChoiceSpec] = &[
    mixed(
        "agent.agent_cache.memory_high_mb",
        "Number of MB…",
        OwnKind::Number,
        "A whole number of megabytes; 0 turns the pass off.",
        "a whole number of MB, or choose auto or off",
        &[
            v(
                "auto",
                "Derive the budget from the memory limit or total RAM.",
            ),
            v("off", "Never shed cached sessions for memory."),
        ],
    ),
    one(
        "agent.coding_context",
        &[
            v(
                "auto",
                "Coding brief only in interactive sessions in a code workspace.",
            ),
            v(
                "focus",
                "Like auto, plus a lean coding tool set and fewer skill details.",
            ),
            v("on", "Coding brief everywhere."),
            v("off", "No coding brief."),
        ],
    ),
    many(
        "agent.disabled_toolsets",
        false,
        Some("None disabled"),
        ManyStore::List,
        &[],
    )
    .custom("Toolset names, separated by commas; Hermes skips a name it does not know.")
    .runtime(RuntimeList::Toolsets),
    mixed(
        "agent.execution_guidance",
        "Own list…",
        OwnKind::List,
        "Model names to match, separated by commas, for example gpt, codex.",
        "model names separated by commas, for example gpt, codex",
        &[
            v(
                "auto",
                "For gpt, codex, grok, deepseek, kimi, qwen, glm, minimax, mimo and mistral models.",
            ),
            v("true", "For every model."),
            v("false", "Never."),
        ],
    ),
    one(
        "agent.image_input_mode",
        &[
            v(
                "auto",
                "Native images when the model supports vision, else a text description.",
            ),
            v("native", "Always attach images to the model."),
            v("text", "Always describe images with vision_analyze first."),
        ],
    ),
    mixed(
        "agent.intent_ack_continuation",
        "Own list…",
        OwnKind::List,
        "Model names to match, separated by commas, for example gpt, codex.",
        "model names separated by commas, for example gpt, codex",
        &[
            v("auto", "Only for the codex_responses API mode."),
            v("true", "For every API mode."),
            v("false", "Never."),
        ],
    ),
    one(
        "agent.service_tier",
        &[
            v("", "Normal speed (fast mode off).")
                .label("Normal")
                .aliases(&["normal", "default", "standard", "off", "none"]),
            v("fast", "Fast mode on every request.").aliases(&["priority", "on"]),
            v(
                "auto",
                "Fast mode for the first fast_auto_seconds of every turn.",
            ),
            v("cold", "Fast mode for the first turn of a session only."),
            v(
                "ultrafast",
                "OpenAI ultrafast tier; only some models accept it.",
            )
            .aliases(&["ultra"]),
        ],
    )
    .fold(Fold::Word),
    one(
        "agent.text_verbosity",
        &[
            v("", "Do not send a verbosity.").label("Not sent"),
            v("low", "Short answers."),
            v("medium", "Medium-length answers."),
            v("high", "Long answers."),
        ],
    ),
    mixed(
        "agent.tool_use_enforcement",
        "Own list…",
        OwnKind::List,
        "Model names to match, separated by commas, for example gpt, codex.",
        "model names separated by commas, for example gpt, codex",
        &[
            v("auto", "Only for GPT and Codex models."),
            v("true", "For every model."),
            v("false", "Never."),
        ],
    ),
    mixed(
        "approvals.approvers",
        "Own list…",
        OwnKind::List,
        "User ids allowed to answer, separated by commas.",
        "user ids separated by commas",
        &[
            v("*", "Any admitted member."),
            v("", "Only the requester and platform admins.").label("Requester and admins only"),
        ],
    )
    .as_list(),
    one(
        "approvals.cron_mode",
        &[
            v(
                "deny",
                "Block the command at once so the agent finds another way.",
            ),
            v("approve", "Approve every dangerous command in cron jobs."),
        ],
    ),
    one(
        "approvals.mode",
        &[
            v("manual", "Always ask before a dangerous command runs."),
            v(
                "smart",
                "An auxiliary model approves low-risk commands; the rest still ask.",
            ),
            v("off", "Never ask (the same as --yolo)."),
        ],
    ),
    one(
        "approvals.single_query_mode",
        &[
            v("deny", "Block the command at once."),
            v("approve", "Approve every dangerous command in -q sessions."),
        ],
    ),
    one(
        "approvals.unattended_mode",
        &[
            v("deny", "Block the command at once."),
            v(
                "approve",
                "Approve every dangerous command on unattended platforms.",
            ),
        ],
    ),
    one(
        "auth.codex_login_flow",
        &[
            v("device_code", "Open a URL and enter a code."),
            v("browser", "Authorization code with PKCE on localhost:1455."),
        ],
    ),
    one(
        "bedrock.guardrail.stream_processing_mode",
        &[
            v("async", "Check streamed output asynchronously."),
            v("sync", "Check each streamed chunk synchronously."),
        ],
    ),
    one(
        "bedrock.guardrail.trace",
        &[
            v("disabled", "No guardrail trace."),
            v("enabled", "Include the guardrail trace."),
            v("enabled_full", "Include the full guardrail trace."),
        ],
    ),
    one(
        "bot_desktop.placement",
        &[
            v("auto", "Follow the terminal backend."),
            v("terminal", "Always inside the terminal backend."),
            v(
                "gateway",
                "Always on the gateway host, outside any sandbox.",
            ),
        ],
    ),
    one(
        "browser.dialog_policy",
        &[
            v("must_respond", "The agent must answer each page dialog."),
            v("auto_dismiss", "Dismiss page dialogs automatically."),
            v("auto_accept", "Accept page dialogs automatically."),
        ],
    ),
    one(
        "browser.engine",
        &[
            v("auto", "Chromium.").aliases(&["chrome"]),
            v(
                "lightpanda",
                "Lightpanda, one per session: faster, no screenshots.",
            ),
        ],
    )
    .fold(Fold::Word),
    one(
        "code_execution.mode",
        &[
            v(
                "project",
                "Run in the session folder with the project's Python.",
            ),
            v(
                "strict",
                "Run in an isolated temp folder with Hermes' own Python.",
            ),
        ],
    ),
    one(
        "compression.codex_app_server_auto",
        &[
            v("native", "Codex decides when to compact."),
            v("hermes", "Hermes' threshold triggers compaction."),
            v("off", "Never compact automatically."),
        ],
    ),
    one(
        "compression.tail_mode",
        &[
            v("lean", "Small token-bounded tail plus digests."),
            v("legacy", "Large verbatim tail (0.20 x threshold)."),
        ],
    ),
    one(
        "computer_use.capture_after_mode",
        &[
            v("som", "Screenshot with overlays."),
            v("ax", "Accessibility elements only, no image; faster."),
            v("vision", "Pixels only."),
        ],
    ),
    one(
        "computer_use.no_overlay",
        &[
            v(
                "",
                "Automatic: off on macOS and headless Linux, on elsewhere.",
            )
            .label("Automatic"),
            v("true", "Always disable the overlay."),
            v("false", "Always show the overlay."),
        ],
    )
    .json(),
    one(
        "computer_use.permission_mode",
        &[
            v("standard", "Use cua-driver's own approval boundary."),
            v(
                "bounded",
                "No prompts; anything outside capability_manifest fails closed.",
            ),
        ],
    ),
    one(
        "context.engine",
        &[v("compressor", "Built-in lossy summarization.")],
    )
    .custom("The name of an installed context-engine plugin.")
    .replace()
    .runtime(RuntimeList::ContextEngines),
    one(
        "dashboard.theme",
        &[
            v("default", "Hermes Teal: classic dark teal."),
            v(
                "default-large",
                "Hermes Teal with bigger fonts and roomier spacing.",
            ),
            v(
                "nous-blue",
                "Light mode: vivid Nous-blue accents on a cream canvas.",
            ),
            v("midnight", "Deep blue-violet with cool accents."),
            v("ember", "Warm crimson and bronze."),
            v("mono", "Clean grayscale."),
            v("cyberpunk", "Neon green on black."),
            v("rose", "Soft pink and warm ivory."),
        ],
    )
    .runtime(RuntimeList::DashboardThemes),
    one(
        "database.journal_mode",
        &[
            v("wal", "Write-ahead log."),
            v(
                "delete",
                "Rollback journal, for shared or weak-fsync file systems (NFS, SMB, virtiofs).",
            ),
        ],
    ),
    one(
        "delegation.gateway_mode",
        &[
            v("inline", "Subagents join the turn that started them."),
            v(
                "background",
                "Subagents run in the background and report back.",
            ),
            v(
                "auto",
                "Inline where interim messages are off, background elsewhere.",
            ),
        ],
    ),
    one(
        "desktop.disable_gpu",
        &[
            v("auto", "Disable the GPU only on remote displays."),
            v("true", "Always software rendering.").aliases(&["1", "yes", "on"]),
            v("false", "Always keep the GPU on.").aliases(&["0", "no", "off"]),
        ],
    )
    .fold(Fold::Word),
    one(
        "desktop.ozone_platform_hint",
        &[
            v("auto", "Chromium default."),
            v("x11", "XWayland."),
            v("wayland", "Native Wayland."),
        ],
    ),
    one(
        "desktop.password_store",
        &[
            v("auto", "Detect the keychain."),
            v("gnome-libsecret", "GNOME libsecret."),
            v("kwallet", "KWallet."),
            v("kwallet5", "KWallet 5."),
            v("kwallet6", "KWallet 6."),
            v("basic", "Unencrypted."),
        ],
    ),
    many(
        "discord.server_actions",
        false,
        Some("All actions"),
        ManyStore::CommaString,
        &[
            v("list_guilds", "List the servers the bot is in."),
            v("server_info", "Read a server's details."),
            v("list_channels", "List a server's channels."),
            v("channel_info", "Read a channel's details."),
            v("list_roles", "List a server's roles."),
            v("member_info", "Read a member's details."),
            v("search_members", "Search a server's members."),
            v("fetch_messages", "Read messages in a channel."),
            v("list_pins", "List a channel's pinned messages."),
            v("pin_message", "Pin a message."),
            v("unpin_message", "Unpin a message."),
            v("create_thread", "Start a thread."),
            v("add_role", "Give a member a role."),
            v("remove_role", "Take a role from a member."),
        ],
    ),
    one(
        "display.background_process_notifications",
        &[
            v("concise", "One line; failures add an output tail."),
            v("all", "Running updates and the final output."),
            v("result", "Final output only."),
            v("error", "Output only on a non-zero exit."),
            v("off", "No notices."),
        ],
    ),
    one(
        "display.busy_input_mode",
        &[
            v(
                "interrupt",
                "Enter while a turn runs stops it and sends the new message.",
            ),
            v(
                "queue",
                "Enter while a turn runs waits and sends after the turn ends.",
            ),
            v(
                "steer",
                "Enter while a turn runs adds the message to the running turn.",
            ),
        ],
    ),
    one(
        "display.cli_layout",
        &[
            v("modern", "Glyph rows with the status under the composer."),
            v("classic", "Boxed panels."),
        ],
    ),
    one(
        "display.copy_shortcut",
        &[
            v("auto", "Platform default."),
            v("ctrl_c", "Ctrl+C copies."),
            v("ctrl_shift_c", "Ctrl+Shift+C copies."),
            v("disabled", "No copy shortcut."),
        ],
    ),
    one(
        "display.final_response_markdown",
        &[
            v("strip", "Remove Markdown from the final reply."),
            v("render", "Render Markdown."),
            v("raw", "Show Markdown as written."),
        ],
    ),
    one(
        "display.focus_saved_tool_progress",
        &[
            v("all", "Show every tool call."),
            v("new", "Show new tool calls only."),
            v("verbose", "Show tool calls with detail."),
            v("off", "Hide tool progress."),
        ],
    ),
    one(
        "display.inline_images",
        &[
            v("auto", "Detect the terminal's image support."),
            v("kitty", "Kitty graphics."),
            v("iterm", "iTerm2 images."),
            v("unicode", "Unicode half-blocks."),
            v("off", "No inline images."),
        ],
    ),
    one(
        "display.interface",
        &[
            v("cli", "Hermes command line (prompt_toolkit)."),
            v("tui", "Ink terminal interface."),
        ],
    ),
    one(
        "display.language",
        &[
            v("en", "English.").aliases(&["english", "en-us", "en-gb"]),
            v("af", "Afrikaans.").aliases(&["afrikaans", "af-za"]),
            v("ar", "Arabic.").aliases(&[
                "arabic",
                "العربية",
                "ar-sa",
                "ar-eg",
                "ar-ae",
                "ar-ma",
                "ar-dz",
            ]),
            v("de", "German.").aliases(&["german", "deutsch", "de-de", "de-at", "de-ch"]),
            v("es", "Spanish.")
                .aliases(&["spanish", "español", "espanol", "es-es", "es-mx", "es-ar"]),
            v("fr", "French.").aliases(&[
                "french",
                "français",
                "france",
                "fr-fr",
                "fr-be",
                "fr-ca",
                "fr-ch",
            ]),
            v("ga", "Irish.").aliases(&["irish", "gaeilge", "ga-ie"]),
            v("hu", "Hungarian.").aliases(&["hungarian", "magyar", "hu-hu"]),
            v("it", "Italian.").aliases(&["italian", "italiano", "it-it", "it-ch"]),
            v("ja", "Japanese.").aliases(&["japanese", "jp", "ja-jp"]),
            v("ko", "Korean.").aliases(&["korean", "한국어", "ko-kr"]),
            v("pt", "Portuguese.").aliases(&[
                "portuguese",
                "português",
                "portugues",
                "pt-pt",
                "pt-br",
                "brazilian",
                "brasileiro",
            ]),
            v("ru", "Russian.").aliases(&["russian", "русский", "ru-ru"]),
            v("tr", "Turkish.").aliases(&["turkish", "türkçe", "tr-tr"]),
            v("uk", "Ukrainian.").aliases(&[
                "ukrainian",
                "ukrainisch",
                "українська",
                "uk-ua",
                "ua",
            ]),
            v("zh", "Chinese, simplified.")
                .aliases(&["chinese", "mandarin", "zh-cn", "zh-hans", "zh-sg"]),
            v("zh-hant", "Chinese, traditional.").aliases(&[
                "traditional-chinese",
                "traditional_chinese",
                "zh-tw",
                "zh-hk",
                "zh-mo",
            ]),
        ],
    )
    .custom("The code of a language from an installed language pack.")
    .fold(Fold::Language),
    one(
        "display.memory_notifications",
        &[
            v("on", "A generic 'Memory updated' notice."),
            v("off", "No notice; review still runs."),
            v("verbose", "Notice with a content preview."),
        ],
    ),
    one(
        "display.personality",
        &[
            v("", "No personality overlay on the system prompt.")
                .label("No personality")
                .aliases(&["none", "default", "neutral"]),
            v("helpful", "A helpful, friendly assistant."),
            v("concise", "Brief and to the point."),
            v("technical", "Detailed, accurate technical information."),
            v("creative", "Thinks outside the box and offers new ideas."),
            v("teacher", "Explains concepts clearly with examples."),
            v("kawaii", "Cute expressions and sparkles."),
            v("catgirl", "A playful anime catgirl, nya."),
            v("pirate", "Speaks like a tech-savvy buccaneer."),
            v("shakespeare", "Flowery Shakespearean prose."),
            v("surfer", "A laid-back surfer."),
            v("noir", "A hard-boiled detective narrator."),
            v("uwu", "Baby talk with uwu and OwO."),
            v(
                "philosopher",
                "Looks for the deeper meaning in every question.",
            ),
            v("hype", "Loud, pumped-up enthusiasm."),
        ],
    )
    .custom("The name of a personality defined in the profile's config.")
    .fold(Fold::Word)
    .runtime(RuntimeList::Personalities),
    one(
        "display.pet.render_mode",
        &[
            v("auto", "Detect kitty, iTerm2 or sixel, else Unicode."),
            v("kitty", "Kitty graphics."),
            v("iterm", "iTerm2 images."),
            v("sixel", "Sixel."),
            v("unicode", "Unicode half-blocks."),
            v("off", "Do not draw the pet."),
        ],
    ),
    one(
        "display.reasoning_style",
        &[
            v("code", "Fenced block."),
            v("blockquote", "Quoted lines."),
            v("subtext", "Discord small grey text."),
        ],
    ),
    one(
        "display.resume_display",
        &[
            v(
                "full",
                "Show the recap of recent exchanges when a session resumes.",
            ),
            v(
                "minimal",
                "Show only a short notice when a session resumes.",
            ),
            v(
                "off",
                "Offered by the dashboard; the CLI reader treats it like full.",
            ),
        ],
    ),
    many(
        "display.runtime_footer.fields",
        true,
        None,
        ManyStore::List,
        &[
            v("model", "Model name."),
            v("context_pct", "Context use in percent."),
            v("cwd", "Working folder."),
            v("latency", "Latency."),
            v("served_model", "Model the routing proxy served."),
        ],
    ),
    one(
        "display.skin",
        &[
            v("default", "Classic Hermes: gold and kawaii."),
            v("ares", "War-god theme: crimson and bronze."),
            v("mono", "Monochrome: clean grayscale."),
            v("slate", "Cool blue, developer-focused."),
            v(
                "daylight",
                "Light theme for bright terminals: dark text, cool blue accents.",
            ),
            v(
                "warm-lightmode",
                "Warm light mode: dark brown and gold text for light backgrounds.",
            ),
            v("poseidon", "Ocean-god theme: deep blue and seafoam."),
            v("sisyphus", "Sisyphean theme: austere grayscale."),
            v("charizard", "Volcanic theme: burnt orange and ember."),
        ],
    )
    .custom("The name of a skin file in the profile's skins folder.")
    .runtime(RuntimeList::Skins),
    many(
        "display.status_bar.fields",
        false,
        Some("Default set (empty list)"),
        ManyStore::List,
        &[
            v("model", "Model name."),
            v("context_detail", "Context use with token counts."),
            v("context_pct", "Context use in percent."),
            v("cache_hit", "Prompt cache hit rate."),
            v("latency", "Latency."),
            v("tps", "Tokens per second."),
            v("compressions", "How often the context was compressed."),
            v("bg_tasks", "Background tasks."),
            v("bg_processes", "Background processes."),
            v("bg_subagents", "Background subagents."),
            v("goal", "The current goal."),
            v("git_branch", "The current git branch."),
            v("duration", "Session duration."),
            v("prompt_elapsed", "Time spent on the current prompt."),
            v("idle_since", "Time since the last final reply."),
            v("focus", "Focus mode."),
            v("yolo", "Whether approvals are off."),
            v("stash", "Parked prompt drafts."),
            v("battery", "Battery level."),
            v("title", "Session title."),
            v("total_tokens", "Tokens used in the session."),
        ],
    ),
    one(
        "display.tool_progress_grouping",
        &[
            v("accumulate", "Edit one message as tools run."),
            v("separate", "One message per tool."),
        ],
    ),
    one(
        "display.tui_status_indicator",
        &[
            v("kaomoji", "Kaomoji faces."),
            v("emoji", "Emoji."),
            v("unicode", "Braille spinner."),
            v("ascii", "Plain ASCII."),
        ],
    ),
    one(
        "gateway.service_install_choice",
        &[
            v("", "Not decided.").label("Not decided"),
            v("install", "Install the gateway service."),
            v("decline", "Do not install it."),
        ],
    )
    .json(),
    one(
        "human_delay.mode",
        &[
            v("off", "No delay before replies."),
            v(
                "natural",
                "A random 800 to 2500 ms pause before each reply.",
            ),
            v(
                "custom",
                "A random pause between human_delay.min_ms and human_delay.max_ms.",
            ),
        ],
    )
    .replace(),
    one(
        "kanban.default_assignee",
        &[v("", "Use the default profile.").label("Default profile")],
    )
    .custom("A profile name.")
    .runtime(RuntimeList::Profiles),
    one(
        "kanban.orchestrator_profile",
        &[v("", "Use the default profile.").label("Default profile")],
    )
    .custom("A profile name.")
    .runtime(RuntimeList::Profiles),
    one(
        "local_runtime.backend",
        &[
            v("auto", "CUDA, Metal or Vulkan when present, else CPU."),
            v("cuda", "NVIDIA CUDA."),
            v("metal", "Apple Metal."),
            v("vulkan", "Vulkan."),
            v("hip", "AMD HIP."),
            v("cpu", "CPU only."),
        ],
    ),
    one(
        "logging.level",
        &[
            v("DEBUG", "Everything, including debug detail."),
            v("INFO", "Normal operation messages."),
            v("WARNING", "Only warnings and errors."),
            v("ERROR", "Only errors."),
        ],
    ),
    one(
        "lsp.install_strategy",
        &[
            v("auto", "Install missing servers on first use."),
            v("manual", "Only servers already on PATH."),
            v("off", "Same as manual."),
        ],
    ),
    one(
        "lsp.package_manager",
        &[
            v("npm", "Install language servers with npm."),
            v("pnpm", "Install language servers with pnpm."),
            v("yarn", "Install language servers with yarn."),
        ],
    ),
    one(
        "lsp.wait_mode",
        &[
            v("document", "Wait for the current file's diagnostics."),
            v("full", "Also wait for workspace-wide diagnostics."),
        ],
    ),
    one(
        "memory.provider",
        &[v("", "Built-in memory only.").label("Built-in only")],
    ),
    one(
        "nous.anthropic_wire",
        &[
            v("chat", "Chat completions wire."),
            v("native", "Anthropic Messages wire."),
            v(
                "auto",
                "Start on chat; switch per session where native is known clean.",
            ),
        ],
    ),
    one(
        "onboarding.profile_build",
        &[
            v("ask", "Offer to build a profile on the first message."),
            v("off", "Plain introduction only."),
        ],
    ),
    one(
        "onboarding.run",
        &[
            v("", "Fresh install.").label("Not set"),
            v("true", "Run onboarding on next launch."),
            v("false", "Onboarding done."),
        ],
    )
    .json(),
    one(
        "plugins.isolation",
        &[
            v("in_process", "Import plugins into Hermes."),
            v("host", "Run plugins in a separate host process."),
        ],
    ),
    one(
        "prompt_caching.cache_ttl",
        &[
            v("5m", "Five-minute cache."),
            v("1h", "One-hour cache."),
            v(
                "auto",
                "1h for human-paced sessions, 5m for subagents and jobs.",
            ),
            v("off", "Disable prompt caching."),
        ],
    ),
    one(
        "proxy.credential_source",
        &[
            v("env", "Read upstream secrets from the process environment."),
            v(
                "bitwarden",
                "Refetch secrets with bws on each proxy restart; needs secrets.bitwarden.enabled.",
            ),
        ],
    ),
    one(
        "security.approval.transport_fallback",
        &[
            v("deny", "Deny when the approval transport fails."),
            v("builtin", "Fall back to the built-in prompt."),
        ],
    ),
    one(
        "sessions.fts_optimize_notice",
        &[
            v("advise", "Print a one-line notice during updates."),
            v("require", "Same notice worded as required."),
            v("off", "No notice."),
        ],
    ),
    many(
        "skills.auto_load",
        false,
        Some("No skills"),
        ManyStore::List,
        &[],
    )
    .custom("Skill names, separated by commas; Hermes skips a name it cannot find.")
    .runtime(RuntimeList::Skills),
    one(
        "streaming.transport",
        &[
            v("auto", "Native drafts where supported, edits elsewhere."),
            v("draft", "Drafts with edit fallback."),
            v("edit", "Progressive message edits only."),
            v("off", "No streaming."),
        ],
    ),
    one(
        "stt.elevenlabs.model_id",
        &[v("scribe_v2", "Scribe v2."), v("scribe_v1", "Scribe v1.")],
    ),
    one(
        "stt.groq.model",
        &[
            v("whisper-large-v3-turbo", "Groq Whisper large v3 turbo."),
            v("whisper-large-v3", "Groq Whisper large v3."),
        ],
    )
    .custom("Any Groq transcription model id."),
    one(
        "stt.local.model",
        &[
            v("base", "faster-whisper base model."),
            v("tiny", "Smallest, fastest."),
            v("small", "Small model."),
            v("medium", "Medium model."),
            v("large-v3", "Largest, most accurate."),
            v("turbo", "Turbo version of large-v3."),
        ],
    ),
    one(
        "stt.mistral.model",
        &[
            v("voxtral-mini-latest", "Voxtral mini, latest."),
            v("voxtral-mini-2602", "Voxtral mini 2602."),
        ],
    )
    .custom("Any Mistral transcription model id."),
    one(
        "stt.openai.model",
        &[
            v("whisper-1", "Whisper v1."),
            v("gpt-4o-mini-transcribe", "GPT-4o mini transcribe."),
            v("gpt-4o-transcribe", "GPT-4o transcribe."),
            v("gpt-transcribe", "GPT transcribe."),
        ],
    )
    .custom("Any OpenAI transcription model id."),
    one(
        "stt.openai.streaming_model",
        &[
            v("gpt-live-transcribe", "Live transcription model."),
            v("gpt-transcribe", "GPT transcribe."),
            v("gpt-4o-transcribe", "GPT-4o transcribe."),
            v("gpt-4o-mini-transcribe", "GPT-4o mini transcribe."),
        ],
    )
    .custom("Any OpenAI live transcription model id."),
    one(
        "stt.xai.model",
        &[
            v(
                "",
                "Built-in model: STT_XAI_MODEL or grok-voice-transcribe-2.0.",
            )
            .label("Built-in model"),
            v("grok-voice-transcribe-2.0", "Grok voice transcribe 2.0."),
            v("grok-voice-transcribe-1.0", "Grok voice transcribe 1.0."),
        ],
    )
    .custom("Any xAI transcription model id."),
    one(
        "terminal.backend",
        &[
            v("local", "Run directly on this machine."),
            v("docker", "Isolated Docker or Podman container."),
            v("modal", "Modal serverless cloud sandbox."),
            v("ssh", "Run on a remote machine over SSH."),
            v(
                "daytona",
                "Daytona persistent cloud development environment.",
            ),
            v("vercel_sandbox", "Vercel Sandbox cloud microVM."),
        ],
    )
    .runtime(RuntimeList::TerminalBackends),
    one(
        "terminal.degraded_mode",
        &[
            v("warn", "Return a degraded tool result with a retry hint."),
            v("fail", "Raise an error with a traceback."),
        ],
    ),
    one(
        "terminal.home_mode",
        &[
            v(
                "auto",
                "Host keeps the real HOME; containers use the profile home.",
            ),
            v("real", "Always the real HOME."),
            v("profile", "Always the profile home."),
        ],
    ),
    one(
        "terminal.modal_mode",
        &[
            v(
                "auto",
                "Use the Nous managed gateway when available, else your own Modal account.",
            ),
            v(
                "managed",
                "Always use the Nous managed gateway; billed to the subscription.",
            ),
            v(
                "direct",
                "Always use your own Modal account (MODAL_TOKEN_ID).",
            ),
        ],
    )
    .replace(),
    one(
        "terminal.vercel_runtime",
        &[
            v("", "No legacy runtime; use terminal.vercel_image.").label("No legacy runtime"),
            v("node24", "Legacy Node 24 runtime (deprecated by Vercel)."),
            v("node22", "Legacy Node 22 runtime (deprecated by Vercel)."),
            v(
                "python3.13",
                "Legacy Python 3.13 runtime (deprecated by Vercel).",
            ),
        ],
    ),
    one(
        "timezone",
        &[v("", "Use the gateway host's own time zone.").label("System time zone")],
    ),
    one(
        "tools.tool_search.enabled",
        &[
            v("auto", "Turn the bridge on when a deferred tool exists."),
            v("on", "Same as auto today."),
            v("off", "No bridge."),
        ],
    ),
    one(
        "tools.tool_search.listing",
        &[
            v("auto", "Include the listing when it fits."),
            v("on", "Same rendering, explicit."),
            v("off", "Always the bare bridge."),
        ],
    ),
    many(
        "toolsets",
        false,
        None,
        ManyStore::List,
        &[v("hermes-cli", "The standard toolset of the Hermes CLI.")],
    )
    .custom("Toolset names, separated by commas; Hermes skips a name it does not know.")
    .runtime(RuntimeList::Toolsets),
    one(
        "tts.edge.voice",
        &[
            v("en-US-AriaNeural", "Edge voice, US English: Aria."),
            v("en-US-JennyNeural", "Edge voice, US English: Jenny."),
            v("en-US-AndrewNeural", "Edge voice, US English: Andrew."),
            v("en-US-BrianNeural", "Edge voice, US English: Brian."),
            v("en-US-GuyNeural", "Edge voice, US English: Guy."),
            v("en-GB-SoniaNeural", "Edge voice, British English: Sonia."),
        ],
    )
    .custom("Any Microsoft Edge voice name."),
    one(
        "tts.elevenlabs.model_id",
        &[v(
            "eleven_multilingual_v2",
            "ElevenLabs multilingual model, version 2.",
        )],
    )
    .custom("Any ElevenLabs model id."),
    one(
        "tts.gemini.voice",
        &[
            v("Zephyr", "Gemini prebuilt voice."),
            v("Puck", "Gemini prebuilt voice."),
            v("Charon", "Gemini prebuilt voice."),
            v("Kore", "Gemini prebuilt voice."),
            v("Fenrir", "Gemini prebuilt voice."),
            v("Leda", "Gemini prebuilt voice."),
            v("Orus", "Gemini prebuilt voice."),
            v("Aoede", "Gemini prebuilt voice."),
            v("Callirrhoe", "Gemini prebuilt voice."),
            v("Autonoe", "Gemini prebuilt voice."),
            v("Enceladus", "Gemini prebuilt voice."),
            v("Iapetus", "Gemini prebuilt voice."),
            v("Umbriel", "Gemini prebuilt voice."),
            v("Algieba", "Gemini prebuilt voice."),
            v("Despina", "Gemini prebuilt voice."),
            v("Erinome", "Gemini prebuilt voice."),
            v("Algenib", "Gemini prebuilt voice."),
            v("Rasalgethi", "Gemini prebuilt voice."),
            v("Laomedeia", "Gemini prebuilt voice."),
            v("Achernar", "Gemini prebuilt voice."),
            v("Alnilam", "Gemini prebuilt voice."),
            v("Schedar", "Gemini prebuilt voice."),
            v("Gacrux", "Gemini prebuilt voice."),
            v("Pulcherrima", "Gemini prebuilt voice."),
            v("Achird", "Gemini prebuilt voice."),
            v("Zubenelgenubi", "Gemini prebuilt voice."),
            v("Vindemiatrix", "Gemini prebuilt voice."),
            v("Sadachbia", "Gemini prebuilt voice."),
            v("Sadaltager", "Gemini prebuilt voice."),
            v("Sulafat", "Gemini prebuilt voice."),
        ],
    )
    .custom("Any Gemini voice name."),
    one(
        "tts.kittentts.model",
        &[
            v(
                "KittenML/kitten-tts-nano-0.8-int8",
                "KittenTTS nano, 25 MB.",
            ),
            v(
                "KittenML/kitten-tts-micro-0.8-int8",
                "KittenTTS micro, 41 MB.",
            ),
            v(
                "KittenML/kitten-tts-mini-0.8-int8",
                "KittenTTS mini, 80 MB.",
            ),
        ],
    )
    .custom("Any KittenTTS model repository on Hugging Face."),
    one(
        "tts.minimax.model",
        &[
            v("speech-02-hd", "MiniMax HD speech model."),
            v("speech-02-turbo", "MiniMax turbo speech model; faster."),
        ],
    )
    .custom("Any MiniMax speech model id."),
    one(
        "tts.neutts.device",
        &[
            v("cpu", "CPU."),
            v("cuda", "NVIDIA CUDA."),
            v("mps", "Apple Metal."),
        ],
    ),
    one(
        "tts.neutts.model",
        &[
            v(
                "neuphonic/neutts-air-q4-gguf",
                "NeuTTS Air, 4-bit GGUF; the smallest.",
            ),
            v("neuphonic/neutts-air-q8-gguf", "NeuTTS Air, 8-bit GGUF."),
            v("neuphonic/neutts-air", "NeuTTS Air, full precision."),
        ],
    )
    .custom("Any NeuTTS model repository on Hugging Face."),
    one(
        "tts.openai.model",
        &[
            v("gpt-4o-mini-tts", "GPT-4o mini speech model."),
            v("tts-1", "TTS 1."),
            v("tts-1-hd", "TTS 1 HD."),
        ],
    )
    .custom("Any OpenAI speech model id."),
    one(
        "tts.openai.voice",
        &[
            v("alloy", "OpenAI voice."),
            v("ash", "OpenAI voice."),
            v("ballad", "OpenAI voice."),
            v("cedar", "OpenAI voice."),
            v("coral", "OpenAI voice."),
            v("echo", "OpenAI voice."),
            v("fable", "OpenAI voice."),
            v("marin", "OpenAI voice."),
            v("nova", "OpenAI voice."),
            v("onyx", "OpenAI voice."),
            v("sage", "OpenAI voice."),
            v("shimmer", "OpenAI voice."),
            v("verse", "OpenAI voice."),
        ],
    )
    .custom("Any voice the OpenAI speech model accepts."),
    one(
        "tts.piper.voice",
        &[
            v(
                "en_US-lessac-medium",
                "Piper voice, US English: Lessac, medium quality.",
            ),
            v(
                "en_US-amy-medium",
                "Piper voice, US English: Amy, medium quality.",
            ),
            v(
                "en_US-ryan-high",
                "Piper voice, US English: Ryan, high quality.",
            ),
            v(
                "en_GB-alan-medium",
                "Piper voice, British English: Alan, medium quality.",
            ),
        ],
    )
    .custom("Any Piper voice name, or the path of a .onnx voice file."),
    one(
        "tts.provider",
        &[
            v("edge", "Microsoft Edge voices; free."),
            v("elevenlabs", "ElevenLabs; premium."),
            v("openai", "OpenAI speech."),
            v("xai", "xAI speech."),
            v("minimax", "MiniMax speech."),
            v("mistral", "Mistral Voxtral speech."),
            v("gemini", "Google Gemini speech."),
            v("deepinfra", "DeepInfra hosted speech models."),
            v("neutts", "NeuTTS; runs locally."),
            v("kittentts", "KittenTTS; runs locally."),
            v("piper", "Piper; runs locally."),
        ],
    )
    .custom("The name of a command provider defined under tts.providers."),
    one(
        "updates.non_interactive_local_changes",
        &[
            v(
                "stash",
                "Stash local source edits, update, then put them back.",
            ),
            v(
                "discard",
                "Stash local source edits and drop them after the update.",
            ),
        ],
    ),
    one(
        "updates.parked_branch_strategy",
        &[
            v(
                "switch",
                "Move to the update target; commits stay on the branch.",
            ),
            v(
                "update_in_place",
                "Merge the target into the branch after tagging it.",
            ),
        ],
    ),
    one(
        "updates.pre_update_backup",
        &[
            v("quick", "Snapshot small critical state."),
            v("full", "Quick plus a full backup zip.").aliases(&["true", "zip"]),
            v("off", "No backup.").aliases(&["false", "none", "disabled"]),
        ],
    )
    .fold(Fold::Word),
    one(
        "vault.key_storage",
        &[
            v("file", "A 0600 vault.key file."),
            v("keychain", "The OS keychain; file when none."),
        ],
    ),
    one(
        "voice.gpt_live.voice",
        &[
            v("marin", "GPT-Live voice."),
            v("cedar", "GPT-Live voice."),
            v("quartz", "GPT-Live voice."),
            v("ripple", "GPT-Live voice."),
            v("vesper", "GPT-Live voice."),
            v("willow", "GPT-Live voice."),
            v("stone", "GPT-Live voice."),
            v("gleam", "GPT-Live voice."),
            v("meridian", "GPT-Live voice."),
            v("bossa", "GPT-Live voice."),
            v("tempo", "GPT-Live voice."),
            v("beacon", "GPT-Live voice."),
            v("delta", "GPT-Live voice."),
            v("cinder", "GPT-Live voice."),
        ],
    )
    .custom("Any GPT-Live voice name."),
    one(
        "voice.submit_mode",
        &[
            v("direct", "Send the transcript at once."),
            v("draft", "Put the transcript in the composer to edit."),
        ],
    ),
    one(
        "voice.voice_chat_mode",
        &[
            v(
                "chained",
                "Speech to text, a Hermes turn, then text to speech.",
            ),
            v(
                "gpt-live",
                "One OpenAI GPT-Live voice model that delegates to Hermes.",
            ),
        ],
    ),
    one(
        "wake_word.capture",
        &[
            v("auto", "Choose automatically."),
            v("local", "This process opens the microphone."),
            v(
                "client",
                "The desktop streams the microphone through wake.feed.",
            ),
        ],
    ),
    one(
        "wake_word.provider",
        &[
            v("auto", "First engine this platform supports."),
            v("openwakeword", "openWakeWord engine."),
            v("sherpa", "Sherpa-onnx keyword spotting engine."),
            v("porcupine", "Porcupine engine; needs PORCUPINE_ACCESS_KEY."),
        ],
    ),
    one(
        "wake_word.surface",
        &[
            v("auto", "First surface that claims it."),
            v("cli", "Hermes CLI."),
            v("tui", "Hermes TUI."),
            v("gui", "Desktop app."),
        ],
    ),
    one(
        "web.search_backend",
        &[
            v("", "Use web.backend or the first available backend.").label("Automatic"),
            v("firecrawl", "Web search backend."),
            v("parallel", "Web search backend."),
            v("tavily", "Web search backend."),
            v("perplexity", "Web search backend."),
            v("exa", "Web search backend."),
            v("searxng", "Web search backend."),
            v("brave-free", "Web search backend."),
            v("ddgs", "Web search backend."),
        ],
    )
    .custom("The name of an installed web search plugin."),
    one(
        "x_search.reasoning_effort",
        &[
            v("", "Use the model's own default reasoning effort.").label("Model default"),
            v("low", "Low reasoning effort."),
            v("medium", "Medium reasoning effort."),
            v("high", "High reasoning effort."),
            v("xhigh", "Xhigh reasoning effort."),
        ],
    )
    .json(),
];
