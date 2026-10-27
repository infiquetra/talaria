use super::{InstallInputError, InstallTarget};

pub fn classify_install_input(input: &str) -> Result<InstallTarget, InstallInputError> {
    let input = input.trim();
    if (1..=64).contains(&input.len())
        && input
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Ok(InstallTarget::CatalogName(input.to_owned()));
    }
    let rest = input.strip_prefix("https://").ok_or(InstallInputError)?;
    if !rest.bytes().all(|b| (b'!'..=b'~').contains(&b))
        || rest.contains(['@', '?', '#', '\\', '[', ']'])
    {
        return Err(InstallInputError);
    }
    let (authority, path) = rest.split_once('/').ok_or(InstallInputError)?;
    if path.is_empty() {
        return Err(InstallInputError);
    }
    let host = if let Some((host, port)) = authority.split_once(':') {
        if !(1..=5).contains(&port.len()) || !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(InstallInputError);
        }
        match port.parse::<u16>() {
            Ok(n) if n > 0 => {}
            _ => return Err(InstallInputError),
        }
        host
    } else {
        authority
    };
    if host.split('.').any(|label| {
        label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err(InstallInputError);
    }
    Ok(InstallTarget::Repository(input.to_owned()))
}

pub fn redact_paths(text: &str, hermes_home: &str, user_home: &str) -> String {
    let mut result = text.to_owned();
    for (home, replacement) in [(hermes_home, "$HERMES_HOME"), (user_home, "~")] {
        if !home.is_empty() && home != "/" {
            result = result.replace(home, replacement);
        }
    }
    result
}
