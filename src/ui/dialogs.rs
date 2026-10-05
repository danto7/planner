//! Adaptive-dialog scaffolding (modal with a header bar), the About dialog
//! and the Keyboard Shortcuts dialog.

use super::theme::{self, keycap, palette_of, title_font, Icon};
use egui::{Align2, Context, Frame, Id, Layout, Margin, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

/// Show a modal dialog with a header bar. `content` receives the start and
/// end areas of the header bar and the body. Returns the content result and
/// whether the dialog asked to close (backdrop click or Escape).
pub fn dialog<R>(
    ctx: &Context,
    id: &str,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut Ui, &mut Ui, &mut Ui) -> R,
) -> (R, bool) {
    let p = palette_of(ctx);
    let frame = Frame::new()
        .fill(p.dialog_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12.0)
        .shadow(ctx.style_of(ctx.theme()).visuals.window_shadow)
        .inner_margin(Margin::ZERO);
    let modal = egui::Modal::new(Id::new(id))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(90))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 47.0), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                title,
                title_font(15.0),
                p.window_fg,
            );
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, p.border),
            );
            let inner = rect.shrink2(Vec2::new(6.0, 6.0));
            let mut start = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::left_to_right(egui::Align::Center)),
            );
            let mut end = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::right_to_left(egui::Align::Center)),
            );
            let avail = ui.available_rect_before_wrap();
            let body_rect = Rect::from_min_max(
                avail.min + Vec2::new(18.0, 14.0),
                egui::Pos2::new(avail.max.x - 18.0, avail.max.y),
            );
            let mut body = ui.new_child(
                UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(Layout::top_down(egui::Align::Min)),
            );
            body.spacing_mut().item_spacing.y = 6.0;
            body.set_width(width - 36.0);
            let result = content(&mut start, &mut end, &mut body);
            let used = body.min_rect();
            ui.allocate_rect(
                Rect::from_min_max(
                    used.min - Vec2::new(18.0, 14.0),
                    egui::Pos2::new(used.max.x + 18.0, used.max.y + 14.0),
                ),
                Sense::hover(),
            );
            result
        });
    let close = modal.should_close();
    (modal.inner, close)
}

pub fn about(ctx: &Context, open: &mut bool) {
    if !*open {
        return;
    }
    let mut close = false;
    let (_, backdrop) = dialog(ctx, "about", "", 360.0, |_, end, ui| {
        if theme::icon_button(end, Icon::Close, "Close").clicked() {
            close = true;
        }
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
            let p = theme::palette(ui);
            ui.painter().rect_filled(rect, 22.0, p.accent_bg);
            theme::paint_icon(
                ui,
                Icon::Calendar,
                Rect::from_center_size(rect.center(), Vec2::splat(52.0)),
                p.accent_fg,
            );
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Week Planner").font(title_font(22.0)));
            ui.add_space(6.0);
            Frame::new()
                .fill(theme::tint_of(p.accent_bg, ui.visuals().dark_mode))
                .corner_radius(12.0)
                .inner_margin(Margin::symmetric(10, 3))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(env!("CARGO_PKG_VERSION"))
                            .color(p.accent)
                            .size(12.0),
                    );
                });
            ui.add_space(12.0);
            ui.label("Plan your week around your CalDAV tasks and calendars.");
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("Built with egui · Inter typeface, SIL Open Font License")
                    .color(p.dim_fg)
                    .size(12.0),
            );
            ui.add_space(6.0);
        });
    });
    if close || backdrop {
        *open = false;
    }
}

pub fn shortcuts(ctx: &Context, open: &mut bool) {
    if !*open {
        return;
    }
    let mut close = false;
    type Group = (&'static str, &'static [(&'static str, &'static [&'static str])]);
    let groups: [Group; 3] = [
        (
            "General",
            &[
                ("Preferences", &["Ctrl", ","]),
                ("Keyboard shortcuts", &["Ctrl", "?"]),
                ("Refresh from server", &["Ctrl", "R"]),
            ],
        ),
        (
            "Navigation",
            &[
                ("Previous week", &["Alt", "←"]),
                ("Next week", &["Alt", "→"]),
                ("Go to today", &["Ctrl", "T"]),
            ],
        ),
        (
            "Tasks",
            &[
                ("New task", &["Ctrl", "N"]),
                ("Save task", &["Ctrl", "Enter"]),
                ("Close dialog", &["Esc"]),
            ],
        ),
    ];
    let (_, backdrop) = dialog(
        ctx,
        "shortcuts",
        "Keyboard Shortcuts",
        420.0,
        |_, end, ui| {
            if theme::icon_button(end, Icon::Close, "Close").clicked() {
                close = true;
            }
            for (title, rows) in groups {
                ui.add_space(6.0);
                theme::group_title(ui, title);
                ui.add_space(6.0);
                theme::card_frame(ui).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, (label, keys)) in rows.iter().enumerate() {
                        theme::list_row(ui, i == 0, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(*label);
                                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                    for key in keys.iter().rev() {
                                        keycap(ui, key);
                                    }
                                });
                            });
                        });
                    }
                });
                ui.add_space(12.0);
            }
        },
    );
    if close || backdrop {
        *open = false;
    }
}
