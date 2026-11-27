//! Tests for the status bar along the bottom of the window.

use super::*;
use crate::status::{Job, STATUS_HEIGHT};

/// Each painted text with its rect, for one frame of `shell`.
fn painted(ctx: &Context, shell: &mut Shell) -> Vec<(String, Rect)> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                out.push((text.galley.job.text.clone(), text.visual_bounding_rect()));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    output.drop_without_applying_deltas();
    out
}

#[test]
fn the_bar_shows_the_jobs_and_the_selection_along_the_bottom() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell
        .gallery_mut()
        .set_folder(Some(PathBuf::from("/images")));
    shell.gallery_mut().set_items(dated_items());
    shell.gallery_mut().select_all_visible();
    shell.set_jobs(vec![Job::Colors { done: 3, total: 9 }]);
    draw(&ctx, &mut shell);

    let texts = painted(&ctx, &mut shell);

    let find = |want: &str| {
        texts
            .iter()
            .find(|(text, _)| text == want)
            .unwrap_or_else(|| panic!("no {want:?} in {texts:?}"))
            .1
    };
    let bar_top = 900.0 - STATUS_HEIGHT;
    for want in ["Indexing colors 3 / 9", "4 selected"] {
        let rect = find(want);
        assert!(
            rect.top() >= bar_top && rect.bottom() <= 900.0,
            "{want} at {rect:?}"
        );
    }
    assert!(find("Indexing colors 3 / 9").left() < find("4 selected").left());
}

#[test]
fn the_bar_drops_a_job_when_the_app_drops_it() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.set_jobs(vec![Job::Library]);
    draw(&ctx, &mut shell);
    assert!(
        painted(&ctx, &mut shell)
            .iter()
            .any(|(t, _)| t == "Loading library")
    );

    shell.set_jobs(Vec::new());

    assert!(
        !painted(&ctx, &mut shell)
            .iter()
            .any(|(t, _)| t == "Loading library")
    );
}
