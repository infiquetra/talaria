//! The `/theme` and `/skin` selectors: what opening, moving, Enter and Esc do.
//!
//! The list itself is `talaria_core::commands::ChoiceList`. A theme preview changes only what the
//! screen resolves, in memory. Enter saves the choice for the current profile. Hermes has no call that shows a skin without making it the
//! profile's skin, so a skin preview writes it once the selection has rested, and Esc or quitting
//! writes the original back. Every write goes through the connection and session the selector
//! opened on, and a session or profile switch closes the selector and waits for the writes.
//!
//! Bare `/effort` opens the same slot with a scale instead of a list
//! (`talaria_core::models::EffortScale`). It writes nothing while it moves; Enter runs the typed
//! `/effort` and `/hivecode` commands for what was staged.

use std::time::Duration;

use hermes_client::Client;

use talaria_core::appearance::{AppearanceChange, AppearanceState, NotAppliedReason, Polarity};
use talaria_core::commands::{ChoiceEffect, ChoiceItem, ChoiceKey, ChoiceList, LocalAction};
use talaria_core::models::{Effort, EffortScale, ModelsEffect, ScaleEffect, ScaleFacts, ScaleKey};
use talaria_core::session::SessionId;

use super::{AppEffect, AppMessage, AppState, Executor, SessionService, notice_lines};
use crate::appearance::{self, SkinChoices, SkinPreviewError, ThemeFileSession, ThemePreview};
use crate::theme_file::{self, ThemeFile};

/// How long the skin selection rests before Talaria writes the preview.
const SKIN_REST: Duration = Duration::from_millis(300);
/// How long quitting waits for the original skin to be written back.
const RESTORE_WAIT: Duration = Duration::from_secs(2);
/// The skin selector's footer note.
const SKIN_NOTE: &str = "Previewing changes the Hermes profile's skin until you cancel.";
/// Enter on the `/effort` selector after the foreground session changed under it.
const EFFORT_SESSION_CHANGED: &str =
    "The session changed while the effort selector was open, so nothing was changed.";
/// Id prefixes of the `/theme` rows.
const FILE: &str = "file:";
const BACKGROUND: &str = "background:";

/// An open selector.
pub struct Selector {
    /// The rows, the filter and the selection. Empty and not drawn for the `/effort` scale.
    pub list: ChoiceList,
    kind: Kind,
}

enum Kind {
    Theme {
        /// Every theme the list offers, loaded, by identifier.
        files: Vec<(String, ThemeFile)>,
    },
    Skin {
        /// The profile's skin when the list opened.
        original: String,
        target: Target,
    },
    /// The `/effort` scale, for the session it opened on.
    Effort {
        scale: Box<EffortScale>,
        session_id: String,
    },
}

/// Where a `/skin` selector writes: the connection and session it opened on.
#[derive(Clone)]
struct Target {
    client: Option<Client>,
    session: Option<String>,
}

/// A skin preview that has written, or may write, the profile's skin.
pub struct SkinPreview {
    original: String,
    /// The skin Talaria last wrote.
    applied: String,
    /// The selection waiting to be written.
    wanted: String,
    /// Bumped by each move; only the newest rest writes.
    timer: u64,
    in_flight: bool,
    /// A refusal stopped previewing.
    stopped: bool,
    /// What to do once the write in flight answers.
    finish: Option<Finish>,
    /// The write in flight puts the original back.
    restoring: bool,
    target: Target,
    /// A session or profile switch waiting for the write in flight to answer.
    held: Vec<AppEffect>,
}

enum Finish {
    Restore,
    Apply(String),
}

/// Work a selector asks the executor for.
pub enum ChoiceWork {
    /// List the theme files off the loop.
    ListThemes,
    /// Print the `/skin` report, then ask Hermes for the skins and their descriptions.
    ListSkins,
    /// Wake after the skin selection has rested.
    Rest(u64),
    /// Write this skin and describe it.
    SetSkin(String),
}

/// A selector's work, finished.
pub enum ChoiceReply {
    Themes(Box<ThemeListing>),
    Skins {
        /// The `/skin` report: the skin state it produced when it changed one, and its lines.
        report: (Option<AppearanceState>, Vec<String>),
        result: Result<SkinChoices, String>,
        /// Why the current theme shows no skin, if it does not.
        ignored: Option<NotAppliedReason>,
        /// The profile and session the list was read for.
        owner: (String, Option<String>),
    },
    Rested(u64),
    SkinSet {
        name: String,
        skin: Option<AppearanceState>,
        result: Result<(), SkinPreviewError>,
    },
}

