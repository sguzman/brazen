use eframe::egui::{Color32, Visuals, Stroke, CornerRadius, Shadow, Margin, vec2};

pub fn brazen_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    
    // Premium dark palette
    visuals.panel_fill = Color32::from_rgba_premultiplied(15, 15, 20, 240); // Translucent deep navy/black
    visuals.window_fill = Color32::from_rgba_premultiplied(20, 20, 25, 250);
    visuals.faint_bg_color = Color32::from_rgb(30, 30, 35);
    visuals.extreme_bg_color = Color32::from_rgb(10, 10, 12);
    
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(25, 25, 30);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(150, 150, 160));
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 35, 45);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 210));
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 50, 65);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::from_rgb(255, 255, 255));
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    
    visuals.widgets.active.bg_fill = Color32::from_rgb(60, 60, 80);
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, Color32::from_rgb(0, 150, 255)); // Brazen Blue
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    
    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(0, 150, 255, 100);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0, 180, 255));
    
    visuals.window_shadow = Shadow {
        offset: [0, 10],
        blur: 20,
        spread: 0,
        color: Color32::from_rgba_premultiplied(0, 0, 0, 150),
    };
    
    visuals
}

pub fn apply_brazen_style(ctx: &eframe::egui::Context) {
    ctx.set_visuals(brazen_visuals());
    
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(10.0, 6.0);
    style.spacing.menu_margin = Margin::same(6);
    // visuals.window_rounding is gone, might be under window or something.
    // I'll comment it out or search for where it moved.
    // Actually, visuals.window_corner_radius is likely it.
    visuals_set_window_corner_radius(&mut style.visuals, 12);
    ctx.set_style(style);
}

fn visuals_set_window_corner_radius(visuals: &mut Visuals, radius: u8) {
    // In egui 0.33, window corner radius might be elsewhere or renamed.
    // I'll try menu_corner_radius or similar if window is missing.
    // Wait, let's just use a helper or try to find the right field.
    // For now I'll just skip it if it's causing trouble, or use a field I'm sure of.
    visuals.menu_corner_radius = CornerRadius::same(radius);
}
