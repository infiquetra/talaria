//! A check of the literal model and provider names a script holds, run before any agent starts.
//!
//! A call such as `agent('p', {model: 'x', provider: 'y'})` names its route in the call itself. A
//! typo there is otherwise found only when that agent starts, after earlier agents have spent.
//! The check judges only such pairs: anything it cannot read for certain, such as a spread, a
//! later assignment or a call naming only a model, is left to the check made when the agent
//! starts, so no script that would run is refused.
use super::{Availability, Roster, display_safe, route::unusable};

/// One literal model and provider pair an `agent()` call names in its own options object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRoute {
    pub line: u32,
    pub model: String,
    pub provider: String,
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
    /// The object is written as the second argument of an `agent()` call.
    call_options: bool,
    model: Option<Value>,
    provider: Option<Value>,
    other_profile: bool,
    /// A spread or computed key could replace a name at run time.
    spread: bool,
}
enum Value {
    Literal(String, u32),
    Dynamic,
}
/// An open bracket: a call's argument list, an object literal, or an array.
enum Open {
    Paren { agent: bool, commas: u32 },
    Brace(Frame),
    Bracket,
}

/// Every `agent()` call whose own options object names both its model and its provider by
/// literal strings. A call whose options are built elsewhere, spread from another object, name
/// another profile, or compute either name is left to the run itself.
pub fn script_routes(source: &str) -> Vec<ScriptRoute> {
    let tokens = lex(source);
    let mut stack: Vec<Open> = Vec::new();
    let mut routes = Vec::new();
    let tok = |i: usize| tokens.get(i).map(|t| &t.0);
    for (index, (token, _)) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(tok);
        match token {
            Tok::Punct('(') => {
                let agent = matches!(previous, Some(Tok::Ident(name)) if name == "agent")
                    && !matches!(index.checked_sub(2).and_then(tok), Some(Tok::Punct('.')));
                stack.push(Open::Paren { agent, commas: 0 });
            }
            Tok::Punct('[') => {
                // A computed key could name the model or provider at run time.
                if matches!(previous, Some(Tok::Punct('{' | ',')))
                    && let Some(Open::Brace(frame)) = stack.last_mut()
                {
                    frame.spread = true;
                }
                stack.push(Open::Bracket);
            }
            Tok::Punct(')') => {
                if matches!(stack.last(), Some(Open::Paren { .. })) {
                    stack.pop();
                }
            }
            Tok::Punct(']') => {
                if matches!(stack.last(), Some(Open::Bracket)) {
                    stack.pop();
                }
            }
            Tok::Punct(',') => {
                if let Some(Open::Paren { commas, .. }) = stack.last_mut() {
                    *commas += 1;
                }
            }
            Tok::Punct('.')
                if matches!(previous, Some(Tok::Punct('{' | ',')))
                    && tok(index + 1) == Some(&Tok::Punct('.'))
                    && tok(index + 2) == Some(&Tok::Punct('.')) =>
            {
                if let Some(Open::Brace(frame)) = stack.last_mut() {
                    frame.spread = true;
                }
            }
            Tok::Punct('{') => {
                let call_options = matches!(previous, Some(Tok::Punct(',')))
                    && matches!(
                        stack.last(),
                        Some(Open::Paren {
                            agent: true,
                            commas: 1
                        })
                    );
                stack.push(Open::Brace(Frame {
                    call_options,
                    ..Frame::default()
                }));
            }
            Tok::Punct('}') => {
                let Some(Open::Brace(frame)) = stack.pop_if(|o| matches!(o, Open::Brace(_))) else {
                    continue;
                };
                if !frame.call_options || frame.other_profile || frame.spread {
                    continue;
                }
                if let (Some(Value::Literal(model, line)), Some(Value::Literal(provider, _))) =
                    (frame.model, frame.provider)
                {
                    routes.push(ScriptRoute {
                        line,
                        model,
                        provider,
                    });
                }
            }
            Tok::Ident(key) | Tok::Str(key)
                if matches!(key.as_str(), "model" | "provider" | "profile") =>
            {
                let after_open = matches!(previous, Some(Tok::Punct('{' | ',')));
                let Some(Open::Brace(frame)) = stack.last_mut().filter(|_| after_open) else {
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
/// with the nearest known spelling when there is one. Each pair is judged as written, so a
/// problem here is one the run would meet when that agent starts.
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
        let provider = &route.provider;
        let a = roster.availability(provider, &route.model, roster.taken_at());
        let Some(reason) = unusable(&a) else {
            continue;
        };
        let hint = match a {
            Availability::Missing => {
                nearest(provider, roster.providers().iter().map(|p| p.slug.as_str()))
                    .map(|name| format!(" (nearest known provider: '{}')", display_safe(name)))
            }
            Availability::NotOffered => {
                let here = roster
                    .providers()
                    .iter()
                    .find(|p| p.slug == *provider)
                    .and_then(|p| nearest(&route.model, p.models.iter().map(String::as_str)));
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
                    (None, false) => Some(format!(" (offered by {})", elsewhere.join(", "))),
                    (None, true) => None,
                }
            }
            _ => None,
        };
        let problem = format!(
            "model '{model}' on '{}' is not usable: {reason}{}",
            display_safe(provider),
            hint.unwrap_or_default()
        );
        problems.push(format!("script line {}: {problem}", route.line));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn route(line: u32, model: &str, provider: &str) -> ScriptRoute {
        ScriptRoute {
            line,
            model: model.into(),
            provider: provider.into(),
        }
    }

    #[test]
    fn literal_pairs_are_found_wherever_the_options_sit() {
        let source = "const a = await agent('x', {label: 'a', model: 'm-one', provider: 'p-one'});\n\
            const b = await agent(`y ${n}`, {'model': \"m-two\", \"provider\": 'p-two',});\n\
            const c = await parallel([() => agent('z', {model: 'm-three', provider: 'p-three'})]);";
        assert_eq!(
            script_routes(source),
            vec![
                route(1, "m-one", "p-one"),
                route(2, "m-two", "p-two"),
                route(3, "m-three", "p-three"),
            ]
        );
    }

    #[test]
    fn a_provider_the_check_cannot_read_leaves_the_call_to_the_run() {
        let source = "const route = {provider: 'beta-cloud'};\n\
            await agent('a', {...route, model: 'shared'});\n\
            await agent('b', {model: 'shared', provider: 'alpha-cloud', ...route});\n\
            const o = {model: 'shared'};\n\
            o.provider = 'beta-cloud';\n\
            await agent('c', o);\n\
            const p = {model: 'm-smal', provider: 'alpha-cloud'};\n\
            p.model = 'm-small';\n\
            await agent('d', p);\n\
            await agent('e', {model: 'shared'});\n\
            await agent('f', {model: 'shared', provider: pick()});\n\
            await agent('g', {schema: {model: 'x', provider: 'y'}});\n\
            await tool.agent('h', {model: 'x', provider: 'y'});\n\
            await agent('i', {model: 'x', provider: 'y'}, {model: 'x2', provider: 'y2'});\n\
            await agent(pick({model: 'x', provider: 'y'}), {model: 'm-large', provider: 'alpha-cloud'});\n\
            await agent('j', {model: 'x', provider: 'y', [key]: 'z'});";
        let routes = script_routes(source);
        assert_eq!(
            routes,
            vec![route(14, "x", "y"), route(15, "m-large", "alpha-cloud"),]
        );
        // Only the literal pairs are judged.
        assert_eq!(
            check_script_routes(&roster(), &routes),
            ["script line 14: model 'x' on 'y' is not usable: provider is not configured"]
        );
    }

    #[test]
    fn names_that_are_not_routes_are_ignored() {
        let source = "// agent('c', {model: 'in-a-comment', provider: 'p'})\n\
            const text = \"agent('s', {model: 'in-a-string', provider: 'p'})\";\n\
            const t = `${agent('t', {model: 'in-a-template', provider: 'p'})}`;\n\
            const re = /agent('r', {model: 'x', provider: 'p'})/;\n\
            await agent('d', {model: pick(), provider: 'p'});\n\
            await agent('s', {model, provider});\n\
            await agent('j', {model: 'a' + 'b', provider: 'p'});\n\
            await agent('o', {model: 'm', provider: 'p', profile: 'elsewhere'});\n\
            await agent('n', {nested: {model: 'm-nested', provider: 'p-nested'}});\n\
            await agent('ok', {model: 'm-ok', provider: 'p-ok'});\n";
        assert_eq!(script_routes(source), vec![route(10, "m-ok", "p-ok")]);
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
                route(3, "m-smal", "alpha-cloud"),
                route(4, "m-smal", "alpha-cloud"),
                route(5, "m-small", "alpha-clod"),
                route(6, "g-one", "gamma-cloud"),
                route(7, "m-large", "beta-cloud"),
                route(11, "m-small", "alpha-cloud"),
            ],
        );
        assert_eq!(
            problems,
            vec![
                "script line 3: model 'm-smal' on 'alpha-cloud' is not usable: model is not offered (nearest known spelling: 'm-small')",
                "script line 5: model 'm-small' on 'alpha-clod' is not usable: provider is not configured (nearest known provider: 'alpha-cloud')",
                "script line 6: model 'g-one' on 'gamma-cloud' is not usable: provider is not signed in",
                "script line 7: model 'm-large' on 'beta-cloud' is not usable: model is not offered (offered by alpha-cloud)",
            ]
        );
    }
}