/// What the `/theme` selector lists, read off the loop.
pub struct ThemeListing {
    users: Vec<(String, Result<ThemeFile, String>)>,
    saved: Option<String>,
    configured: Option<String>,
}

/// The selector the view draws.
pub(super) fn list(state: &AppState) -> Option<&ChoiceList> {
    state
        .choice
        .as_ref()
        .filter(|selector| !matches!(selector.kind, Kind::Effort { .. }))
        .map(|selector| &selector.list)
}

/// The `/effort` scale the view draws.
pub(super) fn scale(state: &AppState) -> Option<&EffortScale> {
    match &state.choice.as_ref()?.kind {
        Kind::Effort { scale, .. } => Some(scale),
        _ => None,
    }
}

/// While the `/effort` scale is open, what can hold the keys above the composer: the
/// foreground session's open questions and approvals, its connection card and the masked
/// requests. `None` while the scale is closed, so nothing is read then.
pub(super) fn cards(state: &AppState) -> Option<Vec<String>> {
    scale(state)?;
    let opened = state.opened.as_ref();
    let prompts = opened
        .into_iter()
        .flat_map(|opened| opened.state.open_prompts())
        .map(|prompt| format!("{:?}", prompt.key));
    let connection = opened
        .and_then(|opened| opened.state.connection_card())
        .map(|card| format!("connection {}", card.op_id));
    let masked = state
        .masked
        .iter()
        .map(|request| format!("masked {}", request.token));
    Some(prompts.chain(connection).chain(masked).collect())
}

/// A question or approval card arrived while the `/effort` scale was open: the card needs the
/// keys and the rows, so the scale closes as Esc would, writing nothing.
pub(super) fn yield_to_card(
    state: &mut AppState,
    before: Option<&[String]>,
    mut effects: Vec<AppEffect>,
) -> Vec<AppEffect> {
    if let Some(before) = before
        && cards(state).is_some_and(|now| now.iter().any(|card| !before.contains(card)))
    {
        effects.extend(close(state));
    }
    effects
}

/// What the scale shows about the foreground session now.
fn effort_facts(state: &AppState) -> Option<(String, ScaleFacts)> {
    let opened = state.opened.as_ref()?;
    let session_id = opened.session_id.clone();
    let model = state.model.clone().or_else(|| opened.info.model.clone());
    let route = opened.info.provider.as_deref().zip(model.as_deref());
    let facts = ScaleFacts {
        current: state.effort.as_deref().and_then(Effort::parse),
        wire: state.effort_wire.clone(),
        offered: route.and_then(|(provider, model)| {
            state.models.effort_offered(&session_id, provider, model)
        }),
        can_disable: route.and_then(|(provider, model)| {
            state
                .models
                .can_disable_reasoning(&session_id, provider, model)
        }),
        hivecode: state
            .hivecode
            .status_input(Some(&SessionId::new(session_id.clone()))),
        profile: state.profile.clone(),
        model,
    };
    Some((session_id, facts))
}

/// Bare `/effort`: open the scale on the session's level and read the level again, quietly.
pub(super) fn open_effort(state: &mut AppState) -> Vec<AppEffect> {
    let mut effects = close(state);
    let Some((session_id, facts)) = effort_facts(state) else {
        return effects;
    };
    state.choice = Some(Selector {
        list: ChoiceList::new(Vec::new()),
        kind: Kind::Effort {
            scale: Box::new(EffortScale::new(facts)),
            session_id: session_id.clone(),
        },
    });
    // Unlike `/effort` before the scale, the answer prints nothing: the scale shows it.
    effects.push(AppEffect::Models(ModelsEffect::ReadEffort { session_id }));
    effects.push(AppEffect::Redraw);
    effects
}

/// A read or an event changed what the scale shows: move an untouched marker with it.
pub(super) fn refresh_effort(state: &mut AppState) {
    let Some(Kind::Effort { session_id, .. }) = state.choice.as_ref().map(|s| &s.kind) else {
        return;
    };
    let owner = session_id.clone();
    let Some((session_id, facts)) = effort_facts(state) else {
        return;
    };
    if let Some(Selector {
        kind: Kind::Effort { scale, .. },
        ..
    }) = &mut state.choice
        && session_id == owner
    {
        scale.set_current(facts.current, facts.wire);
        scale.set_caps(facts.offered, facts.can_disable);
    }
}

