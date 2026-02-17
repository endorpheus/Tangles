use gtk4::prelude::*;
use gtk4::{
    gio, glib, Application, ApplicationWindow, Box, Button, Entry, Image, Label, ListBox,
    ListBoxRow, Orientation, PopoverMenu, ScrolledWindow, Window,
};

mod database;
mod pickers;
mod rich_editor;
mod note_window;
mod theme;
mod tangle_map;

const APP_ID: &str = "com.tangles.Tangles";

const SETTING_ICON_SIZE: &str = "icon_size";
const SETTING_WIN_W: &str = "win_w";
const SETTING_WIN_H: &str = "win_h";
const SETTING_WIN_X: &str = "win_x";
const SETTING_WIN_Y: &str = "win_y";
const SETTING_STAY_ON_TOP: &str = "brain_stay_on_top";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    install_app_icon();
    load_css();

    // Initialize database
    let data_dir = dirs::data_dir()
        .expect("Could not determine data directory")
        .join("tangles");
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    let db_path = data_dir.join("tangles.db");
    let db = database::Database::new(&db_path).expect("Failed to initialize database");

    // Load saved settings
    let icon_size: i32 = db
        .get_setting(SETTING_ICON_SIZE)
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);
    let saved_w: i32 = db.get_setting(SETTING_WIN_W).and_then(|s| s.parse().ok()).unwrap_or(icon_size + 16);
    let saved_h: i32 = db.get_setting(SETTING_WIN_H).and_then(|s| s.parse().ok()).unwrap_or(icon_size + 16);
    let saved_x: Option<i32> = db.get_setting(SETTING_WIN_X).and_then(|s| s.parse().ok());
    let saved_y: Option<i32> = db.get_setting(SETTING_WIN_Y).and_then(|s| s.parse().ok());

    // Create a minimal undecorated window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Tangles")
        .default_width(saved_w)
        .default_height(saved_h)
        .decorated(false)
        .resizable(true)
        .build();

    // Restore saved position on X11
    if let (Some(x), Some(y)) = (saved_x, saved_y) {
        if x > 0 || y > 0 {
            let wx = x;
            let wy = y;
            window.connect_realize(move |_| {
                glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                    let _ = std::process::Command::new("wmctrl")
                        .args(["-r", "Tangles", "-e", &format!("0,{},{},{},{}", wx, wy, -1, -1)])
                        .spawn();
                });
            });
        }
    }

    window.add_css_class("brain-window");

    // Brain icon
    let brain_icon = build_brain_icon(icon_size);

    let icon_box = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .css_classes(["brain-container"])
        .build();
    icon_box.set_size_request(icon_size + 8, icon_size + 8);
    icon_box.append(&brain_icon);

    // Context menu
    let menu_model = build_menu_model();
    let popover = PopoverMenu::from_model(Some(&menu_model));
    popover.set_parent(&icon_box);
    popover.set_has_arrow(true);

    // Right-click → menu
    let click_right = gtk4::GestureClick::builder().button(3).build();
    let popover_ref = popover.clone();
    click_right.connect_pressed(move |_, _, _, _| {
        popover_ref.popup();
    });
    icon_box.add_controller(click_right);

    // Left-click drag to move the window — use root coordinates so cursor stays put
    let drag = gtk4::GestureDrag::builder().button(1).build();
    let window_for_drag = window.clone();
    drag.connect_drag_begin(move |gesture, _x, _y| {
        if let Some(surface) = window_for_drag.surface() {
            if let Some(toplevel) = surface.downcast_ref::<gtk4::gdk::Toplevel>() {
                use gtk4::gdk::prelude::ToplevelExt;
                let device = gesture.device().unwrap();
                let timestamp = gesture.current_event_time();
                // Use the event's surface coords so the cursor doesn't jump
                let (root_x, root_y) = if let Some(event) = gesture.last_event(gesture.current_sequence().as_ref()) {
                    event.position().unwrap_or((_x, _y))
                } else {
                    (_x, _y)
                };
                toplevel.begin_move(&device, 1, root_x, root_y, timestamp);
            }
        }
    });
    // Save icon position after drag with debounce
    let db_for_drag = db.clone();
    let drag_save_timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let drag_timer_ref = drag_save_timer.clone();
    drag.connect_drag_end(move |_, _, _| {
        if let Some(id) = drag_timer_ref.borrow_mut().take() {
            unsafe { glib::ffi::g_source_remove(id.as_raw()); }
        }
        let db_ref = db_for_drag.clone();
        let timer_ref = drag_timer_ref.clone();
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_secs(3),
            move || {
                save_icon_position(&db_ref);
                *timer_ref.borrow_mut() = None;
            },
        );
        *drag_timer_ref.borrow_mut() = Some(source_id);
    });

    icon_box.add_controller(drag);

    // Scroll-to-resize: just resize the icon, window follows naturally
    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    let brain_icon_ref = brain_icon.clone();
    let icon_box_ref = icon_box.clone();
    let db_for_scroll = db.clone();
    let current_size_f = std::rc::Rc::new(std::cell::Cell::new(icon_size as f64));
    let save_pending = std::rc::Rc::new(std::cell::Cell::new(false));
    scroll.connect_scroll(move |_, _, dy| {
        let mut size = current_size_f.get();
        let factor = 1.0 - (dy * 0.12);
        size = (size * factor).clamp(32.0, 256.0);
        current_size_f.set(size);

        let size_i = size.round() as i32;
        brain_icon_ref.set_pixel_size(size_i);
        icon_box_ref.set_size_request(size_i + 8, size_i + 8);

        // Debounce DB write
        if !save_pending.get() {
            save_pending.set(true);
            let db_ref = db_for_scroll.clone();
            let size_ref = current_size_f.clone();
            let pending_ref = save_pending.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                let _ = db_ref.set_setting(SETTING_ICON_SIZE, &(size_ref.get().round() as i32).to_string());
                pending_ref.set(false);
            });
        }

        glib::Propagation::Stop
    });
    icon_box.add_controller(scroll);

    // Apply global theme from settings
    theme::apply_global_theme(&db);

    // Register app actions
    register_actions(app, &window, &db);

    window.set_child(Some(&icon_box));

    // Apply brain stay-on-top and shadowless on realize
    let brain_on_top = db.get_setting(SETTING_STAY_ON_TOP)
        .map(|v| v == "true")
        .unwrap_or(false);
    {
        let win = window.clone();
        window.connect_realize(move |_| {
            let w = win.clone();
            let on_top = brain_on_top;
            glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                set_brain_shadowless(&w);
                if on_top {
                    set_brain_on_top(true);
                }
            });
        });
    }

    window.present();

    // Periodic icon position save (background thread — no UI blocking)
    let db_for_periodic = db.clone();
    let win_for_periodic = window.clone();
    glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
        if !win_for_periodic.is_visible() {
            return glib::ControlFlow::Break;
        }
        let db = db_for_periodic.clone();
        std::thread::spawn(move || {
            if let Some((x, y)) = get_window_position("Tangles") {
                let _ = db.set_setting(SETTING_WIN_X, &x.to_string());
                let _ = db.set_setting(SETTING_WIN_Y, &y.to_string());
            }
        });
        glib::ControlFlow::Continue
    });

    // Save geometry on close and on resize
    let db_for_close = db.clone();
    window.connect_close_request(move |win| {
        save_window_geometry(win, &db_for_close);
        glib::Propagation::Proceed
    });

    let resize_timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let db_for_resize = db.clone();
    let resize_timer_w = resize_timer.clone();
    window.connect_default_width_notify(move |win| {
        if let Some(id) = resize_timer_w.borrow_mut().take() {
            unsafe { glib::ffi::g_source_remove(id.as_raw()); }
        }
        let db = db_for_resize.clone();
        let w = win.width();
        let h = win.height();
        let timer_ref = resize_timer_w.clone();
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_secs(2),
            move || {
                if w > 0 && h > 0 {
                    let _ = db.set_setting(SETTING_WIN_W, &w.to_string());
                    let _ = db.set_setting(SETTING_WIN_H, &h.to_string());
                }
                *timer_ref.borrow_mut() = None;
            },
        );
        *resize_timer_w.borrow_mut() = Some(source_id);
    });
    let db_for_resize2 = db.clone();
    let resize_timer_h = resize_timer.clone();
    window.connect_default_height_notify(move |win| {
        if let Some(id) = resize_timer_h.borrow_mut().take() {
            unsafe { glib::ffi::g_source_remove(id.as_raw()); }
        }
        let db = db_for_resize2.clone();
        let w = win.width();
        let h = win.height();
        let timer_ref = resize_timer_h.clone();
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_secs(2),
            move || {
                if w > 0 && h > 0 {
                    let _ = db.set_setting(SETTING_WIN_W, &w.to_string());
                    let _ = db.set_setting(SETTING_WIN_H, &h.to_string());
                }
                *timer_ref.borrow_mut() = None;
            },
        );
        *resize_timer_h.borrow_mut() = Some(source_id);
    });
}

