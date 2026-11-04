//! The dim layer behind a modal window.

use egui::{Area, Context, Id, Order, Sense};

use crate::tokens;

/// The order of the dim layer. The panels sit below it and the window above.
const BACKDROP_ORDER: Order = Order::Middle;

/// Paint the dim layer over the whole screen and keep pointer and key input
/// from the layers under it. Call it every frame the window is open, before
/// the window. `id` names the layer, so two windows do not share one.
pub(crate) fn show(ctx: &Context, id: &'static str) {
    let area = Area::new(Id::new(id))
        .order(BACKDROP_ORDER)
        // The dim layer holds its full colour from the first frame.
        .fade_in(false)
        .interactable(true)
        .sense(Sense::click_and_drag());
    let layer = area.layer();
    let rect = ctx.content_rect();
    area.fixed_pos(rect.min).show(ctx, |ui| {
        ui.painter().rect_filled(rect, 0.0, tokens::SCRIM);
        // A click on the dim layer is taken here and does nothing.
        ui.allocate_rect(rect, Sense::click_and_drag());
    });
    // egui lets only the layers at or above the modal layer take input.
    ctx.memory_mut(|m| m.set_modal_layer(layer));
}
