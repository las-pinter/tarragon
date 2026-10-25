//! Command-line parsing. The parser is pure so tests can call it.

use std::ffi::{OsStr, OsString};

/// The usage line.
const USAGE: &str = "usage: tarragon [-h] [--version]";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Start the application.
    Run,
    /// Print the version and exit.
    Version,
    /// Print the help and exit.
    Help,
    /// An argument that is not a known flag.
    Unknown(OsString),
}

/// Read the arguments after the program name.
///
/// Only the first argument counts: a known flag selects its command and any
/// other argument is unknown.
pub fn parse<I>(args: I) -> Command
where
    I: IntoIterator<Item = OsString>,
{
    match args.into_iter().next() {
        None => Command::Run,
        Some(arg) => match arg.to_str() {
            Some("-h" | "--help") => Command::Help,
            Some("--version") => Command::Version,
            _ => Command::Unknown(arg),
        },
    }
}

/// The text printed for `--version`.
pub fn version_text() -> String {
    format!("tarragon {}", env!("CARGO_PKG_VERSION"))
}

/// The text printed for `--help`.
pub fn help_text() -> String {
    format!(
        "{USAGE}\n\n\
         Tarragon image browser and cataloging application.\n\n\
         options:\n  \
         -h, --help  show this help message and exit\n  \
         --version   Show the version and exit.\n"
    )
}

/// The text printed to stderr for an unknown argument.
pub fn error_text(arg: &OsStr) -> String {
    format!(
        "{USAGE}\ntarragon: error: unrecognized arguments: {}\n",
        arg.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Command {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_run_the_app() {
        assert_eq!(parse_strs(&[]), Command::Run);
    }

    #[test]
    fn version_flag_is_recognized() {
        assert_eq!(parse_strs(&["--version"]), Command::Version);
    }

    #[test]
    fn short_and_long_help_are_recognized() {
        assert_eq!(parse_strs(&["-h"]), Command::Help);
        assert_eq!(parse_strs(&["--help"]), Command::Help);
    }

    #[test]
    fn unknown_flag_is_reported_by_name() {
        assert_eq!(
            parse_strs(&["--bogus"]),
            Command::Unknown(OsString::from("--bogus"))
        );
    }

    #[test]
    fn extra_positional_is_unknown() {
        assert_eq!(
            parse_strs(&["file.png"]),
            Command::Unknown(OsString::from("file.png"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_argument_is_unknown() {
        use std::os::unix::ffi::OsStringExt;
        let arg = OsString::from_vec(vec![0xff, 0xfe]);
        assert_eq!(parse([arg.clone()]), Command::Unknown(arg));
    }

    #[test]
    fn help_text_lists_both_options() {
        let text = help_text();
        assert!(text.starts_with("usage: tarragon [-h] [--version]\n"));
        assert!(text.contains("--help"));
        assert!(text.contains("--version"));
    }

    #[test]
    fn error_text_names_the_argument() {
        let text = error_text(OsStr::new("--bogus"));
        assert!(text.starts_with("usage:"));
        assert!(text.contains("--bogus"));
    }
}
