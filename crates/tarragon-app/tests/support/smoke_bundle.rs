//! A fake bundle folder for the smoke script tests: stub tools, CPU tables,
//! and canned perf logs. Two test files include this file.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A bundle of stub tools in a temp folder. It removes itself on drop.
pub struct Bundle {
    pub root: PathBuf,
}

/// Start-up lines of a good perf log.
const GOOD_LOAD: &str = "# perf log\nsession: wayland\nrenderer: wgpu\nwindow: 1280x800\n\
cold_start_ms: 1200\ncold_start_basis: spawn\npeak_rss_kib: 900000\nlive_textures: 1000\n";

/// The end lines of a good frame run.
const GOOD_END: &str = "frames: 1800\nspan_s: 30.00\np50_ms: 8.00\np95_ms: 12.00\n\
p99_ms: 16.00\nmax_ms: 40.00\nframe_verdict: PASS\nscroll_max_pt: 90000\n\
live_textures_at_end: 1000\nrun_valid: yes\n";

/// A good perf log of a cold start run.
pub fn good_load() -> String {
    GOOD_LOAD.to_owned()
}

/// A good perf log of a frame run.
pub fn good_frame() -> String {
    format!("{GOOD_LOAD}{GOOD_END}")
}

impl Bundle {
    /// Make a bundle. `bench` is the shell body of the stub `scan_bench`.
    pub fn new(name: &str, bench: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tarragon-smoke-script-{}-{name}",
            tarragon_core::testing::run_tag()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).expect("make bin");
        fs::create_dir_all(root.join("fixtures")).expect("make fixtures");
        fs::write(root.join("fixtures/rgb_8bit.psd"), b"8BPS").expect("write psd");
        // Like the real tool: fail when the folder has files, else fill it.
        stub(
            &root.join("bin/corpus_gen"),
            "echo \"$*\" >> \"$CORPUS_LOG\"\n\
             [ \"$1\" = --out ] || exit 2\n\
             if [ -d \"$2\" ] && [ -n \"$(ls -A \"$2\")\" ]; then exit 2; fi\n\
             mkdir -p \"$2\" && : > \"$2/made.png\"\n",
        );
        stub(
            &root.join("bin/scan_bench"),
            &format!("echo \"$*\" >> \"$SCAN_LOG\"\n{bench}"),
        );
        // Record the arguments, then run the rest of the line without the pin.
        stub(
            &root.join("tools/taskset"),
            "echo \"$*\" >> \"$TASKSET_LOG\"\nshift 2\nexec \"$@\"\n",
        );
        let this = Self { root };
        this.stub_app(&this.root.join("bin/tarragon"));
        this.set_load(&good_load());
        this.set_frame(&good_frame());
        fs::write(
            this.root.join("cores4"),
            "# CPU,Core\n0,0\n1,0\n2,1\n3,1\n4,2\n5,2\n6,3\n7,3\n8,4\n9,5\n",
        )
        .expect("cores4");
        fs::write(this.root.join("cores2"), "# CPU,Core\n0,0\n1,0\n2,1\n3,1\n").expect("cores2");
        fs::write(
            this.root.join("freq"),
            "CPU CORE MAXMHZ\n0 0 4900.0\n1 0 4900.0\n2 1 5000.0\n3 1 5000.0\n4 2 4900.0\n5 2 4900.0\n6 3 4900.0\n7 3 4900.0\n8 4 3800.0\n9 5 3800.0\n",
        )
        .expect("freq");
        this
    }

    /// Write the stub app at `path`. It records its arguments and a few
    /// variables in `app.log`, then copies a canned perf log: `frame.log` for
    /// a frame run, `load.log` otherwise. A run without `WAYLAND_DISPLAY`
    /// copies `frame_x.log` or `load_x.log` when that file exists.
    pub fn stub_app(&self, path: &Path) {
        let root = self.root.display();
        stub(
            path,
            &format!(
                "echo \"args=$* data=$XDG_DATA_HOME wayland=${{WAYLAND_DISPLAY-unset}} \
                 frame=${{TARRAGON_FRAME_RUN-unset}} close=${{TARRAGON_CLOSE_WHEN_LOADED-unset}} \
                 drops=$(cat {root}/drops 2>/dev/null | wc -l)\" >> {root}/app.log\n\
                 kind=load\n\
                 [ -n \"${{TARRAGON_FRAME_RUN-}}\" ] && kind=frame\n\
                 [ -z \"${{WAYLAND_DISPLAY+set}}\" ] && [ -f {root}/$kind\"_x.log\" ] && kind=${{kind}}_x\n\
                 cat {root}/$kind.log > \"$TARRAGON_PERF_LOG\"\n"
            ),
        );
    }

    /// Replace the perf log that a cold start run copies.
    pub fn set_load(&self, log: &str) {
        fs::write(self.root.join("load.log"), log).expect("load log");
    }

    /// Replace the perf log that a frame run copies.
    pub fn set_frame(&self, log: &str) {
        fs::write(self.root.join("frame.log"), log).expect("frame log");
    }

    /// Replace the perf log that a frame run without Wayland copies.
    pub fn set_frame_x(&self, log: &str) {
        fs::write(self.root.join("frame_x.log"), log).expect("frame_x log");
    }

    /// Run the script with the given cores file, drop command, and args.
    pub fn run(&self, cores: &str, drop_cmd: &str, args: &[&str]) -> Output {
        self.run_env(cores, drop_cmd, args, &[])
    }

    /// Like `run`, with more environment variables. The script runs in the
    /// bundle folder, so its result files land there. `DISPLAY` is unset
    /// unless the caller sets it.
    pub fn run_env(
        &self,
        cores: &str,
        drop_cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Output {
        self.command(cores, drop_cmd, args, env)
            .output()
            .expect("the script starts")
    }

    /// The command that `run_env` runs, not started yet.
    pub fn command(
        &self,
        cores: &str,
        drop_cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Command {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/smoke/smoke.sh");
        let path = format!(
            "{}:{}",
            self.root.join("tools").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new("sh");
        command
            .arg(script)
            .args(args)
            .current_dir(&self.root)
            .env_remove("DISPLAY")
            .env("PATH", path)
            .env("TARRAGON_SMOKE_BASE", &self.root)
            .env("TARRAGON_SMOKE_LSCPU_CORES", self.root.join(cores))
            .env("TARRAGON_SMOKE_LSCPU_FREQ", self.root.join("freq"))
            .env("TARRAGON_SMOKE_DROP_CMD", drop_cmd)
            .env("TASKSET_LOG", self.root.join("taskset.log"))
            .env("CORPUS_LOG", self.root.join("corpus_gen.log"))
            .env("SCAN_LOG", self.root.join("scan_bench.log"))
            .envs(env.iter().copied());
        command
    }

    /// Replace the frequency table the script reads.
    pub fn set_freq(&self, table: &str) {
        fs::write(self.root.join("freq"), table).expect("freq");
    }

    /// The corpus folder the script is told to use.
    pub fn corpus(&self) -> PathBuf {
        self.root.join("corpus")
    }
}

impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Write an executable `sh` stub.
pub fn stub(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("make folder");
    fs::write(path, format!("#!/bin/sh\n{body}")).expect("write stub");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Bench that passes.
pub const BENCH_PASS: &str = "echo 'total elapsed:    1.500 s'\necho 'result:           PASS'\n";
/// Bench that prints `result: FAIL` but exits 0.
pub const BENCH_RESULT_FAIL: &str =
    "echo 'total elapsed:    1.500 s'\necho 'result:           FAIL'\n";
/// Bench that exits 1 but prints `result: PASS`.
pub const BENCH_EXIT_FAIL: &str =
    "echo 'total elapsed:    1.500 s'\necho 'result:           PASS'\nexit 1\n";
/// Bench that prints no time line.
pub const BENCH_NO_TIME: &str = "echo 'result:           PASS'\n";

/// The standard output of a run as text.
pub fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}
