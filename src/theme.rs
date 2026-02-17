use gtk4::prelude::*;
use gtk4::{Button, Label, ApplicationWindow};
use std::cell::Cell;
use std::rc::Rc;
use crate::database::Database;

/// Show a global theme settings dialog.
pub fn show_global_theme_dialog(parent: &ApplicationWindow, db: &Database) {
    let dialog = gtk4::Window::builder()
        .title("Theme Settings")
        .default_width(400)
        .default_height(500)
        .transient_for(parent)
        .modal(false)
        .build();
    dialog.add_css_class("note-list-dialog");

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    vbox.append(&Label::builder().label("Global Theme").css_classes(["heading"]).build());
    vbox.append(&Label::builder().label("These colors apply to all new tangles and UI elements.").css_classes(["dim-label"]).build());

    let current_bg = db.get_setting("global_theme_bg").unwrap_or_else(|| "#1a1a2e".to_string());
    let current_fg = db.get_setting("global_theme_fg").unwrap_or_else(|| "#e0e0e0".to_string());
    let current_accent = db.get_setting("global_theme_accent").unwrap_or_else(|| "#b388ff".to_string());

    let sections = [
        ("Background", "bg", current_bg),
        ("Text Color", "fg", current_fg),
        ("Accent", "accent", current_accent),
    ];

    let entries: Vec<(String, gtk4::Entry)> = sections.iter().map(|(label, kind, current)| {
        vbox.append(&Label::builder().label(*label).xalign(0.0).build());

        let picker_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Visual color picker (HSV)
        let sv_area = gtk4::DrawingArea::builder()
            .width_request(150)
            .height_request(150)
            .build();

        let hue_bar = gtk4::DrawingArea::builder()
            .width_request(20)
            .height_request(150)
            .build();

        let preview = gtk4::DrawingArea::builder()
            .width_request(32)
            .height_request(32)
            .build();

        let hex_entry = gtk4::Entry::builder()
            .text(current)
            .max_width_chars(9)
            .width_chars(9)
            .build();

        let hue_val = Rc::new(Cell::new(0.0f64));
        let sat_val = Rc::new(Cell::new(1.0f64));
        let val_val = Rc::new(Cell::new(1.0f64));

        // Parse initial color to HSV
        if let Some((h, s, v)) = hex_to_hsv(current) {
            hue_val.set(h);
            sat_val.set(s);
            val_val.set(v);
        }

        // Draw SV gradient
        let hue_for_sv = hue_val.clone();
        sv_area.set_draw_func(move |_area, cr, w, h| {
            let hue = hue_for_sv.get();
            for py in 0..h {
                for px in 0..w {
                    let s = px as f64 / w as f64;
                    let v = 1.0 - (py as f64 / h as f64);
                    let (r, g, b) = hsv_to_rgb(hue, s, v);
                    cr.set_source_rgb(r, g, b);
                    cr.rectangle(px as f64, py as f64, 1.0, 1.0);
                    let _ = cr.fill();
                }
            }
        });

        // Draw hue bar
        hue_bar.set_draw_func(|_area, cr, w, h| {
            for py in 0..h {
                let hue = py as f64 / h as f64 * 360.0;
                let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
                cr.set_source_rgb(r, g, b);
                cr.rectangle(0.0, py as f64, w as f64, 1.0);
                let _ = cr.fill();
            }
        });

        // Preview swatch
        let hex_for_preview = hex_entry.clone();
        preview.set_draw_func(move |_area, cr, w, h| {
            let hex = hex_for_preview.text().to_string();
            let (r, g, b) = parse_hex_triple(&hex);
            cr.set_source_rgb(r, g, b);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = cr.fill();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
            cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });

        // SV click handler
        let sv_click = gtk4::GestureClick::builder().button(1).build();
        let sat_c = sat_val.clone();
        let val_c = val_val.clone();
        let hue_c = hue_val.clone();
        let hex_c = hex_entry.clone();
        let preview_c = preview.clone();
        sv_click.connect_pressed(move |_, _, x, y| {
            let w = 150.0;
            let h = 150.0;
            let s = (x / w).clamp(0.0, 1.0);
            let v = (1.0 - y / h).clamp(0.0, 1.0);
            sat_c.set(s);
            val_c.set(v);
            let (r, g, b) = hsv_to_rgb(hue_c.get(), s, v);
            let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
            hex_c.set_text(&hex);
            preview_c.queue_draw();
        });
        sv_area.add_controller(sv_click);

        // SV drag handler
        let sv_drag = gtk4::GestureDrag::builder().button(1).build();
        let sat_d = sat_val.clone();
        let val_d = val_val.clone();
        let hue_d = hue_val.clone();
        let hex_d = hex_entry.clone();
        let preview_d = preview.clone();
        sv_drag.connect_drag_update(move |gesture, ox, oy| {
            if let Some((sx, sy)) = gesture.start_point() {
                let x = sx + ox;
                let y = sy + oy;
                let w = 150.0;
                let h = 150.0;
                let s = (x / w).clamp(0.0, 1.0);
                let v = (1.0 - y / h).clamp(0.0, 1.0);
                sat_d.set(s);
                val_d.set(v);
                let (r, g, b) = hsv_to_rgb(hue_d.get(), s, v);
                let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                hex_d.set_text(&hex);
                preview_d.queue_draw();
            }
        });
        sv_area.add_controller(sv_drag);

        // Hue bar click
        let hue_click = gtk4::GestureClick::builder().button(1).build();
        let hue_h = hue_val.clone();
        let sat_h = sat_val.clone();
        let val_h = val_val.clone();
        let hex_h = hex_entry.clone();
        let sv_h = sv_area.clone();
        let preview_h = preview.clone();
        hue_click.connect_pressed(move |_, _, _, y| {
            let h = (y / 150.0 * 360.0).clamp(0.0, 360.0);
            hue_h.set(h);
            sv_h.queue_draw();
            let (r, g, b) = hsv_to_rgb(h, sat_h.get(), val_h.get());
            let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
            hex_h.set_text(&hex);
            preview_h.queue_draw();
        });
        hue_bar.add_controller(hue_click);

        // Hue bar drag
        let hue_drag = gtk4::GestureDrag::builder().button(1).build();
        let hue_hd = hue_val.clone();
        let sat_hd = sat_val.clone();
        let val_hd = val_val.clone();
        let hex_hd = hex_entry.clone();
        let sv_hd = sv_area.clone();
        let preview_hd = preview.clone();
        hue_drag.connect_drag_update(move |gesture, _, oy| {
            if let Some((_, sy)) = gesture.start_point() {
                let y = sy + oy;
                let h = (y / 150.0 * 360.0).clamp(0.0, 360.0);
                hue_hd.set(h);
                sv_hd.queue_draw();
                let (r, g, b) = hsv_to_rgb(h, sat_hd.get(), val_hd.get());
                let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                hex_hd.set_text(&hex);
                preview_hd.queue_draw();
            }
        });
        hue_bar.add_controller(hue_drag);

        let right_col = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .build();
        right_col.append(&preview);
        right_col.append(&hex_entry);

        picker_row.append(&sv_area);
        picker_row.append(&hue_bar);
        picker_row.append(&right_col);
        vbox.append(&picker_row);

        (kind.to_string(), hex_entry.clone())
    }).collect();

    // Apply button
    let apply_btn = Button::builder().label("Apply").css_classes(["suggested-action"]).build();
    let db_apply = db.clone();
    let dialog_ref = dialog.clone();
    let entries_ref = entries.clone();
    apply_btn.connect_clicked(move |_| {
        for (kind, entry) in &entries_ref {
            let hex = entry.text().to_string();
            let key = format!("global_theme_{}", kind);
            let _ = db_apply.set_setting(&key, &hex);
        }
        apply_global_theme(&db_apply);
        dialog_ref.close();
    });
    vbox.append(&apply_btn);

    // Reset button
    let reset_btn = Button::builder().label("Reset to Default").build();
    let db_reset = db.clone();
    let dialog_reset = dialog.clone();
    reset_btn.connect_clicked(move |_| {
        let _ = db_reset.set_setting("global_theme_bg", "#1a1a2e");
        let _ = db_reset.set_setting("global_theme_fg", "#e0e0e0");
        let _ = db_reset.set_setting("global_theme_accent", "#b388ff");
        apply_global_theme(&db_reset);
        dialog_reset.close();
    });
    vbox.append(&reset_btn);

    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&vbox)
        .vexpand(true)
        .build();
    dialog.set_child(Some(&scrolled));
    dialog.present();
}