/// One key for the open `/effort` scale. Enter runs the typed commands for what was staged:
/// `/effort <level> [--default]`, then `/hivecode on|off`.
pub(super) fn scale_key(state: &mut AppState, key: ScaleKey) -> Vec<AppEffect> {
    let Some(Selector {
        kind: Kind::Effort {
            scale,
            session_id: owner,
        },
        ..
    }) = &mut state.choice
    else {
        return Vec::new();
    };
    let (level, save_default, hivecode) = match scale.update(key) {
        ScaleEffect::Redraw => return vec![AppEffect::Redraw],
        ScaleEffect::Cancel => return close(state),
        ScaleEffect::Apply {
            level,
            save_default,
            hivecode,
        } => (level, save_default, hivecode),
    };
    let owner = owner.clone();
    state.choice = None;
    let mut effects = vec![AppEffect::Redraw];
    if session_id(state).as_deref() != Some(owner.as_str()) {
        effects.push(notice_lines(EFFORT_SESSION_CHANGED));
        return effects;
    }
    let mut typed = |state: &mut AppState, action, command: &str, argument: String| {
        let line = format!("/{command} {argument}");
        effects.extend(super::local_effect(state, action, argument, line));
    };
    if let Some(level) = level {
        let argument = if save_default {
            format!("{} --default", level.as_wire())
        } else {
            level.as_wire().to_owned()
        };
        typed(state, LocalAction::Effort, "effort", argument);
    }
    if let Some(on) = hivecode {
        let argument = if on { "on" } else { "off" };
        typed(
            state,
            LocalAction::Hivecode,
            "hivecode",
            argument.to_owned(),
        );
    }
    effects
}

/// Run a selector's work. Replies come back as [`AppMessage::Choice`].
pub(super) fn run(
    state: &AppState,
    executor: &Executor,
    work: ChoiceWork,
    ignored: Option<NotAppliedReason>,
) {
    match work {
        ChoiceWork::ListThemes => {
            let files = state.appearance.theme_files().cloned().or_else(|| {
                let home = state.home.as_ref()?;
                Some(ThemeFileSession::new(
                    crate::config::user_config_path(home),
                    crate::config::Config::default().theme,
                    &state.profile,
                ))
            });
            executor.send_later(async move {
                let listing = tokio::task::spawn_blocking(move || ThemeListing {
                    users: files
                        .as_ref()
                        .map(|files| theme_file::list_themes(files.config_path()))
                        .unwrap_or_default(),
                    saved: files
                        .as_ref()
                        .and_then(|files| files.saved_choice().map(str::to_owned)),
                    configured: files
                        .as_ref()
                        .and_then(|files| files.configured_id().map(str::to_owned)),
                })
                .await
                .unwrap_or(ThemeListing {
                    users: Vec::new(),
                    saved: None,
                    configured: None,
                });
                AppMessage::Choice(ChoiceReply::Themes(Box::new(listing)))
            });
        }
        ChoiceWork::ListSkins => {
            let client = state
                .service
                .as_ref()
                .and_then(SessionService::client)
                .cloned();
            let session = session_id(state);
            let profile = state.profile.clone();
            let owner = (profile.clone(), session.clone());
            let mut service = state.appearance.clone();
            executor.send_later(async move {
                let Some(client) = client else {
                    return AppMessage::AppearanceDone {
                        skin: None,
                        lines: appearance::offline_skin_lines(None),
                        generation: None,
                    };
                };
                let before = service.skin_state().clone();
                let lines = service
                    .skin_command(&client, None, session.as_deref(), &profile)
                    .await;
                let skin = (service.skin_state() != &before).then(|| service.skin_state().clone());
                let result = appearance::skin_choices(&client, session.as_deref()).await;
                AppMessage::Choice(ChoiceReply::Skins {
                    report: (skin, lines),
                    result,
                    ignored,
                    owner,
                })
            });
        }
        ChoiceWork::Rest(timer) => executor.send_later(async move {
            tokio::time::sleep(SKIN_REST).await;
            AppMessage::Choice(ChoiceReply::Rested(timer))
        }),
        ChoiceWork::SetSkin(name) => {
            let Target { client, session } = state
                .skin_preview
                .as_ref()
                .map_or_else(|| target(state), |preview| preview.target.clone());
            let mut service = state.appearance.clone();
            executor.send_later(async move {
                let Some(client) = client else {
                    return AppMessage::Choice(ChoiceReply::SkinSet {
                        name,
                        skin: None,
                        result: Err(SkinPreviewError::Refused(
                            "Talaria could not reach Hermes".to_owned(),
                        )),
                    });
                };
                let before = service.skin_state().clone();
                let result = service
                    .preview_skin(&client, &name, session.as_deref())
                    .await;
                let skin = (service.skin_state() != &before).then(|| service.skin_state().clone());
                AppMessage::Choice(ChoiceReply::SkinSet { name, skin, result })
            });
        }
    }
}

