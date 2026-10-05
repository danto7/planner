//! Adwaita-style theme following the GNOME Human Interface Guidelines:
//! the Adwaita palette, a 6 px spacing grid, flat icon buttons, switches,
//! boxed lists and header bars.

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Pos2,
    Rect, Response, Sense, Shadow, Stroke, Style, TextStyle, Theme, Ui, Vec2, Visuals,
};
use std::sync::Arc;

/// Build a translucent colour in a `const` context.
const fn alpha(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    let a32 = a as u32;
    Color32::from_rgba_premultiplied(
        (r as u32 * a32 / 255) as u8,
        (g as u32 * a32 / 255) as u8,
        (b as u32 * a32 / 255) as u8,
        a,
    )
}

/// Adwaita named colours (https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/named-colors.html).
#[derive(Clone, Copy)]
pub struct Palette {
    pub window_bg: Color32,
    pub window_fg: Color32,
    pub view_bg: Color32,
    pub headerbar_bg: Color32,
    pub sidebar_bg: Color32,
    pub card_bg: Color32,
    pub dialog_bg: Color32,
    pub border: Color32,
    pub shade: Color32,
    pub accent_bg: Color32,
    pub accent_fg: Color32,
    pub accent: Color32,
    pub destructive_bg: Color32,
    pub destructive: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub button_bg: Color32,
    pub button_hover: Color32,
    pub button_active: Color32,
    pub dim_fg: Color32,
}

pub const LIGHT: Palette = Palette {
    window_bg: Color32::from_rgb(0xfa, 0xfa, 0xfa),
    window_fg: alpha(0, 0, 0, 204),
    view_bg: Color32::WHITE,
    headerbar_bg: Color32::WHITE,
    sidebar_bg: Color32::from_rgb(0xeb, 0xeb, 0xeb),
    card_bg: Color32::WHITE,
    dialog_bg: Color32::from_rgb(0xfa, 0xfa, 0xfa),
    border: alpha(0, 0, 0, 38),
    shade: alpha(0, 0, 0, 18),
    accent_bg: Color32::from_rgb(0x35, 0x84, 0xe4),
    accent_fg: Color32::WHITE,
    accent: Color32::from_rgb(0x04, 0x61, 0xbe),
    destructive_bg: Color32::from_rgb(0xe0, 0x1b, 0x24),
    destructive: Color32::from_rgb(0xc3, 0x00, 0x00),
    warning: Color32::from_rgb(0x90, 0x5e, 0x00),
    error: Color32::from_rgb(0xc3, 0x00, 0x00),
    button_bg: alpha(0, 0, 0, 20),
    button_hover: alpha(0, 0, 0, 31),
    button_active: alpha(0, 0, 0, 41),
    dim_fg: alpha(0, 0, 0, 140),
};

pub const DARK: Palette = Palette {
    window_bg: Color32::from_rgb(0x24, 0x24, 0x24),
    window_fg: Color32::WHITE,
    view_bg: Color32::from_rgb(0x1e, 0x1e, 0x1e),
    headerbar_bg: Color32::from_rgb(0x30, 0x30, 0x30),
    sidebar_bg: Color32::from_rgb(0x30, 0x30, 0x30),
    card_bg: Color32::from_rgb(0x38, 0x38, 0x38),
    dialog_bg: Color32::from_rgb(0x38, 0x38, 0x38),
    border: alpha(255, 255, 255, 38),
    shade: alpha(0, 0, 0, 92),
    accent_bg: Color32::from_rgb(0x35, 0x84, 0xe4),
    accent_fg: Color32::WHITE,
    accent: Color32::from_rgb(0x78, 0xae, 0xed),
    destructive_bg: Color32::from_rgb(0xc0, 0x1c, 0x28),
    destructive: Color32::from_rgb(0xff, 0x7b, 0x63),
    warning: Color32::from_rgb(0xff, 0xc2, 0x52),
    error: Color32::from_rgb(0xff, 0x7b, 0x63),
    button_bg: alpha(255, 255, 255, 26),
    button_hover: alpha(255, 255, 255, 38),
    button_active: alpha(255, 255, 255, 51),
    dim_fg: alpha(255, 255, 255, 140),
};

