use gtk4::prelude::*;
use gtk4::{glib, ApplicationWindow, Button, Entry, Label};
use gtk4::gdk::prelude::ToplevelExt;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use crate::database::{Database, Note};
use crate::rich_editor::RichEditor;

pub struct NoteWindow {
    pub window: ApplicationWindow,
}

impl NoteWindow {
    pub fn new(app: &gtk4::Application, db: Database, note: Option<Note>) -> Self {
        let note = note.unwrap_or_else(|| Note {
            id: None,
            title: "New Note".to_string(),
            content: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            position_x: 100.0,
            position_y: 100.0,
            is_visible: true,
            always_on_top: false,
            width: 500,
            height: 400,
            theme_bg: None,
            theme_fg: None,
            theme_accent: None,
            custom_colors: None,
        });

        let win_w = if note.width > 0 { note.width } else { 500 };
        let win_h = if note.height > 0 { note.height } else { 400 };

        let window = ApplicationWindow::builder()
            .application(app)
            .title(&note.title)
            .default_width(win_w)
            .default_height(win_h)
            .build();

        window.add_css_class("note-window");

        // Restore saved position on X11
        let pos_x = note.position_x as i32;
        let pos_y = note.position_y as i32;
        let note_title_for_pos = note.title.clone();
        if pos_x > 0 || pos_y > 0 {
            window.connect_realize(move |_| {
                let title = note_title_for_pos.clone();
                let x = pos_x;
                let y = pos_y;
                glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                    let _ = std::process::Command::new("wmctrl")
                        .args(["-r", &title, "-e", &format!("0,{},{},{},{}", x, y, -1, -1)])
                        .spawn();
                });
            });
        }

        // Unique class for per-note theming
        let note_class = if let Some(id) = note.id {
            format!("note-{}", id)
        } else {
            format!("note-new-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis())
        };
        window.add_css_class(&note_class);

        // Apply chromeless setting
        let chromeless = db.get_setting("chromeless_notes")
            .map(|v| v == "true")
            .unwrap_or(false);
        if chromeless {
            window.set_decorated(false);
            window.add_css_class("chromeless");
        }

        // Create UI
        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        // Title bar
        let title_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .build();

        let title_entry = Entry::builder()
            .text(&note.title)
            .placeholder_text("Note Title...")
            .hexpand(true)
            .css_classes(["note-title-entry"])
            .build();

        let palette_btn = Button::builder()
            .label("\u{1f3a8}")
            .tooltip_text("Note theme")
            .css_classes(["palette-button"])
            .build();

        let always_on_top_btn = Button::builder()
            .label("\u{1f4cc}")
            .tooltip_text("Toggle Always on Top")
            .css_classes(["pin-button"])
            .build();

        title_box.append(&title_entry);
        title_box.append(&palette_btn);
        title_box.append(&always_on_top_btn);
        main_box.append(&title_box);

        // Content area
        let content_frame = gtk4::Frame::builder()
            .css_classes(["content-frame"])
            .vexpand(true)
            .build();

        let editor = RichEditor::new(db.clone(), app.clone(), &note.title);
        editor.set_content(&note.content);
        let source_buf_for_autosave = editor.get_source_buffer().clone();
        let editor_ref: Rc<RichEditor> = Rc::new(editor);

        content_frame.set_child(Some(&editor_ref.widget.clone()));
        main_box.append(&content_frame);

        // Backlinks pane
        let backlinks_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["backlinks-pane"])
            .build();
        backlinks_box.set_visible(false);
        main_box.append(&backlinks_box);

        // Button bar
        let button_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk4::Align::End)
            .build();

        let close_btn = Button::builder()
            .label("Close")
            .css_classes(["close-button"])
            .build();

        button_box.append(&close_btn);
        main_box.append(&button_box);

        window.set_child(Some(&main_box));

        // -- Per-note theme --
        let theme_bg: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(note.theme_bg.clone()));
        let theme_fg: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(note.theme_fg.clone()));
        let theme_accent: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(note.theme_accent.clone()));
        let custom_colors: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(note.custom_colors.clone()));

        let theme_provider = gtk4::CssProvider::new();
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().unwrap(),
            &theme_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );

        let note_class_ref = note_class.clone();
        apply_note_theme(&theme_provider, &note_class_ref, &note.theme_bg, &note.theme_fg, &note.theme_accent);

        // Palette button
        let tb = theme_bg.clone();
        let tf = theme_fg.clone();
        let ta = theme_accent.clone();
        let cc = custom_colors.clone();
        let tp = theme_provider.clone();
        let nc = note_class.clone();
        let prev_popover: Rc<RefCell<Option<gtk4::Popover>>> = Rc::new(RefCell::new(None));
        palette_btn.connect_clicked(move |btn| {
            // Unparent any previous popover to avoid GTK stacking issue
            if let Some(old) = prev_popover.borrow_mut().take() {
                old.unparent();
            }
            let popover = show_theme_picker(btn, &tb, &tf, &ta, &cc, &tp, &nc);
            *prev_popover.borrow_mut() = Some(popover);
        });

        // -- Geometry cache (updated by background thread, never blocks UI) --
        let cached_geo: Arc<Mutex<(i32, i32, i32, i32)>> = Arc::new(Mutex::new((
            note.position_x as i32, note.position_y as i32, win_w, win_h,
        )));

        // Background geometry polling (runs wmctrl off the main thread)
        let win_for_geo = window.clone();
        let cached_geo_poll = cached_geo.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            if !win_for_geo.is_visible() {
                return glib::ControlFlow::Break;
            }
            let title = win_for_geo.title().map(|t| t.to_string()).unwrap_or_default();
            if title.is_empty() {
                return glib::ControlFlow::Continue;
            }
            let cache = cached_geo_poll.clone();
            std::thread::spawn(move || {
                if let Some(geo) = query_wmctrl_geometry(&title) {
                    *cache.lock().unwrap() = geo;
                }
            });
            glib::ControlFlow::Continue
        });

        // -- Shared save logic --
        let note_id: Rc<RefCell<Option<i64>>> = Rc::new(RefCell::new(note.id));
        let note_template = Rc::new(note.clone());
        let note_class_for_save = Rc::new(RefCell::new(note_class.clone()));

        let do_save = {
            let note_id = note_id.clone();
            let note_template = note_template.clone();
            let db = db.clone();
            let title_entry = title_entry.clone();
            let editor_ref = editor_ref.clone();
            let window_ref = window.clone();
            let theme_bg = theme_bg.clone();
            let theme_fg = theme_fg.clone();
            let theme_accent = theme_accent.clone();
            let custom_colors = custom_colors.clone();
            let note_class_ref = note_class_for_save.clone();
            let win_for_class = window.clone();
            let cached_geo = cached_geo.clone();

            Rc::new(move || {
                let title = title_entry.text().to_string();
                let content = editor_ref.get_content();

                let current_id = *note_id.borrow();
                if current_id.is_none() && title == "New Note" && content.is_empty() {
                    return;
                }

                let mut save_note = (*note_template).clone();
                save_note.id = current_id;
                save_note.title = title.clone();
                save_note.content = content;
                save_note.updated_at = chrono::Utc::now().to_rfc3339();

                // Read cached geometry (instant — no subprocess)
                let (gx, gy, gw, gh) = *cached_geo.lock().unwrap();
                save_note.position_x = gx as f64;
                save_note.position_y = gy as f64;
                if gw > 0 { save_note.width = gw; }
                if gh > 0 { save_note.height = gh; }

                save_note.theme_bg = theme_bg.borrow().clone();
                save_note.theme_fg = theme_fg.borrow().clone();
                save_note.theme_accent = theme_accent.borrow().clone();
                save_note.custom_colors = custom_colors.borrow().clone();

                if current_id.is_some() {
                    let db = db.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = db.update_note(&save_note) {
                            eprintln!("Error updating note: {}", e);
                        }
                    });
                } else {
                    let db_bg = db.clone();
                    let note_id_ref = note_id.clone();
                    let note_class_ref2 = note_class_ref.clone();
                    let win_ref = win_for_class.clone();
                    let (tx, rx) = std::sync::mpsc::channel::<i64>();
                    std::thread::spawn(move || {
                        match db_bg.create_note(&save_note) {
                            Ok(id) => { let _ = tx.send(id); }
                            Err(e) => eprintln!("Error creating note: {}", e),
                        }
                    });
                    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                        match rx.try_recv() {
                            Ok(id) => {
                                *note_id_ref.borrow_mut() = Some(id);
                                let old_class = note_class_ref2.borrow().clone();
                                let new_class = format!("note-{}", id);
                                win_ref.remove_css_class(&old_class);
                                win_ref.add_css_class(&new_class);
                                *note_class_ref2.borrow_mut() = new_class;
                                glib::ControlFlow::Break
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                            Err(_) => glib::ControlFlow::Break,
                        }
                    });
                }

                window_ref.set_title(Some(&title));
            })
        };

        // -- Autosave: debounce 5 seconds after any edit --
        let autosave_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        let schedule_autosave = {
            let autosave_timer = autosave_timer.clone();
            let do_save = do_save.clone();

            Rc::new(move || {
                if let Some(id) = autosave_timer.borrow_mut().take() {
                    id.remove();
                }
                let save_fn = do_save.clone();
                let timer_ref = autosave_timer.clone();
                let source_id = glib::timeout_add_local_once(
                    std::time::Duration::from_secs(5),
                    move || {
                        save_fn();
                        *timer_ref.borrow_mut() = None;
                    },
                );
                *autosave_timer.borrow_mut() = Some(source_id);
            })
        };

        // Connect both rich and source buffers to autosave
        let schedule_ref = schedule_autosave.clone();
        let buf_for_autosave = editor_ref_buffer(&window);
        if let Some(buf) = buf_for_autosave {
            buf.connect_changed(move |_| {
                schedule_ref();
            });
        }
        let schedule_ref_src = schedule_autosave.clone();
        source_buf_for_autosave.connect_changed(move |_| {
            schedule_ref_src();
        });

        let schedule_ref2 = schedule_autosave.clone();
        let editor_for_title = editor_ref.clone();
        title_entry.connect_changed(move |entry| {
            editor_for_title.set_own_title(&entry.text());
            schedule_ref2();
        });

        // Periodic geometry save (catches moves/resizes without content edits)
        let do_save_geo = do_save.clone();
        let win_alive = window.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
            if win_alive.is_visible() {
                do_save_geo();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        // Backlinks refresh
        let db_bl = db.clone();
        let app_bl = app.clone();
        let title_bl = title_entry.clone();
        let bl_box = backlinks_box.clone();
        let refresh_backlinks = Rc::new(move || {
            refresh_backlinks_pane(&bl_box, &db_bl, &title_bl.text(), &app_bl);
        });

        // Initial backlinks population
        let refresh_init = refresh_backlinks.clone();
        glib::idle_add_local_once(move || {
            refresh_init();
        });

        // Periodic backlinks refresh (every 15 seconds)
        let refresh_periodic = refresh_backlinks.clone();
        let win_bl = window.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(15), move || {
            if win_bl.is_visible() {
                refresh_periodic();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        // Close — save geometry BEFORE hiding, then close
        let do_save_close = do_save.clone();
        let window_for_close = window.clone();
        close_btn.connect_clicked(move |_| {
            do_save_close(); // save while window is still visible/mapped
            window_for_close.close();
        });

        let do_save_wm = do_save.clone();
        let cached_geo_close = cached_geo.clone();
        window.connect_close_request(move |win| {
            if win.is_visible() {
                // Final sync geometry snapshot before closing
                if let Some(title) = win.title() {
                    if let Some(geo) = query_wmctrl_geometry(&title.to_string()) {
                        *cached_geo_close.lock().unwrap() = geo;
                    }
                }
                do_save_wm();
                win.set_visible(false);
            }
            glib::Propagation::Proceed
        });

        // Always on top toggle
        let is_pinned = Rc::new(RefCell::new(note.always_on_top));
        let window_for_pin = window.clone();
        let pin_btn_ref = always_on_top_btn.clone();
        if note.always_on_top {
            always_on_top_btn.add_css_class("pinned");
            // Apply on-top after the window is mapped
            let win = window.clone();
            glib::idle_add_local_once(move || {
                set_window_above(&win, true);
            });
        }
        always_on_top_btn.connect_clicked(move |_| {
            let mut pinned = is_pinned.borrow_mut();
            *pinned = !*pinned;
            if *pinned {
                pin_btn_ref.add_css_class("pinned");
                pin_btn_ref.set_tooltip_text(Some("Pinned on top (click to unpin)"));
            } else {
                pin_btn_ref.remove_css_class("pinned");
                pin_btn_ref.set_tooltip_text(Some("Toggle Always on Top"));
            }
            window_for_pin.present();
            let win = window_for_pin.clone();
            let above = *pinned;
            glib::idle_add_local_once(move || {
                set_window_above(&win, above);
            });
        });

        // Edge-resize gesture for chromeless windows
        if chromeless {
            let edge_drag = gtk4::GestureDrag::builder().button(1).build();
            let win_for_edge = window.clone();
            edge_drag.connect_drag_begin(move |gesture, x, y| {
                let w = win_for_edge.width() as f64;
                let h = win_for_edge.height() as f64;
                if let Some(edge) = determine_edge(x, y, w, h, 8.0) {
                    if let Some(surface) = win_for_edge.surface() {
                        if let Some(toplevel) = surface.downcast_ref::<gtk4::gdk::Toplevel>() {
                            let device = gesture.device().unwrap();
                            let timestamp = gesture.current_event_time();
                            let (sx, sy) = if let Some(event) = gesture.last_event(gesture.current_sequence().as_ref()) {
                                event.position().unwrap_or((x, y))
                            } else {
                                (x, y)
                            };
                            toplevel.begin_resize(edge, Some(&device), 1, sx, sy, timestamp);
                        }
                    }
                }
            });
            main_box.add_controller(edge_drag);
        }

        NoteWindow { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn editor_ref_buffer(window: &ApplicationWindow) -> Option<gtk4::TextBuffer> {
    // Walk the widget tree to find the TextView
    let main_box = window.child()?;
    let main_box = main_box.downcast::<gtk4::Box>().ok()?;

    // content_frame is the second child (after title_box)
    let mut child = main_box.first_child();
    child = child?.next_sibling(); // skip title_box
    let frame = child?.downcast::<gtk4::Frame>().ok()?;

    // Inside frame: editor.widget (Box) -> [FlowBox toolbar, ScrolledWindow]
    let editor_box = frame.child()?.downcast::<gtk4::Box>().ok()?;
    // Find the ScrolledWindow child
    let mut w = editor_box.first_child();
    while let Some(ref widget) = w {
        if let Ok(scrolled) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
            let text_view = scrolled.child()?.downcast::<gtk4::TextView>().ok()?;
            return Some(text_view.buffer());
        }
        w = widget.next_sibling();
    }
    None
}

// ── Theme picker ───────────────────────────────────────────────────

fn show_theme_picker(
    relative_to: &Button,
    theme_bg: &Rc<RefCell<Option<String>>>,
    theme_fg: &Rc<RefCell<Option<String>>>,
    theme_accent: &Rc<RefCell<Option<String>>>,
    custom_colors: &Rc<RefCell<Option<String>>>,
    provider: &gtk4::CssProvider,
    note_class: &str,
) -> gtk4::Popover {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .css_classes(["color-picker-popover"])
        .build();

    vbox.append(&Label::builder().label("Note Theme").css_classes(["dim-label"]).build());

    let sections = [
        ("Background", "bg"),
        ("Text Color", "fg"),
        ("Accent", "accent"),
    ];

    let bg_colors: &[&str] = &[
        "#1a1a2e", "#16213e", "#1b1b2f", "#2d132c",
        "#1e3a2f", "#2c2c2c", "#f5f0e1", "#fef9ef",
        "#1c1c1c", "#0d1b2a",
    ];
    let fg_colors: &[&str] = &[
        "#ffffff", "#e0e0e0", "#b0b0b0", "#f5f5dc",
        "#a8dadc", "#fca311", "#1d1d1d", "#333333",
        "#c8b6ff", "#90e0ef",
    ];
    let accent_colors: &[&str] = &[
        "#b388ff", "#ff6b6b", "#4ecdc4", "#ffe66d",
        "#7c4dff", "#ff9f1c", "#06d6a0", "#ef476f",
        "#118ab2", "#e0aaff",
    ];

    // Parse existing custom colors
    let custom_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
        custom_colors.borrow().as_deref().unwrap_or("").split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.starts_with('#'))
            .collect()
    ));

    for (label, kind) in &sections {
        vbox.append(&Label::builder().label(*label).xalign(0.0).build());

        let colors = match *kind {
            "bg" => bg_colors,
            "fg" => fg_colors,
            _ => accent_colors,
        };

        // Preset color grid
        let flow = gtk4::FlowBox::builder()
            .max_children_per_line(5)
            .min_children_per_line(5)
            .selection_mode(gtk4::SelectionMode::None)
            .build();

        for color in colors {
            let btn = build_color_swatch_btn(color);

            let tb = theme_bg.clone();
            let tf = theme_fg.clone();
            let ta = theme_accent.clone();
            let tp = provider.clone();
            let nc = note_class.to_string();
            let k = kind.to_string();
            let cv = color.to_string();
            btn.connect_clicked(move |_| {
                match k.as_str() {
                    "bg" => *tb.borrow_mut() = Some(cv.clone()),
                    "fg" => *tf.borrow_mut() = Some(cv.clone()),
                    "accent" => *ta.borrow_mut() = Some(cv.clone()),
                    _ => {}
                }
                apply_note_theme(&tp, &nc, &tb.borrow(), &tf.borrow(), &ta.borrow());
            });

            flow.insert(&btn, -1);
        }

        vbox.append(&flow);

        // Custom colors row
        let custom_flow = gtk4::FlowBox::builder()
            .max_children_per_line(6)
            .min_children_per_line(1)
            .selection_mode(gtk4::SelectionMode::None)
            .build();

        // Populate existing custom swatches (left-click applies, right-click removes)
        for color in custom_list.borrow().iter() {
            add_custom_swatch_to_flow(
                &custom_flow, color, kind, theme_bg, theme_fg, theme_accent,
                provider, note_class, &custom_list, custom_colors,
            );
        }

        // Hex entry + "Add" button for custom colors
        let hex_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(4)
            .build();

        let hex_entry = gtk4::Entry::builder()
            .placeholder_text("#rrggbb")
            .max_width_chars(9)
            .width_chars(9)
            .build();

        let add_btn = Button::builder()
            .label("+")
            .tooltip_text("Add custom color")
            .build();

        let tb = theme_bg.clone();
        let tf = theme_fg.clone();
        let ta = theme_accent.clone();
        let tp = provider.clone();
        let nc = note_class.to_string();
        let k = kind.to_string();
        let cc = custom_colors.clone();
        let cl = custom_list.clone();
        let cf = custom_flow.clone();
        let he = hex_entry.clone();
        add_btn.connect_clicked(move |_| {
            let mut hex = he.text().to_string().trim().to_lowercase();
            if !hex.starts_with('#') {
                hex = format!("#{}", hex);
            }
            // Validate: must be #rrggbb
            if hex.len() != 7 || !hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                return;
            }

            // Add to custom list if not duplicate
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

            // Add swatch to flow
            add_custom_swatch_to_flow(
                &cf, &hex, &k, &tb, &tf, &ta,
                &tp, &nc, &cl, &cc,
            );

            // Apply immediately
            match k.as_str() {
                "bg" => *tb.borrow_mut() = Some(hex.clone()),
                "fg" => *tf.borrow_mut() = Some(hex.clone()),
                "accent" => *ta.borrow_mut() = Some(hex.clone()),
                _ => {}
            }
            apply_note_theme(&tp, &nc, &tb.borrow(), &tf.borrow(), &ta.borrow());
            he.set_text("");
        });

        hex_box.append(&hex_entry);
        hex_box.append(&add_btn);

        vbox.append(&custom_flow);
        vbox.append(&hex_box);
    }

    // Reset button
    let reset_btn = Button::builder().label("Reset to Default").build();
    let tb = theme_bg.clone();
    let tf = theme_fg.clone();
    let ta = theme_accent.clone();
    let cc = custom_colors.clone();
    let cl = custom_list.clone();
    let tp = provider.clone();
    let pop = popover.clone();
    reset_btn.connect_clicked(move |_| {
        *tb.borrow_mut() = None;
        *tf.borrow_mut() = None;
        *ta.borrow_mut() = None;
        *cc.borrow_mut() = None;
        cl.borrow_mut().clear();
        tp.load_from_data("");
        pop.popdown();
    });
    vbox.append(&reset_btn);

    popover.set_child(Some(&vbox));
    popover.popup();
    popover
}