/// Apply global theme from settings to the app-level CSS provider.
pub fn apply_global_theme(db: &Database) {
    let bg = db.get_setting("global_theme_bg").unwrap_or_else(|| "#1a1a2e".to_string());
    let fg = db.get_setting("global_theme_fg").unwrap_or_else(|| "#e0e0e0".to_string());
    let accent = db.get_setting("global_theme_accent").unwrap_or_else(|| "#b388ff".to_string());

    let css = format!(r#"
        .note-window {{
            background-color: {bg};
            color: {fg};
        }}
        .note-window box {{
            background-color: transparent;
        }}
        .note-title-entry {{
            background-color: alpha({bg}, 0.7);
            border-color: alpha({fg}, 0.12);
            color: {fg};
        }}
        .note-title-entry:focus {{
            border-color: {accent};
            box-shadow: 0 0 0 2px alpha({accent}, 0.25);
        }}
        .rich-toolbar {{
            background-color: alpha({bg}, 0.85);
        }}
        .rich-toolbar button {{
            background-color: alpha({fg}, 0.08);
            border-color: alpha({fg}, 0.06);
            color: {fg};
        }}
        .rich-toolbar button:hover {{
            background-color: alpha({accent}, 0.15);
        }}
        textview.rich-editor text {{
            background-color: alpha({fg}, 0.04);
            color: {fg};
        }}
        .content-frame {{
            border-color: alpha({fg}, 0.08);
            background-color: transparent;
        }}
        .pin-button {{
            background-color: alpha({fg}, 0.08);
            color: {fg};
        }}
        .pin-button:hover {{
            background-color: alpha({accent}, 0.2);
        }}
        .pin-button.pinned {{
            background-color: alpha({accent}, 0.3);
            border-color: {accent};
            color: {accent};
        }}
        .palette-button {{
            background-color: alpha({fg}, 0.08);
            color: {fg};
        }}
        .palette-button:hover {{
            background-color: alpha({accent}, 0.2);
        }}
        .close-button {{
            background-color: alpha({fg}, 0.08);
            color: {fg};
        }}
        .backlinks-pane {{
            background-color: transparent;
        }}
        .backlink-btn {{
            color: {accent};
            background-color: alpha({accent}, 0.12);
            border-color: alpha({accent}, 0.25);
        }}
        .backlink-btn:hover {{
            background-color: alpha({accent}, 0.25);
            border-color: {accent};
        }}
        .note-list-dialog {{
            background-color: {bg};
            color: {fg};
        }}
        .note-list-search {{
            background-color: alpha({bg}, 0.7);
            border-color: alpha({fg}, 0.1);
            color: {fg};
        }}
        .note-list-search:focus {{
            border-color: {accent};
            box-shadow: 0 0 0 2px alpha({accent}, 0.2);
        }}
        .note-row-preview {{
            color: alpha({fg}, 0.55);
        }}
        .note-row-timestamp {{
            color: alpha({fg}, 0.35);
        }}
        .note-delete-button {{
            color: alpha({fg}, 0.3);
        }}
        .note-delete-button:hover {{
            background-color: alpha(#ef5350, 0.15);
            color: #ef5350;
        }}
        popover contents {{
            background-color: {bg};
            color: {fg};
        }}
        popover.menu modelbutton:hover {{
            background-color: alpha({accent}, 0.12);
        }}
        .tangle-flash {{
            border: 3px solid {accent};
            box-shadow: 0 0 12px alpha({accent}, 0.5);
        }}
        .toolbar-toggle-btn {{
            min-width: 28px;
            min-height: 28px;
            padding: 2px 6px;
            border-radius: 4px;
            font-size: 14px;
            background-color: alpha({fg}, 0.06);
            border: 1px solid alpha({fg}, 0.06);
            color: {fg};
        }}
        .toolbar-toggle-btn:hover {{
            background-color: alpha({accent}, 0.15);
        }}
        .chromeless-button {{
            border-radius: 50%;
            min-width: 32px;
            min-height: 32px;
            padding: 0;
            background-color: alpha({fg}, 0.08);
            border: 1px solid alpha({fg}, 0.1);
            color: {fg};
        }}
        .chromeless-button:hover {{
            background-color: alpha({accent}, 0.2);
        }}
        .star-button {{
            border-radius: 50%;
            min-width: 32px;
            min-height: 32px;
            padding: 0;
            background-color: alpha({fg}, 0.08);
            border: 1px solid alpha({fg}, 0.1);
            font-size: 16px;
        }}
        .star-button:hover {{
            background-color: alpha({accent}, 0.2);
        }}
        .star-color-btn {{
            min-width: 28px;
            min-height: 28px;
            padding: 2px;
            background: none;
            border: none;
            font-size: 18px;
        }}
        .resize-grip {{
            opacity: 0.4;
            transition: opacity 150ms ease;
        }}
        .resize-grip:hover {{
            opacity: 0.8;
        }}
    "#, bg = bg, fg = fg, accent = accent);

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&css);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
}

/// Build the visual HSV color picker widget.
pub fn build_color_picker_widget() -> gtk4::Box {
    // Stub — the picker is built inline in show_global_theme_dialog
    gtk4::Box::new(gtk4::Orientation::Vertical, 0)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let h = h / 60.0;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        5 => (v, p, q),
        _ => (v, v, v),
    }
}

fn hex_to_hsv(hex: &str) -> Option<(f64, f64, f64)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    Some((h, s, v))
}

fn parse_hex_triple(hex: &str) -> (f64, f64, f64) {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128) as f64 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128) as f64 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128) as f64 / 255.0;
        (r, g, b)
    } else {
        (0.5, 0.5, 0.5)
    }
}
