use std::process::ExitCode;

pub fn is_command(args: &[String]) -> bool {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "configure" => return true,
            "--profile" | "--provider" | "--model" => {
                if args.next().is_none() {
                    return false;
                }
            }
            value
                if ["--profile=", "--provider=", "--model="]
                    .iter()
                    .any(|prefix| value.starts_with(prefix)) => {}
            _ => return false,
        }
    }
    false
}

pub fn run(_args: &[String]) -> ExitCode {
    todo!("configure command")
}