pub fn palette(ui: &Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn palette_of(ctx: &Context) -> &'static Palette {
    if ctx.theme() == Theme::Dark {
        &DARK
    } else {
        &LIGHT
    }
}

/// A keycap label for the shortcuts dialog.
pub fn keycap(ui: &mut Ui, key: &str) {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.button_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(6.0)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(key).size(12.0));
        });
}

pub const BOLD: &str = "Inter SemiBold";

/// Font family used for titles and other bold text.
pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

pub fn title_font(size: f32) -> FontId {
    FontId::new(size, bold_family())
}

/// Install Inter (the typeface Adwaita Sans is derived from) and the Adwaita styles.
pub fn apply(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        BOLD.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Inter".into());
    let mut bold = proportional.clone();
    bold.insert(0, BOLD.into());
    fonts.families.insert(bold_family(), bold);
    ctx.set_fonts(fonts);

    ctx.style_mut_of(Theme::Light, |style| style_for(style, &LIGHT, false));
    ctx.style_mut_of(Theme::Dark, |style| style_for(style, &DARK, true));
    ctx.options_mut(|o| o.fallback_theme = Theme::Light);
    if let Ok(theme) = std::env::var("PLANNER_THEME") {
        match theme.as_str() {
            "light" => ctx.set_theme(Theme::Light),
            "dark" => ctx.set_theme(Theme::Dark),
            _ => {}
        }
    }
}

fn style_for(style: &mut Style, p: &Palette, dark: bool) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, title_font(20.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = Vec2::new(6.0, 6.0);
    s.button_padding = Vec2::new(14.0, 7.0);
    s.interact_size = Vec2::new(34.0, 34.0);
    s.window_margin = Margin::same(12);
    s.menu_margin = Margin::same(6);
    s.icon_width = 16.0;
    s.icon_width_inner = 10.0;
    s.icon_spacing = 8.0;
    s.text_edit_width = 280.0;
    s.combo_width = 160.0;

    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.override_text_color = Some(p.window_fg);
    v.weak_text_color = Some(p.dim_fg);
    v.panel_fill = p.window_bg;
    v.window_fill = p.dialog_bg;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow {
        offset: [0, 4],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 50 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 2],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 40 }),
    };
    v.menu_corner_radius = CornerRadius::same(12);
    v.extreme_bg_color = p.view_bg;
    v.text_edit_bg_color = Some(if dark {
        alpha(255, 255, 255, 20)
    } else {
        alpha(0, 0, 0, 12)
    });
    v.faint_bg_color = p.shade;
    v.code_bg_color = p.shade;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent_bg;
    v.selection.stroke = Stroke::new(1.0, p.accent_fg);
    v.striped = false;
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.window_highlight_topmost = false;
    v.slider_trailing_fill = true;

    let radius = CornerRadius::same(8);
    let fg = Stroke::new(1.0, p.window_fg);
    v.widgets.noninteractive.bg_fill = p.window_bg;
    v.widgets.noninteractive.weak_bg_fill = p.window_bg;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = fg;
    v.widgets.noninteractive.corner_radius = radius;

    v.widgets.inactive.bg_fill = p.button_bg;
    v.widgets.inactive.weak_bg_fill = p.button_bg;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = fg;
    v.widgets.inactive.corner_radius = radius;
    v.widgets.inactive.expansion = 0.0;

    v.widgets.hovered.bg_fill = p.button_hover;
    v.widgets.hovered.weak_bg_fill = p.button_hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.hovered.fg_stroke = fg;
    v.widgets.hovered.corner_radius = radius;
    v.widgets.hovered.expansion = 0.0;

    v.widgets.active.bg_fill = p.button_active;
    v.widgets.active.weak_bg_fill = p.button_active;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.fg_stroke = fg;
    v.widgets.active.corner_radius = radius;
    v.widgets.active.expansion = 0.0;

    v.widgets.open.bg_fill = p.button_active;
    v.widgets.open.weak_bg_fill = p.button_active;
    v.widgets.open.bg_stroke = Stroke::NONE;
    v.widgets.open.fg_stroke = fg;
    v.widgets.open.corner_radius = radius;
    v.widgets.open.expansion = 0.0;

    style.visuals = v;
}