/// The connection and session in the foreground now.
fn target(state: &AppState) -> Target {
    Target {
        client: state
            .service
            .as_ref()
            .and_then(SessionService::client)
            .cloned(),
        session: session_id(state),
    }
}

fn session_id(state: &AppState) -> Option<String> {
    state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
}

/// Fold a finished piece of work into the selector.
pub(super) fn reply(state: &mut AppState, reply: ChoiceReply) -> Vec<AppEffect> {
    match reply {
        // Typing that began before the list arrived goes on to the composer.
        ChoiceReply::Themes(_) if !state.draft.is_empty() => Vec::new(),
        ChoiceReply::Themes(listing) => {
            let mut effects = close(state);
            effects.extend(open_themes(state, *listing));
            effects
        }
        ChoiceReply::Skins {
            report: (skin, lines),
            result,
            ignored,
            owner,
        } => {
            let mut effects = super::update(
                state,
                AppMessage::AppearanceDone {
                    skin,
                    lines,
                    generation: None,
                },
            );
            // A list read before a switch, or for another session, belongs to that one.
            if let Ok(choices) = result
                && !choices.items.is_empty()
                && state.draft.is_empty()
                && !state.switching
                && owner == (state.profile.clone(), session_id(state))
            {
                effects.extend(close(state));
                effects.push(open_skins(state, choices, ignored));
            }
            effects
        }
        ChoiceReply::Rested(timer) => rested(state, timer),
        ChoiceReply::SkinSet { name, skin, result } => skin_set(state, name, skin, result),
    }
}

fn open_themes(state: &mut AppState, listing: ThemeListing) -> Vec<AppEffect> {
    let ThemeListing {
        users,
        saved,
        configured,
    } = &listing;
    let row =
        |id: String, name: &str, description: String, group: &str, current: bool| ChoiceItem {
            id,
            name: name.to_owned(),
            description,
            group: Some(group.to_owned()),
            current,
        };
    let follows = match configured.as_deref() {
        None => "Follow config.toml (no theme file set)".to_owned(),
        Some(id)
            if theme_file::builtin_description(id).is_some()
                && users.iter().all(|(user_id, _)| user_id != id) =>
        {
            format!("Follow config.toml (now {id})")
        }
        Some(id) => format!("Follow config.toml (now themes/{id}.toml)"),
    };
    let mut items = vec![row(
        FILE.to_owned(),
        "Configured",
        follows,
        "Theme",
        saved.is_none(),
    )];
    let mut files = Vec::new();
    for id in theme_file::BUILTIN_THEMES {
        if users.iter().any(|(user_id, _)| user_id == id) {
            continue;
        }
        let description = theme_file::builtin_description(id).unwrap_or_default();
        items.push(row(
            format!("{FILE}{id}"),
            id,
            description.to_owned(),
            "Theme",
            saved.as_deref() == Some(id),
        ));
        if let Some(file) = theme_file::builtin(id) {
            files.push((id.to_owned(), file));
        }
    }
    for (id, loaded) in users {
        let description = match loaded {
            Ok(file) => match &file.name {
                Some(name) => format!("Theme file themes/{id}.toml: {name}"),
                None => format!("Theme file themes/{id}.toml"),
            },
            Err(error) => format!("Cannot be used: {error}"),
        };
        items.push(row(
            format!("{FILE}{id}"),
            id,
            description,
            "Theme",
            saved.as_deref() == Some(id.as_str()),
        ));
        if let Ok(file) = loaded {
            files.push((id.clone(), file.clone()));
        }
    }
    let override_ = state.appearance.polarity_override();
    for (id, description, polarity) in [
        (
            "light",
            "Use a light terminal background",
            Some(Polarity::Light),
        ),
        (
            "dark",
            "Use a dark terminal background",
            Some(Polarity::Dark),
        ),
        ("auto", "Follow configuration and terminal detection", None),
    ] {
        items.push(row(
            format!("{BACKGROUND}{id}"),
            id,
            description.to_owned(),
            "Background",
            override_ == polarity,
        ));
    }
    state.choice = Some(Selector {
        list: ChoiceList::new(items),
        kind: Kind::Theme { files },
    });
    vec![AppEffect::Redraw]
}