fn build_color_swatch_btn(color: &str) -> Button {
    let swatch = gtk4::DrawingArea::builder()
        .width_request(28)
        .height_request(28)
        .build();
    let c = color.to_string();
    swatch.set_draw_func(move |_area, cr, w, h| {
        let (r, g, b) = parse_hex(&c);
        cr.set_source_rgba(r, g, b, 1.0);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = cr.fill();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
        cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    });

    Button::builder()
        .child(&swatch)
        .tooltip_text(color)
        .css_classes(["color-swatch-btn"])
        .build()
}

fn add_custom_swatch_to_flow(
    flow: &gtk4::FlowBox,
    color: &str,
    kind: &str,
    theme_bg: &Rc<RefCell<Option<String>>>,
    theme_fg: &Rc<RefCell<Option<String>>>,
    theme_accent: &Rc<RefCell<Option<String>>>,
    provider: &gtk4::CssProvider,
    note_class: &str,
    custom_list: &Rc<RefCell<Vec<String>>>,
    custom_colors: &Rc<RefCell<Option<String>>>,
) {
    let btn = build_color_swatch_btn(color);
    btn.set_tooltip_text(Some(&format!("{} (right-click to remove)", color)));

    // Left-click: apply color
    let tb = theme_bg.clone();
    let tf = theme_fg.clone();
    let ta = theme_accent.clone();
    let tp = provider.clone();
    let nc = note_class.to_string();
    let k = kind.to_string();
    let cv = color.to_string();
    btn.connect_clicked(move |_| {
        match k.as_str() {
            "bg" => *tb.borrow_mut() = Some(cv.clone()),
            "fg" => *tf.borrow_mut() = Some(cv.clone()),
            "accent" => *ta.borrow_mut() = Some(cv.clone()),
            _ => {}
        }
        apply_note_theme(&tp, &nc, &tb.borrow(), &tf.borrow(), &ta.borrow());
    });

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
        // Remove the swatch widget from the flow
        // Walk up from btn to the FlowBoxChild wrapper
        let mut child = flow_ref.first_child();
        while let Some(ref widget) = child {
            if let Ok(fbc) = widget.clone().downcast::<gtk4::FlowBoxChild>() {
                if let Some(inner_btn) = fbc.child() {
                    if inner_btn.tooltip_text().map_or(false, |t| t.starts_with(&cv)) {
                        flow_ref.remove(&fbc);
                        return;
                    }
                }
            }
            child = widget.next_sibling();
        }
    });
    btn.add_controller(right_click);

    flow.insert(&btn, -1);
}

