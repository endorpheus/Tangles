use gtk4::prelude::*;
use gtk4::{glib, TextView, TextBuffer, TextTag, TextIter, ScrolledWindow, Button, Box, Label};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tendril::StrTendril;

use crate::pickers;
use crate::database::{Database, Note};

const ORC: char = '\u{FFFC}';

#[derive(Clone, Debug)]
struct ImageInfo {
    path: String,
    width: i32,
}

#[allow(dead_code)]
pub struct RichEditor {
    pub widget: Box,
    pub text_view: TextView,
    pub buffer: TextBuffer,
    source_view: TextView,
    source_buffer: TextBuffer,
    is_source_mode: Rc<Cell<bool>>,
    pending_tags: Rc<RefCell<HashSet<String>>>,
    image_map: Rc<RefCell<HashMap<i32, ImageInfo>>>,
    inhibit_changed: Rc<Cell<bool>>,
    own_title: Rc<RefCell<String>>,
}

impl RichEditor {
    pub fn new(db: Database, app: gtk4::Application, title: &str) -> Self {
        let buffer = TextBuffer::new(None);
        let table = buffer.tag_table();
        let own_title: Rc<RefCell<String>> = Rc::new(RefCell::new(title.to_string()));

        // Pre-create formatting tags
        let bold = TextTag::builder().name("bold").weight(700).build();
        let italic = TextTag::builder().name("italic").style(gtk4::pango::Style::Italic).build();
        let underline = TextTag::builder().name("underline").underline(gtk4::pango::Underline::Single).build();
        let strikethrough = TextTag::builder().name("strikethrough").strikethrough(true).build();

        let h1 = TextTag::builder().name("h1").scale(2.0).weight(700).build();
        let h2 = TextTag::builder().name("h2").scale(1.5).weight(700).build();
        let h3 = TextTag::builder().name("h3").scale(1.25).weight(700).build();
        let h4 = TextTag::builder().name("h4").scale(1.1).weight(700).build();

        let bullet_list = TextTag::builder().name("bullet-list").left_margin(24).build();
        let numbered_list = TextTag::builder().name("numbered-list").left_margin(24).build();

        for tag in [&bold, &italic, &underline, &strikethrough, &h1, &h2, &h3, &h4, &bullet_list, &numbered_list] {
            table.add(tag);
        }

        let widget = Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(0)
            .build();

        let pending_tags: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        let image_map: Rc<RefCell<HashMap<i32, ImageInfo>>> = Rc::new(RefCell::new(HashMap::new()));
        let inhibit_changed = Rc::new(Cell::new(false));

        // -- Toolbar --
        let toolbar = gtk4::FlowBox::builder()
            .max_children_per_line(30)
            .min_children_per_line(1)
            .selection_mode(gtk4::SelectionMode::None)
            .homogeneous(false)
            .css_classes(["rich-toolbar"])
            .build();

        // Inline format buttons
        let fmt_buttons = [
            ("B", "bold", "Bold (Ctrl+B)"),
            ("I", "italic", "Italic (Ctrl+I)"),
            ("U", "underline", "Underline (Ctrl+U)"),
            ("S", "strikethrough", "Strikethrough (Ctrl+Shift+S)"),
        ];

        for (label, tag_name, tooltip) in &fmt_buttons {
            let btn = Button::builder()
                .label(*label)
                .tooltip_text(*tooltip)
                .build();
            let buf = buffer.clone();
            let pt = pending_tags.clone();
            let tn = tag_name.to_string();
            btn.connect_clicked(move |_| {
                toggle_inline_tag(&buf, &tn, &pt);
            });
            toolbar.insert(&btn, -1);
        }

        // Heading buttons
        for level in 1..=4 {
            let label = format!("H{}", level);
            let tag_name = format!("h{}", level);
            let btn = Button::builder()
                .label(&label)
                .tooltip_text(&format!("Heading {}", level))
                .build();
            let buf = buffer.clone();
            let tn = tag_name;
            btn.connect_clicked(move |_| {
                apply_heading(&buf, &tn);
            });
            toolbar.insert(&btn, -1);
        }

        // List buttons
        let bullet_btn = Button::builder().label("\u{2022}").tooltip_text("Bullet list").build();
        let buf_bl = buffer.clone();
        bullet_btn.connect_clicked(move |_| {
            toggle_list(&buf_bl, "bullet-list");
        });
        toolbar.insert(&bullet_btn, -1);

        let num_btn = Button::builder().label("1.").tooltip_text("Numbered list").build();
        let buf_nl = buffer.clone();
        num_btn.connect_clicked(move |_| {
            toggle_list(&buf_nl, "numbered-list");
        });
        toolbar.insert(&num_btn, -1);

        // Web link button
        let link_btn = Button::builder().label("\u{1f517}").tooltip_text("Insert web link (Ctrl+K)").build();
        let buf_link = buffer.clone();
        link_btn.connect_clicked(move |btn| {
            insert_web_link_dialog(btn, &buf_link);
        });
        toolbar.insert(&link_btn, -1);

        // Tangle (note-to-note link) button
        let tangle_btn = Button::builder().label("\u{1f9e0}").tooltip_text("Link to another note (Tangle)").build();
        let buf_tangle = buffer.clone();
        let db_tangle = db.clone();
        let app_tangle = app.clone();
        tangle_btn.connect_clicked(move |btn| {
            insert_tangle_dialog(btn, &buf_tangle, &db_tangle, &app_tangle);
        });
        toolbar.insert(&tangle_btn, -1);

        // Color buttons
        let fg_btn = Button::builder().label("A").tooltip_text("Text color").css_classes(["fg-color-btn"]).build();
        let buf_fg = buffer.clone();
        let pt_fg = pending_tags.clone();
        fg_btn.connect_clicked(move |btn| {
            show_color_picker(btn, "fg", &buf_fg, &pt_fg);
        });
        toolbar.insert(&fg_btn, -1);

        let bg_btn = Button::builder().label("BG").tooltip_text("Highlight color").css_classes(["bg-color-btn"]).build();
        let buf_bg = buffer.clone();
        let pt_bg = pending_tags.clone();
        bg_btn.connect_clicked(move |btn| {
            show_color_picker(btn, "bg", &buf_bg, &pt_bg);
        });
        toolbar.insert(&bg_btn, -1);

        // Pickers
        let emoji_btn = Button::builder().label("\u{1f600}").tooltip_text("Insert emoji").build();
        let buf_emoji = buffer.clone();
        emoji_btn.connect_clicked(move |btn| {
            let buf = buf_emoji.clone();
            pickers::show_emoji_picker(btn, move |emoji| {
                buf.insert_at_cursor(emoji);
            });
        });
        toolbar.insert(&emoji_btn, -1);

        let icon_btn = Button::builder().label("\u{2b50}").tooltip_text("Insert system icon").build();
        let buf_icon = buffer.clone();
        let tv_holder: Rc<RefCell<Option<TextView>>> = Rc::new(RefCell::new(None));
        let tv_holder_for_icon = tv_holder.clone();
        let im_icon = image_map.clone();
        icon_btn.connect_clicked(move |btn| {
            let buf = buf_icon.clone();
            let tv_h = tv_holder_for_icon.clone();
            let im = im_icon.clone();
            pickers::show_icon_picker(btn, move |_name, path| {
                insert_image_widget(&buf, &tv_h, path, 48, &im);
            });
        });
        toolbar.insert(&icon_btn, -1);

        let img_btn = Button::builder().label("\u{1f5bc}").tooltip_text("Insert image from file").build();
        let buf_img = buffer.clone();
        let tv_holder_for_img = tv_holder.clone();
        let im_img = image_map.clone();
        img_btn.connect_clicked(move |btn| {
            let buf = buf_img.clone();
            let tv_h = tv_holder_for_img.clone();
            let im = im_img.clone();
            pickers::open_image_file_picker(btn, move |path| {
                insert_image_widget(&buf, &tv_h, path, 300, &im);
            });
        });
        toolbar.insert(&img_btn, -1);

        // Source view toggle button
        let is_source_mode: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let source_toggle_btn = Button::builder()
            .label("</>")
            .tooltip_text("Toggle HTML source view")
            .build();
        toolbar.insert(&source_toggle_btn, -1);

        widget.append(&toolbar);

        // -- Text View --
        let text_view = TextView::builder()
            .buffer(&buffer)
            .wrap_mode(gtk4::WrapMode::Word)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["rich-editor"])
            .left_margin(8)
            .right_margin(8)
            .top_margin(8)
            .bottom_margin(8)
            .build();