fn open_skins(
    state: &mut AppState,
    choices: SkinChoices,
    ignored: Option<NotAppliedReason>,
) -> AppEffect {
    let items = choices
        .items
        .into_iter()
        .map(|(name, description)| ChoiceItem {
            current: name == choices.configured,
            id: name.clone(),
            name,
            description,
            group: Some("Skins Hermes has".to_owned()),
        })
        .collect();
    let mut list = ChoiceList::new(items);
    let note = match ignored {
        None => SKIN_NOTE.to_owned(),
        Some(reason) => {
            let why = match reason {
                NotAppliedReason::ColoursOff => "colours off",
                NotAppliedReason::FollowOff => "follow_hermes_skin = false",
                NotAppliedReason::SixteenColours => "16-colour terminal",
            };
            format!(
                "Skins do not show under the current theme ({why}); previewing still changes \
                 the Hermes profile's skin until you cancel."
            )
        }
    };
    list.set_note(Some(note));
    state.choice = Some(Selector {
        list,
        kind: Kind::Skin {
            original: choices.configured,
            target: target(state),
        },
    });
    AppEffect::Redraw
}

/// One key for the open selector.
pub(super) fn key(state: &mut AppState, key: ChoiceKey) -> Vec<AppEffect> {
    let Some(selector) = state
        .choice
        .as_mut()
        .filter(|selector| !matches!(selector.kind, Kind::Effort { .. }))
    else {
        return Vec::new();
    };
    match selector.list.update(key) {
        ChoiceEffect::Redraw => vec![AppEffect::Redraw],
        ChoiceEffect::Preview(id) => preview(state, &id),
        ChoiceEffect::Apply(id) => apply(state, &id),
        ChoiceEffect::Cancel => close(state),
    }
}

fn preview(state: &mut AppState, id: &str) -> Vec<AppEffect> {
    let Some(selector) = &state.choice else {
        return Vec::new();
    };
    match &selector.kind {
        Kind::Theme { files, .. } => {
            let preview = if let Some(id) = id.strip_prefix(BACKGROUND) {
                ThemePreview::Background(match id {
                    "light" => Some(Polarity::Light),
                    "dark" => Some(Polarity::Dark),
                    _ => None,
                })
            } else {
                let id = id.strip_prefix(FILE).unwrap_or(id);
                let wanted = if id.is_empty() {
                    state
                        .appearance
                        .theme_files()
                        .and_then(ThemeFileSession::configured_id)
                        .map(str::to_owned)
                } else {
                    Some(id.to_owned())
                };
                let found = wanted.and_then(|wanted| files.iter().find(|(id, _)| *id == wanted));
                match found {
                    Some(found) => ThemePreview::File(Some(found.clone())),
                    // A file that cannot be loaded previews nothing.
                    None if !id.is_empty() => {
                        state.appearance.set_theme_preview(None);
                        return vec![AppEffect::Appearance, AppEffect::Redraw];
                    }
                    None => ThemePreview::File(None),
                }
            };
            state.appearance.set_theme_preview(Some(preview));
            vec![AppEffect::Appearance, AppEffect::Redraw]
        }
        Kind::Skin { original, target } => {
            let preview = state.skin_preview.get_or_insert_with(|| SkinPreview {
                original: original.clone(),
                applied: original.clone(),
                wanted: original.clone(),
                timer: 0,
                in_flight: false,
                stopped: false,
                finish: None,
                restoring: false,
                target: target.clone(),
                held: Vec::new(),
            });
            if preview.stopped || preview.restoring || preview.finish.is_some() {
                return vec![AppEffect::Redraw];
            }
            preview.wanted = id.to_owned();
            preview.timer += 1;
            vec![
                AppEffect::Choice(ChoiceWork::Rest(preview.timer)),
                AppEffect::Redraw,
            ]
        }
        Kind::Effort { .. } => Vec::new(),
    }
}

