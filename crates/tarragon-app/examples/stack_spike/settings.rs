//! Command-line settings of the spike.

use std::time::Duration;

/// The renderer that the run uses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// The `glow` (OpenGL) renderer.
    Glow,
    /// The `wgpu` renderer.
    Wgpu,
}

impl Choice {
    /// The name that the flag and the log use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Glow => "glow",
            Self::Wgpu => "wgpu",
        }
    }
}

/// The settings from the command line.
#[derive(Clone)]
pub struct Settings {
    /// The renderer.
    pub renderer: Choice,
    /// The measured time.
    pub duration: Duration,
    /// The warm-up time.
    pub warmup: Duration,
    /// The number of live textures.
    pub textures: usize,
    /// The log file.
    pub out: String,
}

/// Read the command line. The error text is the message for the user.
pub fn parse_args() -> Result<Settings, String> {
    let mut renderer = Choice::Wgpu;
    let mut duration = 30.0_f64;
    let mut warmup = 5.0_f64;
    let mut textures = 1000_usize;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--renderer" => {
                renderer = match value("--renderer")?.as_str() {
                    "glow" => Choice::Glow,
                    "wgpu" => Choice::Wgpu,
                    other => return Err(format!("unknown renderer: {other}")),
                };
            }
            "--duration" => duration = parse_number("--duration", &value("--duration")?)?,
            "--warmup" => warmup = parse_number("--warmup", &value("--warmup")?)?,
            "--textures" => {
                textures = value("--textures")?
                    .parse()
                    .map_err(|_| "--textures needs a whole number".to_owned())?;
            }
            "--out" => out = Some(value("--out")?),
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(Settings {
        renderer,
        duration: Duration::from_secs_f64(duration),
        warmup: Duration::from_secs_f64(warmup),
        textures: textures.max(1),
        out: out.unwrap_or_else(|| format!("frame-time-{}.log", renderer.name())),
    })
}

/// Read a number of seconds, zero or more.
fn parse_number(name: &str, text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(number) if number.is_finite() && number >= 0.0 => Ok(number),
        _ => Err(format!("{name} needs a number of seconds")),
    }
}