fn apply_note_theme(
    provider: &gtk4::CssProvider,
    note_class: &str,
    bg: &Option<String>,
    fg: &Option<String>,
    accent: &Option<String>,
) {
    let nc = note_class;
    let mut css = String::new();

    // Determine fg_or_default for computing derived colors even with bg-only themes
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

fn parse_hex(hex: &str) -> (f64, f64, f64) {
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

fn query_wmctrl_geometry(win_title: &str) -> Option<(i32, i32, i32, i32)> {
    let output = std::process::Command::new("wmctrl")
        .args(["-l", "-G"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 8 && parts[7..].join(" ") == win_title {
            let x = parts[2].parse().ok()?;
            let y = parts[3].parse().ok()?;
            let w = parts[4].parse().ok()?;
            let h = parts[5].parse().ok()?;
            if w > 0 && h > 0 {
                return Some((x, y, w, h));
            }
        }
    }
    None
}

fn set_window_above(window: &ApplicationWindow, above: bool) {
    // Use wmctrl to set/unset _NET_WM_STATE_ABOVE on X11
    let title = window.title().unwrap_or_default().to_string();
    if title.is_empty() {
        return;
    }
    let action = if above { "add" } else { "remove" };
    let _ = std::process::Command::new("wmctrl")
        .args(["-r", &title, "-b", &format!("{},above", action)])
        .spawn();
}

fn refresh_backlinks_pane(
    backlinks_box: &gtk4::Box,
    db: &Database,
    title: &str,
    app: &gtk4::Application,
) {
    if title.is_empty() || title == "New Note" {
        backlinks_box.set_visible(false);
        return;
    }

    // DB query on background thread, UI update on main thread via channel
    let db_bg = db.clone();
    let title = title.to_string();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();

    std::thread::spawn(move || {
        let linking_notes = db_bg.get_notes_linking_to(&title).unwrap_or_default();
        let titles: Vec<String> = linking_notes.iter().map(|n| n.title.clone()).collect();
        let _ = tx.send(titles);
    });

    let bl_box = backlinks_box.clone();
    let db = db.clone();
    let app = app.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(titles) => {
                while let Some(child) = bl_box.first_child() {
                    bl_box.remove(&child);
                }

                if titles.is_empty() {
                    bl_box.set_visible(false);
                    return glib::ControlFlow::Break;
                }

                bl_box.set_visible(true);

                let label = Label::builder()
                    .label("Origin Tangles:")
                    .css_classes(["backlinks-label"])
                    .build();
                bl_box.append(&label);

                for note_title in &titles {
                    let btn = Button::builder()
                        .label(note_title)
                        .css_classes(["backlink-btn"])
                        .build();
                    let db_ref = db.clone();
                    let app_ref = app.clone();
                    let nt = note_title.clone();
                    btn.connect_clicked(move |_| {
                        crate::rich_editor::open_tangle_note(&db_ref, &app_ref, &nt);
                    });
                    bl_box.append(&btn);
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        }
    });
}

fn determine_edge(x: f64, y: f64, w: f64, h: f64, margin: f64) -> Option<gtk4::gdk::SurfaceEdge> {
    let left = x < margin;
    let right = x > w - margin;
    let top = y < margin;
    let bottom = y > h - margin;

    match (top, bottom, left, right) {
        (true, _, true, _) => Some(gtk4::gdk::SurfaceEdge::NorthWest),
        (true, _, _, true) => Some(gtk4::gdk::SurfaceEdge::NorthEast),
        (_, true, true, _) => Some(gtk4::gdk::SurfaceEdge::SouthWest),
        (_, true, _, true) => Some(gtk4::gdk::SurfaceEdge::SouthEast),
        (true, _, _, _) => Some(gtk4::gdk::SurfaceEdge::North),
        (_, true, _, _) => Some(gtk4::gdk::SurfaceEdge::South),
        (_, _, true, _) => Some(gtk4::gdk::SurfaceEdge::West),
        (_, _, _, true) => Some(gtk4::gdk::SurfaceEdge::East),
        _ => None,
    }
}