fn apply(state: &mut AppState, id: &str) -> Vec<AppEffect> {
    let Some(selector) = state.choice.take() else {
        return Vec::new();
    };
    match selector.kind {
        Kind::Theme { .. } => {
            state.appearance.set_theme_preview(None);
            let mut effects = vec![AppEffect::Appearance, AppEffect::Redraw];
            if let Some(background) = id.strip_prefix(BACKGROUND) {
                effects.extend(super::theme_command(state, background));
            } else {
                let file = id.strip_prefix(FILE).unwrap_or(id);
                if file.is_empty() {
                    effects.push(AppEffect::ThemeFileCommand("use".to_owned()));
                } else {
                    effects.push(AppEffect::ThemeFileCommand(format!("use {file}")));
                }
            }
            effects
        }
        Kind::Skin { .. } => {
            let skin = AppEffect::Skin(Some(id.to_owned()));
            let Some(preview) = &mut state.skin_preview else {
                return vec![skin, AppEffect::Redraw];
            };
            preview.timer += 1;
            if preview.in_flight {
                preview.finish = Some(Finish::Apply(id.to_owned()));
                return vec![AppEffect::Redraw];
            }
            state.skin_preview = None;
            vec![skin, AppEffect::Redraw]
        }
        Kind::Effort { .. } => vec![AppEffect::Redraw],
    }
}

/// Close the open selector, putting back what was in force when it opened.
pub(super) fn close(state: &mut AppState) -> Vec<AppEffect> {
    let Some(selector) = state.choice.take() else {
        return Vec::new();
    };
    match selector.kind {
        Kind::Theme { .. } => {
            state.appearance.set_theme_preview(None);
            vec![AppEffect::Appearance, AppEffect::Redraw]
        }
        Kind::Skin { .. } => {
            let Some(preview) = &mut state.skin_preview else {
                return vec![AppEffect::Redraw];
            };
            preview.timer += 1;
            if preview.in_flight {
                preview.finish = Some(Finish::Restore);
                return vec![AppEffect::Redraw];
            }
            let mut effects = restore(state);
            effects.push(AppEffect::Redraw);
            effects
        }
        // The scale wrote nothing.
        Kind::Effort { .. } => vec![AppEffect::Redraw],
    }
}

/// Write the original skin back, when a preview changed it.
fn restore(state: &mut AppState) -> Vec<AppEffect> {
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    if preview.applied == preview.original {
        state.skin_preview = None;
        return Vec::new();
    }
    preview.restoring = true;
    preview.in_flight = true;
    vec![AppEffect::Choice(ChoiceWork::SetSkin(
        preview.original.clone(),
    ))]
}

fn rested(state: &mut AppState, timer: u64) -> Vec<AppEffect> {
    let open = matches!(
        state.choice.as_ref().map(|selector| &selector.kind),
        Some(Kind::Skin { .. })
    );
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    if !open
        || timer != preview.timer
        || preview.in_flight
        || preview.stopped
        || preview.wanted == preview.applied
    {
        return Vec::new();
    }
    preview.in_flight = true;
    vec![AppEffect::Choice(ChoiceWork::SetSkin(
        preview.wanted.clone(),
    ))]
}

/// Fold a write's answer in. A switch held for it starts once no write is in flight.
fn skin_set(
    state: &mut AppState,
    name: String,
    skin: Option<AppearanceState>,
    result: Result<(), SkinPreviewError>,
) -> Vec<AppEffect> {
    let held = state
        .skin_preview
        .as_mut()
        .map(|preview| std::mem::take(&mut preview.held))
        .unwrap_or_default();
    let mut effects = settle(state, name, skin, result);
    match &mut state.skin_preview {
        Some(preview) if preview.in_flight => preview.held = held,
        _ => effects.extend(held),
    }
    effects
}