fn save_window_geometry(window: &ApplicationWindow, db: &database::Database) {
    let (w, h) = (window.width(), window.height());
    let db = db.clone();
    std::thread::spawn(move || {
        if w > 0 && h > 0 {
            let _ = db.set_setting(SETTING_WIN_W, &w.to_string());
            let _ = db.set_setting(SETTING_WIN_H, &h.to_string());
        }
        save_icon_position(&db);
    });
}

/// Query a window's position by exact title via wmctrl.
/// wmctrl -l -G format: WINID DESKTOP X Y W H HOST TITLE...
pub(crate) fn get_window_position(title: &str) -> Option<(i32, i32)> {
    let output = std::process::Command::new("wmctrl")
        .args(["-l", "-G"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // parts: [WINID, DESKTOP, X, Y, W, H, HOST, TITLE...]
        if parts.len() >= 8 {
            let win_title = parts[7..].join(" ");
            if win_title == title {
                let x = parts[2].parse::<i32>().ok()?;
                let y = parts[3].parse::<i32>().ok()?;
                return Some((x, y));
            }
        }
    }
    None
}

/// Save the brain icon window position to DB.
fn save_icon_position(db: &database::Database) {
    if let Some((x, y)) = get_window_position("Tangles") {
        let _ = db.set_setting(SETTING_WIN_X, &x.to_string());
        let _ = db.set_setting(SETTING_WIN_Y, &y.to_string());
    }
}

fn install_app_icon() {
    // Install brain.svg into the hicolor icon theme so docks/taskbars pick it up
    if let Some(data_dir) = dirs::data_dir() {
        let icon_dir = data_dir.join("icons/hicolor/scalable/apps");
        let dest = icon_dir.join("tangles.svg");
        if !dest.exists() {
            if let Some(src) = find_asset_path("brain.svg") {
                let _ = std::fs::create_dir_all(&icon_dir);
                let _ = std::fs::copy(&src, &dest);
            }
        }
    }
    gtk4::Window::set_default_icon_name("tangles");
}

fn load_css() {
    let provider = gtk4::CssProvider::new();
    if let Some(css_path) = find_asset_path("style.css") {
        provider.load_from_path(&css_path);
    } else {
        provider.load_from_data(
            ".brain-window { background-color: transparent; }
             .brain-container { padding: 4px; border-radius: 12px; }"
        );
    }
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not get default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_brain_icon(size: i32) -> Image {
    let svg_path = find_asset_path("brain.svg");
    if let Some(path) = svg_path {
        let image = Image::from_file(&path);
        image.set_pixel_size(size);
        image.set_tooltip_text(Some("Tangles\nRight Click Menu"));
        image
    } else {
        let image = Image::from_icon_name("brain-symbolic");
        image.set_pixel_size(size);
        image.set_tooltip_text(Some("Tangles\nRight Click Menu"));
        image
    }
}

fn find_asset_path(filename: &str) -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("assets").join(filename);
            if path.exists() {
                return Some(path.to_string_lossy().into_owned());
            }
            let path = dir.join("../../assets").join(filename);
            if path.exists() {
                return Some(path.canonicalize().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let cwd_path = std::path::PathBuf::from("assets").join(filename);
    if cwd_path.exists() {
        return Some(cwd_path.canonicalize().unwrap().to_string_lossy().into_owned());
    }
    if let Some(data_dir) = dirs::data_dir() {
        let path = data_dir.join("tangles").join(filename);
        if path.exists() {
            return Some(path.to_string_lossy().into_owned());
        }
    }
    None
}

/// Suppress compositor shadows on the brain window (X11 + picom/compton).
fn set_brain_shadowless(_window: &ApplicationWindow) {
    if std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "x11" {
        // Set shadow-suppression hints for picom/compton compositors
        let _ = std::process::Command::new("sh")
            .args(["-c", "sleep 0.3 && xprop -name Tangles -f _COMPTON_SHADOW 32c -set _COMPTON_SHADOW 0 2>/dev/null; xprop -name Tangles -f _PICOM_SHADOW 32c -set _PICOM_SHADOW 0 2>/dev/null"])
            .spawn();
    }
}

/// Get the X11 window ID for a window by title.
fn get_x11_window_id(title: &str) -> Option<String> {
    let output = std::process::Command::new("xdotool")
        .args(["search", "--name", &format!("^{}$", title)])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|s| s.trim().to_string())
}

/// Set or remove always-on-top for a window by title using xdotool + wmctrl.
fn set_brain_on_top(above: bool) {
    // Try xdotool first (more reliable with special window types)
    if let Some(wid) = get_x11_window_id("Tangles") {
        let _ = std::process::Command::new("wmctrl")
            .args(["-i", "-r", &wid, "-b", &format!("{},above", if above { "add" } else { "remove" })])
            .output();
    } else {
        // Fallback to title match
        let _ = std::process::Command::new("wmctrl")
            .args(["-r", "Tangles", "-b", &format!("{},above", if above { "add" } else { "remove" })])
            .output();
    }
}

fn build_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();

    menu.append(Some("New Tangle"), Some("app.new-note"));

    let recent_section = gio::Menu::new();
    recent_section.append(Some("Recent Tangles..."), Some("app.recent-notes"));
    menu.append_section(None, &recent_section);

    let browse_section = gio::Menu::new();
    browse_section.append(Some("Search Tangles..."), Some("app.search-notes"));
    browse_section.append(Some("All Tangles..."), Some("app.all-notes"));
    browse_section.append(Some("Tangle Map..."), Some("app.tangle-map"));
    menu.append_section(None, &browse_section);

    let prefs_section = gio::Menu::new();
    prefs_section.append(Some("Stay on Top"), Some("app.stay-on-top"));
    prefs_section.append(Some("Theme Settings..."), Some("app.theme-settings"));
    menu.append_section(None, &prefs_section);

    let quit_section = gio::Menu::new();
    quit_section.append(Some("Quit"), Some("app.quit"));
    menu.append_section(None, &quit_section);

    menu
}

fn register_actions(app: &Application, window: &ApplicationWindow, db: &database::Database) {
    // New Note
    let new_note_action = gio::SimpleAction::new("new-note", None);
    let app_clone = app.clone();
    let db_clone = db.clone();
    new_note_action.connect_activate(move |_, _| {
        let nw = note_window::NoteWindow::new(&app_clone, db_clone.clone(), None);
        nw.present();
    });
    app.add_action(&new_note_action);

    // Recent Notes
    let recent_action = gio::SimpleAction::new("recent-notes", None);
    let app_clone = app.clone();
    let db_clone = db.clone();
    let win_clone = window.clone();
    recent_action.connect_activate(move |_, _| {
        show_note_list_dialog(&app_clone, &win_clone, &db_clone, NoteListMode::Recent);
    });
    app.add_action(&recent_action);

    // Search Notes
    let search_action = gio::SimpleAction::new("search-notes", None);
    let app_clone = app.clone();
    let db_clone = db.clone();
    let win_clone = window.clone();
    search_action.connect_activate(move |_, _| {
        show_note_list_dialog(&app_clone, &win_clone, &db_clone, NoteListMode::Search);
    });
    app.add_action(&search_action);

    // All Notes
    let all_notes_action = gio::SimpleAction::new("all-notes", None);
    let app_clone = app.clone();
    let db_clone = db.clone();
    let win_clone = window.clone();
    all_notes_action.connect_activate(move |_, _| {
        show_note_list_dialog(&app_clone, &win_clone, &db_clone, NoteListMode::All);
    });
    app.add_action(&all_notes_action);

    // Stay on Top toggle for brain icon
    let stay_on_top_on = db.get_setting(SETTING_STAY_ON_TOP)
        .map(|v| v == "true")
        .unwrap_or(false);
    let stay_on_top_action = gio::SimpleAction::new_stateful(
        "stay-on-top",
        None,
        &stay_on_top_on.to_variant(),
    );
    let db_for_sot = db.clone();
    stay_on_top_action.connect_activate(move |action, _| {
        let current = action.state().and_then(|v| v.get::<bool>()).unwrap_or(false);
        let new_val = !current;
        action.set_state(&new_val.to_variant());
        let _ = db_for_sot.set_setting(SETTING_STAY_ON_TOP, if new_val { "true" } else { "false" });
        // Defer to next iteration so the popover menu closes first
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            set_brain_on_top(new_val);
        });
    });
    app.add_action(&stay_on_top_action);

    // Theme Settings (global theme editor)
    let theme_settings_action = gio::SimpleAction::new("theme-settings", None);
    let db_for_theme = db.clone();
    let win_for_theme = window.clone();
    theme_settings_action.connect_activate(move |_, _| {
        crate::theme::show_global_theme_dialog(&win_for_theme, &db_for_theme);
    });
    app.add_action(&theme_settings_action);

    // Tangle Map
    let tangle_map_action = gio::SimpleAction::new("tangle-map", None);
    let app_for_map = app.clone();
    let db_for_map = db.clone();
    let win_for_map = window.clone();
    tangle_map_action.connect_activate(move |_, _| {
        crate::tangle_map::show_tangle_map(&app_for_map, &win_for_map, &db_for_map);
    });
    app.add_action(&tangle_map_action);

    // Quit
    let quit_action = gio::SimpleAction::new("quit", None);
    let app_clone = app.clone();
    quit_action.connect_activate(move |_, _| {
        app_clone.quit();
    });
    app.add_action(&quit_action);
}

