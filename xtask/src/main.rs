//! Repository maintenance tasks. The only task is `attribution`.

use std::error::Error;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const SVG_PATH: &str = "assets/attribution.svg";
const BINARY_SCAN_BYTES: u64 = 8000;
const BAR_WIDTH: u64 = 560;
const AI_COLOUR: &str = "#6639ba";
const HUMAN_COLOUR: &str = "#1a7f37";

/// Line counts for one repository state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Counts {
    ai: u64,
    human: u64,
}

impl Counts {
    fn total(self) -> u64 {
        self.ai + self.human
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match (args.next().as_deref(), args.next()) {
        (Some("attribution"), None) => attribution(),
        _ => Err("usage: cargo xtask attribution".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn attribution() -> Result<()> {
    let root = git(None, &["rev-parse", "--show-toplevel"])?;
    let root = Path::new(root.trim_end());
    let listing = git(Some(root), &["ls-files", "-z"])?;

    let mut counts = Counts::default();
    for path in listing.split('\0').filter(|path| !path.is_empty()) {
        if path.starts_with("assets/") || !is_text_file(&root.join(path))? {
            continue;
        }
        let blame = git(Some(root), &["blame", "--line-porcelain", "--", path])?;
        for summary in blame_summaries(&blame) {
            if is_ai_heading(summary) {
                counts.ai += 1;
            } else {
                counts.human += 1;
            }
        }
    }

    let svg_path = root.join(SVG_PATH);
    if let Some(dir) = svg_path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&svg_path, build_svg(counts))?;

    let (ai_percent, human_percent) = percent_pair(counts);
    println!(
        "attribution: {}, {} AI ({ai_percent}%), {} human ({human_percent}%), wrote {SVG_PATH}",
        lines(counts.total()),
        counts.ai,
        counts.human,
    );
    Ok(())
}

/// Runs git and returns stdout. `dir` is the repository root once it is known.
fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let output = command.args(args).output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), message.trim()).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Tracked paths can be symlinks, submodules, or absent; only a real file can be blamed.
fn is_text_file(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(!is_binary(path)?),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// A NUL byte in the first bytes marks a file as binary, as git does.
fn is_binary(path: &Path) -> Result<bool> {
    let mut head = Vec::new();
    fs::File::open(path)?
        .take(BINARY_SCAN_BYTES)
        .read_to_end(&mut head)?;
    Ok(head.contains(&0))
}

/// Returns the commit summary of each blamed line, in file order.
///
/// Lines not yet committed carry a non-AI summary, so they count as human.
fn blame_summaries(porcelain: &str) -> Vec<&str> {
    let mut summaries = Vec::new();
    let mut current = "";
    for line in porcelain.lines() {
        if let Some(summary) = line.strip_prefix("summary ") {
            current = summary;
        } else if line.starts_with('\t') {
            summaries.push(current);
        }
    }
    summaries
}

fn is_ai_heading(heading: &str) -> bool {
    heading.starts_with("ai:") || heading.starts_with("ai-")
}

/// Both percentages with one decimal, rounded half up. They sum to 100.0.
/// A part that is not an end value but rounds to one shows as `<0.1` or `>99.9`.
fn percent_pair(counts: Counts) -> (String, String) {
    let total = counts.total();
    if total == 0 {
        return ("0.0".into(), "0.0".into());
    }
    let ai_tenths = (counts.ai * 1000 + total / 2) / total;
    (
        percent_label(counts.ai, total, ai_tenths),
        percent_label(counts.human, total, 1000 - ai_tenths),
    )
}

fn percent_label(part: u64, total: u64, tenths: u64) -> String {
    if part > 0 && tenths == 0 {
        "<0.1".into()
    } else if part < total && tenths == 1000 {
        ">99.9".into()
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

fn lines(count: u64) -> String {
    if count == 1 {
        "1 line".into()
    } else {
        format!("{count} lines")
    }
}

/// SVG text must not hold a raw `<` or `>`.
fn escape(text: &str) -> String {
    text.replace('<', "&lt;").replace('>', "&gt;")
}

/// The SVG has its own background so it reads on light and dark pages.
fn build_svg(counts: Counts) -> String {
    let total = counts.total();
    let mut ai_width = (BAR_WIDTH * counts.ai + total / 2)
        .checked_div(total)
        .unwrap_or(0);
    // A non-zero part must stay visible.
    if counts.ai > 0 {
        ai_width = ai_width.max(1);
    }
    if counts.human > 0 {
        ai_width = ai_width.min(BAR_WIDTH - 1);
    }
    let human_width = if total == 0 { 0 } else { BAR_WIDTH - ai_width };
    let human_x = 20 + ai_width;
    let (ai_percent, human_percent) = percent_pair(counts);
    let ai_label = escape(&format!("AI: {ai_percent}% ({})", lines(counts.ai)));
    let human_label = escape(&format!(
        "Human: {human_percent}% ({})",
        lines(counts.human)
    ));
    let description = format!("{ai_label}; {human_label}; {} in total", lines(total));

    let mut rows = vec![
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"600\" height=\"130\" ",
            "viewBox=\"0 0 600 130\" role=\"img\" aria-labelledby=\"title desc\">"
        )
        .to_string(),
        "<title id=\"title\">AI and human lines</title>".to_string(),
        format!("<desc id=\"desc\">{description}</desc>"),
        concat!(
            "<rect x=\"0.5\" y=\"0.5\" width=\"599\" height=\"129\" rx=\"8\" ",
            "fill=\"#ffffff\" stroke=\"#d0d7de\"/>"
        )
        .to_string(),
        "<g font-family=\"Verdana, DejaVu Sans, sans-serif\" fill=\"#1f2328\">".to_string(),
        "<text x=\"20\" y=\"30\" font-size=\"16\" font-weight=\"bold\">AI and human lines</text>"
            .to_string(),
        format!(
            "<text x=\"580\" y=\"30\" font-size=\"13\" text-anchor=\"end\">{}</text>",
            lines(total)
        ),
        "<rect x=\"20\" y=\"46\" width=\"560\" height=\"28\" fill=\"#eaeef2\"/>".to_string(),
    ];
    if ai_width > 0 {
        rows.push(format!(
            "<rect x=\"20\" y=\"46\" width=\"{ai_width}\" height=\"28\" fill=\"{AI_COLOUR}\"/>"
        ));
    }
    if human_width > 0 {
        rows.push(format!(
            "<rect x=\"{human_x}\" y=\"46\" width=\"{human_width}\" height=\"28\" \
             fill=\"{HUMAN_COLOUR}\"/>"
        ));
    }
    rows.extend([
        format!("<rect x=\"20\" y=\"95\" width=\"14\" height=\"14\" fill=\"{AI_COLOUR}\"/>"),
        format!("<text x=\"42\" y=\"107\" font-size=\"13\">{ai_label}</text>"),
        format!("<rect x=\"300\" y=\"95\" width=\"14\" height=\"14\" fill=\"{HUMAN_COLOUR}\"/>"),
        format!("<text x=\"322\" y=\"107\" font-size=\"13\">{human_label}</text>"),
        "</g>".to_string(),
        "</svg>".to_string(),
    ]);
    rows.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_headings_are_detected() {
        assert!(is_ai_heading("ai: feat(core): add a thing"));
        assert!(is_ai_heading("ai- legacy heading"));
        assert!(!is_ai_heading("feat: add a thing"));
        assert!(!is_ai_heading("AI: upper case"));
        assert!(!is_ai_heading("main: fix ai: inside"));
        assert!(!is_ai_heading(""));
    }

    #[test]
    fn blame_summaries_follow_line_order() {
        let porcelain = "aaaa 1 1 1\nsummary ai: one\nfilename f\n\tfirst\n\
                         bbbb 2 2 1\nsummary fix: two\nfilename f\n\tsummary 9\n";
        assert_eq!(blame_summaries(porcelain), ["ai: one", "fix: two"]);
    }

    #[test]
    fn percent_pair_sums_to_100() {
        let pair = percent_pair(Counts { ai: 1, human: 2 });
        assert_eq!(pair, ("33.3".into(), "66.7".into()));
        let pair = percent_pair(Counts { ai: 5, human: 0 });
        assert_eq!(pair, ("100.0".into(), "0.0".into()));
        assert_eq!(
            percent_pair(Counts::default()),
            ("0.0".into(), "0.0".into())
        );
    }

    #[test]
    fn tiny_parts_show_as_less_than_a_tenth() {
        let (ai, human) = percent_pair(Counts { ai: 1, human: 9999 });
        assert_eq!((ai.as_str(), human.as_str()), ("<0.1", ">99.9"));
        let (ai, human) = percent_pair(Counts { ai: 9999, human: 1 });
        assert_eq!((ai.as_str(), human.as_str()), (">99.9", "<0.1"));
    }

    #[test]
    fn lines_is_singular_for_one() {
        assert_eq!(lines(0), "0 lines");
        assert_eq!(lines(1), "1 line");
        assert_eq!(lines(2), "2 lines");
    }

    #[test]
    fn svg_shows_counts_and_both_parts() {
        let svg = build_svg(Counts { ai: 1, human: 3 });
        assert!(svg.contains("AI: 25.0% (1 line)"));
        assert!(svg.contains("Human: 75.0% (3 lines)"));
        assert!(svg.contains(">4 lines</text>"));
        assert!(svg.contains("width=\"140\" height=\"28\" fill=\"#6639ba\""));
        assert!(svg.contains("x=\"160\" y=\"46\" width=\"420\""));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn svg_keeps_tiny_parts_visible_and_escaped() {
        let svg = build_svg(Counts {
            ai: 1,
            human: 99_999,
        });
        assert!(svg.contains("AI: &lt;0.1% (1 line)"));
        assert!(svg.contains("Human: &gt;99.9% (99999 lines)"));
        assert!(svg.contains("width=\"1\" height=\"28\" fill=\"#6639ba\""));
        let svg = build_svg(Counts {
            ai: 99_999,
            human: 1,
        });
        assert!(svg.contains("x=\"579\" y=\"46\" width=\"1\""));
    }

    #[test]
    fn svg_skips_empty_parts() {
        let svg = build_svg(Counts { ai: 0, human: 7 });
        assert!(!svg.contains("width=\"0\""));
        assert!(svg.contains("AI: 0.0% (0 lines)"));
        let empty = build_svg(Counts::default());
        assert!(empty.contains(">0 lines</text>"));
    }

    #[test]
    fn svg_is_deterministic() {
        let counts = Counts { ai: 12, human: 34 };
        assert_eq!(build_svg(counts), build_svg(counts));
    }

    #[test]
    fn only_regular_text_files_count() {
        let dir = std::env::temp_dir().join(format!("xtask-filter-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("text.txt"), "hello\n").unwrap();
        fs::write(dir.join("bin.dat"), [1, 0, 2]).unwrap();
        assert!(is_text_file(&dir.join("text.txt")).unwrap());
        assert!(!is_text_file(&dir.join("bin.dat")).unwrap());
        assert!(!is_text_file(&dir.join("missing")).unwrap());
        assert!(!is_text_file(&dir.join("sub")).unwrap());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("gone"), dir.join("dangling")).unwrap();
            std::os::unix::fs::symlink(dir.join("sub"), dir.join("dirlink")).unwrap();
            assert!(!is_text_file(&dir.join("dangling")).unwrap());
            assert!(!is_text_file(&dir.join("dirlink")).unwrap());
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
