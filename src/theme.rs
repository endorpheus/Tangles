use gtk4::prelude::*;
use gtk4::{Button, Label};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use crate::database::Database;

pub enum ThemeTarget {
    Global {
        db: Database,
    },
    Note {
        provider: gtk4::CssProvider,
        note_class: String,
        theme_bg: Rc<RefCell<Option<String>>>,
        theme_fg: Rc<RefCell<Option<String>>>,
        theme_accent: Rc<RefCell<Option<String>>>,
        custom_colors: Rc<RefCell<Option<String>>>,
    },
}

const BG_SWATCHES: &[&str] = &[
    "#1a1a2e", "#16213e", "#1b1b2f", "#2d132c", "#1e3a2f",
    "#2c2c2c", "#f5f0e1", "#fef9ef", "#1c1c1c", "#0d1b2a",
];
const FG_SWATCHES: &[&str] = &[
    "#ffffff", "#e0e0e0", "#b0b0b0", "#f5f5dc", "#a8dadc",
    "#fca311", "#1d1d1d", "#333333", "#c8b6ff", "#90e0ef",
];
const ACCENT_SWATCHES: &[&str] = &[
    "#b388ff", "#ff6b6b", "#4ecdc4", "#ffe66d", "#7c4dff",
    "#ff9f1c", "#06d6a0", "#ef476f", "#118ab2", "#e0aaff",
];