#[derive(Clone, Copy)]
enum NoteListMode {
    Recent,
    All,
    Search,
}

fn show_note_list_dialog(
    app: &Application,
    parent: &ApplicationWindow,
    db: &database::Database,
    mode: NoteListMode,
) {
    let dialog = Window::builder()
        .title(match mode {
            NoteListMode::Recent => "Recent Tangles",
            NoteListMode::All => "All Tangles",
            NoteListMode::Search => "Search Tangles",
        })
        .default_width(420)
        .default_height(480)
        .transient_for(parent)
        .modal(false)
        .build();

    dialog.add_css_class("note-list-dialog");

    let vbox = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let search_entry = Entry::builder()
        .placeholder_text("Search...")
        .margin_bottom(4)
        .css_classes(["note-list-search"])
        .build();

    let list_box = ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .build();
    list_box.add_css_class("boxed-list");

    let scrolled = ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(300)
        .build();

    vbox.append(&search_entry);
    vbox.append(&scrolled);

    // Show the dialog immediately with a loading placeholder
    dialog.set_child(Some(&vbox));
    dialog.present();

    if matches!(mode, NoteListMode::Search) {
        search_entry.grab_focus();
    }

    // Wire up row activation
    let app_clone = app.clone();
    let db_clone = db.clone();
    let dialog_clone = dialog.clone();
    list_box.connect_row_activated(move |_, row| {
        if let Some(note_id) = get_note_id_from_row(row) {
            if let Ok(Some(note)) = db_clone.get_note(note_id) {
                let nw = note_window::NoteWindow::new(&app_clone, db_clone.clone(), Some(note));
                nw.present();
                dialog_clone.close();
            }
        }
    });

    // Load initial notes on background thread
    if !matches!(mode, NoteListMode::Search) {
        let db_init = db.clone();
        let list_box_init = list_box.clone();
        let db_for_pop = db.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Vec<database::Note>>();
        std::thread::spawn(move || {
            let notes = match mode {
                NoteListMode::Recent => db_init.get_recent_notes(10).unwrap_or_default(),
                _ => db_init.get_all_notes().unwrap_or_default(),
            };
            let _ = tx.send(notes);
        });
        glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
            match rx.try_recv() {
                Ok(notes) => {
                    populate_note_list(&list_box_init, &notes, &db_for_pop);
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => glib::ControlFlow::Break,
            }
        });
    }

    // Search with debounce + background thread
    let db_for_search = db.clone();
    let list_box_for_search = list_box.clone();
    let search_timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    search_entry.connect_changed(move |entry| {
        if let Some(id) = search_timer.borrow_mut().take() {
            unsafe { glib::ffi::g_source_remove(id.as_raw()); }
        }
        let query = entry.text().to_string();
        let db = db_for_search.clone();
        let list_box = list_box_for_search.clone();
        let timer_ref = search_timer.clone();
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(250),
            move || {
                let db_bg = db.clone();
                let db_pop = db.clone();
                let lb = list_box.clone();
                let q = query.clone();
                let (tx, rx) = std::sync::mpsc::channel::<Vec<database::Note>>();
                std::thread::spawn(move || {
                    let results = if q.is_empty() {
                        match mode {
                            NoteListMode::Recent => db_bg.get_recent_notes(10).unwrap_or_default(),
                            _ => db_bg.get_all_notes().unwrap_or_default(),
                        }
                    } else {
                        db_bg.search_notes(&q).unwrap_or_default()
                    };
                    let _ = tx.send(results);
                });
                glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
                    match rx.try_recv() {
                        Ok(results) => {
                            populate_note_list(&lb, &results, &db_pop);
                            glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                        Err(_) => glib::ControlFlow::Break,
                    }
                });
                *timer_ref.borrow_mut() = None;
            },
        );
        *search_timer.borrow_mut() = Some(source_id);
    });
}

