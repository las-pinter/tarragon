//! Binary entry point for the tarragon image browser.

mod app;
mod cli;
mod data_dir;
mod instance;
mod logging;
mod logview;
// The grid does not call the pipeline yet, so its items look unused.
#[allow(dead_code)]
mod pipeline;
mod run;
mod window;

use std::process::ExitCode;
use std::sync::Arc;

use app::App;

fn main() -> ExitCode {
    // Flags return before any I/O, so they never touch the data directory.
    match cli::parse(std::env::args_os().skip(1)) {
        cli::Command::Run => {}
        cli::Command::Version => {
            println!("{}", cli::version_text());
            return ExitCode::SUCCESS;
        }
        cli::Command::Help => {
            print!("{}", cli::help_text());
            return ExitCode::SUCCESS;
        }
        cli::Command::Unknown(arg) => {
            eprint!("{}", cli::error_text(&arg));
            return ExitCode::from(2);
        }
    }

    // The offset lookup must run before any other thread starts.
    let offset = logging::local_offset();
    let override_db = std::env::var_os("TARRAGON_DB");
    let db_path = match data_dir::db_path(override_db.clone()) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tarragon: {error}");
            return ExitCode::FAILURE;
        }
    };

    // A second start must not roll the running instance's log, so the guard
    // comes before the log file.
    let location = data_dir::log_location(override_db);
    if let Ok(found) = &location {
        logging::install_panic_hook(found.crash_dir.clone());
    }
    let outcome = logging::with_stderr(offset, || {
        let outcome = instance::acquire(&db_path);
        match &outcome {
            instance::Outcome::Forwarded => {
                tracing::info!("another instance is running; asked it to show its window");
            }
            instance::Outcome::Unguarded(reason) => {
                tracing::warn!("starting without a single-instance guard: {reason}");
            }
            instance::Outcome::Primary(_) => {}
        }
        outcome
    });
    let mut unguarded = None;
    let guard = match outcome {
        instance::Outcome::Primary(guard) => Some(Arc::new(guard)),
        // A second start ends here, before it touches the log or the database.
        instance::Outcome::Forwarded => return ExitCode::SUCCESS,
        instance::Outcome::Unguarded(reason) => {
            unguarded = Some(reason);
            None
        }
    };

    // Logging is best effort: a missing location only means no file log.
    let logging = logging::init(location.as_ref().ok().map(|l| l.log_file.as_path()), offset);
    if let Err(error) = &location {
        tracing::warn!("no log file or crash reports: {error}");
    }
    // Said on stderr above; say it again where the file and the Log panel see it.
    if let Some(reason) = unguarded {
        tracing::warn!("starting without a single-instance guard: {reason}");
    }

    // The guard stays alive here until the window closes.
    let open = || {
        App::open(
            &db_path,
            logging.ring.clone(),
            logging.control.clone(),
            guard.clone(),
        )
    };
    // Open once first, so a bad database ends the start before any window.
    let first = match open() {
        Ok(app) => app,
        Err(error) => {
            report(logging.installed, &format!("cannot start: {error}"));
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("tarragon {} started", env!("CARGO_PKG_VERSION"));
    match run::run(first, &open) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(logging.installed, &error.to_string());
            ExitCode::FAILURE
        }
    }
}

/// Tell the user about a fatal error.
fn report(logger_installed: bool, message: &str) {
    if logger_installed {
        tracing::error!("{message}");
    } else {
        // No logger is running, so stderr is the only way to tell the user.
        eprintln!("tarragon: {message}");
    }
}