/// Build one HSV picker column with SV area, hue bar, preview, hex entry, and swatches.
/// Calls `on_change` with the hex string whenever the color changes.
fn build_picker_column(
    label: &str,
    current: &str,
    swatches: &[&str],
    on_change: Rc<dyn Fn(String)>,
) -> (gtk4::Box, gtk4::Entry) {
    let col = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();

    col.append(&Label::builder().label(label).css_classes(["dim-label"]).xalign(0.5).build());

    // HSV picker row
    let picker_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk4::Align::Center)
        .build();

    let sv_area = gtk4::DrawingArea::builder()
        .width_request(120)
        .height_request(120)
        .build();

    let hue_bar = gtk4::DrawingArea::builder()
        .width_request(16)
        .height_request(120)
        .build();

    let hue_val = Rc::new(Cell::new(0.0f64));
    let sat_val = Rc::new(Cell::new(1.0f64));
    let val_val = Rc::new(Cell::new(1.0f64));

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

    // Hex entry + preview
    let hex_entry = gtk4::Entry::builder()
        .text(current)
        .max_width_chars(9)
        .width_chars(9)
        .build();

    let preview = gtk4::DrawingArea::builder()
        .width_request(0)
        .height_request(20)
        .hexpand(true)
        .build();
    let hex_for_preview = hex_entry.clone();
    preview.set_draw_func(move |_area, cr, w, h| {
        let hex = hex_for_preview.text().to_string();
        let (r, g, b) = parse_hex_triple(&hex);
        cr.set_source_rgb(r, g, b);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = cr.fill();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
        cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });

    // SV click + drag
    let sv_click = gtk4::GestureClick::builder().button(1).build();
    let (sc, vc, hc) = (sat_val.clone(), val_val.clone(), hue_val.clone());
    let (hexc, prc, svc) = (hex_entry.clone(), preview.clone(), sv_area.clone());
    sv_click.connect_pressed(move |_, _, x, y| {
        let w = svc.width() as f64;
        let h = svc.height() as f64;
        let s = (x / w).clamp(0.0, 1.0);
        let v = (1.0 - y / h).clamp(0.0, 1.0);
        sc.set(s); vc.set(v);
        let (r, g, b) = hsv_to_rgb(hc.get(), s, v);
        hexc.set_text(&format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8));
        prc.queue_draw();
    });
    sv_area.add_controller(sv_click);

    let sv_drag = gtk4::GestureDrag::builder().button(1).build();
    let (sd, vd, hd) = (sat_val.clone(), val_val.clone(), hue_val.clone());
    let (hexd, prd, svd) = (hex_entry.clone(), preview.clone(), sv_area.clone());
    sv_drag.connect_drag_update(move |gesture, ox, oy| {
        if let Some((sx, sy)) = gesture.start_point() {
            let w = svd.width() as f64;
            let h = svd.height() as f64;
            let s = ((sx + ox) / w).clamp(0.0, 1.0);
            let v = (1.0 - (sy + oy) / h).clamp(0.0, 1.0);
            sd.set(s); vd.set(v);
            let (r, g, b) = hsv_to_rgb(hd.get(), s, v);
            hexd.set_text(&format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8));
            prd.queue_draw();
        }
    });
    sv_area.add_controller(sv_drag);

    // Hue bar click + drag
    let hue_click = gtk4::GestureClick::builder().button(1).build();
    let (hh, sh, vh) = (hue_val.clone(), sat_val.clone(), val_val.clone());
    let (hexh, svh, prh, hbh) = (hex_entry.clone(), sv_area.clone(), preview.clone(), hue_bar.clone());
    hue_click.connect_pressed(move |_, _, _, y| {
        let h = (y / hbh.height() as f64 * 360.0).clamp(0.0, 360.0);
        hh.set(h); svh.queue_draw();
        let (r, g, b) = hsv_to_rgb(h, sh.get(), vh.get());
        hexh.set_text(&format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8));
        prh.queue_draw();
    });
    hue_bar.add_controller(hue_click);

    let hue_drag = gtk4::GestureDrag::builder().button(1).build();
    let (hhd, shd, vhd) = (hue_val.clone(), sat_val.clone(), val_val.clone());
    let (hexhd, svhd, prhd, hbhd) = (hex_entry.clone(), sv_area.clone(), preview.clone(), hue_bar.clone());
    hue_drag.connect_drag_update(move |gesture, _, oy| {
        if let Some((_, sy)) = gesture.start_point() {
            let h = ((sy + oy) / hbhd.height() as f64 * 360.0).clamp(0.0, 360.0);
            hhd.set(h); svhd.queue_draw();
            let (r, g, b) = hsv_to_rgb(h, shd.get(), vhd.get());
            hexhd.set_text(&format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8));
            prhd.queue_draw();
        }
    });
    hue_bar.add_controller(hue_drag);

    picker_row.append(&sv_area);
    picker_row.append(&hue_bar);
    col.append(&picker_row);

    // Preview + hex entry row
    let entry_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk4::Align::Center)
        .build();
    entry_row.append(&preview);
    entry_row.append(&hex_entry);
    col.append(&entry_row);

    // Swatches grid
    let flow = gtk4::FlowBox::builder()
        .max_children_per_line(5)
        .min_children_per_line(5)
        .selection_mode(gtk4::SelectionMode::None)
        .row_spacing(2)
        .column_spacing(2)
        .halign(gtk4::Align::Center)
        .build();

    for &color in swatches {
        let btn = gtk4::DrawingArea::builder()
            .width_request(20)
            .height_request(20)
            .build();
        let c = color.to_string();
        btn.set_draw_func(move |_area, cr, w, h| {
            let (r, g, b) = parse_hex_triple(&c);
            cr.set_source_rgb(r, g, b);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = cr.fill();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
            cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });

        let click = gtk4::GestureClick::builder().button(1).build();
        let hex_e = hex_entry.clone();
        let pr = preview.clone();
        let sv = sv_area.clone();
        let hv = hue_val.clone();
        let sv2 = sat_val.clone();
        let vv = val_val.clone();
        let cv = color.to_string();
        click.connect_pressed(move |_, _, _, _| {
            hex_e.set_text(&cv);
            if let Some((h, s, v)) = hex_to_hsv(&cv) {
                hv.set(h); sv2.set(s); vv.set(v);
            }
            sv.queue_draw();
            pr.queue_draw();
        });
        btn.add_controller(click);

        flow.insert(&btn, -1);
    }
    col.append(&flow);

    // Fire on_change whenever hex entry changes with a valid color
    let oc = on_change;
    hex_entry.connect_changed(move |entry| {
        let hex = entry.text().to_string();
        if hex.len() == 7 && hex.starts_with('#') && hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            oc(hex);
        }
    });

    (col, hex_entry)
}