fn populate_note_list(list_box: &ListBox, notes: &[database::Note], db: &database::Database) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    // Sort starred tangles to top
    let mut sorted: Vec<&database::Note> = notes.iter().collect();
    sorted.sort_by(|a, b| {
        let a_starred = a.star_color.is_some();
        let b_starred = b.star_color.is_some();
        b_starred.cmp(&a_starred)
    });
    let notes = sorted;

    if notes.is_empty() {
        let empty = Label::builder()
            .label("No tangles found")
            .css_classes(["dim-label"])
            .margin_top(20)
            .margin_bottom(20)
            .build();
        let row = ListBoxRow::new();
        row.set_child(Some(&empty));
        row.set_activatable(false);
        list_box.append(&row);
        return;
    }

    for note in &notes {
        let outer_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(["note-row"])
            .build();

        // Star indicator
        if let Some(ref color) = note.star_color {
            let star = Label::builder()
                .label("\u{2605}")
                .css_classes(["star-indicator"])
                .build();
            star.set_markup(&format!("<span foreground=\"{}\">\u{2605}</span>", color));
            outer_box.append(&star);
        }

        let info_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();

        let title = Label::builder()
            .label(&note.title)
            .xalign(0.0)
            .css_classes(["note-row-title"])
            .build();

        let preview_text = note
            .content
            .chars()
            .take(80)
            .collect::<String>()
            .replace('\n', " ");
        let preview = Label::builder()
            .label(&preview_text)
            .xalign(0.0)
            .css_classes(["note-row-preview"])
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();

        let timestamp = format_timestamp(&note.updated_at);
        let time_label = Label::builder()
            .label(&timestamp)
            .xalign(0.0)
            .css_classes(["note-row-timestamp"])
            .build();

        info_box.append(&title);
        info_box.append(&preview);
        info_box.append(&time_label);
        outer_box.append(&info_box);

        if let Some(note_id) = note.id {
            let delete_btn = Button::builder()
                .label("x")
                .tooltip_text("Delete note")
                .css_classes(["note-delete-button"])
                .valign(gtk4::Align::Center)
                .build();

            let list_box_ref = list_box.clone();
            let db_for_delete = db.clone();
            delete_btn.connect_clicked(move |btn| {
                if let Err(e) = db_for_delete.delete_note(note_id) {
                    eprintln!("Error deleting note: {}", e);
                    return;
                }
                if let Some(row) = btn.ancestor(ListBoxRow::static_type()) {
                    let row = row.downcast::<ListBoxRow>().unwrap();
                    list_box_ref.remove(&row);
                }
            });

            outer_box.append(&delete_btn);
        }

        let row = ListBoxRow::new();
        row.set_child(Some(&outer_box));
        if let Some(id) = note.id {
            row.set_widget_name(&format!("note-{}", id));
        }
        list_box.append(&row);
    }
}

fn format_timestamp(rfc3339: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rfc3339) {
        dt.format("%b %d, %Y  %H:%M").to_string()
    } else {
        rfc3339.to_string()
    }
}

fn get_note_id_from_row(row: &ListBoxRow) -> Option<i64> {
    let name = row.widget_name();
    name.strip_prefix("note-")
        .and_then(|id_str| id_str.parse::<i64>().ok())
}