        // -- Source View (plain text for raw HTML editing) --
        let source_buffer = TextBuffer::new(None);
        let source_view = TextView::builder()
            .buffer(&source_buffer)
            .wrap_mode(gtk4::WrapMode::Word)
            .hexpand(true)
            .vexpand(true)
            .monospace(true)
            .css_classes(["rich-editor"])
            .left_margin(8)
            .right_margin(8)
            .top_margin(8)
            .bottom_margin(8)
            .build();

        // Store text_view reference for pickers
        *tv_holder.borrow_mut() = Some(text_view.clone());

        // Keyboard shortcuts
        let key_controller = gtk4::EventControllerKey::new();
        let buf_for_keys = buffer.clone();
        let pt_for_keys = pending_tags.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
            let ctrl = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shift = modifier.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            if !ctrl {
                return glib::Propagation::Proceed;
            }
            match keyval {
                gtk4::gdk::Key::b => {
                    toggle_inline_tag(&buf_for_keys, "bold", &pt_for_keys);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::i => {
                    toggle_inline_tag(&buf_for_keys, "italic", &pt_for_keys);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::u => {
                    toggle_inline_tag(&buf_for_keys, "underline", &pt_for_keys);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::k => {
                    // Ctrl+K: we can't show a popover from here easily, just toggle link pending
                    glib::Propagation::Proceed
                }
                gtk4::gdk::Key::h => {
                    toggle_inline_tag(&buf_for_keys, "underline", &pt_for_keys); // highlight = bg color
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::s if shift => {
                    toggle_inline_tag(&buf_for_keys, "strikethrough", &pt_for_keys);
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::_1 => { apply_heading(&buf_for_keys, "h1"); glib::Propagation::Stop }
                gtk4::gdk::Key::_2 => { apply_heading(&buf_for_keys, "h2"); glib::Propagation::Stop }
                gtk4::gdk::Key::_3 => { apply_heading(&buf_for_keys, "h3"); glib::Propagation::Stop }
                gtk4::gdk::Key::_4 => { apply_heading(&buf_for_keys, "h4"); glib::Propagation::Stop }
                _ => glib::Propagation::Proceed,
            }
        });
        text_view.add_controller(key_controller);

        // Enter key handler for list continuation — CAPTURE phase to intercept before default handler
        let enter_controller = gtk4::EventControllerKey::new();
        enter_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let buf_enter = buffer.clone();
        enter_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Return {
                if handle_enter_key(&buf_enter) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        text_view.add_controller(enter_controller);

        // Tangle actions for the native context menu
        let tangle_target: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let create_target: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let link_target: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let action_group = gtk4::gio::SimpleActionGroup::new();

        let jump_action = gtk4::gio::SimpleAction::new("jump-tangle", None);
        jump_action.set_enabled(false);
        let db_jump = db.clone();
        let app_jump = app.clone();
        let tangle_t = tangle_target.clone();
        jump_action.connect_activate(move |_, _| {
            if let Some(ref title) = *tangle_t.borrow() {
                open_tangle_note(&db_jump, &app_jump, title);
            }
        });
        action_group.add_action(&jump_action);

        let create_action = gtk4::gio::SimpleAction::new("create-tangle", None);
        create_action.set_enabled(false);
        let db_create = db.clone();
        let app_create = app.clone();
        let buf_create = buffer.clone();
        let create_t = create_target.clone();
        create_action.connect_activate(move |_, _| {
            if let Some(ref title) = *create_t.borrow() {
                ensure_tangle_note_exists(&db_create, title);
                if let Some((start, end)) = buf_create.selection_bounds() {
                    let tag_name = format!("tangle::{}", title);
                    let tag = get_or_create_tag(&buf_create.tag_table(), &tag_name);
                    buf_create.apply_tag(&tag, &start, &end);
                }
                open_tangle_note(&db_create, &app_create, title);
            }
        });
        action_group.add_action(&create_action);

        let link_action = gtk4::gio::SimpleAction::new("link-tangle", None);
        link_action.set_enabled(false);
        let buf_link = buffer.clone();
        let link_t = link_target.clone();
        link_action.connect_activate(move |_, _| {
            if let Some(ref title) = *link_t.borrow() {
                if let Some((start, end)) = buf_link.selection_bounds() {
                    let tag_name = format!("tangle::{}", title);
                    let tag = get_or_create_tag(&buf_link.tag_table(), &tag_name);
                    buf_link.apply_tag(&tag, &start, &end);
                }
            }
        });
        action_group.add_action(&link_action);

        text_view.insert_action_group("tangle", Some(&action_group));

        // Add tangle items to the native context menu
        let tangle_menu = gtk4::gio::Menu::new();
        tangle_menu.append(Some("Jump to Tangle"), Some("tangle.jump-tangle"));
        tangle_menu.append(Some("Link to Tangle"), Some("tangle.link-tangle"));
        tangle_menu.append(Some("Create Tangle"), Some("tangle.create-tangle"));
        let extra_menu = gtk4::gio::Menu::new();
        extra_menu.append_section(Some("Tangles"), &tangle_menu);
        text_view.set_extra_menu(Some(&extra_menu));

        // Update tangle action states on right-click (before menu shows)
        let right_click = gtk4::GestureClick::builder().button(3).build();
        let buf_rc = buffer.clone();
        let tv_rc = text_view.clone();
        let db_rc = db.clone();
        let tangle_t2 = tangle_target.clone();
        let create_t2 = create_target.clone();
        let link_t2 = link_target.clone();
        let jump_a = jump_action.clone();
        let create_a = create_action.clone();
        let link_a = link_action.clone();
        right_click.connect_pressed(move |_, _, x, y| {
            update_tangle_actions(
                &tv_rc, &buf_rc, x, y, &db_rc,
                &jump_a, &create_a, &link_a,
                &tangle_t2, &create_t2, &link_t2,
            );
        });
        right_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        text_view.add_controller(right_click);

        // Pending tags: apply to newly inserted text
        let pt_insert = pending_tags.clone();
        let inhibit_insert = inhibit_changed.clone();
        buffer.connect_insert_text(move |buf, iter, text| {
            if inhibit_insert.get() {
                return;
            }
            let tags = pt_insert.borrow().clone();
            if tags.is_empty() {
                return;
            }
            let end_offset = iter.offset();
            let start_offset = end_offset - text.chars().count() as i32;
            let start = buf.iter_at_offset(start_offset);
            let end = buf.iter_at_offset(end_offset);
            for tag_name in &tags {
                if let Some(tag) = buf.tag_table().lookup(tag_name) {
                    buf.apply_tag(&tag, &start, &end);
                } else {
                    let tag = get_or_create_tag(&buf.tag_table(), tag_name);
                    buf.apply_tag(&tag, &start, &end);
                }
            }
        });

        // Clear pending tags on cursor movement
        let pt_mark = pending_tags.clone();
        buffer.connect_mark_set(move |_, _, mark| {
            if mark.name().as_deref() == Some("insert") {
                pt_mark.borrow_mut().clear();
            }
        });

        let scrolled = ScrolledWindow::builder()
            .child(&text_view)
            .vexpand(true)
            .hexpand(true)
            .build();

        let source_scrolled = ScrolledWindow::builder()
            .child(&source_view)
            .vexpand(true)
            .hexpand(true)
            .visible(false)
            .build();

        widget.append(&scrolled);
        widget.append(&source_scrolled);

        // Wire up the source toggle
        let buf_toggle = buffer.clone();
        let src_buf_toggle = source_buffer.clone();
        let tv_toggle = text_view.clone();
        let im_toggle = image_map.clone();
        let is_src = is_source_mode.clone();
        let scrolled_ref = scrolled.clone();
        let source_scrolled_ref = source_scrolled.clone();
        let inhibit_toggle = inhibit_changed.clone();
        source_toggle_btn.connect_clicked(move |btn| {
            let currently_source = is_src.get();
            if currently_source {
                // Source → Rich: parse HTML from source buffer back into rich buffer
                let html = src_buf_toggle.text(
                    &src_buf_toggle.start_iter(),
                    &src_buf_toggle.end_iter(),
                    false,
                ).to_string();
                inhibit_toggle.set(true);
                buf_toggle.set_text("");
                if !html.is_empty() {
                    deserialize_html(&buf_toggle, &tv_toggle, &html, &im_toggle);
                }
                inhibit_toggle.set(false);
                source_scrolled_ref.set_visible(false);
                scrolled_ref.set_visible(true);
                btn.remove_css_class("pinned");
                is_src.set(false);
            } else {
                // Rich → Source: serialize to HTML and show in source buffer
                let html = serialize_to_html(&buf_toggle, &im_toggle);
                src_buf_toggle.set_text(&html);
                scrolled_ref.set_visible(false);
                source_scrolled_ref.set_visible(true);
                btn.add_css_class("pinned");
                is_src.set(true);
            }
        });

        // Auto-link timer: scan for note title matches after edit pause
        let autolink_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let buf_for_autolink = buffer.clone();
        let db_for_autolink = db.clone();
        let own_title_for_autolink = own_title.clone();
        let inhibit_for_autolink = inhibit_changed.clone();
        buffer.connect_changed(move |_| {
            if inhibit_for_autolink.get() {
                return;
            }
            if let Some(id) = autolink_timer.borrow_mut().take() {
                id.remove();
            }
            let timer_ref = autolink_timer.clone();
            let buf = buf_for_autolink.clone();
            let db = db_for_autolink.clone();
            let title = own_title_for_autolink.clone();
            let source_id = glib::timeout_add_local_once(
                std::time::Duration::from_millis(3000),
                move || {
                    auto_link_note_titles(&buf, &db, &title.borrow());
                    *timer_ref.borrow_mut() = None;
                },
            );
            *autolink_timer.borrow_mut() = Some(source_id);
        });

        RichEditor {
            widget,
            text_view,
            buffer,
            source_view,
            source_buffer,
            is_source_mode,
            pending_tags,
            image_map,
            inhibit_changed,
            own_title,
        }
    }

    pub fn set_content(&self, html: &str) {
        self.inhibit_changed.set(true);
        self.buffer.set_text("");
        if !html.is_empty() {
            deserialize_html(&self.buffer, &self.text_view, html, &self.image_map);
        }
        self.inhibit_changed.set(false);
    }

    pub fn get_content(&self) -> String {
        if self.is_source_mode.get() {
            self.source_buffer.text(
                &self.source_buffer.start_iter(),
                &self.source_buffer.end_iter(),
                false,
            ).to_string()
        } else {
            serialize_to_html(&self.buffer, &self.image_map)
        }
    }

    pub fn get_source_buffer(&self) -> &TextBuffer {
        &self.source_buffer
    }

    pub fn set_own_title(&self, title: &str) {
        *self.own_title.borrow_mut() = title.to_string();
    }
}

// ── Inline tag toggling ────────────────────────────────────────────

fn toggle_inline_tag(buffer: &TextBuffer, tag_name: &str, pending: &Rc<RefCell<HashSet<String>>>) {
    let tag = get_or_create_tag(&buffer.tag_table(), tag_name);
    if let Some((start, end)) = buffer.selection_bounds() {
        if has_tag_in_range(buffer, tag_name, &start, &end) {
            buffer.remove_tag(&tag, &start, &end);
        } else {
            buffer.apply_tag(&tag, &start, &end);
        }
    } else {
        let mut p = pending.borrow_mut();
        if p.contains(tag_name) {
            p.remove(tag_name);
        } else {
            p.insert(tag_name.to_string());
        }
    }
}

fn has_tag_in_range(buffer: &TextBuffer, tag_name: &str, start: &TextIter, end: &TextIter) -> bool {
    let tag = match buffer.tag_table().lookup(tag_name) {
        Some(t) => t,
        None => return false,
    };
    let mut iter = *start;
    while iter.offset() < end.offset() {
        if iter.has_tag(&tag) {
            return true;
        }
        if !iter.forward_char() {
            break;
        }
    }
    false
}

fn get_or_create_tag(table: &gtk4::TextTagTable, name: &str) -> TextTag {
    if let Some(tag) = table.lookup(name) {
        return tag;
    }
    let tag = if name.starts_with("fg::") {
        let color = &name[4..];
        TextTag::builder()
            .name(name)
            .foreground(color)
            .build()
    } else if name.starts_with("bg::") {
        let color = &name[4..];
        TextTag::builder()
            .name(name)
            .background(color)
            .build()
    } else if name.starts_with("link::") {
        TextTag::builder()
            .name(name)
            .foreground("#6699ff")
            .underline(gtk4::pango::Underline::Single)
            .build()
    } else if name.starts_with("tangle::") {
        TextTag::builder()
            .name(name)
            .foreground("#b388ff")
            .underline(gtk4::pango::Underline::Single)
            .style(gtk4::pango::Style::Italic)
            .build()
    } else {
        TextTag::builder().name(name).build()
    };
    table.add(&tag);
    tag
}

// ── Headings ───────────────────────────────────────────────────────

fn apply_heading(buffer: &TextBuffer, tag_name: &str) {
    let mark = buffer.get_insert();
    let iter = buffer.iter_at_mark(&mark);
    let line_start = buffer.iter_at_line(iter.line()).unwrap_or(iter);
    let mut line_end = line_start;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }

    let heading_tags = ["h1", "h2", "h3", "h4"];
    let already_has = has_tag_in_range(buffer, tag_name, &line_start, &line_end);

    // Remove all heading tags from line
    for ht in &heading_tags {
        if let Some(tag) = buffer.tag_table().lookup(*ht) {
            buffer.remove_tag(&tag, &line_start, &line_end);
        }
    }

    if !already_has {
        let tag = get_or_create_tag(&buffer.tag_table(), tag_name);
        buffer.apply_tag(&tag, &line_start, &line_end);
    }
}

// ── Lists ──────────────────────────────────────────────────────────

/// Count the character length of the list prefix on a line (bullet or numbered).
/// Returns 0 if no recognized prefix found.
fn count_list_prefix_chars(line_text: &str) -> i32 {
    if line_text.starts_with("  \u{2022} ") {
        // "  • " = 4 characters (2 spaces + bullet + space)
        return "  \u{2022} ".chars().count() as i32;
    }
    // Check for numbered prefix "  N. "
    let trimmed = line_text.trim_start();
    let leading_chars = line_text.chars().count() - trimmed.chars().count();
    if let Some(dot_pos) = trimmed.find(". ") {
        let num_part = &trimmed[..dot_pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) && dot_pos <= 4 {
            let num_prefix_chars = num_part.chars().count() + 2; // digits + ". "
            return (leading_chars + num_prefix_chars) as i32;
        }
    }
    0
}

fn toggle_list(buffer: &TextBuffer, list_tag_name: &str) {
    let mark = buffer.get_insert();
    let iter = buffer.iter_at_mark(&mark);
    let line = iter.line();
    let line_start = buffer.iter_at_line(line).unwrap_or(iter);
    let mut line_end = line_start;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }

    let tag = get_or_create_tag(&buffer.tag_table(), list_tag_name);

    if has_tag_in_range(buffer, list_tag_name, &line_start, &line_end) {
        // Remove list tag and bullet prefix — only delete the prefix chars to preserve inline tags
        buffer.remove_tag(&tag, &line_start, &line_end);
        let line_text = buffer.text(&line_start, &line_end, false).to_string();
        let prefix_chars = count_list_prefix_chars(&line_text);
        if prefix_chars > 0 {
            let mut ls = buffer.iter_at_line(line).unwrap_or(iter);
            let mut prefix_end = ls;
            prefix_end.forward_chars(prefix_chars);
            buffer.delete(&mut ls, &mut prefix_end);
        }
    } else {
        // Remove other list tag first (and its prefix)
        let other = if list_tag_name == "bullet-list" { "numbered-list" } else { "bullet-list" };
        if has_tag_in_range(buffer, other, &line_start, &line_end) {
            if let Some(other_tag) = buffer.tag_table().lookup(other) {
                buffer.remove_tag(&other_tag, &line_start, &line_end);
            }
            let line_text = buffer.text(&line_start, &line_end, false).to_string();
            let prefix_chars = count_list_prefix_chars(&line_text);
            if prefix_chars > 0 {
                let mut ls = buffer.iter_at_line(line).unwrap_or(iter);
                let mut prefix_end = ls;
                prefix_end.forward_chars(prefix_chars);
                buffer.delete(&mut ls, &mut prefix_end);
            }
        }

        let prefix = if list_tag_name == "bullet-list" { "  \u{2022} " } else { "  1. " };
        let mut ls = buffer.iter_at_line(line).unwrap_or(iter);
        buffer.insert(&mut ls, prefix);

        // Re-grab iterators after insert
        let ls = buffer.iter_at_line(line).unwrap_or(buffer.start_iter());
        let mut le = ls;
        if !le.ends_line() {
            le.forward_to_line_end();
        }
        buffer.apply_tag(&tag, &ls, &le);
    }
}

fn handle_enter_key(buffer: &TextBuffer) -> bool {
    let mark = buffer.get_insert();
    let iter = buffer.iter_at_mark(&mark);
    let line = iter.line();
    let line_start = match buffer.iter_at_line(line) {
        Some(it) => it,
        None => return false,
    };
    let mut line_end = line_start;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }

    let line_text = buffer.text(&line_start, &line_end, false).to_string();

    let is_bullet = has_tag_in_range(buffer, "bullet-list", &line_start, &line_end);
    let is_numbered = has_tag_in_range(buffer, "numbered-list", &line_start, &line_end);

    if !is_bullet && !is_numbered {
        return false;
    }

    // Check if the line only has the prefix (no real content after it)
    let prefix_chars = count_list_prefix_chars(&line_text);
    let content_after_prefix: String = line_text.chars().skip(prefix_chars as usize).collect();
    let is_empty_item = content_after_prefix.trim().is_empty();

    if is_empty_item {
        // Empty list item — remove prefix and list tag
        let mut ls = buffer.iter_at_line(line).unwrap();
        let mut le = ls;
        if !le.ends_line() {
            le.forward_to_line_end();
        }
        let tag_name = if is_bullet { "bullet-list" } else { "numbered-list" };
        if let Some(tag) = buffer.tag_table().lookup(tag_name) {
            buffer.remove_tag(&tag, &ls, &le);
        }
        buffer.delete(&mut ls, &mut le);
        return true;
    }

    // Insert newline with list prefix
    let mut insert_iter = buffer.iter_at_mark(&mark);
    if is_bullet {
        buffer.insert(&mut insert_iter, "\n  \u{2022} ");
        // Apply bullet-list tag to the new line
        let new_line = line + 1;
        if let Some(new_ls) = buffer.iter_at_line(new_line) {
            let mut new_le = new_ls;
            if !new_le.ends_line() {
                new_le.forward_to_line_end();
            }
            let tag = get_or_create_tag(&buffer.tag_table(), "bullet-list");
            buffer.apply_tag(&tag, &new_ls, &new_le);
        }
        return true;
    }

