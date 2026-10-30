//! Conservative lexical checks for result-dependent graph changes.
use std::collections::BTreeSet;
#[derive(Debug)]
struct Token {
    text: String,
    at: usize,
}
fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut template = false;
    let mut interpolation = Vec::new();
    let mut braces = 0usize;
    while i < bytes.len() {
        if template {
            if bytes[i] == b'\\' {
                i = (i + 2).min(bytes.len());
                continue;
            }
            if bytes[i] == b'`' {
                template = false;
                i += 1;
                continue;
            }
            if bytes[i..].starts_with(b"${") {
                interpolation.push(braces);
                template = false;
                i += 2;
                braces += 1;
                continue;
            }
            i += 1;
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        let at = i;
        if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
            }
            i = (i + 1).min(bytes.len());
            out.push(Token {
                text: "<string>".into(),
                at,
            });
            continue;
        }
        if bytes[i] == b'`' {
            template = true;
            i += 1;
            out.push(Token {
                text: "<template>".into(),
                at,
            });
            continue;
        }
        if bytes[i] == b'{' {
            braces += 1;
        }
        if bytes[i] == b'}' {
            braces = braces.saturating_sub(1);
            if interpolation.last() == Some(&braces) {
                interpolation.pop();
                template = true;
                i += 1;
                continue;
            }
        }
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$' {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
        } else {
            i += 1;
            if i < bytes.len()
                && matches!(
                    &bytes[at..i + 1],
                    b"=>" | b"==" | b"!=" | b">=" | b"<=" | b"&&" | b"||" | b"++" | b"--"
                )
            {
                i += 1;
            }
        }
        out.push(Token {
            text: String::from_utf8_lossy(&bytes[at..i]).into_owned(),
            at,
        });
    }
    out
}
fn end(t: &[Token], open: usize) -> Option<usize> {
    let (left, right) = match t.get(open)?.text.as_str() {
        "(" => ("(", ")"),
        "[" => ("[", "]"),
        "{" => ("{", "}"),
        _ => return None,
    };
    let mut depth = 0;
    for (i, item) in t.iter().enumerate().skip(open) {
        if item.text == left {
            depth += 1;
        }
        if item.text == right {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}
fn args(t: &[Token], open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = open + 1;
    let mut i = start;
    while i < close {
        if matches!(t[i].text.as_str(), "(" | "[" | "{")
            && let Some(next) = end(t, i)
        {
            i = next + 1;
            continue;
        }
        if t[i].text == "," {
            spans.push((start, i));
            start = i + 1;
        }
        i += 1;
    }
    spans.push((start, close));
    spans
}
pub(super) fn check(source: &str) -> Result<(), String> {
    let t = tokens(source);
    let mut tracked = BTreeSet::new();
    let mut bindings = BTreeSet::new();
    let refusal = |rule: &str, i: usize, detail: &str| {
        Err(format!(
            "not compilable: {rule} at line {}: {detail}; run it with talaria workflow run instead",
            source[..t[i].at].bytes().filter(|c| *c == b'\n').count() + 1
        ))
    };
    let is_result = |s: &str| matches!(s, "agent" | "parallel" | "pipeline");
    for i in 0..t.len() {
        if matches!(t[i].text.as_str(), "now" | "random")
            && t.get(i + 1).is_some_and(|x| x.text == "(")
        {
            return refusal(
                "G5 (helpers)",
                i,
                "clock and random calls cannot be compiled",
            );
        }
        if matches!(t[i].text.as_str(), "const" | "let" | "var") {
            let Some(equal) = (i + 1..t.len())
                .take_while(|j| t[*j].text != ";")
                .find(|j| t[*j].text == "=")
            else {
                continue;
            };
            let from = equal + 1 + usize::from(t.get(equal + 1).is_some_and(|v| v.text == "await"));
            if t.get(from).is_some_and(|v| is_result(&v.text)) {
                for (j, token) in t.iter().enumerate().take(equal).skip(i + 1) {
                    if token
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                    {
                        tracked.insert(token.text.clone());
                        bindings.insert(j);
                    }
                }
            }
        }
    }
    // One-hop aliases cannot hide a value from subsequent condition checks.
    for i in 0..t.len().saturating_sub(2) {
        if t[i + 1].text == "=" {
            let from = i + 2 + usize::from(t[i + 2].text == "await");
            if t.get(from).is_some_and(|v| is_result(&v.text)) {
                tracked.insert(t[i].text.clone());
                bindings.insert(i);
            }
        }
    }
    let originals = tracked.clone();
    let mut args_variables = BTreeSet::new();
    for i in 0..t.len().saturating_sub(3) {
        if matches!(t[i].text.as_str(), "const" | "let" | "var")
            && t[i + 2].text == "="
            && t[i + 3].text == "args"
        {
            args_variables.insert(t[i + 1].text.clone());
        }
    }
    for i in 0..t.len().saturating_sub(3) {
        if matches!(t[i].text.as_str(), "const" | "let" | "var")
            && t[i + 2].text == "="
            && originals.contains(&t[i + 3].text)
        {
            tracked.insert(t[i + 1].text.clone());
            bindings.insert(i + 1);
        }
    }
    for i in 0..t.len().saturating_sub(2) {
        if t[i + 1].text == "=" && originals.contains(&t[i + 2].text) {
            tracked.insert(t[i].text.clone());
            bindings.insert(i);
        }
    }
    let mut prompts = Vec::new();
    let mut options = Vec::new();
    let mut controls = Vec::new();
    for i in 0..t.len().saturating_sub(1) {
        if t[i + 1].text != "(" {
            continue;
        }
        let Some(close) = end(&t, i + 1) else {
            continue;
        };
        let spans = args(&t, i + 1, close);
        match t[i].text.as_str() {
            "agent" => {
                if let Some(p) = spans.first() {
                    prompts.push(*p);
                }
                if let Some(p) = spans.get(1) {
                    options.push(*p);
                }
            }
            "if" | "while" | "for" | "switch" => {
                controls.push((i + 2, close));
                if t[i].text == "for" {
                    let body_end = if t.get(close + 1).is_some_and(|x| x.text == "{") {
                        end(&t, close + 1).unwrap_or(close + 1)
                    } else {
                        t.len()
                    };
                    let has_agent = t[close + 1..body_end].iter().any(|v| v.text == "agent");
                    let known = t[i + 2..close].iter().any(|v| {
                        v.text == "args"
                            || args_variables.contains(&v.text)
                            || v.text.parse::<u64>().is_ok()
                    });
                    if has_agent && !known {
                        return refusal(
                            "G6 (shape)",
                            i,
                            "call count has an unclassifiable loop bound",
                        );
                    }
                }
            }
            "pipeline" => {
                for (start, stop) in spans.iter().skip(1) {
                    let mut j = *start;
                    if t.get(j).is_some_and(|v| v.text == "async") {
                        j += 1;
                    }
                    if t.get(j).is_some_and(|v| v.text == "(") {
                        j += 1;
                    }
                    if j < *stop {
                        tracked.insert(t[j].text.clone());
                        bindings.insert(j);
                    }
                }
            }
            _ => {}
        }
    }
    let inside = |spans: &[(usize, usize)], i| spans.iter().any(|(a, b)| *a <= i && i < *b);
    for i in 0..t.len() {
        if !tracked.contains(&t[i].text) || bindings.contains(&i) {
            continue;
        }
        if inside(&controls, i) {
            return refusal("G1 (conditions)", i, "a result controls execution");
        }
        if inside(&options, i) {
            return refusal("G2 (options)", i, "a result changes call options");
        }
        let mut j = i + 1;
        while j + 1 < t.len() && t[j].text == "." {
            let member = &t[j + 1].text;
            if member == "length" {
                return refusal("G3 (computation)", i, "a result length changes computation");
            }
            if t.get(j + 2).is_some_and(|v| v.text == "(") {
                return refusal(
                    "G4 (transformation)",
                    i,
                    "a result method would destroy its marker",
                );
            }
            j += 2;
        }
        let prev = i.checked_sub(1).map(|n| t[n].text.as_str()).unwrap_or("");
        let next = t.get(j).map(|v| v.text.as_str()).unwrap_or("");
        if next == "?" {
            return refusal(
                "G1 (conditions)",
                i,
                "a result controls a conditional expression",
            );
        }
        if prev == "[" || next == "+" && t.get(j + 1).is_some_and(|v| v.text.parse::<f64>().is_ok())
        {
            return refusal(
                "G3 (computation)",
                i,
                "a result is used in arithmetic or as an index",
            );
        }
        if matches!(
            next,
            "*" | "/" | "%" | "-" | ">" | "<" | "==" | "!=" | ">=" | "<=" | "?" | "++" | "--"
        ) || matches!(prev, "*" | "/" | "%" | "-" | "!" | "[") && !inside(&prompts, i)
        {
            return refusal(
                "G3 (computation)",
                i,
                "a result is used in computation or indexing",
            );
        }
        if matches!(next, "+") && !inside(&prompts, i) {
            return refusal(
                "G3 (computation)",
                i,
                "a result is used outside prompt text",
            );
        }
        if i >= 4 && t[i - 4].text == "JSON" && t[i - 2].text == "stringify" {
            return refusal(
                "G4 (transformation)",
                i,
                "serialising a result can destroy its marker",
            );
        }
        if !inside(&prompts, i)
            && !matches!(prev, "void" | "=" | "await")
            && !matches!(next, ";" | "," | "(")
        {
            return refusal(
                "G6 (shape)",
                i,
                "result use cannot be classified as prompt interpolation",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;
    fn source(tail: &str) -> String {
        format!("const r = await agent('measure');\n{tail}")
    }
    #[test]
    fn refuses_result_controls_options_and_transformations() {
        for expression in [
            "if (r) await agent('branch');",
            "const alias = r; if (alias) await agent('branch');",
            "await agent('next', {model:r});",
            "await agent(`${r.length}`);",
            "await agent(`${r.toUpperCase()}`);",
            "await agent(`${r + 2}`);",
            "await agent(`${JSON.stringify(r)}`);",
            "await agent(table[r]);",
            "const next = r ? 'a' : 'b';",
        ] {
            assert!(check(&source(expression)).is_err(), "accepted {expression}");
        }
    }
    #[test]
    fn accepts_prompt_interpolation_and_identifier_boundaries() {
        for expression in [
            "await agent(`value ${r.field}`);",
            "await agent('value '+r);",
            "void r;",
        ] {
            assert!(check(&source(expression)).is_ok(), "refused {expression}");
        }
        assert!(check("const snow = () => 1; snow();").is_ok());
        assert!(check("Date.now();").is_err());
        assert!(check("random();").is_err());
    }
}
