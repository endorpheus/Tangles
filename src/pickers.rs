use gtk4::prelude::*;
use gtk4::{Button, Box, Label, Image, ScrolledWindow};
use gtk4::gdk_pixbuf::Pixbuf;
use std::cell::Cell;
use std::rc::Rc;

/// Show emoji picker popover. Calls `on_pick` with the chosen emoji string.
pub fn show_emoji_picker(relative_to: &impl IsA<gtk4::Widget>, on_pick: impl Fn(&str) + 'static) {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let categories: &[(&str, &[&str])] = &[
        ("Faces", &["\u{1f600}", "\u{1f602}", "\u{1f914}", "\u{1f60d}", "\u{1f60e}", "\u{1f92f}", "\u{1f634}", "\u{1f973}", "\u{1f631}", "\u{1f644}"]),
        ("Hands", &["\u{1f44d}", "\u{1f44e}", "\u{1f44b}", "\u{1f91d}", "\u{270c}\u{fe0f}", "\u{1f91e}", "\u{1f44f}", "\u{1f64f}", "\u{270b}", "\u{1f919}"]),
        ("Symbols", &["\u{2705}", "\u{274c}", "\u{2b50}", "\u{2764}\u{fe0f}", "\u{1f525}", "\u{1f4a1}", "\u{26a1}", "\u{1f3af}", "\u{1f4cc}", "\u{1f517}"]),
        ("Objects", &["\u{1f4dd}", "\u{1f4ce}", "\u{1f4c1}", "\u{1f511}", "\u{1f512}", "\u{1f4bb}", "\u{1f4ca}", "\u{1f5c2}\u{fe0f}", "\u{1f4cb}", "\u{1f3f7}\u{fe0f}"]),
    ];

    let on_pick = Rc::new(on_pick);

    for (cat_name, emojis) in categories {
        let label = Label::builder()
            .label(*cat_name)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        vbox.append(&label);

        let flow = gtk4::FlowBox::builder()
            .max_children_per_line(10)
            .min_children_per_line(5)
            .selection_mode(gtk4::SelectionMode::None)
            .build();

        for emoji in *emojis {
            let btn = Button::builder()
                .label(*emoji)
                .css_classes(["emoji-btn"])
                .build();
            let e = emoji.to_string();
            let pop_ref = popover.clone();
            let cb = on_pick.clone();
            btn.connect_clicked(move |_| {
                cb(&e);
                pop_ref.popdown();
            });
            flow.insert(&btn, -1);
        }
        vbox.append(&flow);
    }

    let scrolled = ScrolledWindow::builder()
        .child(&vbox)
        .min_content_height(250)
        .min_content_width(300)
        .build();

    popover.set_child(Some(&scrolled));
    popover.popup();
}

/// Show icon picker popover. Calls `on_pick` with (icon_name, icon_file_path).
pub fn show_icon_picker(relative_to: &impl IsA<gtk4::Widget>, on_pick: impl Fn(&str, &str) + 'static) {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let search_entry = gtk4::Entry::builder()
        .placeholder_text("Search icons...")
        .build();
    vbox.append(&search_entry);

    let flow = gtk4::FlowBox::builder()
        .max_children_per_line(8)
        .min_children_per_line(4)
        .selection_mode(gtk4::SelectionMode::None)
        .homogeneous(true)
        .build();

    let scrolled = ScrolledWindow::builder()
        .child(&flow)
        .min_content_height(280)
        .min_content_width(320)
        .build();
    vbox.append(&scrolled);

    let icon_names: &[&str] = &[
        "document-new", "document-open", "document-save", "document-edit",
        "edit-copy", "edit-paste", "edit-cut", "edit-delete", "edit-undo", "edit-redo",
        "list-add", "list-remove", "view-list", "view-grid",
        "folder", "folder-open", "user-home", "user-trash",
        "dialog-information", "dialog-warning", "dialog-error", "dialog-question",
        "starred", "non-starred", "emblem-favorite", "emblem-important",
        "go-next", "go-previous", "go-up", "go-down",
        "process-stop", "media-playback-start", "media-playback-pause",
        "system-search", "system-run", "system-shutdown",
        "preferences-system", "applications-system", "utilities-terminal",
        "network-wired", "network-wireless", "computer",
        "mail-unread", "mail-read", "mail-send",
        "weather-clear", "weather-few-clouds", "weather-overcast",
        "bookmark-new", "contact-new",
        "security-high", "security-medium", "security-low",
        "camera-photo", "camera-video",
        "accessories-text-editor", "accessories-calculator",
        "help-about", "help-contents", "help-faq",
    ];

    let on_pick = Rc::new(on_pick);
    populate_icon_flow(&flow, icon_names, &on_pick, &popover);

    let flow_ref = flow.clone();
    let pop_ref = popover.clone();
    let on_pick_ref = on_pick.clone();
    let icon_names_owned: Vec<String> = icon_names.iter().map(|s| s.to_string()).collect();
    search_entry.connect_changed(move |entry| {
        let query = entry.text().to_string().to_lowercase();
        while let Some(child) = flow_ref.first_child() {
            flow_ref.remove(&child);
        }
        let filtered: Vec<&str> = if query.is_empty() {
            icon_names_owned.iter().map(|s| s.as_str()).collect()
        } else {
            icon_names_owned.iter()
                .filter(|name| name.contains(&query))
                .map(|s| s.as_str())
                .collect()
        };
        populate_icon_flow(&flow_ref, &filtered, &on_pick_ref, &pop_ref);
    });

    popover.set_child(Some(&vbox));
    popover.popup();
}