    if is_numbered {
        // Parse current number
        let num = line_text.trim().split('.').next()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .unwrap_or(0) + 1;
        buffer.insert(&mut insert_iter, &format!("\n  {}. ", num));
        let new_line = line + 1;
        if let Some(new_ls) = buffer.iter_at_line(new_line) {
            let mut new_le = new_ls;
            if !new_le.ends_line() {
                new_le.forward_to_line_end();
            }
            let tag = get_or_create_tag(&buffer.tag_table(), "numbered-list");
            buffer.apply_tag(&tag, &new_ls, &new_le);
        }
        return true;
    }

    false
}

// ── Link insertion ─────────────────────────────────────────────────

fn insert_web_link_dialog(relative_to: &Button, buffer: &TextBuffer) {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    vbox.append(&Label::builder().label("Web Link").css_classes(["dim-label"]).build());

    let url_entry = gtk4::Entry::builder()
        .placeholder_text("https://...")
        .width_chars(30)
        .build();
    vbox.append(&url_entry);

    let label_entry = gtk4::Entry::builder()
        .placeholder_text("Display text (optional)")
        .width_chars(30)
        .build();

    // Pre-fill display text from selection
    if let Some((start, end)) = buffer.selection_bounds() {
        let sel_text = buffer.text(&start, &end, false).to_string();
        label_entry.set_text(&sel_text);
    }
    vbox.append(&label_entry);

    let insert_btn = Button::builder().label("Insert Link").build();
    let buf = buffer.clone();
    let pop = popover.clone();
    let url_ref = url_entry.clone();
    let label_ref = label_entry.clone();
    insert_btn.connect_clicked(move |_| {
        let url = url_ref.text().to_string();
        if url.is_empty() {
            pop.popdown();
            return;
        }
        let display = label_ref.text().to_string();
        let display = if display.is_empty() { url.clone() } else { display };
        let tag_name = format!("link::{}", url);

        // If there's a selection, apply the tag to it
        if let Some((start, end)) = buf.selection_bounds() {
            let tag = get_or_create_tag(&buf.tag_table(), &tag_name);
            buf.apply_tag(&tag, &start, &end);
        } else {
            // Insert display text with link tag
            let cursor = buf.cursor_position();
            let mut iter = buf.iter_at_offset(cursor);
            buf.insert(&mut iter, &display);
            let start = buf.iter_at_offset(cursor);
            let end = buf.iter_at_offset(cursor + display.chars().count() as i32);
            let tag = get_or_create_tag(&buf.tag_table(), &tag_name);
            buf.apply_tag(&tag, &start, &end);
        }
        pop.popdown();
    });
    vbox.append(&insert_btn);

    popover.set_child(Some(&vbox));
    popover.popup();
    url_entry.grab_focus();
}