/// Unified theme editor for both global and per-note themes.
pub fn show_theme_editor(parent: &impl IsA<gtk4::Window>, target: ThemeTarget) -> gtk4::Window {
    let is_note = matches!(&target, ThemeTarget::Note { .. });

    let win = gtk4::Window::builder()
        .title(if is_note { "Note Theme" } else { "Theme Settings" })
        .default_width(520)
        .default_height(if is_note { 440 } else { 380 })
        .transient_for(parent)
        .modal(false)
        .build();
    win.add_css_class("note-list-dialog");

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    vbox.append(&Label::builder()
        .label(if is_note { "Note Theme" } else { "Global Theme" })
        .css_classes(["heading"])
        .build());

    // Get current values
    let (current_bg, current_fg, current_accent) = match &target {
        ThemeTarget::Global { db } => (
            db.get_setting("global_theme_bg").unwrap_or_else(|| "#1a1a2e".to_string()),
            db.get_setting("global_theme_fg").unwrap_or_else(|| "#e0e0e0".to_string()),
            db.get_setting("global_theme_accent").unwrap_or_else(|| "#b388ff".to_string()),
        ),
        ThemeTarget::Note { theme_bg, theme_fg, theme_accent, .. } => (
            theme_bg.borrow().clone().unwrap_or_else(|| "#1a1a2e".to_string()),
            theme_fg.borrow().clone().unwrap_or_else(|| "#e0e0e0".to_string()),
            theme_accent.borrow().clone().unwrap_or_else(|| "#b388ff".to_string()),
        ),
    };

    // Create on_change callbacks based on target variant
    let bg_cb: Rc<dyn Fn(String)>;
    let fg_cb: Rc<dyn Fn(String)>;
    let accent_cb: Rc<dyn Fn(String)>;

    match &target {
        ThemeTarget::Global { db } => {
            let db1 = db.clone();
            let db2 = db.clone();
            let db3 = db.clone();
            bg_cb = Rc::new(move |hex: String| {
                let _ = db1.set_setting("global_theme_bg", &hex);
                apply_global_theme(&db1);
            });
            fg_cb = Rc::new(move |hex: String| {
                let _ = db2.set_setting("global_theme_fg", &hex);
                apply_global_theme(&db2);
            });
            accent_cb = Rc::new(move |hex: String| {
                let _ = db3.set_setting("global_theme_accent", &hex);
                apply_global_theme(&db3);
            });
        }
        ThemeTarget::Note { provider, note_class, theme_bg, theme_fg, theme_accent, .. } => {
            let (tb1, tf1, ta1) = (theme_bg.clone(), theme_fg.clone(), theme_accent.clone());
            let (tp1, nc1) = (provider.clone(), note_class.clone());
            bg_cb = Rc::new(move |hex: String| {
                *tb1.borrow_mut() = Some(hex);
                apply_note_theme(&tp1, &nc1, &tb1.borrow(), &tf1.borrow(), &ta1.borrow());
            });

            let (tb2, tf2, ta2) = (theme_bg.clone(), theme_fg.clone(), theme_accent.clone());
            let (tp2, nc2) = (provider.clone(), note_class.clone());
            fg_cb = Rc::new(move |hex: String| {
                *tf2.borrow_mut() = Some(hex);
                apply_note_theme(&tp2, &nc2, &tb2.borrow(), &tf2.borrow(), &ta2.borrow());
            });

            let (tb3, tf3, ta3) = (theme_bg.clone(), theme_fg.clone(), theme_accent.clone());
            let (tp3, nc3) = (provider.clone(), note_class.clone());
            accent_cb = Rc::new(move |hex: String| {
                *ta3.borrow_mut() = Some(hex);
                apply_note_theme(&tp3, &nc3, &tb3.borrow(), &tf3.borrow(), &ta3.borrow());
            });
        }
    }

    // Build 3 columns
    let (bg_col, bg_entry) = build_picker_column("Background", &current_bg, BG_SWATCHES, bg_cb);
    let (fg_col, fg_entry) = build_picker_column("Text", &current_fg, FG_SWATCHES, fg_cb);
    let (accent_col, accent_entry) = build_picker_column("Accent", &current_accent, ACCENT_SWATCHES, accent_cb);

    let columns = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .build();
    columns.append(&bg_col);
    columns.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));
    columns.append(&fg_col);
    columns.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));
    columns.append(&accent_col);
    vbox.append(&columns);

    // Mode-specific UI
    match &target {
        ThemeTarget::Global { .. } => {
            let btn_row = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(8)
                .halign(gtk4::Align::End)
                .margin_top(8)
                .build();

            let reset_btn = Button::builder().label("Reset to Default").build();
            let bg_e = bg_entry.clone();
            let fg_e = fg_entry.clone();
            let ac_e = accent_entry.clone();
            reset_btn.connect_clicked(move |_| {
                bg_e.set_text("#1a1a2e");
                fg_e.set_text("#e0e0e0");
                ac_e.set_text("#b388ff");
            });

            btn_row.append(&reset_btn);
            vbox.append(&btn_row);
        }
        ThemeTarget::Note { provider, theme_bg, theme_fg, theme_accent, custom_colors, .. } => {
            // Custom colors section
            let custom_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
                custom_colors.borrow().as_deref().unwrap_or("").split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s.starts_with('#'))
                    .collect()
            ));

            // Track last focused column entry for applying custom colors
            let last_entry: Rc<RefCell<gtk4::Entry>> = Rc::new(RefCell::new(bg_entry.clone()));
            for entry in &[bg_entry.clone(), fg_entry.clone(), accent_entry.clone()] {
                let le = last_entry.clone();
                let e = entry.clone();
                let focus_ctl = gtk4::EventControllerFocus::new();
                focus_ctl.connect_enter(move |_| {
                    *le.borrow_mut() = e.clone();
                });
                entry.add_controller(focus_ctl);
            }

            vbox.append(&Label::builder()
                .label("Custom Colors (click to apply, right-click to remove)")
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build());

            let custom_flow = gtk4::FlowBox::builder()
                .max_children_per_line(10)
                .min_children_per_line(1)
                .selection_mode(gtk4::SelectionMode::None)
                .row_spacing(2)
                .column_spacing(2)
                .build();

            for color in custom_list.borrow().iter() {
                add_custom_swatch_widget(
                    &custom_flow, color, &last_entry, &custom_list, custom_colors,
                );
            }

            vbox.append(&custom_flow);

            // Hex entry + Add button for custom colors
            let add_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(4)
                .build();

            let custom_entry = gtk4::Entry::builder()
                .placeholder_text("#rrggbb")
                .max_width_chars(9)
                .width_chars(9)
                .build();

            let add_btn = Button::builder().label("+").tooltip_text("Add custom color").build();

            let cl = custom_list.clone();
            let cc = custom_colors.clone();
            let cf = custom_flow.clone();
            let he = custom_entry.clone();
            let le = last_entry.clone();
            add_btn.connect_clicked(move |_| {
                let mut hex = he.text().to_string().trim().to_lowercase();
                if !hex.starts_with('#') {
                    hex = format!("#{}", hex);
                }
                if hex.len() != 7 || !hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    return;
                }
                {
                    let mut list = cl.borrow_mut();
                    if !list.contains(&hex) {
                        list.push(hex.clone());
                    }
                    *cc.borrow_mut() = if list.is_empty() {
                        None
                    } else {
                        Some(list.join(","))
                    };
                }
                add_custom_swatch_widget(&cf, &hex, &le, &cl, &cc);
                // Apply immediately to last focused column
                le.borrow().set_text(&hex);
                he.set_text("");
            });

            add_box.append(&custom_entry);
            add_box.append(&add_btn);
            vbox.append(&add_box);

            // Reset to Global button
            let btn_row = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(8)
                .halign(gtk4::Align::End)
                .margin_top(8)
                .build();

            let reset_btn = Button::builder().label("Reset to Global").build();
            let tb = theme_bg.clone();
            let tf = theme_fg.clone();
            let ta = theme_accent.clone();
            let cc = custom_colors.clone();
            let tp = provider.clone();
            let win_ref = win.clone();
            reset_btn.connect_clicked(move |_| {
                *tb.borrow_mut() = None;
                *tf.borrow_mut() = None;
                *ta.borrow_mut() = None;
                *cc.borrow_mut() = None;
                tp.load_from_data("");
                win_ref.close();
            });

            btn_row.append(&reset_btn);
            vbox.append(&btn_row);
        }
    }

    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&vbox)
        .vexpand(true)
        .build();
    win.set_child(Some(&scrolled));
    win
}