fn settle(
    state: &mut AppState,
    name: String,
    skin: Option<AppearanceState>,
    result: Result<(), SkinPreviewError>,
) -> Vec<AppEffect> {
    let Some(preview) = &mut state.skin_preview else {
        return Vec::new();
    };
    preview.in_flight = false;
    let mut effects = Vec::new();
    let failure = match result {
        Ok(()) => {
            preview.applied.clone_from(&name);
            None
        }
        Err(SkinPreviewError::Refused(reason)) => Some(reason),
        Err(SkinPreviewError::Unknown(reason)) => {
            preview.applied.clone_from(&name);
            Some(reason)
        }
    };
    if let Some(skin) = skin
        && state.appearance.adopt(skin) != AppearanceChange::Unchanged
    {
        effects.push(AppEffect::Appearance);
    }
    let Some(preview) = &mut state.skin_preview else {
        return effects;
    };
    if preview.restoring {
        let original = preview.original.clone();
        state.skin_preview = None;
        effects.push(notice_lines(&match failure {
            None => format!("Skin preview cancelled; the profile's skin is \"{original}\" again."),
            Some(reason) => format!(
                "Talaria could not put the profile's skin back to \"{original}\" ({reason}). \
                 Run /skin {original} to restore it."
            ),
        }));
        return effects;
    }
    if let Some(reason) = failure {
        preview.stopped = true;
        if let Some(selector) = &mut state.choice {
            selector.list.set_note(Some(format!(
                "Preview stopped: {reason}. Enter still applies."
            )));
            effects.push(AppEffect::Redraw);
        }
    }
    let Some(preview) = &mut state.skin_preview else {
        return effects;
    };
    match preview.finish.take() {
        Some(Finish::Restore) => effects.extend(restore(state)),
        Some(Finish::Apply(name)) => {
            state.skin_preview = None;
            effects.push(AppEffect::Skin(Some(name)));
        }
        None if !preview.stopped && preview.wanted != preview.applied => {
            preview.timer += 1;
            effects.push(AppEffect::Choice(ChoiceWork::Rest(preview.timer)));
        }
        None => {}
    }
    effects
}

/// Before a session or profile switch starts: close the open selector, which puts the original
/// look back through the connection it opened on. While a skin write is in flight the switch
/// waits for it, so neither a late preview nor the restore can land after the switch.
pub(super) fn before_switch(state: &mut AppState, effects: Vec<AppEffect>) -> Vec<AppEffect> {
    let is_switch = |effect: &AppEffect| {
        matches!(
            effect,
            AppEffect::Switch { .. } | AppEffect::StartInProfile { .. }
        )
    };
    let writing = state
        .skin_preview
        .as_ref()
        .is_some_and(|preview| preview.in_flight);
    if !effects.iter().any(is_switch) || (state.choice.is_none() && !writing) {
        return effects;
    }
    let (switches, mut effects): (Vec<_>, Vec<_>) = effects.into_iter().partition(is_switch);
    effects.extend(close(state));
    match &mut state.skin_preview {
        Some(preview) if preview.in_flight => preview.held.extend(switches),
        _ => effects.extend(switches),
    }
    effects
}