fn populate_icon_flow(
    flow: &gtk4::FlowBox,
    names: &[&str],
    on_pick: &Rc<impl Fn(&str, &str) + 'static>,
    popover: &gtk4::Popover,
) {
    let display = gtk4::gdk::Display::default().unwrap();
    let theme = gtk4::IconTheme::for_display(&display);

    for name in names {
        if !theme.has_icon(name) {
            continue;
        }
        let btn = Button::builder().tooltip_text(*name).build();
        let icon = Image::from_icon_name(name);
        icon.set_pixel_size(24);
        btn.set_child(Some(&icon));

        let icon_name = name.to_string();
        let pop_ref = popover.clone();
        let cb = on_pick.clone();
        btn.connect_clicked(move |_| {
            if let Some(path) = find_icon_path(&icon_name) {
                cb(&icon_name, &path);
            }
            pop_ref.popdown();
        });
        flow.insert(&btn, -1);
    }
}

pub fn find_icon_path(icon_name: &str) -> Option<String> {
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    for size in &[512, 256, 128, 64, 48] {
        let paintable = theme.lookup_icon(
            icon_name, &[], *size, 1,
            gtk4::TextDirection::None,
            gtk4::IconLookupFlags::empty(),
        );
        let file = paintable.file()?;
        let path = file.path()?;
        return Some(path.to_string_lossy().into_owned());
    }
    None
}

