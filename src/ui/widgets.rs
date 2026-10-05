//! Small shared widgets and colour helpers.

use crate::model::Calendar;
use egui::{Color32, Id, LayerId, Order, Response, Sense, Ui, UiBuilder};

/// Payload carried while a task is dragged between the inbox and the grid.
#[derive(Debug, Clone)]
pub struct DragTask {
    pub uid: String,
}

/// Like `Ui::dnd_drag_source`, but also senses clicks so the item can be
/// opened with a plain click and moved with a drag.
pub fn drag_item(
    ui: &mut Ui,
    id: Id,
    payload: DragTask,
    content: impl FnOnce(&mut Ui),
) -> Response {
    if ui.ctx().is_being_dragged(id) {
        egui::DragAndDrop::set_payload(ui.ctx(), payload);
        let layer_id = LayerId::new(Order::Tooltip, id);
        let response = ui
            .scope_builder(UiBuilder::new().layer_id(layer_id), content)
            .response;
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            let delta = pointer - response.rect.center();
            ui.ctx().transform_layer_shapes(
                layer_id,
                egui::emath::TSTransform::from_translation(delta),
            );
        }
        response
    } else {
        let response = ui.scope(content).response;
        ui.interact(response.rect, id, Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab)
    }
}

pub const DEFAULT_TASK_COLOR: Color32 = Color32::from_rgb(0x35, 0x84, 0xe4);
pub const DEFAULT_EVENT_COLOR: Color32 = Color32::from_rgb(0x77, 0x76, 0x7b);

pub fn calendar_color(cal: Option<&Calendar>, fallback: Color32) -> Color32 {
    cal.and_then(Calendar::rgb)
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
        .unwrap_or(fallback)
}

/// A translucent fill derived from an accent colour that reads well on both themes.
pub fn tint(color: Color32, dark_mode: bool) -> Color32 {
    let alpha = if dark_mode { 70 } else { 45 };
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn find_calendar<'a>(calendars: &'a [Calendar], url: &str) -> Option<&'a Calendar> {
    calendars.iter().find(|c| c.url == url)
}

/// A drop target for [`DragTask`] payloads. Draws `frame` normally and
/// highlights it while a task is dragged over it. Returns the dropped payload.
pub fn drop_zone<R>(
    ui: &mut Ui,
    frame: egui::Frame,
    content: impl FnOnce(&mut Ui) -> R,
) -> (egui::InnerResponse<R>, Option<std::sync::Arc<DragTask>>) {
    let dragging = egui::DragAndDrop::has_payload_of_type::<DragTask>(ui.ctx());
    let mut prepared = frame.begin(ui);
    let inner = content(&mut prepared.content_ui);
    let response = prepared.allocate_space(ui);
    if dragging && response.contains_pointer() {
        let accent = ui.visuals().selection.bg_fill;
        prepared.frame.fill = tint(accent, ui.visuals().dark_mode);
        prepared.frame.stroke = egui::Stroke::new(1.5, accent);
    }
    prepared.paint(ui);
    let payload = response.dnd_release_payload::<DragTask>();
    (egui::InnerResponse { inner, response }, payload)
}
