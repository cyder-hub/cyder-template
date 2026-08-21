use std::ffi::OsString;

pub const HELP: &str = concat!(
    "Usage:\n  ",
    env!("CARGO_PKG_NAME"),
    "\n  ",
    env!("CARGO_PKG_NAME"),
    " config endpoint --format json\n  ",
    env!("CARGO_PKG_NAME"),
    " config check [--format json]\n  ",
    env!("CARGO_PKG_NAME"),
    " healthcheck\n  ",
    env!("CARGO_PKG_NAME"),
    " --help\n",
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    ConfigEndpointJson,
    ConfigCheck { format: OutputFormat },
    Healthcheck,
    Help,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}\n\n{HELP}")]
pub struct ParseError {
    message: String,
}

pub fn parse<I>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let utf8_args = args
        .iter()
        .map(|argument| argument.to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| ParseError {
            message: "command-line arguments must be valid UTF-8".to_string(),
        })?;

    match utf8_args.as_slice() {
        [] => Ok(Command::Serve),
        ["--help" | "-h"]
        | ["config", "--help" | "-h"]
        | ["config", "endpoint", "--help" | "-h"]
        | ["config", "check", "--help" | "-h"]
        | ["healthcheck", "--help" | "-h"] => Ok(Command::Help),
        ["config", "endpoint", "--format", "json"] => Ok(Command::ConfigEndpointJson),
        ["config", "check"] => Ok(Command::ConfigCheck {
            format: OutputFormat::Text,
        }),
        ["config", "check", "--format", "json"] => Ok(Command::ConfigCheck {
            format: OutputFormat::Json,
        }),
        ["healthcheck"] => Ok(Command::Healthcheck),
        _ => Err(ParseError {
            message: format!(
                "unsupported command-line arguments: {}",
                display_args(&args)
            ),
        }),
    }
}

fn display_args(args: &[OsString]) -> String {
    if args.is_empty() {
        return "<none>".to_string();
    }

    args.iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_starts_the_server() {
        assert_eq!(parse(args(&[])).unwrap(), Command::Serve);
    }

    #[test]
    fn endpoint_json_command_is_exact() {
        assert_eq!(
            parse(args(&["config", "endpoint", "--format", "json"])).unwrap(),
            Command::ConfigEndpointJson
        );

        let error = parse(args(&["config", "endpoint"])).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported command-line arguments")
        );
        assert!(error.to_string().contains("--format json"));
    }

    #[test]
    fn config_check_supports_text_and_json_formats() {
        assert_eq!(
            parse(args(&["config", "check"])).unwrap(),
            Command::ConfigCheck {
                format: OutputFormat::Text
            }
        );
        assert_eq!(
            parse(args(&["config", "check", "--format", "json"])).unwrap(),
            Command::ConfigCheck {
                format: OutputFormat::Json
            }
        );
    }

    #[test]
    fn healthcheck_command_is_exact() {
        assert_eq!(parse(args(&["healthcheck"])).unwrap(), Command::Healthcheck);
        assert!(parse(args(&["healthcheck", "extra"])).is_err());
    }

    #[test]
    fn help_is_available_at_each_command_level() {
        for values in [
            &["--help"][..],
            &["-h"][..],
            &["config", "--help"][..],
            &["config", "endpoint", "--help"][..],
            &["config", "check", "--help"][..],
            &["healthcheck", "--help"][..],
        ] {
            assert_eq!(parse(args(values)).unwrap(), Command::Help);
        }
    }

    #[test]
    fn unknown_arguments_never_fall_back_to_serving() {
        let error = parse(args(&["serve"])).unwrap_err();
        assert!(error.to_string().contains("serve"));
    }
}