fn insert_tangle_dialog(relative_to: &Button, buffer: &TextBuffer, db: &Database, app: &gtk4::Application) {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    vbox.append(&Label::builder().label("Link to Note (Tangle)").css_classes(["dim-label"]).build());

    let note_entry = gtk4::Entry::builder()
        .placeholder_text("Type to search notes...")
        .width_chars(25)
        .build();
    vbox.append(&note_entry);

    // Scrollable list of matching notes from DB
    let list_box = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .build();
    list_box.add_css_class("boxed-list");

    let list_scrolled = ScrolledWindow::builder()
        .child(&list_box)
        .max_content_height(200)
        .propagate_natural_height(true)
        .build();
    vbox.append(&list_scrolled);

    // Populate with all notes initially
    let all_notes = db.get_all_notes().unwrap_or_default();
    populate_tangle_list(&list_box, &all_notes);

    // Filter as user types (debounced)
    let db_search = db.clone();
    let list_ref = list_box.clone();
    let tangle_search_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    note_entry.connect_changed(move |entry| {
        if let Some(id) = tangle_search_timer.borrow_mut().take() {
            id.remove();
        }
        let query = entry.text().to_string();
        let db = db_search.clone();
        let list = list_ref.clone();
        let timer_ref = tangle_search_timer.clone();
        let source_id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(250),
            move || {
                let notes = if query.is_empty() {
                    db.get_all_notes().unwrap_or_default()
                } else {
                    db.search_notes(&query).unwrap_or_default()
                };
                populate_tangle_list(&list, &notes);
                *timer_ref.borrow_mut() = None;
            },
        );
        *tangle_search_timer.borrow_mut() = Some(source_id);
    });

    // Clicking a row fills the entry
    let entry_for_row = note_entry.clone();
    list_box.connect_row_activated(move |_, row| {
        if let Some(label) = row.child().and_then(|c| c.downcast::<Label>().ok()) {
            entry_for_row.set_text(&label.text());
        }
    });

    // Status label showing whether note exists
    let status_label = Label::builder()
        .css_classes(["dim-label"])
        .xalign(0.0)
        .build();
    vbox.append(&status_label);

    let db_status = db.clone();
    let status_ref = status_label.clone();
    let entry_for_status = note_entry.clone();
    // Update status when entry text changes
    let db_status2 = db_status.clone();
    entry_for_status.connect_changed(move |entry| {
        let title = entry.text().to_string();
        if title.is_empty() {
            status_ref.set_text("");
        } else {
            match db_status2.get_note_by_title(&title) {
                Ok(Some(_)) => status_ref.set_text("Existing note"),
                Ok(None) => status_ref.set_text("Will create new note"),
                Err(_) => status_ref.set_text(""),
            }
        }
    });

    let insert_btn = Button::builder().label("Insert Tangle").build();
    let buf = buffer.clone();
    let pop = popover.clone();
    let entry_ref = note_entry.clone();
    let db_ref = db.clone();
    let app_ref = app.clone();
    insert_btn.connect_clicked(move |_| {
        let note_title = entry_ref.text().to_string();
        if note_title.is_empty() {
            pop.popdown();
            return;
        }

        // Auto-create the target note if it doesn't exist
        ensure_tangle_note_exists(&db_ref, &note_title);

        let tag_name = format!("tangle::{}", note_title);

        if let Some((start, end)) = buf.selection_bounds() {
            let tag = get_or_create_tag(&buf.tag_table(), &tag_name);
            buf.apply_tag(&tag, &start, &end);
        } else {
            let cursor = buf.cursor_position();
            let mut iter = buf.iter_at_offset(cursor);
            buf.insert(&mut iter, &note_title);
            let start = buf.iter_at_offset(cursor);
            let end = buf.iter_at_offset(cursor + note_title.chars().count() as i32);
            let tag = get_or_create_tag(&buf.tag_table(), &tag_name);
            buf.apply_tag(&tag, &start, &end);
        }
        pop.popdown();

        // Open the tangle note
        open_tangle_note(&db_ref, &app_ref, &note_title);
    });
    vbox.append(&insert_btn);

    popover.set_child(Some(&vbox));
    popover.popup();
    note_entry.grab_focus();
}

fn populate_tangle_list(list_box: &gtk4::ListBox, notes: &[Note]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    for note in notes {
        let label = Label::builder()
            .label(&note.title)
            .xalign(0.0)
            .margin_start(4)
            .margin_end(4)
            .margin_top(2)
            .margin_bottom(2)
            .build();
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&label));
        list_box.append(&row);
    }
}

/// Ensure a note with the given title exists; create it (blank) if not.
fn ensure_tangle_note_exists(db: &Database, title: &str) {
    match db.get_note_by_title(title) {
        Ok(Some(_)) => {} // already exists
        Ok(None) => {
            let now = chrono::Utc::now().to_rfc3339();
            let note = Note {
                id: None,
                title: title.to_string(),
                content: String::new(),
                created_at: now.clone(),
                updated_at: now,
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
            };
            if let Err(e) = db.create_note(&note) {
                eprintln!("Error auto-creating tangle note '{}': {}", title, e);
            }
        }
        Err(e) => {
            eprintln!("Error checking tangle note '{}': {}", title, e);
        }
    }
}

/// Update tangle action enabled state based on cursor/selection context.
fn update_tangle_actions(
    text_view: &TextView,
    buffer: &TextBuffer,
    x: f64,
    y: f64,
    db: &Database,
    jump_action: &gtk4::gio::SimpleAction,
    create_action: &gtk4::gio::SimpleAction,
    link_action: &gtk4::gio::SimpleAction,
    tangle_target: &Rc<RefCell<Option<String>>>,
    create_target: &Rc<RefCell<Option<String>>>,
    link_target: &Rc<RefCell<Option<String>>>,
) {
    let (bx, by) = text_view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x as i32, y as i32);
    let iter_at_click = text_view.iter_at_location(bx, by);

    // Check if clicking on a tangle tag
    let tangle_title = iter_at_click.as_ref().and_then(|iter| {
        iter.tags().into_iter().find_map(|tag| {
            let name = tag.name()?.to_string();
            name.strip_prefix("tangle::").map(|s| s.to_string())
        })
    });

    // Check selected text against DB
    let selected_text = buffer.selection_bounds().map(|(start, end)| {
        buffer.text(&start, &end, false).to_string()
    });

    let (show_create, show_link) = if let Some(ref text) = selected_text {
        let text = text.trim();
        if text.is_empty() || text.contains('\n') {
            (None, None)
        } else {
            match db.get_note_by_title(text) {
                Ok(Some(_)) => (None, Some(text.to_string())),
                Ok(None) => (Some(text.to_string()), None),
                Err(_) => (None, None),
            }
        }
    } else {
        (None, None)
    };

    // Update targets and enabled state
    *tangle_target.borrow_mut() = tangle_title.clone();
    jump_action.set_enabled(tangle_title.is_some());

    *create_target.borrow_mut() = show_create.clone();
    create_action.set_enabled(show_create.is_some());

    *link_target.borrow_mut() = show_link.clone();
    link_action.set_enabled(show_link.is_some());
}

/// Open a note by title (for tangle navigation).
/// If the note is already open in a window, focus it and flash its border.
pub fn open_tangle_note(db: &Database, app: &gtk4::Application, title: &str) {
    let note = match db.get_note_by_title(title) {
        Ok(Some(n)) => n,
        Ok(None) => {
            ensure_tangle_note_exists(db, title);
            match db.get_note_by_title(title) {
                Ok(Some(n)) => n,
                _ => return,
            }
        }
        Err(e) => {
            eprintln!("Error opening tangle '{}': {}", title, e);
            return;
        }
    };

    // Check if a window for this note is already open
    if let Some(note_id) = note.id {
        let target_class = format!("note-{}", note_id);
        for win in app.windows() {
            if win.css_classes().iter().any(|c| c == &target_class) {
                win.present();
                flash_window_border(&win);
                return;
            }
        }
    }

    let nw = crate::note_window::NoteWindow::new(app, db.clone(), Some(note));
    nw.present();
}

/// Briefly flash a highlight border on a window to draw attention.
fn flash_window_border(window: &gtk4::Window) {
    window.add_css_class("tangle-flash");

    let win_ref = window.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
        win_ref.remove_css_class("tangle-flash");
    });
}

// ── Auto-linking: scan buffer for note title matches ───────────────