/// Open image file browser dialog. Calls `on_pick` with the selected file path.
pub fn open_image_file_picker(relative_to: &impl IsA<gtk4::Widget>, on_pick: impl Fn(&str) + 'static) {
    let win = relative_to.root()
        .and_then(|r| r.downcast::<gtk4::Window>().ok());

    let dialog = gtk4::Window::builder()
        .title("Choose an image")
        .default_width(700)
        .default_height(500)
        .modal(true)
        .build();
    if let Some(ref w) = win {
        dialog.set_transient_for(Some(w));
    }
    dialog.add_css_class("note-list-dialog");

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let path_bar = Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(4)
        .build();

    let back_btn = Button::builder()
        .label("\u{2190}")
        .tooltip_text("Go up one directory")
        .build();
    path_bar.append(&back_btn);

    let home_btn = Button::builder()
        .label("\u{1f3e0}")
        .tooltip_text("Home directory")
        .build();
    path_bar.append(&home_btn);

    let path_entry = gtk4::Entry::builder()
        .hexpand(true)
        .text(&*dirs::home_dir().unwrap_or_default().to_string_lossy())
        .build();
    path_bar.append(&path_entry);

    vbox.append(&path_bar);

    let flow = gtk4::FlowBox::builder()
        .max_children_per_line(6)
        .min_children_per_line(3)
        .selection_mode(gtk4::SelectionMode::None)
        .homogeneous(true)
        .row_spacing(8)
        .column_spacing(8)
        .build();

    let scrolled = ScrolledWindow::builder()
        .child(&flow)
        .vexpand(true)
        .hexpand(true)
        .build();
    vbox.append(&scrolled);

    let status_label = Label::builder()
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    vbox.append(&status_label);

    dialog.set_child(Some(&vbox));

    let on_pick = Rc::new(on_pick);

    let initial_dir = dirs::home_dir().unwrap_or_default();
    populate_image_grid(&flow, &initial_dir, &on_pick, &dialog, &status_label, &path_entry);

    let flow_for_nav = flow.clone();
    let cb_nav = on_pick.clone();
    let dlg_nav = dialog.clone();
    let status_nav = status_label.clone();
    let pe_nav = path_entry.clone();
    path_entry.connect_activate(move |entry| {
        let dir = std::path::PathBuf::from(entry.text().to_string());
        if dir.is_dir() {
            populate_image_grid(&flow_for_nav, &dir, &cb_nav, &dlg_nav, &status_nav, &pe_nav);
        }
    });

    let path_entry_ref = path_entry.clone();
    let flow_for_back = flow.clone();
    let cb_back = on_pick.clone();
    let dlg_back = dialog.clone();
    let status_back = status_label.clone();
    back_btn.connect_clicked(move |_| {
        let current = std::path::PathBuf::from(path_entry_ref.text().to_string());
        if let Some(parent) = current.parent() {
            path_entry_ref.set_text(&parent.to_string_lossy());
            populate_image_grid(&flow_for_back, parent, &cb_back, &dlg_back, &status_back, &path_entry_ref);
        }
    });

    let path_entry_ref2 = path_entry.clone();
    let flow_for_home = flow.clone();
    let cb_home = on_pick.clone();
    let dlg_home = dialog.clone();
    let status_home = status_label.clone();
    home_btn.connect_clicked(move |_| {
        let home = dirs::home_dir().unwrap_or_default();
        path_entry_ref2.set_text(&home.to_string_lossy());
        populate_image_grid(&flow_for_home, &home, &cb_home, &dlg_home, &status_home, &path_entry_ref2);
    });

    dialog.present();
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico"];

fn populate_image_grid(
    flow: &gtk4::FlowBox,
    dir: &std::path::Path,
    on_pick: &Rc<impl Fn(&str) + 'static>,
    dialog: &gtk4::Window,
    status: &Label,
    path_entry: &gtk4::Entry,
) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        status.set_text("Cannot read directory");
        return;
    };

    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    let mut images: Vec<std::path::PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            dirs.push(path);
        } else if let Some(ext) = path.extension() {
            if IMAGE_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str()) {
                images.push(path);
            }
        }
    }

    dirs.sort();
    images.sort();

    status.set_text(&format!("{} folders, {} images", dirs.len(), images.len()));

    for dir_path in &dirs {
        let name = dir_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let item_box = Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .halign(gtk4::Align::Center)
            .build();

        let icon = Image::from_icon_name("folder");
        icon.set_pixel_size(64);
        item_box.append(&icon);

        let label = Label::builder()
            .label(&name)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .max_width_chars(14)
            .build();
        item_box.append(&label);

        let btn = Button::builder()
            .child(&item_box)
            .css_classes(["thumbnail-btn"])
            .build();

        let flow_ref = flow.clone();
        let cb_ref = on_pick.clone();
        let dlg_ref = dialog.clone();
        let status_ref = status.clone();
        let pe_ref = path_entry.clone();
        let dp = dir_path.clone();
        btn.connect_clicked(move |_| {
            pe_ref.set_text(&dp.to_string_lossy());
            populate_image_grid(&flow_ref, &dp, &cb_ref, &dlg_ref, &status_ref, &pe_ref);
        });

        flow.insert(&btn, -1);
    }

    for img_path in &images {
        let name = img_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let item_box = Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .halign(gtk4::Align::Center)
            .build();

        let picture = gtk4::Picture::for_filename(img_path.to_string_lossy().as_ref());
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_size_request(96, 96);

        let thumb_frame = gtk4::Frame::builder()
            .child(&picture)
            .css_classes(["thumbnail-frame"])
            .build();
        item_box.append(&thumb_frame);

        let label = Label::builder()
            .label(&name)
            .ellipsize(gtk4::pango::EllipsizeMode::Middle)
            .max_width_chars(14)
            .css_classes(["caption"])
            .build();
        item_box.append(&label);

        let btn = Button::builder()
            .child(&item_box)
            .tooltip_text(img_path.to_string_lossy().as_ref())
            .css_classes(["thumbnail-btn"])
            .build();

        let cb_ref = on_pick.clone();
        let dlg_ref = dialog.clone();
        let ip = img_path.clone();
        btn.connect_clicked(move |_| {
            cb_ref(&ip.to_string_lossy());
            dlg_ref.close();
        });

        flow.insert(&btn, -1);
    }

    if dirs.is_empty() && images.is_empty() {
        let empty = Label::builder()
            .label("No images in this directory")
            .css_classes(["dim-label"])
            .margin_top(40)
            .build();
        flow.insert(&empty, -1);
    }
}

/// Load a texture from file, applying EXIF orientation if present.
pub fn load_texture_with_exif(path: &str) -> Option<gtk4::gdk::Texture> {
    let orientation = (|| -> Option<u32> {
        let file = std::fs::File::open(path).ok()?;
        let mut bufreader = std::io::BufReader::new(&file);
        let exif = exif::Reader::new().read_from_container(&mut bufreader).ok()?;
        let orient_field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
        orient_field.value.get_uint(0)
    })();

    if let Some(orient) = orientation {
        if orient > 1 {
            if let Ok(pixbuf) = Pixbuf::from_file(path) {
                let transformed = apply_exif_orientation(&pixbuf, orient);
                return Some(gtk4::gdk::Texture::for_pixbuf(&transformed));
            }
        }
    }

    let file = gtk4::gio::File::for_path(path);
    gtk4::gdk::Texture::from_file(&file).ok()
}