/// Before Talaria exits: write the original skin back when a preview changed it, through the
/// connection and session the selector opened on. A skin chosen with Enter that still waits for
/// a preview write is the person's decision, so that skin is written instead.
pub(super) async fn restore_on_exit(state: &mut AppState) {
    let Some(preview) = state.skin_preview.take() else {
        return;
    };
    if preview.applied == preview.original && !preview.in_flight {
        return;
    }
    let Target {
        client: Some(client),
        session,
    } = preview.target
    else {
        return;
    };
    let skin = match &preview.finish {
        Some(Finish::Apply(chosen)) => chosen,
        _ => &preview.original,
    };
    let mut service = state.appearance.clone();
    let _ = tokio::time::timeout(
        RESTORE_WAIT,
        service.preview_skin(&client, skin, session.as_deref()),
    )
    .await;
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::{UserAction, update};
    use super::*;
    use talaria_core::commands::ChoiceKey;

    /// A `/skin` selector open on `fixture` over `default`.
    fn selector() -> AppState {
        let mut state = AppState::new("choice-tests");
        state.profile = "fixture".into();
        open(&mut state);
        state
    }

    /// Open a `/skin` selector over `default`, offering `ares`.
    pub(in crate::app) fn open(state: &mut AppState) {
        let _ = open_skins(
            state,
            SkinChoices {
                configured: "default".into(),
                items: vec![
                    ("ares".into(), "Synthetic red".into()),
                    ("default".into(), "Synthetic gold".into()),
                ],
            },
            None,
        );
    }

    /// The skin each effect asks to write, in order.
    pub(in crate::app) fn writes(effects: &[AppEffect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::Choice(ChoiceWork::SetSkin(name)) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    pub(in crate::app) fn switches(effects: &[AppEffect]) -> usize {
        effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    AppEffect::Switch { .. } | AppEffect::StartInProfile { .. }
                )
            })
            .count()
    }

    /// Move to `ares` and let the selection rest: the preview write is in flight.
    pub(in crate::app) fn previewing(state: &mut AppState) {
        let effects = key(state, ChoiceKey::Prev);
        let timer = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Choice(ChoiceWork::Rest(timer)) => Some(*timer),
                _ => None,
            })
            .expect("the move waits for the selection to rest");
        assert_eq!(writes(&reply(state, ChoiceReply::Rested(timer))), ["ares"]);
    }

    pub(in crate::app) fn answered(state: &mut AppState, name: &str) -> Vec<AppEffect> {
        update(
            state,
            AppMessage::Choice(ChoiceReply::SkinSet {
                name: name.into(),
                skin: None,
                result: Ok(()),
            }),
        )
    }

    fn resume(state: &mut AppState) -> Vec<AppEffect> {
        update(
            state,
            AppMessage::User(UserAction::Submit("/resume other-session".into())),
        )
    }

    #[test]
    fn a_switch_closes_the_skin_selector_and_waits_for_the_restore() {
        let mut state = selector();
        previewing(&mut state);
        let _ = answered(&mut state, "ares");
        // The profile now shows `ares`. A switch puts `default` back on this profile before it
        // starts, so the next profile never receives this one's original skin.
        let effects = resume(&mut state);
        assert!(
            state.choice.is_none(),
            "the selector closes before a switch"
        );
        assert_eq!(writes(&effects), ["default"]);
        assert_eq!(switches(&effects), 0, "the switch waits for the restore");
        let effects = answered(&mut state, "default");
        assert_eq!(
            switches(&effects),
            1,
            "the switch starts once the restore answers"
        );
        assert!(state.skin_preview.is_none());
    }

    #[test]
    fn a_switch_during_a_preview_write_waits_for_it_and_for_the_restore() {
        let mut state = selector();
        previewing(&mut state);
        let effects = resume(&mut state);
        assert!(state.choice.is_none());
        assert!(writes(&effects).is_empty(), "one write at a time");
        assert_eq!(switches(&effects), 0);
        // The late preview answers first; only then is the original written back.
        let effects = answered(&mut state, "ares");
        assert_eq!(writes(&effects), ["default"]);
        assert_eq!(switches(&effects), 0);
        let effects = answered(&mut state, "default");
        assert_eq!(switches(&effects), 1);
    }

    #[test]
    fn a_switch_with_nothing_previewed_closes_the_selector_and_goes_ahead() {
        let mut state = selector();
        let effects = resume(&mut state);
        assert!(state.choice.is_none());
        assert!(writes(&effects).is_empty());
        assert_eq!(switches(&effects), 1);
    }

    #[test]
    fn a_paste_closes_the_selector_and_reaches_the_composer() {
        let mut state = selector();
        previewing(&mut state);
        let _ = answered(&mut state, "ares");
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Paste {
                text: "pasted words".into(),
                draft: String::new(),
                caret: 0,
            }),
        );
        // The next Enter sends the pasted text instead of applying a skin.
        assert!(state.choice.is_none(), "the selector closes on a paste");
        assert_eq!(writes(&effects), ["default"], "the original skin goes back");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Attachment(crate::attachments::AttachmentEffect::EditDraft { text, .. })
                if text == "pasted words"
        )));
    }

    #[test]
    fn a_skin_list_that_arrives_during_a_switch_opens_no_selector() {
        let mut state = AppState::new("choice-tests");
        state.profile = "fixture".into();
        state.switching = true;
        let _ = reply(
            &mut state,
            ChoiceReply::Skins {
                report: (None, Vec::new()),
                result: Ok(SkinChoices {
                    configured: "default".into(),
                    items: vec![("ares".into(), String::new())],
                }),
                ignored: None,
                owner: ("fixture".into(), None),
            },
        );
        assert!(state.choice.is_none(), "no selector while a switch runs");
        state.switching = false;
        let _ = reply(
            &mut state,
            ChoiceReply::Skins {
                report: (None, Vec::new()),
                result: Ok(SkinChoices {
                    configured: "default".into(),
                    items: vec![("ares".into(), String::new())],
                }),
                ignored: None,
                owner: ("elsewhere".into(), None),
            },
        );
        assert!(
            state.choice.is_none(),
            "no selector for another profile's list"
        );
    }
}