fn add_custom_swatch_widget(
    flow: &gtk4::FlowBox,
    color: &str,
    last_entry: &Rc<RefCell<gtk4::Entry>>,
    custom_list: &Rc<RefCell<Vec<String>>>,
    custom_colors: &Rc<RefCell<Option<String>>>,
) {
    let swatch = gtk4::DrawingArea::builder()
        .width_request(20)
        .height_request(20)
        .build();
    let c = color.to_string();
    swatch.set_draw_func(move |_area, cr, w, h| {
        let (r, g, b) = parse_hex_triple(&c);
        cr.set_source_rgb(r, g, b);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = cr.fill();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
        cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });
    swatch.set_tooltip_text(Some(color));

    // Left-click: apply to last focused column
    let click = gtk4::GestureClick::builder().button(1).build();
    let le = last_entry.clone();
    let cv = color.to_string();
    click.connect_pressed(move |_, _, _, _| {
        le.borrow().set_text(&cv);
    });
    swatch.add_controller(click);

    // Right-click: remove from custom colors
    let right_click = gtk4::GestureClick::builder().button(3).build();
    let cl = custom_list.clone();
    let cc = custom_colors.clone();
    let cv = color.to_string();
    let flow_ref = flow.clone();
    right_click.connect_pressed(move |_, _, _, _| {
        cl.borrow_mut().retain(|c| c != &cv);
        *cc.borrow_mut() = {
            let list = cl.borrow();
            if list.is_empty() { None } else { Some(list.join(",")) }
        };
        let mut child = flow_ref.first_child();
        while let Some(ref widget) = child {
            if let Ok(fbc) = widget.clone().downcast::<gtk4::FlowBoxChild>() {
                if fbc.child().and_then(|w| w.tooltip_text()).map_or(false, |t| t.as_str() == cv) {
                    flow_ref.remove(&fbc);
                    return;
                }
            }
            child = widget.next_sibling();
        }
    });
    swatch.add_controller(right_click);

    flow.insert(&swatch, -1);
}

