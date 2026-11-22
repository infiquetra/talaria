//! A check of the literal model and provider names a script holds, run before any agent starts.
//!
//! A script names a model in an options object such as `{model: 'x', provider: 'y'}`. A typo
//! there is otherwise found only when that agent starts, after earlier agents have spent.
use super::{Availability, Roster, display_safe, route::unusable};

/// One literal route found in a script. `provider` is `None` when the object names no provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRoute {
    pub line: u32,
    pub model: String,
    pub provider: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
    /// A number, a template, a regular expression: anything that is not a name.
    Other,
}

fn lex(source: &str) -> Vec<(Tok, u32)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out: Vec<(Tok, u32)> = Vec::new();
    let (mut i, mut line) = (0usize, 1u32);
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if next == Some('/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
            }
            '\'' | '"' => {
                let start_line = line;
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != c {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    text.push(chars[i]);
                    i += 1;
                }
                i += 1;
                out.push((Tok::Str(text), start_line));
            }
            '`' => {
                let start_line = line;
                let mut depth = 0usize;
                i += 1;
                while i < chars.len() && (chars[i] != '`' || depth > 0) {
                    match chars[i] {
                        '\\' => i += 1,
                        '$' if chars.get(i + 1) == Some(&'{') => {
                            depth += 1;
                            i += 1;
                        }
                        '}' if depth > 0 => depth -= 1,
                        '\n' => line += 1,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                out.push((Tok::Other, start_line));
            }
            '/' if !matches!(
                out.last().map(|t| &t.0),
                Some(Tok::Str(_) | Tok::Other | Tok::Punct(')' | ']' | '}'))
            ) && !matches!(
                out.last().map(|t| &t.0),
                Some(Tok::Ident(w)) if !matches!(w.as_str(), "return" | "typeof" | "case" | "await" | "yield")
            ) =>
            {
                // A regular expression literal.
                i += 1;
                let mut class = false;
                while i < chars.len() && (chars[i] != '/' || class) && chars[i] != '\n' {
                    match chars[i] {
                        '\\' => i += 1,
                        '[' => class = true,
                        ']' => class = false,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                out.push((Tok::Other, line));
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
                out.push((Tok::Ident(chars[start..i].iter().collect()), line));
            }
            c if c.is_ascii_digit() => {
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
                {
                    i += 1;
                }
                out.push((Tok::Other, line));
            }
            c => {
                out.push((Tok::Punct(c), line));
                i += 1;
            }
        }
    }
    out
}

#[derive(Default)]
struct Frame {
    model: Option<Value>,
    provider: Option<Value>,
    other_profile: bool,
}
enum Value {
    Literal(String, u32),
    Dynamic,
}

/// Every options object of the script that names a model by a literal string. An object that
/// names another profile, or whose model is computed, is left to the run itself.
pub fn script_routes(source: &str) -> Vec<ScriptRoute> {
    let tokens = lex(source);
    let mut stack: Vec<Frame> = Vec::new();
    let mut routes = Vec::new();
    for (index, (token, _)) in tokens.iter().enumerate() {
        match token {
            Tok::Punct('{') => stack.push(Frame::default()),
            Tok::Punct('}') => {
                let Some(frame) = stack.pop() else { continue };
                if frame.other_profile {
                    continue;
                }
                if let Some(Value::Literal(model, line)) = frame.model {
                    match frame.provider {
                        None => routes.push(ScriptRoute {
                            line,
                            model,
                            provider: None,
                        }),
                        Some(Value::Literal(provider, _)) => routes.push(ScriptRoute {
                            line,
                            model,
                            provider: Some(provider),
                        }),
                        Some(Value::Dynamic) => {}
                    }
                }
            }
            Tok::Ident(key) | Tok::Str(key)
                if matches!(key.as_str(), "model" | "provider" | "profile") =>
            {
                let after_open = index > 0 && matches!(tokens[index - 1].0, Tok::Punct('{' | ','));
                let Some(frame) = stack.last_mut().filter(|_| after_open) else {
                    continue;
                };
                let value = match (
                    tokens.get(index + 1),
                    tokens.get(index + 2),
                    tokens.get(index + 3),
                ) {
                    (Some((Tok::Punct(':'), _)), Some((Tok::Str(text), line)), end)
                        if matches!(end.map(|e| &e.0), Some(Tok::Punct(',' | '}')) | None) =>
                    {
                        Value::Literal(text.clone(), *line)
                    }
                    _ => Value::Dynamic,
                };
                match key.as_str() {
                    "model" => frame.model = Some(value),
                    "provider" => frame.provider = Some(value),
                    _ => frame.other_profile = true,
                }
            }
            _ => {}
        }
    }
    routes
}

/// The edit distance between two names, ignoring case.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The known name closest to `wanted`, when one is close enough to be a misspelling of it.
fn nearest<'a>(wanted: &str, known: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (wanted.chars().count() / 3).max(3);
    known
        .map(|name| (distance(wanted, name), name))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| name)
}