/// Compute title matches off-thread, then apply tags on main thread.
fn auto_link_note_titles(buffer: &TextBuffer, db: &Database, own_title: &str) {
    let full_text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
    let own = own_title.to_string();
    let db = db.clone();

    // Heavy work: DB query + string matching → background thread
    // Only Send types cross thread boundary; buffer stays on main thread via idle callback
    let (tx, rx) = std::sync::mpsc::channel::<Vec<(usize, String)>>();

    std::thread::spawn(move || {
        let mut titles = db.get_all_note_titles().unwrap_or_default();
        titles.sort_by(|a, b| b.len().cmp(&a.len()));

        let chars: Vec<char> = full_text.chars().collect();
        let text_len = chars.len();

        let mut matches: Vec<(usize, String)> = Vec::new();

        for title in &titles {
            if title == &own || title.is_empty() || title == "New Note" {
                continue;
            }
            let title_chars: Vec<char> = title.chars().collect();
            let title_len = title_chars.len();
            if title_len == 0 || title_len > text_len {
                continue;
            }

            let mut i = 0;
            while i + title_len <= text_len {
                if chars[i..i + title_len] == title_chars[..] {
                    let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                    let after_ok = i + title_len >= text_len || !chars[i + title_len].is_alphanumeric();
                    if before_ok && after_ok {
                        matches.push((i, title.clone()));
                    }
                    i += title_len;
                } else {
                    i += 1;
                }
            }
        }

        let _ = tx.send(matches);
    });

    // Poll for results on main thread without blocking
    let buf = buffer.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(matches) => {
                for (offset, title) in &matches {
                    let start_iter = buf.iter_at_offset(*offset as i32);
                    let already_tagged = start_iter.tags().iter().any(|t| {
                        t.name().map_or(false, |n| n.starts_with("tangle::"))
                    });
                    if !already_tagged {
                        let title_len = title.chars().count();
                        let end_iter = buf.iter_at_offset((*offset + title_len) as i32);
                        let tag_name = format!("tangle::{}", title);
                        let tag = get_or_create_tag(&buf.tag_table(), &tag_name);
                        buf.apply_tag(&tag, &start_iter, &end_iter);
                    }
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break, // channel closed
        }
    });
}

// ── Color picker ───────────────────────────────────────────────────

fn show_color_picker(relative_to: &Button, kind: &str, buffer: &TextBuffer, pending: &Rc<RefCell<HashSet<String>>>) {
    let popover = gtk4::Popover::new();
    popover.set_parent(relative_to);

    let vbox = Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();

    let title = if kind == "fg" { "Text Color" } else { "Highlight Color" };
    vbox.append(&Label::builder().label(title).css_classes(["dim-label"]).build());

    let colors: &[&str] = &[
        "#ef5350", "#ff7043", "#ffca28", "#66bb6a",
        "#42a5f5", "#7e57c2", "#ec407a", "#ffffff",
        "#e0e0e0", "#9e9e9e", "#616161", "#000000",
    ];

    let flow = gtk4::FlowBox::builder()
        .max_children_per_line(6)
        .min_children_per_line(6)
        .selection_mode(gtk4::SelectionMode::None)
        .build();

    for color in colors {
        let swatch = gtk4::DrawingArea::builder()
            .width_request(28)
            .height_request(28)
            .build();
        let c = color.to_string();
        swatch.set_draw_func(move |_area, cr, w, h| {
            let rgba = parse_hex_color(&c);
            cr.set_source_rgba(rgba.0, rgba.1, rgba.2, 1.0);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = cr.fill();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
            cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });

        let btn = Button::builder()
            .child(&swatch)
            .tooltip_text(*color)
            .css_classes(["color-swatch-btn"])
            .build();

        let buf = buffer.clone();
        let pt = pending.clone();
        let k = kind.to_string();
        let color_str = color.to_string();
        let pop = popover.clone();
        btn.connect_clicked(move |_| {
            apply_color_tag(&buf, &k, &color_str, &pt);
            pop.popdown();
        });

        flow.insert(&btn, -1);
    }

    vbox.append(&flow);

    // Custom color button
    let custom_btn = Button::builder().label("Custom...").build();
    let buf = buffer.clone();
    let pt = pending.clone();
    let k = kind.to_string();
    let pop = popover.clone();
    let parent_widget = relative_to.clone();
    custom_btn.connect_clicked(move |_| {
        pop.popdown();
        let dialog = gtk4::ColorDialog::new();
        let buf = buf.clone();
        let pt = pt.clone();
        let k = k.clone();
        let win = parent_widget.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
        dialog.choose_rgba(
            win.as_ref(),
            None,
            None::<&gtk4::gio::Cancellable>,
            move |result| {
                if let Ok(rgba) = result {
                    let hex = format!("#{:02x}{:02x}{:02x}",
                        (rgba.red() * 255.0) as u8,
                        (rgba.green() * 255.0) as u8,
                        (rgba.blue() * 255.0) as u8,
                    );
                    apply_color_tag(&buf, &k, &hex, &pt);
                }
            },
        );
    });
    vbox.append(&custom_btn);

    // Remove color button
    let clear_btn = Button::builder().label("Remove Color").build();
    let buf = buffer.clone();
    let k = kind.to_string();
    let pop2 = popover.clone();
    clear_btn.connect_clicked(move |_| {
        remove_color_tags(&buf, &k);
        pop2.popdown();
    });
    vbox.append(&clear_btn);

    popover.set_child(Some(&vbox));
    popover.popup();
}

fn apply_color_tag(buffer: &TextBuffer, kind: &str, color: &str, pending: &Rc<RefCell<HashSet<String>>>) {
    let tag_name = format!("{}::{}", kind, color);
    let tag = get_or_create_tag(&buffer.tag_table(), &tag_name);

    if let Some((start, end)) = buffer.selection_bounds() {
        // Remove existing color tags of this kind from the selection first
        remove_color_tags_in_range(buffer, kind, &start, &end);
        buffer.apply_tag(&tag, &start, &end);
    } else {
        let mut p = pending.borrow_mut();
        // Remove any other color pending tags of the same kind
        p.retain(|t| !t.starts_with(&format!("{}::", kind)));
        p.insert(tag_name);
    }
}

fn remove_color_tags(buffer: &TextBuffer, kind: &str) {
    if let Some((start, end)) = buffer.selection_bounds() {
        remove_color_tags_in_range(buffer, kind, &start, &end);
    }
}

fn remove_color_tags_in_range(buffer: &TextBuffer, kind: &str, start: &TextIter, end: &TextIter) {
    let prefix = format!("{}::", kind);
    let tags: Vec<TextTag> = start.tags().into_iter()
        .chain(end.tags().into_iter())
        .filter(|t| t.name().map_or(false, |n| n.starts_with(&prefix)))
        .collect();
    for tag in tags {
        buffer.remove_tag(&tag, start, end);
    }
}

fn parse_hex_color(hex: &str) -> (f64, f64, f64) {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
        (r, g, b)
    } else {
        (0.5, 0.5, 0.5)
    }
}

// ── Image insertion ────────────────────────────────────────────────

fn insert_image_widget(
    buffer: &TextBuffer,
    tv_holder: &Rc<RefCell<Option<TextView>>>,
    path: &str,
    width: i32,
    image_map: &Rc<RefCell<HashMap<i32, ImageInfo>>>,
) {
    if !std::path::Path::new(path).exists() {
        return;
    }
    let tv = match tv_holder.borrow().as_ref() {
        Some(tv) => tv.clone(),
        None => return,
    };

    let cursor = buffer.cursor_position();
    let mut iter = buffer.iter_at_offset(cursor);
    let anchor = buffer.create_child_anchor(&mut iter);
    // The ORC char is at cursor (the position before iter advanced)
    let orc_offset = cursor;

    let im_cb = image_map.clone();
    let offset_cb = orc_offset;
    let frame = pickers::build_resizable_picture(path, width, Some(std::boxed::Box::new(move |new_w| {
        if let Some(info) = im_cb.borrow_mut().get_mut(&offset_cb) {
            info.width = new_w;
        }
    })));
    tv.add_child_at_anchor(&frame, &anchor);

    image_map.borrow_mut().insert(orc_offset, ImageInfo {
        path: path.to_string(),
        width,
    });
}

// ── Serialization: Buffer → HTML ───────────────────────────────────

