use super::*;

pub(super) fn with_paths(
    kind: CapText,
    args: &TextArgs,
    hermes_home: &str,
    user_home: &str,
) -> String {
    let mut a = args.clone();
    let redact = |s: &str| redact_paths(s, hermes_home, user_home);
    for s in [
        &mut a.profile,
        &mut a.name,
        &mut a.label,
        &mut a.key,
        &mut a.message,
        &mut a.source,
        &mut a.trust,
        &mut a.verdict,
        &mut a.reason,
        &mut a.query,
        &mut a.repository,
    ] {
        *s = redact(s);
    }
    for list in [
        &mut a.sources,
        &mut a.warnings,
        &mut a.missing_env,
        &mut a.known_issues,
        &mut a.activation.live_servers,
        &mut a.activation.live_skills,
    ] {
        for s in list {
            *s = redact(s);
        }
    }
    for (_, list) in &mut a.activation.deferred {
        for s in list {
            *s = redact(s);
        }
    }
    a.sha = a.sha.as_deref().map(redact);
    a.session_line = a
        .session_line
        .map(|line| line.map(|s| redact(&s)).map_err(|s| redact(&s)));
    format_text(kind, &a)
}

pub(super) fn clean(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '\t' => result.push_str("    "),
            '\n' => result.push('␊'),
            '\r' => result.push('␍'),
            '\u{00}'..='\u{1f}' => result.push(char::from_u32(0x2400 + c as u32).unwrap_or('�')),
            '\u{7f}' => result.push('␡'),
            '\u{80}'..='\u{9f}'
            | '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{e0000}'..='\u{e007f}' => result.push_str(&format!("‹U+{:04X}›", c as u32)),
            _ => result.push(c),
        }
    }
    if result.chars().count() > 300 {
        result = result
            .chars()
            .take(299)
            .chain(std::iter::once('…'))
            .collect();
    }
    result
}

pub(super) fn long_text(text: &str) -> Vec<String> {
    let mut end = text.len().min(256 * 1024);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut lines: Vec<_> = text[..end]
        .lines()
        .map(|line| {
            // Long documents retain their length; individual lines still render literally.
            line.chars()
                .flat_map(|c| match c {
                    '\t' => "    ".chars().collect::<Vec<_>>(),
                    '\u{00}'..='\u{1f}' => vec![char::from_u32(0x2400 + c as u32).unwrap_or('�')],
                    '\u{7f}' => vec!['␡'],
                    '\u{80}'..='\u{9f}'
                    | '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{e0000}'..='\u{e007f}' => {
                        format!("‹U+{:04X}›", c as u32).chars().collect()
                    }
                    _ => vec![c],
                })
                .collect()
        })
        .collect();
    if end < text.len() {
        lines.push(format_text(CapText::Truncated, &TextArgs::default()));
    }
    lines
}

fn activation(a: &TextArgs) -> Vec<String> {
    let mut lines = Vec::new();
    let live: Vec<_> = a
        .activation
        .live_servers
        .iter()
        .chain(&a.activation.live_skills)
        .map(|s| clean(s))
        .collect();
    if !live.is_empty() {
        lines.push(format!("Live now: {}.", live.join(", ")));
    }
    let next: Vec<_> = a
        .activation
        .deferred
        .iter()
        .flat_map(|(_, list)| list)
        .map(|s| clean(s))
        .collect();
    if !next.is_empty() {
        lines.push(format!("From the next session: {}.", next.join(", ")));
    }
    if a.restart_required {
        lines.push("Restart the gateway for this to take effect.".into());
    }
    lines
}

