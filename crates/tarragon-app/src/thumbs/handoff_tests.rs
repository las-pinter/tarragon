//! Tests for running pipeline work on a thread.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;

use super::{Answer, spawn};
use crate::pipeline::Pipeline;
use crate::thumbs::tests::test_pipeline;

/// Long enough for a thread to answer on a loaded machine.
const LIMIT: Duration = Duration::from_secs(10);

/// Start `work` with the pipeline, or fail the test.
fn started<R: Send + 'static>(
    pipeline: Pipeline,
    work: impl FnOnce(&mut Pipeline) -> R + Send + 'static,
) -> Receiver<Answer<R>> {
    let Ok(done) = spawn("tarragon-test", pipeline, Arc::new(|| {}), work) else {
        panic!("the thread did not start");
    };
    done
}

#[test]
fn the_answer_carries_the_pipeline_and_the_result() {
    let (_dir, pipeline) = test_pipeline("answer");
    let answer = started(pipeline, |_| 7)
        .recv_timeout(LIMIT)
        .expect("an answer");
    assert_eq!(answer.result, Some(7));
}

#[test]
fn a_panic_in_the_work_still_sends_the_pipeline_back() {
    let (_dir, pipeline) = test_pipeline("panic");
    let answer = started(pipeline, |_| -> u8 { panic!("boom") })
        .recv_timeout(LIMIT)
        .expect("an answer after the panic");
    assert_eq!(answer.result, None);
    let again = started(answer.pipeline, |_| 1)
        .recv_timeout(LIMIT)
        .expect("an answer from the kept pipeline");
    assert_eq!(again.result, Some(1));
}
