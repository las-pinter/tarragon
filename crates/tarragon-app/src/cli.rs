//! Command-line parsing. The parser is pure so tests can call it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The usage line.
const USAGE: &str = "usage: tarragon [-h] [--version] [FOLDER]";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Start the application, and open the folder when one is given.
    Run(Option<PathBuf>),
    /// Print the version and exit.
    Version,
    /// Print the help and exit.
    Help,
    /// An argument that is not a known flag.
    Unknown(OsString),
}

/// Read the arguments after the program name.
///
/// The first argument is a known flag or a folder. Anything else that starts
/// with a dash is unknown, and so is a second argument. A name that is not
/// UTF-8 is a folder, because a Unix path may be any bytes.
pub fn parse<I>(args: I) -> Command
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(arg) = args.next() else {
        return Command::Run(None);
    };
    match arg.to_str() {
        Some("-h" | "--help") => return Command::Help,
        Some("--version") => return Command::Version,
        Some(text) if text.starts_with('-') => return Command::Unknown(arg),
        _ => {}
    }
    match args.next() {
        Some(extra) => Command::Unknown(extra),
        None => Command::Run(Some(PathBuf::from(arg))),
    }
}

/// Make the folder argument absolute, or say why it cannot open. The check
/// runs before any window, so a typo ends the start at once.
///
/// The path is made absolute and not canonical, so a link stays a link and
/// the database keys do not change with the working folder.
///
/// # Errors
///
/// Returns the text for stderr when the path is not a directory.
pub fn check_folder(folder: &Path) -> Result<PathBuf, String> {
    if !folder.is_dir() {
        return Err(format!(
            "{USAGE}\ntarragon: error: not a folder: {}\n",
            folder.display()
        ));
    }
    std::path::absolute(folder).map_err(|error| {
        format!(
            "tarragon: error: cannot resolve {}: {error}\n",
            folder.display()
        )
    })
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
         arguments:\n  \
         FOLDER      open this folder and show its thumbnails\n\n\
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
        assert_eq!(parse_strs(&[]), Command::Run(None));
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
    fn one_positional_is_the_folder() {
        assert_eq!(
            parse_strs(&["pics"]),
            Command::Run(Some(PathBuf::from("pics")))
        );
    }

    #[test]
    fn second_positional_is_unknown() {
        assert_eq!(
            parse_strs(&["pics", "more"]),
            Command::Unknown(OsString::from("more"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_argument_is_a_folder() {
        use std::os::unix::ffi::OsStringExt;
        let arg = OsString::from_vec(vec![0xff, 0xfe]);
        assert_eq!(parse([arg.clone()]), Command::Run(Some(PathBuf::from(arg))));
    }

    #[test]
    fn a_directory_passes_the_check_as_an_absolute_path() {
        let dir = std::env::temp_dir();
        assert_eq!(check_folder(&dir), Ok(dir));
    }

    #[test]
    fn a_missing_path_fails_the_check() {
        let missing = std::env::temp_dir().join("tarragon-cli-no-such-folder");
        let text = check_folder(&missing).expect_err("a missing path");
        assert!(text.contains("not a folder"));
        assert!(text.contains("tarragon-cli-no-such-folder"));
    }

    #[test]
    fn a_file_fails_the_check() {
        let file = std::env::temp_dir().join(format!("tarragon-cli-file-{}", std::process::id()));
        std::fs::write(&file, b"x").expect("write a file");
        let result = check_folder(&file);
        let _ = std::fs::remove_file(&file);
        assert!(result.expect_err("a file").contains("not a folder"));
    }

    #[test]
    fn help_text_lists_the_folder_and_both_options() {
        let text = help_text();
        assert!(text.starts_with("usage: tarragon [-h] [--version] [FOLDER]\n"));
        assert!(text.contains("FOLDER"));
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