pub fn format_text(kind: CapText, a: &TextArgs) -> String {
    let p = clean(&a.profile);
    let n = clean(&a.name);
    let label = clean(&a.label);
    let key = clean(&a.key);
    let message = clean(&a.message);
    let source = clean(&a.source);
    let trust = clean(&a.trust);
    let on = if a.enabled { "on" } else { "off" };
    match kind {
        CapText::FilterPrompt => "Filter: ".into(),
        CapText::SearchPrompt => "Search: ".into(),
        CapText::InstallPrompt => "Install: ".into(),
        CapText::Loading => match a.tab {CapTab::Skills=>"Loading skills...",CapTab::Hub=>"Loading the skill hub...",CapTab::Toolsets=>"Loading toolsets...",CapTab::Plugins=>"Loading plugins..."}.into(),
        CapText::EmptySkills=>format!("No skills are installed in profile {p}. Press Tab to open the skill hub."),
        CapText::EmptyToolsets=>format!("Hermes reported no configurable toolsets for profile {p}."),
        CapText::EmptyPlugins=>format!("You have not installed any plugins in profile {p}. Press a to show the {} that ship with Hermes, or i to install one.",a.number),
        CapText::EmptyHub=>"The skill hub returned nothing. Check the network, then press r.".into(),
        CapText::NoSearchResults=>format!("No hub skills match \"{}\".",clean(&a.query)),
        CapText::SlowSources=>format!("Some hub sources did not answer in time: {}.",a.sources.iter().map(|s|clean(s)).collect::<Vec<_>>().join(", ")),
        CapText::SkillChanged=>format!("Turned {on} skill {n} in profile {p}. New sessions use this."),
        CapText::ToolsetChanged=>format!("Turned {on} toolset {label} in profile {p}. Sessions started from now on use this; open sessions keep their toolsets."),
        CapText::PostSetup=>format!("Hermes is installing what {label} needs ({key}) in the background."),
        CapText::PluginChanged=>{let mut lines=vec![format!("Turned {on} plugin {n}.")];lines.extend(activation(a));lines.join("\n")},
        CapText::Mismatch=>format!("Hermes did not keep the change to {n}: it now reads {on}. Press r to refresh."),
        CapText::Busy=>"Wait for the current change to finish.".into(),
        CapText::LostAnswer=>"Talaria lost the answer to this change. Refreshing to see what Hermes did.".into(),
        CapText::ConfirmSkill=>format!("Install {n} into profile {p}? Source: {source} ({trust}). Scan: {}; {} critical, {} high, {} medium, {} low. Press y to install, any other key to cancel.",clean(&a.verdict),a.counts[0],a.counts[1],a.counts[2],a.counts[3]),
        CapText::DeniedSkill=>format!("Hermes will not install {n} from Talaria: {}",clean(&a.reason)),
        CapText::InstallingSkill=>format!("Installing {n}... Hermes fetches and scans it; this can take a minute."),
        CapText::InstalledSkill=>{let mut s=format!("Installed {n} into profile {p}.");if let Some(line)=&a.session_line {s.push('\n');s.push_str(&match line {Ok(s)=>format!("This session: {}.",clean(s)),Err(s)=>format!("This session could not reload its skills: {}.",clean(s))});}s},
        CapText::InstallFailed=>format!("Hermes did not install {n}: {message}"),
        CapText::Unsupported=>format!("This gateway does not answer {key} on Talaria's connection."),
        CapText::Refused=>format!("Hermes refused this request: {message}"),
        CapText::HubUnavailable=>"The skill hub did not answer. Press r to try again.".into(),
        CapText::ConfirmCatalog=>format!("Install plugin {n} from the Hermes catalog ({}, {}) into profile {p}? Press y to install, any other key to cancel.",clean(&a.repository),a.sha.as_ref().map(|s|format!("commit {}",clean(&s.chars().take(7).collect::<String>()))).unwrap_or_else(||"commit unknown".into())),
        CapText::ConfirmRepository=>format!("Install the plugin at {} into profile {p}? It is not from the Hermes catalog, so nobody has reviewed it. Press y to install, any other key to cancel.",clean(&a.repository)),
        CapText::InvalidInput=>"Type a catalog name (lowercase letters, digits, _ or -) or an https:// repository address.".into(),
        CapText::UnknownPlugin=>format!("The Hermes plugin catalog has no plugin named {n}."),
        CapText::CatalogUnavailable=>"Hermes could not read its plugin catalog. Try again later, or install from an https:// repository address.".into(),
        CapText::InstalledPlugin=>{let mut lines=vec![format!("Installed plugin {n}.")];for w in &a.warnings {lines.push(format!("Warning: {}",clean(w)));}if !a.missing_env.is_empty(){lines.push(format!("It needs these settings before it works: {}.",a.missing_env.iter().map(|s|clean(s)).collect::<Vec<_>>().join(", ")));}for k in &a.known_issues {lines.push(format!("Known issue: {}",clean(k)));}lines.extend(activation(a));if !a.enabled{lines.push("It is turned off; press Space to turn it on.".into());}lines.join("\n")},
        CapText::ConfirmRemove=>format!("Remove plugin {n} from profile {p}? Press y to remove, any other key to cancel."),
        CapText::RemovedPlugin=>format!("Removed plugin {n}.{}",if a.cleared_memory_provider {"\nIt was the memory provider; Hermes now uses its built-in memory."}else{""}),
        CapText::BundledRemove=>format!("{n} ships with Hermes and cannot be removed; turn it off instead."),
        CapText::ReadOnly=>format!("Talaria shows {n} read-only: the gateway listed it without a key or with a status Talaria does not know."),
        CapText::ProfileUnavailable=>format!("Profile {p} is not served by the gateway, so its capabilities cannot be read or changed."),
        CapText::Truncated=>"(showing the first 256 KiB)".into(),
        CapText::CatalogListUnavailable=>"The plugin catalog list is not available from this gateway; type a catalog name instead.".into(),
        CapText::Unauthorized=>"Hermes refused Talaria's sign-in for this request. Press r to retry.".into(),
        CapText::Unreachable=>"Talaria could not reach the gateway. Press r to retry.".into(),
        CapText::MissingInstall=>format!("Hermes reported {n} installed, but profile {p} does not list it. Press r to refresh."),
        CapText::ReadTimeout=>"Hermes did not answer in time. Press r to try again.".into(),
        CapText::ReadBackFailed=>format!("Hermes accepted the change to {n}, but Talaria could not read it back. Press r to refresh."),
        CapText::MissingContent=>format!("Hermes has no skill {n} with a SKILL.md in profile {p}. Press r to refresh."),
        CapText::MissingHubItem=>format!("The skill hub has no skill {key}."),
        CapText::PackageRemove=>format!("{n} is a Python package; remove its package outside Talaria."),
        CapText::UnchangedPlugin=>format!("Plugin {n} was already {on}."),
        CapText::AmbiguousRemove=>format!("More than one installed plugin is named {n}, so Talaria cannot tell which one Hermes would remove."),
        CapText::MissingChangedRow=>format!("Hermes did not keep the change to {n}: it is no longer installed. Press r to refresh."),
        CapText::BadCommand=>"Use /capabilities skills, hub, toolsets or plugins.".into(),
        CapText::TooSmall=>"Capabilities needs at least 40 by 10.".into(),
        CapText::FindingsTail=>format!("and {} more",a.number),
        CapText::EmptyFilter=>format!("Nothing matches \"{}\".",clean(&a.query)),
        CapText::EmptyLog=>"(Hermes sent no install log)".into(),
    }
}
