use super::reply::{cut, sanitize_text};
use super::validate::lookup;
use super::{BRIEF_MAX_BYTES, CONTEXT_REPLY_MAX_BYTES, Context, Need, SecretDetector, Section};
use crate::settings::{contains_secret_key, env_reference_names, is_secret_key};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Brief {
    pub text: String,
    pub omitted_keys: usize,
    pub shortened_lists: Vec<&'static str>,
    pub withheld: usize,
}
const SKILL: &str = include_str!("skill.md");
struct Fields {
    detect: SecretDetector,
    withheld: usize,
}
impl Fields {
    fn text(&mut self, text: &str) -> String {
        if (self.detect)(text) {
            self.withheld += 1;
            super::WITHHELD.into()
        } else {
            sanitize_text(text, self.detect)
        }
    }
    fn value(&mut self, key: &str, value: Option<&Value>) -> String {
        if is_secret_key(key) || value.is_some_and(contains_secret_key) {
            return "<secret>".into();
        }
        value.map_or_else(|| "unset".into(), |v| self.text(&v.to_string()))
    }
}
struct List {
    name: &'static str,
    items: Vec<String>,
    kept: usize,
    unavailable: Option<String>,
    omitted: bool,
}
impl List {
    fn render(&self) -> String {
        if self.omitted {
            return format!("{}: omitted (ask with need {})", self.name, self.name);
        }
        if let Some(reason) = &self.unavailable {
            return format!("{}: unavailable ({reason})", self.name);
        }
        let mut line = format!(
            "{}: {}",
            self.name,
            if self.kept == 0 {
                "none".into()
            } else {
                self.items[..self.kept].join(", ")
            }
        );
        if self.kept < self.items.len() {
            line.push_str(&format!(" … {} more", self.items.len() - self.kept));
        }
        line
    }
    fn len(&self) -> usize {
        if self.omitted {
            return self.name.len() * 2 + ": omitted (ask with need )".len();
        }
        if let Some(reason) = &self.unavailable {
            return self.name.len() + reason.len() + ": unavailable ()".len();
        }
        self.name.len()
            + 2
            + if self.kept == 0 {
                4
            } else {
                self.items[..self.kept]
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    + 2 * (self.kept - 1)
            }
            + if self.kept < self.items.len() {
                format!(" … {} more", self.items.len() - self.kept).len()
            } else {
                0
            }
    }
}
fn list<T>(
    name: &'static str,
    section: &Section<T>,
    fields: &mut Fields,
    render: impl Fn(&T, &mut Fields) -> String,
) -> List {
    match section {
        Section::Unavailable(reason) => List {
            name,
            items: vec![],
            kept: 0,
            unavailable: Some(fields.text(reason)),
            omitted: false,
        },
        Section::Available(items) => {
            let items: Vec<_> = items
                .iter()
                .map(|item| cut(&render(item, fields), 200))
                .collect();
            let kept = items.len();
            List {
                name,
                items,
                kept,
                unavailable: None,
                omitted: false,
            }
        }
    }
}
fn inventories(ctx: &Context, fields: &mut Fields, detail: bool) -> Vec<List> {
    let i = &ctx.inventory;
    vec![
        list("mcp_servers", &i.mcp_servers, fields, |r, f| {
            format!("{} ({})", f.text(&r.name), on_off(r.enabled))
        }),
        list("mcp_catalogue", &i.mcp_catalogue, fields, |r, f| {
            if detail {
                format!(
                    "{}: {}; requires: {}",
                    f.text(&r.name),
                    cut(&f.text(&r.description), 120),
                    r.requires
                        .iter()
                        .map(|v| f.text(v))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                format!("{} ({})", f.text(&r.name), on_off(r.enabled))
            }
        }),
        list("skills", &i.skills, fields, |r, f| {
            format!(
                "{} ({}){}",
                f.text(&r.name),
                on_off(r.enabled),
                if detail {
                    format!(": {}", cut(&f.text(&r.description), 120))
                } else {
                    String::new()
                }
            )
        }),
        list("toolsets", &i.toolsets, fields, |r, f| {
            format!(
                "{} ({}){}",
                f.text(&r.name),
                on_off(r.enabled),
                if detail {
                    format!(": {}", cut(&f.text(&r.description), 120))
                } else {
                    String::new()
                }
            )
        }),
        list("plugins", &i.plugins, fields, |r, f| {
            format!(
                "{} ({}){}",
                f.text(&r.name),
                on_off(r.enabled),
                if detail {
                    format!(": {}", cut(&f.text(&r.description), 120))
                } else {
                    String::new()
                }
            )
        }),
        list("cron_jobs", &i.cron_jobs, fields, |r, f| {
            format!(
                "{} {} \"{}\" {}",
                f.text(&r.job_id),
                f.text(&r.name),
                f.text(&r.schedule),
                f.text(&r.state)
            )
        }),
        list("providers", &i.providers, fields, |r, f| {
            format!(
                "{} ({})",
                f.text(&r.slug),
                if r.configured {
                    "configured"
                } else {
                    "not configured"
                }
            )
        }),
        list("main_model", &i.main_model, fields, |r, f| {
            format!("{}/{}", f.text(&r.provider), f.text(&r.model))
        }),
        list("profiles", &i.profiles, fields, |r, f| f.text(r)),
        list("connectors", &i.connectors, fields, |(name, enabled), f| {
            format!("{} ({})", f.text(name), on_off(*enabled))
        }),
    ]
}
fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}
fn flatten<'a>(
    value: &'a Value,
    prefix: &str,
    ctx: &Context,
    output: &mut BTreeMap<String, &'a Value>,
) {
    if !prefix.is_empty()
        && (ctx.schema.fields.iter().any(|f| f.key == prefix) || is_secret_key(prefix))
    {
        output.insert(prefix.into(), value);
        return;
    }
    if let Value::Object(map) = value {
        for (key, value) in map {
            let key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            flatten(value, &key, ctx, output);
        }
    } else if !prefix.is_empty() {
        output.insert(prefix.into(), value);
    }
}
fn raw_lines(ctx: &Context, fields: &mut Fields) -> Vec<String> {
    let mut flat = BTreeMap::new();
    flatten(&ctx.raw, "", ctx, &mut flat);
    let mut ordered = Vec::new();
    for field in &ctx.schema.fields {
        if let Some(value) = flat.remove(&field.key) {
            ordered.push((field.key.clone(), value));
        }
    }
    ordered.extend(flat);
    ordered
        .into_iter()
        .map(|(key, value)| {
            format!(
                "{} = {}",
                fields.text(&key),
                cut(&fields.value(&key, Some(value)), 200)
            )
        })
        .collect()
}
struct Parts {
    mandatory: String,
    lists: Vec<List>,
    raw: Vec<String>,
    raw_kept: usize,
    keys: Vec<(String, usize)>,
    keys_kept: usize,
}
impl Parts {
    fn omitted_keys(&self) -> usize {
        self.keys[self.keys_kept..]
            .iter()
            .map(|(_, count)| count)
            .sum()
    }
    fn raw_marker(&self) -> String {
        if self.raw_kept == self.raw.len() {
            String::new()
        } else {
            format!(
                "\n[{} more settings set; ask with need settings:<prefix>]",
                self.raw.len() - self.raw_kept
            )
        }
    }
    fn key_marker(&self) -> String {
        let n = self.omitted_keys();
        if n == 0 {
            String::new()
        } else {
            format!("\n[{n} setting keys omitted; ask with need settings:<prefix>]")
        }
    }
    fn len(&self) -> usize {
        self.mandatory.len()
            + "\n\nCurrent state:\n".len()
            + self.lists.iter().map(List::len).sum::<usize>()
            + self.lists.len().saturating_sub(1)
            + "\n\nSettings set in config.yaml:\n".len()
            + self.raw[..self.raw_kept]
                .iter()
                .map(String::len)
                .sum::<usize>()
            + self.raw_kept.saturating_sub(1)
            + self.raw_marker().len()
            + "\n\nSetting keys:\n".len()
            + self.keys[..self.keys_kept]
                .iter()
                .map(|(line, _)| line.len())
                .sum::<usize>()
            + self.keys_kept.saturating_sub(1)
            + self.key_marker().len()
    }
    fn render(&self) -> String {
        format!(
            "{}\n\nCurrent state:\n{}\n\nSettings set in config.yaml:\n{}{}\n\nSetting keys:\n{}{}",
            self.mandatory,
            self.lists
                .iter()
                .map(List::render)
                .collect::<Vec<_>>()
                .join("\n"),
            self.raw[..self.raw_kept].join("\n"),
            self.raw_marker(),
            self.keys[..self.keys_kept]
                .iter()
                .map(|(line, _)| line.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            self.key_marker()
        )
    }
}
pub fn build_brief(ctx: &Context, request: &str, detect: SecretDetector) -> Brief {
    let mut fields = Fields {
        detect,
        withheld: 0,
    };
    let mandatory = format!(
        "{}\n\nProfile: {}. Hermes {}. Talaria applies changes to this profile only.\n\nRequest: {}",
        SKILL.trim_end_matches('\n'),
        fields.text(&ctx.profile),
        fields.text(ctx.hermes_version.as_deref().unwrap_or("version unknown")),
        fields.text(request)
    );
    debug_assert!(SKILL.len() <= super::SKILL_MAX_BYTES);
    debug_assert!(mandatory.len() < BRIEF_MAX_BYTES);
    let lists = inventories(ctx, &mut fields, false);
    let raw = raw_lines(ctx, &mut fields);
    let keys: Vec<_> = ctx
        .schema
        .categories()
        .into_iter()
        .map(|category| {
            let keys: Vec<_> = ctx
                .schema
                .fields
                .iter()
                .filter(|f| f.category == category)
                .map(|f| fields.text(&f.key))
                .collect();
            (
                format!("{}: {}", fields.text(&category), keys.join(", ")),
                keys.len(),
            )
        })
        .collect();
    let mut parts = Parts {
        mandatory,
        raw_kept: raw.len(),
        keys_kept: keys.len(),
        lists,
        raw,
        keys,
    };
    while parts.len() > BRIEF_MAX_BYTES && parts.keys_kept > 0 {
        parts.keys_kept -= 1;
    }
    if parts.len() > BRIEF_MAX_BYTES {
        for list in &mut parts.lists {
            list.kept = list.kept.min(50);
        }
    }
    if parts.len() > BRIEF_MAX_BYTES {
        parts.raw_kept = parts.raw_kept.min(200);
    }
    while parts.len() > BRIEF_MAX_BYTES && parts.raw_kept > 0 {
        parts.raw_kept -= 1;
    }
    for index in (0..parts.lists.len()).rev() {
        while parts.len() > BRIEF_MAX_BYTES && parts.lists[index].kept > 0 {
            parts.lists[index].kept -= 1;
        }
    }
    for index in (0..parts.lists.len()).rev() {
        if parts.len() <= BRIEF_MAX_BYTES {
            break;
        }
        parts.lists[index].omitted = true;
    }
    let shortened_lists = parts
        .lists
        .iter()
        .filter(|list| list.omitted || list.kept < list.items.len())
        .map(|list| list.name)
        .collect();
    let text = parts.render();
    debug_assert!(text.len() <= BRIEF_MAX_BYTES);
    Brief {
        text,
        omitted_keys: parts.omitted_keys(),
        shortened_lists,
        withheld: fields.withheld,
    }
}
pub fn build_context_reply(ctx: &Context, need: &Need, detect: SecretDetector) -> String {
    let mut fields = Fields {
        detect,
        withheld: 0,
    };
    let mut lines = vec!["Talaria context (requested):".into()];
    let lists = inventories(ctx, &mut fields, true);
    for section in &need.sections {
        if let Some(prefix) = section.strip_prefix("settings:") {
            for field in ctx
                .schema
                .fields
                .iter()
                .filter(|f| f.key.starts_with(prefix))
            {
                let raw = lookup(&ctx.raw, &field.key);
                let current = raw
                    .filter(|v| !env_reference_names(v).is_empty())
                    .or_else(|| lookup(&ctx.effective, &field.key));
                let current = fields.value(&field.key, current);
                let default = fields.value(&field.key, lookup(&ctx.defaults, &field.key));
                let options = field
                    .options
                    .as_ref()
                    .map(|v| fields.value(&field.key, Some(&Value::Array(v.clone()))))
                    .unwrap_or_else(|| "none".into());
                lines.push(format!(
                    "{} | {} | {} | {} | {}",
                    fields.text(&field.key),
                    fields.text(field.kind.name()),
                    options,
                    default,
                    current
                ));
            }
        } else if let Some(list) = lists
            .iter()
            .find(|list| list.name == section || list.name == "cron_jobs" && section == "cron")
        {
            lines.push(list.render());
        } else {
            lines.push(format!("unknown section: {}", fields.text(section)));
        }
    }
    let ending = "Reply with one talaria-configure block.";
    let mut text = String::new();
    let mut cut = false;
    for line in lines {
        if text.len() + line.len() + 1 + "[cut]\n".len() + ending.len() > CONTEXT_REPLY_MAX_BYTES {
            cut = true;
            break;
        }
        text.push_str(&line);
        text.push('\n');
    }
    if cut {
        text.push_str("[cut]\n");
    }
    text.push_str(ending);
    text
}
pub fn build_refine_text(operator_text: &str) -> String {
    format!(
        "Refine the proposal:\n{}\n\nReply with one talaria-configure block.",
        sanitize_text(operator_text, |_| false)
    )
}
