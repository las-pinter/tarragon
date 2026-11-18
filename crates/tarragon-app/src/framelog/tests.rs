use super::*;

const MS: Duration = Duration::from_millis(1);

/// Feed frames whose gaps are `gaps` (in ms) after a first frame at
/// `base`. Return the phase of the last frame.
fn feed(rec: &mut FrameRecorder, base: Instant, gaps: &[u64]) -> Phase {
    let mut at = base;
    let mut phase = rec.frame(at);
    for gap in gaps {
        at += MS * u32::try_from(*gap).unwrap();
        phase = rec.frame(at);
    }
    phase
}

fn report_of(gaps: &[u64]) -> FrameReport {
    let mut rec = FrameRecorder::new(Duration::ZERO, Duration::from_secs(3600));
    feed(&mut rec, Instant::now(), gaps);
    rec.report().expect("enough frames")
}

#[test]
fn steady_10ms_frames_pass_with_every_stat_at_10ms() {
    let report = report_of(&[10; 1000]);
    assert_eq!(report.count, 1000);
    for stat in [report.p50, report.p95, report.p99, report.max] {
        assert_eq!(stat, 10 * MS);
    }
    assert_eq!(report.verdict(), Verdict::Pass);
}

#[test]
fn lines_list_the_keys_in_order_with_two_decimals() {
    let text = report_of(&[10; 1000]).lines();
    assert_eq!(
        text,
        "frames: 1000\nspan_s: 10.00\np50_ms: 10.00\np95_ms: 10.00\n\
         p99_ms: 10.00\nmax_ms: 10.00\nframe_verdict: PASS\n"
    );
}

#[test]
fn one_stall_over_100ms_fails_though_p99_is_low() {
    let mut gaps = vec![10; 99];
    gaps.push(120);
    let report = report_of(&gaps);
    assert_eq!(report.p99, 10 * MS);
    assert_eq!(report.verdict(), Verdict::Fail);
    assert!(report.lines().ends_with("frame_verdict: FAIL\n"));
}

#[test]
fn a_p99_of_exactly_33ms_fails() {
    let mut gaps = vec![10; 98];
    gaps.extend([33, 33]);
    assert_eq!(report_of(&gaps).verdict(), Verdict::Fail);
}

#[test]
fn a_p99_of_32ms_passes() {
    let mut gaps = vec![10; 98];
    gaps.extend([32, 32]);
    assert_eq!(report_of(&gaps).verdict(), Verdict::Pass);
}

#[test]
fn a_max_that_prints_as_100ms_passes() {
    let mut rec = FrameRecorder::new(Duration::ZERO, Duration::from_secs(3600));
    let mut at = Instant::now();
    rec.frame(at);
    for gap in (0..99)
        .map(|_| Duration::from_millis(10))
        .chain([Duration::from_micros(100_004)])
    {
        at += gap;
        rec.frame(at);
    }
    let report = rec.report().expect("enough frames");
    assert!(report.lines().contains("max_ms: 100.00\n"));
    assert_eq!(report.verdict(), Verdict::Pass);
}

#[test]
fn a_stall_of_exactly_100ms_is_not_over_the_limit() {
    let mut gaps = vec![10; 99];
    gaps.push(100);
    assert_eq!(report_of(&gaps).verdict(), Verdict::Pass);
}

#[test]
fn two_slow_frames_in_a_hundred_fail_on_p99() {
    let mut gaps = vec![10; 98];
    gaps.extend([40, 40]);
    let report = report_of(&gaps);
    assert_eq!(report.p99, 40 * MS);
    assert_eq!(report.verdict(), Verdict::Fail);
}

#[test]
fn warmup_frames_are_not_recorded() {
    let base = Instant::now();
    let mut rec = FrameRecorder::new(Duration::from_millis(500), Duration::from_secs(3600));
    // A 300 ms stall inside the warm-up must not reach the report.
    let mut at = base;
    for step in [0, 10, 300, 10, 10] {
        at += step * MS;
        assert_eq!(rec.frame(at), Phase::Warmup);
    }
    assert_eq!(rec.gaps.len(), 0);
    // The first frame after the warm-up is the baseline, not a gap.
    at = base + 600 * MS;
    assert_eq!(rec.frame(at), Phase::Recording);
    assert_eq!(rec.gaps.len(), 0);
    for _ in 0..100 {
        at += 10 * MS;
        assert_eq!(rec.frame(at), Phase::Recording);
    }
    let report = rec.report().expect("enough frames");
    assert_eq!(report.count, 100);
    assert_eq!(report.max, 10 * MS);
}

#[test]
fn done_appears_once_at_the_end_of_the_run() {
    let base = Instant::now();
    let mut rec = FrameRecorder::new(Duration::ZERO, Duration::from_millis(1000));
    let mut at = base;
    let mut phases = vec![rec.frame(at)];
    for _ in 0..100 {
        at += 10 * MS;
        phases.push(rec.frame(at));
    }
    let done = phases.iter().filter(|p| **p == Phase::Done).count();
    assert_eq!(done, 1);
    assert_eq!(phases.last(), Some(&Phase::Done));
    // A late frame neither changes the report nor ends the run again.
    let before = rec.report();
    at += 10 * MS;
    rec.frame(at);
    assert_eq!(rec.report(), before);
    assert_eq!(before.expect("enough frames").count, 100);
}

#[test]
fn fewer_than_100_frames_give_no_report() {
    let mut rec = FrameRecorder::new(Duration::ZERO, Duration::from_secs(3600));
    feed(&mut rec, Instant::now(), &[10; 99]);
    assert_eq!(rec.report(), None);
}