fn serialize_to_html(buffer: &TextBuffer, image_map: &Rc<RefCell<HashMap<i32, ImageInfo>>>) -> String {
    let mut html = String::new();
    let line_count = buffer.line_count();
    let mut in_list: Option<String> = None; // "bullet-list" or "numbered-list"

    for line_idx in 0..line_count {
        let line_start = match buffer.iter_at_line(line_idx) {
            Some(it) => it,
            None => continue,
        };
        let mut line_end = line_start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }

        // Skip trailing empty line (GTK adds one after final \n)
        if line_idx == line_count - 1 && line_start.offset() == line_end.offset()
            && line_start.offset() == buffer.end_iter().offset()
        {
            continue;
        }

        // Determine block-level tag
        let block_tag = determine_block_tag(buffer, &line_start, &line_end);

        // Handle list container transitions
        let current_list = match block_tag.as_deref() {
            Some("bullet-list") => Some("bullet-list".to_string()),
            Some("numbered-list") => Some("numbered-list".to_string()),
            _ => None,
        };

        if in_list != current_list {
            // Close previous list if any
            if let Some(ref prev) = in_list {
                match prev.as_str() {
                    "bullet-list" => html.push_str("</ul>\n"),
                    "numbered-list" => html.push_str("</ol>\n"),
                    _ => {}
                }
            }
            // Open new list if any
            if let Some(ref cur) = current_list {
                match cur.as_str() {
                    "bullet-list" => html.push_str("<ul>\n"),
                    "numbered-list" => html.push_str("<ol>\n"),
                    _ => {}
                }
            }
            in_list = current_list;
        }

        match block_tag.as_deref() {
            Some("h1") => html.push_str("<h1>"),
            Some("h2") => html.push_str("<h2>"),
            Some("h3") => html.push_str("<h3>"),
            Some("h4") => html.push_str("<h4>"),
            Some("bullet-list") | Some("numbered-list") => html.push_str("<li>"),
            _ => html.push_str("<p>"),
        }

        // Serialize inline content
        serialize_line_content(buffer, &line_start, &line_end, &mut html, image_map);

        match block_tag.as_deref() {
            Some("h1") => html.push_str("</h1>"),
            Some("h2") => html.push_str("</h2>"),
            Some("h3") => html.push_str("</h3>"),
            Some("h4") => html.push_str("</h4>"),
            Some("bullet-list") | Some("numbered-list") => html.push_str("</li>"),
            _ => html.push_str("</p>"),
        }
        html.push('\n');
    }

    // Close any remaining open list
    if let Some(ref prev) = in_list {
        match prev.as_str() {
            "bullet-list" => html.push_str("</ul>\n"),
            "numbered-list" => html.push_str("</ol>\n"),
            _ => {}
        }
    }

    html
}

fn determine_block_tag(buffer: &TextBuffer, start: &TextIter, end: &TextIter) -> Option<String> {
    for tag_name in &["h1", "h2", "h3", "h4", "bullet-list", "numbered-list"] {
        if has_tag_in_range(buffer, tag_name, start, end) {
            return Some(tag_name.to_string());
        }
    }
    None
}

fn serialize_line_content(
    buffer: &TextBuffer,
    start: &TextIter,
    end: &TextIter,
    html: &mut String,
    image_map: &Rc<RefCell<HashMap<i32, ImageInfo>>>,
) {
    let mut iter = *start;
    let im = image_map.borrow();

    while iter.offset() < end.offset() {
        let ch = iter.char();

        // Check for child anchor (image)
        if ch == ORC {
            if let Some(info) = im.get(&iter.offset()) {
                html.push_str(&format!("<img src=\"{}\" width=\"{}\" alt=\"image\"/>",
                    escape_html_attr(&info.path), info.width));
            }
            iter.forward_char();
            continue;
        }

        // Collect contiguous text with the same tags
        let tags_here = get_inline_tag_names(&iter);
        let seg_start = iter.offset();

        loop {
            if !iter.forward_char() || iter.offset() >= end.offset() {
                break;
            }
            let next_ch = iter.char();
            if next_ch == ORC {
                break;
            }
            let next_tags = get_inline_tag_names(&iter);
            if next_tags != tags_here {
                break;
            }
        }

        let seg_end_offset = if iter.offset() > end.offset() { end.offset() } else { iter.offset() };
        let seg_start_iter = buffer.iter_at_offset(seg_start);
        let seg_end_iter = buffer.iter_at_offset(seg_end_offset);
        let text = buffer.text(&seg_start_iter, &seg_end_iter, false).to_string();

        // Strip list prefixes from output
        let text = strip_list_prefix(&text);

        if text.is_empty() && !tags_here.is_empty() {
            continue;
        }
        if text.is_empty() {
            continue;
        }

        // Open tags
        let mut open_tags: Vec<String> = Vec::new();
        for tag_name in &tags_here {
            match tag_name.as_str() {
                "bold" => open_tags.push("<b>".to_string()),
                "italic" => open_tags.push("<i>".to_string()),
                "underline" => open_tags.push("<u>".to_string()),
                "strikethrough" => open_tags.push("<s>".to_string()),
                n if n.starts_with("fg::") => {
                    let color = &n[4..];
                    open_tags.push(format!("<span style=\"color:{}\">", color));
                }
                n if n.starts_with("bg::") => {
                    let color = &n[4..];
                    open_tags.push(format!("<span style=\"background-color:{}\">", color));
                }
                n if n.starts_with("link::") => {
                    let url = &n[6..];
                    open_tags.push(format!("<a href=\"{}\">", escape_html_attr(url)));
                }
                n if n.starts_with("tangle::") => {
                    let note_title = &n[8..];
                    open_tags.push(format!("<a href=\"tangle://{}\" class=\"tangle\">", escape_html_attr(note_title)));
                }
                _ => {} // block tags handled at line level
            }
        }

        for t in &open_tags {
            html.push_str(t);
        }

        html.push_str(&escape_html(&text));

        // Close tags in reverse
        for t in open_tags.iter().rev() {
            let close = match t.as_str() {
                s if s == "<b>" => "</b>",
                s if s == "<i>" => "</i>",
                s if s == "<u>" => "</u>",
                s if s == "<s>" => "</s>",
                s if s.starts_with("<span") => "</span>",
                s if s.starts_with("<a ") => "</a>",
                _ => continue,
            };
            html.push_str(close);
        }
    }
}

fn strip_list_prefix(text: &str) -> String {
    // Remove "  • " or "  1. " prefix
    let bullet_prefix = "  \u{2022} ";
    if text.starts_with(bullet_prefix) {
        let char_count = bullet_prefix.chars().count();
        return text.chars().skip(char_count).collect();
    }
    if text.starts_with("  ") {
        let trimmed = text.trim_start();
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.chars().all(|c| c.is_ascii_digit()) && dot_pos <= 4 {
                return trimmed[dot_pos + 2..].to_string();
            }
        }
    }
    text.to_string()
}