// ---------------------------------------------------------------------------
// Icons, drawn with the painter so they render crisply at any size.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    Menu,
    Refresh,
    Plus,
    Close,
    Check,
    Calendar,
}

pub fn paint_icon(ui: &Ui, icon: Icon, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 2.0; // half size
    let stroke = Stroke::new(1.6, color);
    match icon {
        Icon::ChevronLeft => painter.add(egui::Shape::line(
            vec![
                Pos2::new(c.x + s * 0.3, c.y - s * 0.55),
                Pos2::new(c.x - s * 0.3, c.y),
                Pos2::new(c.x + s * 0.3, c.y + s * 0.55),
            ],
            stroke,
        )),
        Icon::ChevronRight => painter.add(egui::Shape::line(
            vec![
                Pos2::new(c.x - s * 0.3, c.y - s * 0.55),
                Pos2::new(c.x + s * 0.3, c.y),
                Pos2::new(c.x - s * 0.3, c.y + s * 0.55),
            ],
            stroke,
        )),
        Icon::Menu => {
            for dy in [-0.5, 0.0, 0.5] {
                painter.line_segment(
                    [
                        Pos2::new(c.x - s * 0.7, c.y + s * dy),
                        Pos2::new(c.x + s * 0.7, c.y + s * dy),
                    ],
                    stroke,
                );
            }
            egui::layers::ShapeIdx(0)
        }
        Icon::Refresh => {
            let r = s * 0.65;
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + i as f32 / 24.0 * std::f32::consts::TAU * 0.78;
                    Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            // arrow head at the start of the arc
            let tip = Pos2::new(c.x, c.y - r);
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(tip.x - s * 0.35, tip.y - s * 0.35),
                    tip,
                    Pos2::new(tip.x - s * 0.35, tip.y + s * 0.35),
                ],
                stroke,
            ))
        }
        Icon::Plus => {
            painter.line_segment(
                [Pos2::new(c.x - s * 0.6, c.y), Pos2::new(c.x + s * 0.6, c.y)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x, c.y - s * 0.6), Pos2::new(c.x, c.y + s * 0.6)],
                stroke,
            )
        }
        Icon::Close => {
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.5, c.y - s * 0.5),
                    Pos2::new(c.x + s * 0.5, c.y + s * 0.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.5, c.y - s * 0.5),
                    Pos2::new(c.x - s * 0.5, c.y + s * 0.5),
                ],
                stroke,
            )
        }
        Icon::Check => painter.add(egui::Shape::line(
            vec![
                Pos2::new(c.x - s * 0.55, c.y + s * 0.05),
                Pos2::new(c.x - s * 0.15, c.y + s * 0.45),
                Pos2::new(c.x + s * 0.6, c.y - s * 0.45),
            ],
            Stroke::new(2.0, color),
        )),
        Icon::Calendar => {
            let r = Rect::from_center_size(c, Vec2::splat(s * 1.4));
            painter.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(r.left(), r.top() + s * 0.45),
                    Pos2::new(r.right(), r.top() + s * 0.45),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(r.left() + s * 0.4, r.top() - s * 0.2),
                    Pos2::new(r.left() + s * 0.4, r.top() + s * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(r.right() - s * 0.4, r.top() - s * 0.2),
                    Pos2::new(r.right() - s * 0.4, r.top() + s * 0.2),
                ],
                stroke,
            )
        }
    };
}

/// A flat 34×34 icon button, as used in header bars.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = palette(ui);
        let enabled = ui.is_enabled();
        let fill = if enabled && response.is_pointer_button_down_on() {
            p.button_active
        } else if enabled && response.hovered() {
            p.button_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 8.0, fill);
        let color = if enabled { p.window_fg } else { p.dim_fg };
        paint_icon(
            ui,
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
            color,
        );
    }
    response.on_hover_text(tooltip)
}

pub fn icon_button_enabled(ui: &mut Ui, icon: Icon, tooltip: &str, enabled: bool) -> Response {
    ui.add_enabled_ui(enabled, |ui| icon_button(ui, icon, tooltip))
        .inner
}

