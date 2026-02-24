use gtk4::prelude::*;
use gtk4::{glib, ApplicationWindow, Button, Entry, Label};
use gtk4::gdk::prelude::ToplevelExt;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::database::{Database, Note};
use crate::rich_editor::RichEditor;

static APP_QUITTING: AtomicBool = AtomicBool::new(false);

pub fn set_app_quitting(val: bool) {
    APP_QUITTING.store(val, Ordering::SeqCst);
}

fn is_app_quitting() -> bool {
    APP_QUITTING.load(Ordering::SeqCst)
}

pub struct NoteWindow {
    pub window: ApplicationWindow,
}

impl NoteWindow {
    pub fn new(app: &gtk4::Application, db: Database, note: Option<Note>) -> Self {
        let note = note.unwrap_or_else(|| Note {
            id: None,
            title: "New Tangle".to_string(),
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
            chromeless: false,
            star_color: None,
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

        // Apply per-note chromeless setting
        let chromeless = note.chromeless;
        let is_chromeless: Rc<RefCell<bool>> = Rc::new(RefCell::new(chromeless));
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
            .placeholder_text("Tangle Title...")
            .hexpand(true)
            .css_classes(["note-title-entry"])
            .build();

        let palette_btn = Button::builder()
            .label("\u{1f3a8}")
            .tooltip_text("Tangle theme")
            .css_classes(["palette-button"])
            .build();

        // Star button for labeling
        let star_color_rc: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(note.star_color.clone()));
        let star_btn = Button::builder()
            .label(if note.star_color.is_some() { "\u{2605}" } else { "\u{2606}" })
            .tooltip_text("Star label")
            .css_classes(["star-button"])
            .build();
        if let Some(ref color) = note.star_color {
            let lbl = Label::new(None);
            lbl.set_markup(&format!("<span foreground=\"{}\">\u{2605}</span>", color));
            star_btn.set_child(Some(&lbl));
        }

        // Chromeless toggle per-tangle
        let chromeless_btn = Button::builder()
            .label(if note.chromeless { "\u{25a1}" } else { "\u{25a0}" })
            .tooltip_text("Toggle chromeless (borderless)")
            .css_classes(["chromeless-button"])
            .build();

        let always_on_top_btn = Button::builder()
            .label("\u{1f4cc}")
            .tooltip_text("Toggle Always on Top")
            .css_classes(["pin-button"])
            .build();

        // Create editor early so we can grab its hamburger button for the title bar
        let editor = RichEditor::new(db.clone(), app.clone(), &note.title);
        editor.set_content(&note.content);
        let source_buf_for_autosave = editor.get_source_buffer().clone();

        title_box.append(&title_entry);
        title_box.append(&editor.hamburger_btn);
        title_box.append(&star_btn);
        title_box.append(&chromeless_btn);
        title_box.append(&palette_btn);
        title_box.append(&always_on_top_btn);
        main_box.append(&title_box);

        // Content area
        let content_frame = gtk4::Frame::builder()
            .css_classes(["content-frame"])
            .vexpand(true)
            .build();

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
        crate::theme::apply_note_theme(&theme_provider, &note_class_ref, &note.theme_bg, &note.theme_fg, &note.theme_accent);

        // -- Geometry cache (updated by background thread, never blocks UI) --
        let cached_geo: Arc<Mutex<(i32, i32, i32, i32)>> = Arc::new(Mutex::new((
            note.position_x as i32, note.position_y as i32, win_w, win_h,
        )));

        // Background geometry polling (runs wmctrl off the main thread)
        let win_for_geo = window.clone();
        let cached_geo_poll = cached_geo.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(3), move || {
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

        let is_pinned: Rc<RefCell<bool>> = Rc::new(RefCell::new(note.always_on_top));

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
            let is_chromeless = is_chromeless.clone();
            let star_color_rc = star_color_rc.clone();
            let is_pinned = is_pinned.clone();

            Rc::new(move || {
                let title = title_entry.text().to_string();
                let content = editor_ref.get_content();

                let current_id = *note_id.borrow();
                if current_id.is_none() && title == "New Tangle" && content.is_empty() {
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
                save_note.chromeless = *is_chromeless.borrow();
                save_note.star_color = star_color_rc.borrow().clone();
                save_note.always_on_top = *is_pinned.borrow();
                save_note.is_visible = true;

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

        // Palette button — open unified theme editor
        {
            let tb = theme_bg.clone();
            let tf = theme_fg.clone();
            let ta = theme_accent.clone();
            let cc = custom_colors.clone();
            let tp = theme_provider.clone();
            let nc = note_class.clone();
            let win_for_palette = window.clone();
            let prev_theme_win: Rc<RefCell<Option<gtk4::Window>>> = Rc::new(RefCell::new(None));
            let do_save_theme = do_save.clone();
            palette_btn.connect_clicked(move |_| {
                if let Some(old) = prev_theme_win.borrow_mut().take() {
                    old.close();
                }
                let theme_win = crate::theme::show_theme_editor(
                    &win_for_palette,
                    crate::theme::ThemeTarget::Note {
                        provider: tp.clone(),
                        note_class: nc.clone(),
                        theme_bg: tb.clone(),
                        theme_fg: tf.clone(),
                        theme_accent: ta.clone(),
                        custom_colors: cc.clone(),
                    },
                );
                let save_fn = do_save_theme.clone();
                theme_win.connect_close_request(move |_| {
                    save_fn();
                    glib::Propagation::Proceed
                });
                *prev_theme_win.borrow_mut() = Some(theme_win.clone());
                theme_win.present();
            });
        }

        // Star button handler
        {
            let star_c = star_color_rc.clone();
            let star_b = star_btn.clone();
            let do_save_star = do_save.clone();
            let prev_star_pop: Rc<RefCell<Option<gtk4::Popover>>> = Rc::new(RefCell::new(None));
            star_btn.connect_clicked(move |btn| {
                if let Some(old) = prev_star_pop.borrow_mut().take() {
                    old.unparent();
                }
                let popover = gtk4::Popover::new();
                popover.set_parent(btn);
                let hbox = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Horizontal)
                    .spacing(4)
                    .margin_top(4).margin_bottom(4).margin_start(4).margin_end(4)
                    .build();
                let colors = ["#ef5350", "#ffca28", "#66bb6a", "#42a5f5", "#7e57c2"];
                for color in &colors {
                    let c = color.to_string();
                    let sc = star_c.clone();
                    let sb = star_b.clone();
                    let pop = popover.clone();
                    let save_fn = do_save_star.clone();
                    let cbtn = Button::builder()
                        .label("\u{2605}")
                        .css_classes(["star-color-btn"])
                        .tooltip_text(*color)
                        .build();
                    let clbl = Label::new(None);
                    clbl.set_markup(&format!("<span foreground=\"{}\">\u{2605}</span>", c));
                    cbtn.set_child(Some(&clbl));
                    cbtn.connect_clicked(move |_| {
                        *sc.borrow_mut() = Some(c.clone());
                        let lbl = Label::new(None);
                        lbl.set_markup(&format!("<span foreground=\"{}\">\u{2605}</span>", c));
                        sb.set_child(Some(&lbl));
                        pop.popdown();
                        save_fn();
                    });
                    hbox.append(&cbtn);
                }
                let sc = star_c.clone();
                let sb = star_b.clone();
                let pop = popover.clone();
                let save_fn = do_save_star.clone();
                let none_btn = Button::builder().label("x").tooltip_text("Remove star").build();
                none_btn.connect_clicked(move |_| {
                    *sc.borrow_mut() = None;
                    sb.set_label("\u{2606}");
                    pop.popdown();
                    save_fn();
                });
                hbox.append(&none_btn);
                popover.set_child(Some(&hbox));
                *prev_star_pop.borrow_mut() = Some(popover.clone());
                glib::idle_add_local_once(move || {
                    popover.popup();
                });
            });
        }

        // Chromeless toggle per-tangle
        {
            let is_cl = is_chromeless.clone();
            let win_cl = window.clone();
            let cl_btn = chromeless_btn.clone();
            let do_save_cl = do_save.clone();
            chromeless_btn.connect_clicked(move |_| {
                let mut cl = is_cl.borrow_mut();
                *cl = !*cl;
                if *cl {
                    win_cl.set_decorated(false);
                    win_cl.add_css_class("chromeless");
                    cl_btn.set_label("\u{25a1}");
                } else {
                    win_cl.set_decorated(true);
                    win_cl.remove_css_class("chromeless");
                    cl_btn.set_label("\u{25a0}");
                }
                drop(cl);
                do_save_cl();
            });
        }

        // -- Autosave: debounce 2 seconds after any edit --
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
                    std::time::Duration::from_secs(2),
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
        glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            if win_alive.is_visible() {
                do_save_geo();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        // Backlinks refresh with dedup tracking
        let db_bl = db.clone();
        let app_bl = app.clone();
        let title_bl = title_entry.clone();
        let bl_box = backlinks_box.clone();
        let bl_poll_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let bl_poll_ref = bl_poll_id.clone();
        let refresh_backlinks = Rc::new(move || {
            // Cancel any in-flight poll (ignore error if source already completed)
            if let Some(id) = bl_poll_ref.borrow_mut().take() {
                unsafe { glib::ffi::g_source_remove(id.as_raw()); }
            }
            let poll_ref = bl_poll_ref.clone();
            let source_id = refresh_backlinks_pane(&bl_box, &db_bl, &title_bl.text(), &app_bl);
            *poll_ref.borrow_mut() = source_id;
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

        // Build a synchronous save closure for use at close time
        let do_save_sync = {
            let note_id = note_id.clone();
            let note_template = note_template.clone();
            let db = db.clone();
            let title_entry = title_entry.clone();
            let editor_ref = editor_ref.clone();
            let theme_bg = theme_bg.clone();
            let theme_fg = theme_fg.clone();
            let theme_accent = theme_accent.clone();
            let custom_colors = custom_colors.clone();
            let cached_geo = cached_geo.clone();
            let is_chromeless = is_chromeless.clone();
            let star_color_rc = star_color_rc.clone();
            let is_pinned = is_pinned.clone();

            Rc::new(move |visible: bool| {
                let title = title_entry.text().to_string();
                let content = editor_ref.get_content();
                let current_id = *note_id.borrow();
                if current_id.is_none() && title == "New Tangle" && content.is_empty() {
                    return;
                }
                let mut save_note = (*note_template).clone();
                save_note.id = current_id;
                save_note.title = title;
                save_note.content = content;
                save_note.updated_at = chrono::Utc::now().to_rfc3339();
                let (gx, gy, gw, gh) = *cached_geo.lock().unwrap();
                save_note.position_x = gx as f64;
                save_note.position_y = gy as f64;
                if gw > 0 { save_note.width = gw; }
                if gh > 0 { save_note.height = gh; }
                save_note.theme_bg = theme_bg.borrow().clone();
                save_note.theme_fg = theme_fg.borrow().clone();
                save_note.theme_accent = theme_accent.borrow().clone();
                save_note.custom_colors = custom_colors.borrow().clone();
                save_note.chromeless = *is_chromeless.borrow();
                save_note.star_color = star_color_rc.borrow().clone();
                save_note.always_on_top = *is_pinned.borrow();
                save_note.is_visible = visible;
                if current_id.is_some() {
                    if let Err(e) = db.update_note(&save_note) {
                        eprintln!("Error updating note: {}", e);
                    }
                } else if let Err(e) = db.create_note(&save_note) {
                    eprintln!("Error creating note: {}", e);
                }
            })
        };

        // Close — save synchronously with is_visible=false
        let do_sync_close = do_save_sync.clone();
        let cached_geo_btn = cached_geo.clone();
        let window_for_close = window.clone();
        close_btn.connect_clicked(move |_| {
            // Snapshot geometry before close
            if let Some(title) = window_for_close.title() {
                if let Some(geo) = query_wmctrl_geometry(&title.to_string()) {
                    *cached_geo_btn.lock().unwrap() = geo;
                }
            }
            do_sync_close(false);
            window_for_close.close();
        });

        let do_sync_wm = do_save_sync.clone();
        let cached_geo_close = cached_geo.clone();
        window.connect_close_request(move |win| {
            if win.is_visible() {
                // Final sync geometry snapshot before closing
                if let Some(title) = win.title() {
                    if let Some(geo) = query_wmctrl_geometry(&title.to_string()) {
                        *cached_geo_close.lock().unwrap() = geo;
                    }
                }
                // If app is quitting, keep is_visible=true so notes reopen on next launch
                let keep_visible = is_app_quitting();
                do_sync_wm(keep_visible);
                win.set_visible(false);
            }
            glib::Propagation::Proceed
        });

        // Always on top toggle
        let window_for_pin = window.clone();
        let pin_btn_ref = always_on_top_btn.clone();
        let is_pinned_pin = is_pinned.clone();
        let do_save_pin = do_save.clone();
        if note.always_on_top {
            always_on_top_btn.add_css_class("pinned");
            // Apply on-top after the window is mapped
            let win = window.clone();
            glib::idle_add_local_once(move || {
                set_window_above(&win, true);
            });
        }
        always_on_top_btn.connect_clicked(move |_| {
            let mut pinned = is_pinned_pin.borrow_mut();
            *pinned = !*pinned;
            if *pinned {
                pin_btn_ref.add_css_class("pinned");
                pin_btn_ref.set_tooltip_text(Some("Pinned on top (click to unpin)"));
            } else {
                pin_btn_ref.remove_css_class("pinned");
                pin_btn_ref.set_tooltip_text(Some("Toggle Always on Top"));
            }
            let above = *pinned;
            drop(pinned);
            do_save_pin();
            window_for_pin.present();
            let win = window_for_pin.clone();
            glib::idle_add_local_once(move || {
                set_window_above(&win, above);
            });
        });

        // Edge-resize gesture (always active — works for both chromeless and decorated)
        {
            let edge_drag = gtk4::GestureDrag::builder().button(1).build();
            let win_for_edge = window.clone();
            let is_cl_for_edge = is_chromeless.clone();
            edge_drag.connect_drag_begin(move |gesture, x, y| {
                if !*is_cl_for_edge.borrow() {
                    return;
                }
                let w = win_for_edge.width() as f64;
                let h = win_for_edge.height() as f64;
                if let Some(edge) = determine_edge(x, y, w, h, 12.0) {
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

            // Edge cursor change on hover
            let edge_motion = gtk4::EventControllerMotion::new();
            let win_for_cursor = window.clone();
            let is_cl_for_cursor = is_chromeless.clone();
            edge_motion.connect_motion(move |_, x, y| {
                if !*is_cl_for_cursor.borrow() {
                    return;
                }
                let w = win_for_cursor.width() as f64;
                let h = win_for_cursor.height() as f64;
                let cursor_name = match determine_edge(x, y, w, h, 12.0) {
                    Some(gtk4::gdk::SurfaceEdge::North) | Some(gtk4::gdk::SurfaceEdge::South) => Some("ns-resize"),
                    Some(gtk4::gdk::SurfaceEdge::East) | Some(gtk4::gdk::SurfaceEdge::West) => Some("ew-resize"),
                    Some(gtk4::gdk::SurfaceEdge::NorthWest) | Some(gtk4::gdk::SurfaceEdge::SouthEast) => Some("nwse-resize"),
                    Some(gtk4::gdk::SurfaceEdge::NorthEast) | Some(gtk4::gdk::SurfaceEdge::SouthWest) => Some("nesw-resize"),
                    _ => None,
                };
                if let Some(name) = cursor_name {
                    if let Some(cursor) = gtk4::gdk::Cursor::from_name(name, None) {
                        win_for_cursor.set_cursor(Some(&cursor));
                    }
                } else {
                    win_for_cursor.set_cursor(gtk4::gdk::Cursor::from_name("default", None).as_ref());
                }
            });
            main_box.add_controller(edge_motion);

            // Resize grip in bottom-right corner
            let grip = gtk4::DrawingArea::builder()
                .width_request(16)
                .height_request(16)
                .halign(gtk4::Align::End)
                .valign(gtk4::Align::End)
                .css_classes(["resize-grip"])
                .build();
            grip.set_draw_func(|_area, cr, w, h| {
                let w = w as f64;
                let h = h as f64;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
                cr.set_line_width(1.0);
                for offset in &[4.0, 8.0, 12.0] {
                    cr.move_to(w, h - offset);
                    cr.line_to(w - offset, h);
                    let _ = cr.stroke();
                }
            });
            let grip_drag = gtk4::GestureDrag::builder().button(1).build();
            let win_for_grip = window.clone();
            grip_drag.connect_drag_begin(move |gesture, x, y| {
                if let Some(surface) = win_for_grip.surface() {
                    if let Some(toplevel) = surface.downcast_ref::<gtk4::gdk::Toplevel>() {
                        let device = gesture.device().unwrap();
                        let timestamp = gesture.current_event_time();
                        let (sx, sy) = if let Some(event) = gesture.last_event(gesture.current_sequence().as_ref()) {
                            event.position().unwrap_or((x, y))
                        } else {
                            (x, y)
                        };
                        toplevel.begin_resize(gtk4::gdk::SurfaceEdge::SouthEast, Some(&device), 1, sx, sy, timestamp);
                    }
                }
            });
            grip.add_controller(grip_drag);
            main_box.append(&grip);
        }

        NoteWindow { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

pub fn editor_ref_buffer(window: &ApplicationWindow) -> Option<gtk4::TextBuffer> {
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
) -> Option<glib::SourceId> {
    if title.is_empty() || title == "New Tangle" {
        backlinks_box.set_visible(false);
        return None;
    }

    // DB query on background thread, UI update on main thread via channel
    let db_bg = db.clone();
    let title = title.to_string();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();

    std::thread::spawn(move || {
        let linking_notes = db_bg.get_notes_linking_to(&title).unwrap_or_default();
        // Dedup with HashSet
        let mut seen = std::collections::HashSet::new();
        let titles: Vec<String> = linking_notes.iter()
            .filter_map(|n| {
                if seen.insert(n.title.clone()) {
                    Some(n.title.clone())
                } else {
                    None
                }
            })
            .collect();
        let _ = tx.send(titles);
    });

    let bl_box = backlinks_box.clone();
    let db = db.clone();
    let app = app.clone();
    let source_id = glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
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
    Some(source_id)
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