/// One problem line per route the roster cannot serve, worded as the run's own refusal is,
/// with the nearest known spelling when there is one.
pub fn check_script_routes(roster: &Roster, routes: &[ScriptRoute]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut problems = Vec::new();
    for route in routes {
        if !seen.insert((route.model.clone(), route.provider.clone())) {
            continue;
        }
        let model = display_safe(&route.model);
        let signed_in: Vec<_> = roster
            .providers()
            .iter()
            .filter(|p| p.authenticated)
            .collect();
        let problem = match &route.provider {
            Some(provider) => {
                let a = roster.availability(provider, &route.model, roster.taken_at());
                let Some(reason) = unusable(&a) else {
                    continue;
                };
                let hint = match a {
                    Availability::Missing => {
                        nearest(provider, roster.providers().iter().map(|p| p.slug.as_str())).map(
                            |name| format!(" (nearest known provider: '{}')", display_safe(name)),
                        )
                    }
                    Availability::NotOffered => {
                        let here = roster
                            .providers()
                            .iter()
                            .find(|p| p.slug == *provider)
                            .and_then(|p| {
                                nearest(&route.model, p.models.iter().map(String::as_str))
                            });
                        let elsewhere: Vec<_> = signed_in
                            .iter()
                            .filter(|p| p.models.contains(&route.model))
                            .map(|p| display_safe(&p.slug))
                            .collect();
                        match (here, elsewhere.is_empty()) {
                            (Some(name), _) => Some(format!(
                                " (nearest known spelling: '{}')",
                                display_safe(name)
                            )),
                            (None, false) => {
                                Some(format!(" (offered by {})", elsewhere.join(", ")))
                            }
                            (None, true) => None,
                        }
                    }
                    _ => None,
                };
                format!(
                    "model '{model}' on '{}' is not usable: {reason}{}",
                    display_safe(provider),
                    hint.unwrap_or_default()
                )
            }
            None => {
                let offered: Vec<_> = signed_in
                    .iter()
                    .filter(|p| p.models.contains(&route.model))
                    .collect();
                match offered.as_slice() {
                    [] => {
                        let hint = signed_in
                            .iter()
                            .flat_map(|p| {
                                p.models.iter().map(move |m| (m.as_str(), p.slug.as_str()))
                            })
                            .map(|(m, p)| (distance(&route.model, m), m, p))
                            .filter(|(d, _, _)| *d <= (route.model.chars().count() / 3).max(3))
                            .min_by_key(|(d, _, _)| *d)
                            .map(|(_, m, p)| {
                                format!(
                                    " (nearest known spelling: '{}' on '{}')",
                                    display_safe(m),
                                    display_safe(p)
                                )
                            });
                        format!(
                            "model '{model}' is not offered by any signed-in provider{}",
                            hint.unwrap_or_default()
                        )
                    }
                    [one] => {
                        let a = roster.availability(&one.slug, &route.model, roster.taken_at());
                        match unusable(&a) {
                            Some(reason) => format!(
                                "model '{model}' on '{}' is not usable: {reason}",
                                display_safe(&one.slug)
                            ),
                            None => continue,
                        }
                    }
                    many => format!(
                        "model '{model}' is offered by {}; name a provider",
                        many.iter()
                            .map(|p| display_safe(&p.slug))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            }
        };
        problems.push(format!("script line {}: {problem}", route.line));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn route(line: u32, model: &str, provider: Option<&str>) -> ScriptRoute {
        ScriptRoute {
            line,
            model: model.into(),
            provider: provider.map(str::to_owned),
        }
    }

    #[test]
    fn literal_pairs_are_found_wherever_the_options_sit() {
        let source = "const a = await agent('x', {label: 'a', model: 'm-one', provider: 'p-one'});\n\
            const o = {'model': \"m-two\", \"provider\": 'p-two',};\n\
            const b = await agent('y', {model: 'm-three'});";
        assert_eq!(
            script_routes(source),
            vec![
                route(1, "m-one", Some("p-one")),
                route(2, "m-two", Some("p-two")),
                route(3, "m-three", None),
            ]
        );
    }

    #[test]
    fn names_that_are_not_routes_are_ignored() {
        let source = "// {model: 'in-a-comment'}\n\
            const text = 'model: \"in-a-string\"';\n\
            const t = `{model: 'in-a-template'} ${ {model: 'inside'} }`;\n\
            const re = /{model: 'x'}/;\n\
            const dynamic = {model: pick(), provider: 'p'};\n\
            const shorthand = {model, provider};\n\
            const joined = {model: 'a' + 'b'};\n\
            const other = {model: 'm', profile: 'elsewhere'};\n\
            const key = {nested: {model: 'm-nested', provider: 'p-nested'}};\n";
        assert_eq!(
            script_routes(source),
            vec![route(9, "m-nested", Some("p-nested"))]
        );
    }

    fn roster() -> Roster {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
            .unwrap()
            .to_utc();
        Roster::from_model_options(
            &json!({"model":"m-small","provider":"alpha-cloud","providers":[
                {"slug":"alpha-cloud","name":"Alpha","authenticated":true,
                 "models":["m-small","m-large","shared"],"unavailable_models":["m-retired"]},
                {"slug":"beta-cloud","name":"Beta","authenticated":true,"models":["shared","b-one"]},
                {"slug":"gamma-cloud","name":"Gamma","authenticated":false,"models":["g-one"]}]}),
            "fixture",
            now,
        )
        .unwrap()
    }

    #[test]
    fn unknown_names_are_listed_with_the_nearest_spelling() {
        let problems = check_script_routes(
            &roster(),
            &[
                route(3, "m-smal", Some("alpha-cloud")),
                route(4, "m-smal", Some("alpha-cloud")),
                route(5, "m-small", Some("alpha-clod")),
                route(6, "g-one", Some("gamma-cloud")),
                route(7, "m-large", Some("beta-cloud")),
                route(8, "b-on", None),
                route(9, "shared", None),
                route(10, "nothing-alike-at-all", None),
                route(11, "m-small", Some("alpha-cloud")),
                route(12, "b-one", None),
            ],
        );
        assert_eq!(
            problems,
            vec![
                "script line 3: model 'm-smal' on 'alpha-cloud' is not usable: model is not offered (nearest known spelling: 'm-small')",
                "script line 5: model 'm-small' on 'alpha-clod' is not usable: provider is not configured (nearest known provider: 'alpha-cloud')",
                "script line 6: model 'g-one' on 'gamma-cloud' is not usable: provider is not signed in",
                "script line 7: model 'm-large' on 'beta-cloud' is not usable: model is not offered (offered by alpha-cloud)",
                "script line 8: model 'b-on' is not offered by any signed-in provider (nearest known spelling: 'b-one' on 'beta-cloud')",
                "script line 9: model 'shared' is offered by alpha-cloud, beta-cloud; name a provider",
                "script line 10: model 'nothing-alike-at-all' is not offered by any signed-in provider",
            ]
        );
    }
}