/// A suggested-action (accent coloured) button.
pub fn suggested_button(ui: &mut Ui, text: &str) -> Response {
    let p = palette(ui);
    ui.add(
        egui::Button::new(
            egui::RichText::new(text)
                .family(bold_family())
                .color(p.accent_fg),
        )
        .fill(p.accent_bg)
        .corner_radius(8.0),
    )
}

/// A destructive-action (red) button.
pub fn destructive_button(ui: &mut Ui, text: &str) -> Response {
    let p = palette(ui);
    ui.add(
        egui::Button::new(
            egui::RichText::new(text)
                .family(bold_family())
                .color(Color32::WHITE),
        )
        .fill(p.destructive_bg)
        .corner_radius(8.0),
    )
}

/// A flat text button (no background until hovered).
pub fn flat_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    ui.add(egui::Button::new(text).frame(false))
}

/// An Adwaita-style switch. Returns true when toggled.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let size = Vec2::new(46.0, 26.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let p = palette(ui);
        let t = ui.ctx().animate_bool(response.id, *on);
        let track = if *on { p.accent_bg } else { p.button_hover };
        ui.painter().rect_filled(rect, 13.0, track);
        let knob_x = egui::lerp((rect.left() + 13.0)..=(rect.right() - 13.0), t);
        let knob = if *on {
            p.accent_fg
        } else if ui.visuals().dark_mode {
            Color32::from_rgb(0xd0, 0xd0, 0xd0)
        } else {
            Color32::WHITE
        };
        ui.painter()
            .circle_filled(Pos2::new(knob_x, rect.center().y), 10.0, knob);
        if *on {
            paint_icon(
                ui,
                Icon::Check,
                Rect::from_center_size(Pos2::new(knob_x, rect.center().y), Vec2::splat(12.0)),
                p.accent_bg,
            );
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, "")
    });
    response
}

/// A translucent tint of an accent colour.
pub fn tint_of(color: Color32, dark: bool) -> Color32 {
    let a = if dark { 70 } else { 40 };
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

/// A GNOME-style check button: a 16 px rounded square that fills with the
/// accent colour when checked.
pub fn check_button(ui: &mut Ui, checked: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let p = palette(ui);
        let box_rect = Rect::from_center_size(rect.center(), Vec2::splat(16.0));
        if *checked {
            ui.painter().rect_filled(box_rect, 4.0, p.accent_bg);
            paint_icon(ui, Icon::Check, box_rect.shrink(2.0), p.accent_fg);
        } else {
            let fill = if response.hovered() {
                p.button_hover
            } else {
                p.button_bg
            };
            ui.painter().rect_filled(box_rect, 4.0, fill);
            ui.painter().rect_stroke(
                box_rect,
                4.0,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *checked, "")
    });
    response.on_hover_text(if *checked {
        "Mark as not completed"
    } else {
        "Mark as completed"
    })
}

/// Frame for a boxed list / card: rounded 12 px, hairline border.
pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card_bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12.0)
}

/// A row inside a boxed list. Draws a separator above every row but the first.
pub fn list_row<R>(
    ui: &mut Ui,
    first: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    if !first {
        let p = palette(ui);
        let y = ui.cursor().top();
        ui.painter()
            .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    }
    egui::Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_height(28.0);
            ui.set_width(ui.available_width());
            content(ui)
        })
}

/// Group heading used in preferences and sidebars.
pub fn group_title(ui: &mut Ui, title: &str) {
    ui.label(egui::RichText::new(title).font(title_font(15.0)));
}

/// Centered empty-state status page.
pub fn status_page(ui: &mut Ui, icon: Icon, title: &str, description: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(36.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
        let p = palette(ui);
        ui.painter().circle_filled(rect.center(), 32.0, p.shade);
        paint_icon(
            ui,
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(30.0)),
            p.dim_fg,
        );
        ui.add_space(12.0);
        ui.label(egui::RichText::new(title).font(title_font(17.0)));
        ui.add_space(4.0);
        ui.add(egui::Label::new(egui::RichText::new(description).color(p.dim_fg)).wrap());
    });
}