fn get_inline_tag_names(iter: &TextIter) -> Vec<String> {
    let block_tags = ["h1", "h2", "h3", "h4", "bullet-list", "numbered-list"];
    iter.tags()
        .into_iter()
        .filter_map(|t| {
            let name = t.name()?.to_string();
            if block_tags.contains(&name.as_str()) {
                None
            } else {
                Some(name)
            }
        })
        .collect()
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_html_attr(text: &str) -> String {
    escape_html(text)
}

// ── Deserialization: HTML → Buffer ─────────────────────────────────

struct HtmlSink {
    tokens: RefCell<Vec<HtmlToken>>,
}

#[derive(Debug)]
enum HtmlToken {
    StartTag(String, Vec<(String, String)>),
    EndTag(String),
    Text(String),
}

impl TokenSink for HtmlSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                let attrs: Vec<(String, String)> = tag.attrs.iter()
                    .map(|a| (a.name.local.to_string(), a.value.to_string()))
                    .collect();
                match tag.kind {
                    TagKind::StartTag => self.tokens.borrow_mut().push(HtmlToken::StartTag(name, attrs)),
                    TagKind::EndTag => self.tokens.borrow_mut().push(HtmlToken::EndTag(name)),
                }
            }
            Token::CharacterTokens(s) => {
                self.tokens.borrow_mut().push(HtmlToken::Text(s.to_string()));
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn deserialize_html(
    buffer: &TextBuffer,
    text_view: &TextView,
    html: &str,
    image_map: &Rc<RefCell<HashMap<i32, ImageInfo>>>,
) {
    // Tokenize
    let sink = HtmlSink { tokens: RefCell::new(Vec::new()) };
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let mut queue = BufferQueue::default();
    queue.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&mut queue);
    tokenizer.end();

    let tokens = tokenizer.sink.tokens.into_inner();

    // Process tokens
    let mut tag_stack: Vec<(String, Vec<(String, String)>, i32)> = Vec::new(); // (tag_name, attrs, start_offset)
    let mut list_context: Vec<String> = Vec::new(); // "ul" or "ol"
    let mut ol_counter: Vec<i32> = Vec::new();
    let mut need_newline_before_block = false;
    let mut in_block = false; // true when inside a block element (p, h1-h4, li)

    for token in &tokens {
        match token {
            HtmlToken::StartTag(name, attrs) => {
                match name.as_str() {
                    "ul" => {
                        list_context.push("ul".to_string());
                    }
                    "ol" => {
                        list_context.push("ol".to_string());
                        ol_counter.push(0);
                    }
                    "li" => {
                        if need_newline_before_block {
                            let mut end = buffer.end_iter();
                            buffer.insert(&mut end, "\n");
                        }
                        need_newline_before_block = true;
                        in_block = true;

                        let mut end = buffer.end_iter();
                        match list_context.last().map(|s| s.as_str()) {
                            Some("ul") => {
                                buffer.insert(&mut end, "  \u{2022} ");
                            }
                            Some("ol") => {
                                if let Some(counter) = ol_counter.last_mut() {
                                    *counter += 1;
                                    buffer.insert(&mut end, &format!("  {}. ", counter));
                                }
                            }
                            _ => {}
                        }
                        let new_offset = buffer.end_iter().offset();
                        tag_stack.push((name.clone(), attrs.clone(), new_offset));
                    }
                    "h1" | "h2" | "h3" | "h4" | "p" => {
                        if need_newline_before_block {
                            let mut end = buffer.end_iter();
                            buffer.insert(&mut end, "\n");
                        }
                        need_newline_before_block = true;
                        in_block = true;
                        let offset = buffer.end_iter().offset();
                        tag_stack.push((name.clone(), attrs.clone(), offset));
                    }
                    "img" => {
                        let src = attrs.iter().find(|(k, _)| k == "src").map(|(_, v)| v.as_str()).unwrap_or("");
                        let width: i32 = attrs.iter()
                            .find(|(k, _)| k == "width")
                            .and_then(|(_, v)| v.parse().ok())
                            .unwrap_or(300);

                        if !src.is_empty() && std::path::Path::new(src).exists() {
                            let mut end = buffer.end_iter();
                            let anchor = buffer.create_child_anchor(&mut end);
                            let img_offset = buffer.end_iter().offset() - 1;

                            let im_cb = image_map.clone();
                            let offset_cb = img_offset;
                            let frame = pickers::build_resizable_picture(src, width, Some(std::boxed::Box::new(move |new_w| {
                                if let Some(info) = im_cb.borrow_mut().get_mut(&offset_cb) {
                                    info.width = new_w;
                                }
                            })));
                            text_view.add_child_at_anchor(&frame, &anchor);

                            image_map.borrow_mut().insert(img_offset, ImageInfo {
                                path: src.to_string(),
                                width,
                            });
                        }
                    }
                    "br" => {
                        let mut end = buffer.end_iter();
                        buffer.insert(&mut end, "\n");
                    }
                    _ => {
                        // Inline tags
                        let offset = buffer.end_iter().offset();
                        tag_stack.push((name.clone(), attrs.clone(), offset));
                    }
                }
            }
            HtmlToken::EndTag(name) => {
                match name.as_str() {
                    "ul" => {
                        list_context.pop();
                    }
                    "ol" => {
                        list_context.pop();
                        ol_counter.pop();
                    }
                    _ => {
                        // Find matching start tag
                        if let Some(pos) = tag_stack.iter().rposition(|(n, _, _)| n == name) {
                            let (tag_name, attrs, start_offset) = tag_stack.remove(pos);
                            let end_offset = buffer.end_iter().offset();

                            // Reset in_block for block-level elements BEFORE the size check,
                            // so empty blocks (e.g. <p></p>) still reset the flag and
                            // whitespace between blocks is correctly skipped.
                            match tag_name.as_str() {
                                "p" | "h1" | "h2" | "h3" | "h4" | "li" => {
                                    in_block = false;
                                }
                                _ => {}
                            }

                            if start_offset < end_offset {
                                let start = buffer.iter_at_offset(start_offset);
                                let end = buffer.iter_at_offset(end_offset);

                                match tag_name.as_str() {
                                    "b" | "strong" => {
                                        let tag = get_or_create_tag(&buffer.tag_table(), "bold");
                                        buffer.apply_tag(&tag, &start, &end);
                                    }
                                    "i" | "em" => {
                                        let tag = get_or_create_tag(&buffer.tag_table(), "italic");
                                        buffer.apply_tag(&tag, &start, &end);
                                    }
                                    "u" => {
                                        let tag = get_or_create_tag(&buffer.tag_table(), "underline");
                                        buffer.apply_tag(&tag, &start, &end);
                                    }
                                    "s" | "strike" | "del" => {
                                        let tag = get_or_create_tag(&buffer.tag_table(), "strikethrough");
                                        buffer.apply_tag(&tag, &start, &end);
                                    }
                                    "h1" | "h2" | "h3" | "h4" => {
                                        let tag = get_or_create_tag(&buffer.tag_table(), &tag_name);
                                        buffer.apply_tag(&tag, &start, &end);
                                    }
                                    "li" => {
                                        let list_tag = match list_context.last().map(|s| s.as_str()) {
                                            Some("ul") => "bullet-list",
                                            Some("ol") => "numbered-list",
                                            _ => "bullet-list",
                                        };
                                        // Apply from start of prefix
                                        let line = start.line();
                                        let line_start = buffer.iter_at_line(line).unwrap_or(start);
                                        let tag = get_or_create_tag(&buffer.tag_table(), list_tag);
                                        buffer.apply_tag(&tag, &line_start, &end);
                                    }
                                    "span" => {
                                        // Parse style attribute
                                        if let Some((_, style_val)) = attrs.iter().find(|(k, _)| k == "style") {
                                            if let Some(color) = parse_style_color(style_val) {
                                                let tag = get_or_create_tag(&buffer.tag_table(), &color);
                                                buffer.apply_tag(&tag, &start, &end);
                                            }
                                        }
                                    }
                                    "a" => {
                                        if let Some((_, href)) = attrs.iter().find(|(k, _)| k == "href") {
                                            let is_tangle = href.starts_with("tangle://")
                                                || attrs.iter().any(|(k, v)| k == "class" && v == "tangle");
                                            let tag_name = if is_tangle {
                                                let note_title = href.strip_prefix("tangle://").unwrap_or(href);
                                                format!("tangle::{}", note_title)
                                            } else {
                                                format!("link::{}", href)
                                            };
                                            let tag = get_or_create_tag(&buffer.tag_table(), &tag_name);
                                            buffer.apply_tag(&tag, &start, &end);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            HtmlToken::Text(text) => {
                if !text.is_empty() {
                    // Skip whitespace-only text between block elements
                    if !in_block && text.chars().all(|c| c.is_whitespace()) {
                        continue;
                    }
                    let mut end = buffer.end_iter();
                    buffer.insert(&mut end, text);
                }
            }
        }
    }
}

fn parse_style_color(style: &str) -> Option<String> {
    // Parse "color:#hex" or "background-color:#hex"
    for part in style.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("color:") {
            return Some(format!("fg::{}", val.trim()));
        }
        if let Some(val) = part.strip_prefix("background-color:") {
            return Some(format!("bg::{}", val.trim()));
        }
    }
    None
}