fn apply_exif_orientation(pixbuf: &Pixbuf, orientation: u32) -> Pixbuf {
    match orientation {
        2 => pixbuf.flip(true).unwrap_or_else(|| pixbuf.clone()),
        3 => pixbuf.rotate_simple(gtk4::gdk_pixbuf::PixbufRotation::Upsidedown)
            .unwrap_or_else(|| pixbuf.clone()),
        4 => pixbuf.flip(false).unwrap_or_else(|| pixbuf.clone()),
        5 => {
            let rotated = pixbuf.rotate_simple(gtk4::gdk_pixbuf::PixbufRotation::Clockwise)
                .unwrap_or_else(|| pixbuf.clone());
            rotated.flip(true).unwrap_or_else(|| rotated)
        }
        6 => pixbuf.rotate_simple(gtk4::gdk_pixbuf::PixbufRotation::Clockwise)
            .unwrap_or_else(|| pixbuf.clone()),
        7 => {
            let rotated = pixbuf.rotate_simple(gtk4::gdk_pixbuf::PixbufRotation::Counterclockwise)
                .unwrap_or_else(|| pixbuf.clone());
            rotated.flip(true).unwrap_or_else(|| rotated)
        }
        8 => pixbuf.rotate_simple(gtk4::gdk_pixbuf::PixbufRotation::Counterclockwise)
            .unwrap_or_else(|| pixbuf.clone()),
        _ => pixbuf.clone(),
    }
}

/// Build a Picture widget inside a frame with drag-corner resize.
/// `on_resize` is called with the new width when the user finishes dragging.
pub fn build_resizable_picture(
    path: &str,
    initial_width: i32,
    on_resize: Option<std::boxed::Box<dyn Fn(i32) + 'static>>,
) -> gtk4::Frame {
    // Load texture once and compute aspect ratio
    let texture = load_texture_with_exif(path);
    let aspect = texture.as_ref().map(|t| {
        let iw = t.width() as f64;
        let ih = t.height() as f64;
        if iw > 0.0 { ih / iw } else { 1.0 }
    }).unwrap_or(1.0);

    let req_w = initial_width;
    let req_h = (initial_width as f64 * aspect).round() as i32;
    let req_h = req_h.max(24);

    let picture = gtk4::Picture::builder()
        .can_shrink(true)
        .content_fit(gtk4::ContentFit::Contain)
        .halign(gtk4::Align::Start)
        .build();

    if let Some(ref tex) = texture {
        picture.set_paintable(Some(tex));
    } else {
        picture.set_filename(Some(path));
    }
    picture.set_size_request(req_w, req_h);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&picture));

    let handle = gtk4::DrawingArea::builder()
        .width_request(20)
        .height_request(20)
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::End)
        .css_classes(["image-resize-handle"])
        .build();

    if let Some(cursor) = gtk4::gdk::Cursor::from_name("se-resize", None) {
        handle.set_cursor(Some(&cursor));
    }

    handle.set_draw_func(|_area, cr, w, h| {
        let w = w as f64;
        let h = h as f64;
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
        cr.set_line_width(1.0);
        for offset in &[4.0, 8.0, 12.0] {
            cr.move_to(w, h - offset);
            cr.line_to(w - offset, h);
            let _ = cr.stroke();
        }
    });

    overlay.add_overlay(&handle);

    let frame = gtk4::Frame::builder()
        .child(&overlay)
        .css_classes(["preview-image-frame"])
        .build();

    let drag = gtk4::GestureDrag::builder().button(1).build();
    let start_w: Rc<Cell<f64>> = Rc::new(Cell::new(req_w as f64));
    let current_w: Rc<Cell<f64>> = Rc::new(Cell::new(req_w as f64));
    let pic_ref = picture.clone();
    let aspect_ratio = Rc::new(Cell::new(aspect));

    let start_w_ref = start_w.clone();
    let current_w_ref = current_w.clone();
    drag.connect_drag_begin(move |_, _, _| {
        start_w_ref.set(current_w_ref.get());
    });

    let current_w_ref2 = current_w.clone();
    let ar = aspect_ratio.clone();
    drag.connect_drag_update(move |_, offset_x, _| {
        let new_w = (start_w.get() + offset_x).clamp(48.0, 800.0);
        current_w_ref2.set(new_w);
        let new_h = (new_w * ar.get()).round() as i32;
        pic_ref.set_size_request(new_w.round() as i32, new_h.max(24));
    });

    if let Some(cb) = on_resize {
        let cb: Rc<std::boxed::Box<dyn Fn(i32)>> = Rc::new(cb);
        let final_w = current_w.clone();
        drag.connect_drag_end(move |_, _, _| {
            cb(final_w.get().round() as i32);
        });
    }

    handle.add_controller(drag);

    frame
}