/// Apply per-note theme CSS via the given provider.
pub fn apply_note_theme(
    provider: &gtk4::CssProvider,
    note_class: &str,
    bg: &Option<String>,
    fg: &Option<String>,
    accent: &Option<String>,
) {
    let nc = note_class;
    let mut css = String::new();

    let fg_or_default = fg.as_deref().unwrap_or("@theme_fg_color");

    if let Some(bg_color) = bg {
        css.push_str(&format!(
            "window.{nc}.note-window {{ background-color: {bg}; }}\n\
             window.{nc}.note-window box {{ background-color: transparent; }}\n\
             window.{nc} .note-title-entry {{ background-color: alpha({bg}, 0.7); border-color: alpha({fg}, 0.12); }}\n\
             window.{nc} .rich-toolbar {{ background-color: alpha({bg}, 0.85); }}\n\
             window.{nc} .rich-toolbar button {{ background-color: alpha({fg}, 0.08); border-color: alpha({fg}, 0.06); }}\n\
             window.{nc} textview.rich-editor text {{ background-color: alpha({fg}, 0.04); }}\n\
             window.{nc} .content-frame {{ border-color: alpha({fg}, 0.08); background-color: transparent; }}\n\
             window.{nc} .pin-button {{ background-color: alpha({fg}, 0.08); }}\n\
             window.{nc} .palette-button {{ background-color: alpha({fg}, 0.08); }}\n\
             window.{nc} .close-button {{ background-color: alpha({fg}, 0.08); }}\n\
             window.{nc} .backlinks-pane {{ background-color: transparent; }}\n",
            nc = nc, bg = bg_color, fg = fg_or_default
        ));
    }

    if let Some(fg_color) = fg {
        css.push_str(&format!(
            "window.{nc} {{ color: {fg}; }}\n\
             window.{nc} textview.rich-editor text {{ color: {fg}; }}\n\
             window.{nc} .note-title-entry {{ color: {fg}; }}\n\
             window.{nc} label {{ color: {fg}; }}\n\
             window.{nc} button {{ color: {fg}; }}\n",
            nc = nc, fg = fg_color
        ));
    }

    if let Some(accent_color) = accent {
        css.push_str(&format!(
            "window.{nc} .note-title-entry:focus {{ border-color: {ac}; box-shadow: 0 0 0 2px alpha({ac}, 0.25); }}\n\
             window.{nc} .pin-button.pinned {{ background-color: alpha({ac}, 0.3); border-color: {ac}; color: {ac}; }}\n\
             window.{nc} .pin-button:hover {{ background-color: alpha({ac}, 0.2); }}\n\
             window.{nc} .rich-toolbar button:hover {{ background-color: alpha({ac}, 0.15); }}\n\
             window.{nc} .backlink-btn {{ color: {ac}; }}\n\
             window.{nc} .backlink-btn:hover {{ background-color: alpha({ac}, 0.15); }}\n",
            nc = nc, ac = accent_color,
        ));
    }

    if css.is_empty() {
        css.push_str("/* no theme */");
    }

    provider.load_from_data(&css);
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
        .note-list-dialog list {{
            background-color: transparent;
        }}
        .note-list-dialog list row {{
            background-color: transparent;
            color: {fg};
        }}
        .note-list-dialog list row:hover {{
            background-color: alpha({accent}, 0.08);
        }}
        .note-list-dialog list row:selected {{
            background-color: alpha({accent}, 0.15);
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
        .note-row {{
            background-color: transparent;
        }}
        .note-row:hover {{
            background-color: alpha({accent}, 0.08);
        }}
        .note-row-title {{
            color: {fg};
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
            font-size: 22px;
        }}
        .star-button:hover {{
            background-color: alpha({accent}, 0.2);
        }}
        .star-color-btn {{
            min-width: 32px;
            min-height: 32px;
            padding: 2px;
            background: none;
            border: none;
            font-size: 24px;
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

pub fn parse_hex_triple(hex: &str) -> (f64, f64, f64) {
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
