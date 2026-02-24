use gtk4::prelude::*;
use gtk4::{glib, ApplicationWindow};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use crate::database::Database;

struct MapNode {
    note_id: i64,
    title: String,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    w: f64,
    h: f64,
    has_saved_pos: bool,
}

struct MapEdge {
    source: usize,
    target: usize,
}

pub fn show_tangle_map(app: &gtk4::Application, parent: &ApplicationWindow, db: &Database) {
    let dialog = gtk4::Window::builder()
        .title("Tangle Map")
        .default_width(800)
        .default_height(600)
        .transient_for(parent)
        .modal(false)
        .build();
    dialog.add_css_class("note-list-dialog");

    // Extract graph data from DB
    let all_notes = db.get_all_notes().unwrap_or_default();
    let tangle_re = regex::Regex::new(r#"tangle://([^"<]+)"#).unwrap();

    let mut title_to_idx: HashMap<String, usize> = HashMap::new();
    let mut nodes: Vec<MapNode> = Vec::new();

    for note in &all_notes {
        let idx = nodes.len();
        title_to_idx.insert(note.title.clone(), idx);
        let tw = (note.title.len() as f64 * 7.0).max(60.0);
        let has_saved = note.position_x != 0.0 || note.position_y != 0.0;
        nodes.push(MapNode {
            note_id: note.id.unwrap_or(0),
            title: note.title.clone(),
            x: if has_saved { note.position_x } else { 400.0 + (idx as f64 * 37.0).sin() * 200.0 },
            y: if has_saved { note.position_y } else { 300.0 + (idx as f64 * 23.0).cos() * 200.0 },
            vx: 0.0,
            vy: 0.0,
            w: tw + 20.0,
            h: 30.0,
            has_saved_pos: has_saved,
        });
    }

    let mut edges: Vec<MapEdge> = Vec::new();
    for note in &all_notes {
        if let Some(src_idx) = title_to_idx.get(&note.title) {
            for cap in tangle_re.captures_iter(&note.content) {
                if let Some(target_title) = cap.get(1) {
                    let target_title = target_title.as_str();
                    if let Some(tgt_idx) = title_to_idx.get(target_title) {
                        if src_idx != tgt_idx {
                            edges.push(MapEdge { source: *src_idx, target: *tgt_idx });
                        }
                    }
                }
            }
        }
    }

    // Force-directed layout (Fruchterman-Reingold) — only for nodes without saved positions
    let needs_layout = nodes.iter().any(|n| !n.has_saved_pos);
    if needs_layout && nodes.len() > 1 {
        let area = 800.0 * 600.0;
        let k = (area / nodes.len() as f64).sqrt();
        let iterations = 100;

        for iter in 0..iterations {
            let temp = 10.0 * (1.0 - iter as f64 / iterations as f64);

            let positions: Vec<(f64, f64)> = nodes.iter().map(|n| (n.x, n.y)).collect();
            for i in 0..nodes.len() {
                if nodes[i].has_saved_pos { continue; }
                nodes[i].vx = 0.0;
                nodes[i].vy = 0.0;
                for j in 0..nodes.len() {
                    if i == j { continue; }
                    let dx = positions[i].0 - positions[j].0;
                    let dy = positions[i].1 - positions[j].1;
                    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                    let force = k * k / dist;
                    nodes[i].vx += dx / dist * force;
                    nodes[i].vy += dy / dist * force;
                }
            }

            for edge in &edges {
                if edge.source >= nodes.len() || edge.target >= nodes.len() {
                    continue;
                }
                if nodes[edge.source].has_saved_pos && nodes[edge.target].has_saved_pos { continue; }
                let dx = nodes[edge.source].x - nodes[edge.target].x;
                let dy = nodes[edge.source].y - nodes[edge.target].y;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let force = dist * dist / k;
                let fx = dx / dist * force;
                let fy = dy / dist * force;
                if !nodes[edge.source].has_saved_pos {
                    nodes[edge.source].vx -= fx;
                    nodes[edge.source].vy -= fy;
                }
                if !nodes[edge.target].has_saved_pos {
                    nodes[edge.target].vx += fx;
                    nodes[edge.target].vy += fy;
                }
            }

            for node in &mut nodes {
                if node.has_saved_pos { continue; }
                let mag = (node.vx * node.vx + node.vy * node.vy).sqrt().max(1.0);
                node.x += node.vx / mag * temp.min(mag);
                node.y += node.vy / mag * temp.min(mag);
                node.x = node.x.clamp(50.0, 750.0);
                node.y = node.y.clamp(50.0, 550.0);
            }
        }
    }

    let node_count = nodes.len();
    let nodes = Rc::new(RefCell::new(nodes));
    let edges: Rc<RefCell<Vec<MapEdge>>> = Rc::new(RefCell::new(edges));
    let zoom = Rc::new(Cell::new(1.0f64));
    let pan_x = Rc::new(Cell::new(0.0f64));
    let pan_y = Rc::new(Cell::new(0.0f64));
    let search_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    // Link-drag state: source node index, current mouse world coords
    let link_drag_src: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let link_drag_end: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));
    // Selection state
    let selected_nodes: Rc<RefCell<std::collections::HashSet<usize>>> = Rc::new(RefCell::new(std::collections::HashSet::new()));
    // Lasso rect in world coords: (x1, y1, x2, y2), None if not active
    let lasso_rect: Rc<Cell<Option<(f64, f64, f64, f64)>>> = Rc::new(Cell::new(None));

    let drawing_area = gtk4::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    // Draw
    let nodes_draw = nodes.clone();
    let edges_draw = edges.clone();
    let zoom_draw = zoom.clone();
    let pan_draw_x = pan_x.clone();
    let pan_draw_y = pan_y.clone();
    let search_draw = search_query.clone();
    let link_drag_src_draw = link_drag_src.clone();
    let link_drag_end_draw = link_drag_end.clone();
    let selected_draw = selected_nodes.clone();
    let lasso_draw = lasso_rect.clone();
    drawing_area.set_draw_func(move |_area, cr, w, h| {
        // Dark background
        cr.set_source_rgba(0.1, 0.1, 0.18, 1.0);
        cr.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = cr.fill();

        let z = zoom_draw.get();
        let px = pan_draw_x.get();
        let py = pan_draw_y.get();

        let _ = cr.save();
        cr.translate(px, py);
        cr.scale(z, z);

        let nodes = nodes_draw.borrow();
        let edges = edges_draw.borrow();
        let query = search_draw.borrow().to_lowercase();

        // Draw edges
        cr.set_source_rgba(0.7, 0.53, 1.0, 0.4);
        cr.set_line_width(1.5);
        for edge in edges.iter() {
            if edge.source >= nodes.len() || edge.target >= nodes.len() {
                continue;
            }
            let s = &nodes[edge.source];
            let t = &nodes[edge.target];
            let mx = (s.x + t.x) / 2.0;
            let my = (s.y + t.y) / 2.0 - 20.0;
            cr.move_to(s.x, s.y);
            cr.curve_to(mx, my, mx, my, t.x, t.y);
            let _ = cr.stroke();
        }

        // Draw link-drag preview line
        if let Some(src_idx) = link_drag_src_draw.get() {
            if src_idx < nodes.len() {
                let s = &nodes[src_idx];
                let (ex, ey) = link_drag_end_draw.get();
                cr.set_source_rgba(1.0, 0.8, 0.2, 0.7);
                cr.set_line_width(2.0);
                let dashes = [6.0, 4.0];
                cr.set_dash(&dashes, 0.0);
                cr.move_to(s.x, s.y);
                cr.line_to(ex, ey);
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }
        }

        // Draw nodes
        let sel = selected_draw.borrow();
        for (i, node) in nodes.iter().enumerate() {
            let x = node.x - node.w / 2.0;
            let y = node.y - node.h / 2.0;

            // Rounded rect
            let radius = 6.0;
            let nw = node.w.max(1.0);
            let nh = node.h.max(1.0);
            cr.new_sub_path();
            cr.arc(x + nw - radius, y + radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(x + nw - radius, y + nh - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(x + radius, y + nh - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
            cr.arc(x + radius, y + radius, radius, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
            cr.close_path();

            let highlighted = !query.is_empty() && node.title.to_lowercase().contains(&query);
            let is_selected = sel.contains(&i);

            cr.set_source_rgba(0.1, 0.1, 0.18, 0.9);
            let _ = cr.fill_preserve();
            if is_selected {
                // Cyan border for selected nodes
                cr.set_source_rgba(0.2, 0.8, 1.0, 0.9);
                cr.set_line_width(3.0);
            } else if highlighted {
                cr.set_source_rgba(0.2, 1.0, 0.4, 0.9);
                cr.set_line_width(3.0);
            } else {
                cr.set_source_rgba(0.7, 0.53, 1.0, 0.7);
                cr.set_line_width(1.5);
            }
            let _ = cr.stroke();

            // Title text
            if is_selected {
                cr.set_source_rgba(0.2, 0.8, 1.0, 1.0);
            } else if highlighted {
                cr.set_source_rgba(0.2, 1.0, 0.4, 1.0);
            } else {
                cr.set_source_rgba(0.88, 0.88, 0.88, 1.0);
            }
            cr.set_font_size(11.0);
            let (text_x, text_y) = if let Ok(extents) = cr.text_extents(&node.title) {
                (x + (nw - extents.width()) / 2.0, y + nh / 2.0 + extents.height() / 2.0)
            } else {
                (x + 10.0, y + nh / 2.0 + 4.0)
            };
            cr.move_to(text_x, text_y);
            let _ = cr.show_text(&node.title);
        }
        drop(sel);

        // Draw lasso rectangle
        if let Some((x1, y1, x2, y2)) = lasso_draw.get() {
            let lx = x1.min(x2);
            let ly = y1.min(y2);
            let lw = (x2 - x1).abs();
            let lh = (y2 - y1).abs();
            cr.set_source_rgba(0.2, 0.8, 1.0, 0.15);
            cr.rectangle(lx, ly, lw, lh);
            let _ = cr.fill();
            cr.set_source_rgba(0.2, 0.8, 1.0, 0.6);
            cr.set_line_width(1.0);
            let dashes = [4.0, 3.0];
            cr.set_dash(&dashes, 0.0);
            cr.rectangle(lx, ly, lw, lh);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }

        // Show empty message if no nodes
        if nodes.is_empty() {
            cr.set_source_rgba(0.6, 0.6, 0.6, 0.7);
            cr.set_font_size(16.0);
            cr.move_to(300.0, 300.0);
            let _ = cr.show_text("No tangles yet");
        }

        let _ = cr.restore();
    });

    // Scroll → zoom toward cursor
    let scroll_ctrl = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    let zoom_s = zoom.clone();
    let pan_s_x = pan_x.clone();
    let pan_s_y = pan_y.clone();
    let da_s = drawing_area.clone();
    scroll_ctrl.connect_scroll(move |ctrl, _, dy| {
        let old_z = zoom_s.get();
        let mut new_z = old_z * (1.0 - dy * 0.1);
        new_z = new_z.clamp(0.2, 5.0);
        zoom_s.set(new_z);

        // Zoom toward cursor position
        if let Some(event) = ctrl.current_event() {
            if let Some((mx, my)) = event.position() {
                let px = pan_s_x.get();
                let py = pan_s_y.get();
                pan_s_x.set(mx - (mx - px) * new_z / old_z);
                pan_s_y.set(my - (my - py) * new_z / old_z);
            }
        }

        da_s.queue_draw();
        glib::Propagation::Stop
    });
    drawing_area.add_controller(scroll_ctrl);

    // Right-click drag → link two nodes
    let link_drag_ctrl = gtk4::GestureDrag::builder().button(3).build();
    let ld_src = link_drag_src.clone();
    let ld_end = link_drag_end.clone();
    let nodes_ld = nodes.clone();
    let zoom_ld = zoom.clone();
    let pan_ld_x = pan_x.clone();
    let pan_ld_y = pan_y.clone();
    link_drag_ctrl.connect_drag_begin(move |_gesture, x, y| {
        let z = zoom_ld.get();
        if z == 0.0 { ld_src.set(None); return; }
        let mx = (x - pan_ld_x.get()) / z;
        let my = (y - pan_ld_y.get()) / z;
        let nodes = nodes_ld.borrow();
        for (i, node) in nodes.iter().enumerate() {
            let nx = node.x - node.w / 2.0;
            let ny = node.y - node.h / 2.0;
            if mx >= nx && mx <= nx + node.w && my >= ny && my <= ny + node.h {
                ld_src.set(Some(i));
                ld_end.set((node.x, node.y));
                return;
            }
        }
        ld_src.set(None);
    });

    let ld_src_u = link_drag_src.clone();
    let ld_end_u = link_drag_end.clone();
    let nodes_ldu = nodes.clone();
    let zoom_ldu = zoom.clone();
    let pan_ldu_x = pan_x.clone();
    let pan_ldu_y = pan_y.clone();
    let da_ldu = drawing_area.clone();
    link_drag_ctrl.connect_drag_update(move |_, ox, oy| {
        if ld_src_u.get().is_none() { return; }
        let z = zoom_ldu.get();
        if z == 0.0 { return; }
        // Get the start position from the source node, offset by drag delta
        if let Some(src_idx) = ld_src_u.get() {
            let nodes = nodes_ldu.borrow();
            if src_idx < nodes.len() {
                let sx = nodes[src_idx].x;
                let sy = nodes[src_idx].y;
                // The drag offset is in screen coords, convert to world
                let end_x = sx + ox / z;
                let end_y = sy + oy / z;
                ld_end_u.set((end_x, end_y));
            }
        }
        da_ldu.queue_draw();
    });

    let ld_src_e = link_drag_src.clone();
    let ld_end_e = link_drag_end.clone();
    let nodes_lde = nodes.clone();
    let edges_lde = edges.clone();
    let zoom_lde = zoom.clone();
    let pan_lde_x = pan_x.clone();
    let pan_lde_y = pan_y.clone();
    let db_link = db.clone();
    let app_link = app.clone();
    let da_lde = drawing_area.clone();
    link_drag_ctrl.connect_drag_end(move |_, ox, oy| {
        let src_idx = match ld_src_e.get() {
            Some(i) => i,
            None => return,
        };
        ld_src_e.set(None);

        let z = zoom_lde.get();
        if z == 0.0 { return; }

        let nodes = nodes_lde.borrow();
        if src_idx >= nodes.len() { return; }

        // Final world position of drag end
        let end_x = nodes[src_idx].x + ox / z;
        let end_y = nodes[src_idx].y + oy / z;

        // Hit-test for target node
        for (i, node) in nodes.iter().enumerate() {
            if i == src_idx { continue; }
            let nx = node.x - node.w / 2.0;
            let ny = node.y - node.h / 2.0;
            if end_x >= nx && end_x <= nx + node.w && end_y >= ny && end_y <= ny + node.h {
                // Found target — add edge and tangle link
                let src_title = nodes[src_idx].title.clone();
                let tgt_title = node.title.clone();
                drop(nodes);

                // Add visual edge
                edges_lde.borrow_mut().push(MapEdge { source: src_idx, target: i });

                // Append tangle link — inject into open editor if possible,
                // otherwise write directly to DB.
                let mut injected = false;
                if let Ok(Some(note)) = db_link.get_note_by_title(&src_title) {
                    if let Some(note_id) = note.id {
                        let target_class = format!("note-{}", note_id);
                        for win in app_link.windows() {
                            if win.css_classes().iter().any(|c| c == &target_class) {
                                if let Ok(app_win) = win.downcast::<gtk4::ApplicationWindow>() {
                                    if let Some(buf) = crate::note_window::editor_ref_buffer(&app_win) {
                                        let mut end_iter = buf.end_iter();
                                        let offset = end_iter.offset();
                                        buf.insert(&mut end_iter, &format!("\n{}", &tgt_title));
                                        let start = buf.iter_at_offset(offset + 1); // after \n
                                        let end = buf.end_iter();
                                        let tag_name = format!("link::tangle://{}", tgt_title);
                                        let tag = if let Some(t) = buf.tag_table().lookup(&tag_name) {
                                            t
                                        } else {
                                            let t = gtk4::TextTag::builder()
                                                .name(&tag_name)
                                                .foreground("#b388ff")
                                                .underline(gtk4::pango::Underline::Single)
                                                .style(gtk4::pango::Style::Italic)
                                                .build();
                                            buf.tag_table().add(&t);
                                            t
                                        };
                                        buf.apply_tag(&tag, &start, &end);
                                        injected = true;
                                    }
                                }
                                break;
                            }
                        }
                    }
                    // Fallback: no open editor window, append to content directly
                    if !injected {
                        if let Some(note_id) = note.id {
                            let link_html = format!(
                                "\n<p><a href=\"tangle://{}\" class=\"tangle\">{}</a></p>",
                                tgt_title, tgt_title
                            );
                            if let Err(e) = db_link.append_note_content(note_id, &link_html) {
                                eprintln!("Error saving tangle link: {}", e);
                            }
                        }
                    }
                }

                da_lde.queue_draw();
                return;
            }
        }
        // No target hit — just clear the preview
        da_lde.queue_draw();
    });
    drawing_area.add_controller(link_drag_ctrl);

    // Left-drag on node → move node (or selected group); on empty space → pan handled below
    let node_drag_ctrl = gtk4::GestureDrag::builder().button(1).build();
    let dragged_node: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    // Store initial positions of all nodes being moved: Vec<(index, start_x, start_y)>
    let drag_start_positions: Rc<RefCell<Vec<(usize, f64, f64)>>> = Rc::new(RefCell::new(Vec::new()));

    let dn_begin = dragged_node.clone();
    let dsp_begin = drag_start_positions.clone();
    let nodes_nd = nodes.clone();
    let zoom_nd = zoom.clone();
    let pan_nd_x = pan_x.clone();
    let pan_nd_y = pan_y.clone();
    let sel_nd = selected_nodes.clone();
    node_drag_ctrl.connect_drag_begin(move |_gesture, x, y| {
        let z = zoom_nd.get();
        if z == 0.0 { dn_begin.set(None); return; }
        let mx = (x - pan_nd_x.get()) / z;
        let my = (y - pan_nd_y.get()) / z;
        let nodes = nodes_nd.borrow();
        let sel = sel_nd.borrow();
        for (i, node) in nodes.iter().enumerate() {
            let nx = node.x - node.w / 2.0;
            let ny = node.y - node.h / 2.0;
            if mx >= nx && mx <= nx + node.w && my >= ny && my <= ny + node.h {
                dn_begin.set(Some(i));
                // If dragged node is in selection, move entire selection
                // Otherwise just move this single node
                let mut starts = Vec::new();
                if sel.contains(&i) && sel.len() > 1 {
                    for &si in sel.iter() {
                        if si < nodes.len() {
                            starts.push((si, nodes[si].x, nodes[si].y));
                        }
                    }
                } else {
                    starts.push((i, node.x, node.y));
                }
                *dsp_begin.borrow_mut() = starts;
                return;
            }
        }
        dn_begin.set(None);
        dsp_begin.borrow_mut().clear();
    });

    let dn_update = dragged_node.clone();
    let dsp_update = drag_start_positions.clone();
    let nodes_nu = nodes.clone();
    let zoom_nu = zoom.clone();
    let da_nu = drawing_area.clone();
    node_drag_ctrl.connect_drag_update(move |_, ox, oy| {
        if dn_update.get().is_none() { return; }
        let z = zoom_nu.get();
        if z == 0.0 { return; }
        let starts = dsp_update.borrow();
        let mut nodes = nodes_nu.borrow_mut();
        for &(idx, sx, sy) in starts.iter() {
            if idx < nodes.len() {
                nodes[idx].x = sx + ox / z;
                nodes[idx].y = sy + oy / z;
            }
        }
        da_nu.queue_draw();
    });

    let dn_end = dragged_node.clone();
    let dsp_end = drag_start_positions.clone();
    let nodes_save = nodes.clone();
    let db_save = db.clone();
    node_drag_ctrl.connect_drag_end(move |_, _, _| {
        // Save all moved node positions to DB (position-only update, won't clobber content)
        if dn_end.get().is_some() {
            let nodes = nodes_save.borrow();
            let starts = dsp_end.borrow();
            let db = db_save.clone();
            let to_save: Vec<(i64, f64, f64)> = starts.iter()
                .filter(|(idx, _, _)| *idx < nodes.len())
                .map(|(idx, _, _)| (nodes[*idx].note_id, nodes[*idx].x, nodes[*idx].y))
                .collect();
            drop(nodes);
            drop(starts);
            std::thread::spawn(move || {
                for (id, x, y) in to_save {
                    if let Err(e) = db.update_note_position(id, x, y) {
                        eprintln!("Error saving node position: {}", e);
                    }
                }
            });
        }
        dn_end.set(None);
        dsp_end.borrow_mut().clear();
    });
    drawing_area.add_controller(node_drag_ctrl);

    // Alt+Drag → lasso select nodes
    let lasso_ctrl = gtk4::GestureDrag::builder().button(1).build();
    let lasso_active = Rc::new(Cell::new(false));
    let lasso_start = Rc::new(Cell::new((0.0f64, 0.0f64)));

    let la_begin = lasso_active.clone();
    let ls_begin = lasso_start.clone();
    let lr_begin = lasso_rect.clone();
    let zoom_la = zoom.clone();
    let pan_la_x = pan_x.clone();
    let pan_la_y = pan_y.clone();
    let sel_la = selected_nodes.clone();
    lasso_ctrl.connect_drag_begin(move |gesture, x, y| {
        let state = gesture.current_event_state();
        let has_shift = state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
        let has_alt = state.contains(gtk4::gdk::ModifierType::ALT_MASK);
        if !(has_shift && has_alt) {
            la_begin.set(false);
            return;
        }
        la_begin.set(true);
        let z = zoom_la.get();
        if z == 0.0 { return; }
        let wx = (x - pan_la_x.get()) / z;
        let wy = (y - pan_la_y.get()) / z;
        ls_begin.set((wx, wy));
        lr_begin.set(Some((wx, wy, wx, wy)));
        // Clear previous selection (Shift+Alt always starts fresh)
        sel_la.borrow_mut().clear();
    });

    let la_update = lasso_active.clone();
    let ls_update = lasso_start.clone();
    let lr_update = lasso_rect.clone();
    let zoom_lu = zoom.clone();
    let pan_lu_x = pan_x.clone();
    let pan_lu_y = pan_y.clone();
    let da_lu = drawing_area.clone();
    lasso_ctrl.connect_drag_update(move |_, ox, oy| {
        if !la_update.get() { return; }
        let z = zoom_lu.get();
        if z == 0.0 { return; }
        let (sx, sy) = ls_update.get();
        let ex = sx + ox / z;
        let ey = sy + oy / z;
        lr_update.set(Some((sx, sy, ex, ey)));
        da_lu.queue_draw();
    });

    let la_end = lasso_active.clone();
    let lr_end = lasso_rect.clone();
    let nodes_le = nodes.clone();
    let sel_le = selected_nodes.clone();
    let da_le = drawing_area.clone();
    lasso_ctrl.connect_drag_end(move |_, _, _| {
        if !la_end.get() { return; }
        la_end.set(false);
        // Select nodes inside lasso rect
        if let Some((x1, y1, x2, y2)) = lr_end.get() {
            let lx = x1.min(x2);
            let ly = y1.min(y2);
            let rx = x1.max(x2);
            let ry = y1.max(y2);
            let nodes = nodes_le.borrow();
            let mut sel = sel_le.borrow_mut();
            for (i, node) in nodes.iter().enumerate() {
                // Select if node center is inside lasso
                if node.x >= lx && node.x <= rx && node.y >= ly && node.y <= ry {
                    sel.insert(i);
                }
            }
        }
        lr_end.set(None);
        da_le.queue_draw();
    });
    drawing_area.add_controller(lasso_ctrl);

    // Drag → pan (plain drag without Ctrl)
    let drag_ctrl = gtk4::GestureDrag::builder().button(1).build();
    let pan_sx = pan_x.clone();
    let pan_sy = pan_y.clone();
    let start_px = Rc::new(Cell::new(0.0f64));
    let start_py = Rc::new(Cell::new(0.0f64));
    let spx = start_px.clone();
    let spy = start_py.clone();
    let psx = pan_sx.clone();
    let psy = pan_sy.clone();
    drag_ctrl.connect_drag_begin(move |_, _, _| {
        spx.set(psx.get());
        spy.set(psy.get());
    });
    let da_d = drawing_area.clone();
    let dn_pan_u = dragged_node.clone();
    let ld_pan_u = link_drag_src.clone();
    let la_pan_u = lasso_active.clone();
    drag_ctrl.connect_drag_update(move |_, ox, oy| {
        // Skip panning if another drag mode is active
        if dn_pan_u.get().is_some() || ld_pan_u.get().is_some() || la_pan_u.get() { return; }
        pan_sx.set(start_px.get() + ox);
        pan_sy.set(start_py.get() + oy);
        da_d.queue_draw();
    });
    drawing_area.add_controller(drag_ctrl);

    // Plain click on empty space → clear selection
    {
        let desel_click = gtk4::GestureClick::builder().button(1).build();
        desel_click.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let nodes_ds = nodes.clone();
        let zoom_ds = zoom.clone();
        let pan_ds_x = pan_x.clone();
        let pan_ds_y = pan_y.clone();
        let sel_ds = selected_nodes.clone();
        let da_ds = drawing_area.clone();
        desel_click.connect_pressed(move |_, n_press, x, y| {
            if n_press != 1 { return; }
            let z = zoom_ds.get();
            if z == 0.0 { return; }
            let mx = (x - pan_ds_x.get()) / z;
            let my = (y - pan_ds_y.get()) / z;
            let nodes = nodes_ds.borrow();
            for node in nodes.iter() {
                let nx = node.x - node.w / 2.0;
                let ny = node.y - node.h / 2.0;
                if mx >= nx && mx <= nx + node.w && my >= ny && my <= ny + node.h {
                    return; // Clicked a node, don't deselect
                }
            }
            sel_ds.borrow_mut().clear();
            da_ds.queue_draw();
        });
        drawing_area.add_controller(desel_click);
    }

    // Ctrl+Click → toggle node selection
    if node_count > 0 {
        let sel_click = gtk4::GestureClick::builder().button(1).build();
        sel_click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let nodes_sc = nodes.clone();
        let zoom_sc = zoom.clone();
        let pan_sc_x = pan_x.clone();
        let pan_sc_y = pan_y.clone();
        let sel_sc = selected_nodes.clone();
        let da_sc = drawing_area.clone();
        sel_click.connect_pressed(move |gesture, n_press, x, y| {
            if n_press != 1 { return; }
            let state = gesture.current_event_state();
            if !state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) { return; }
            let z = zoom_sc.get();
            if z == 0.0 { return; }
            let mx = (x - pan_sc_x.get()) / z;
            let my = (y - pan_sc_y.get()) / z;
            let nodes = nodes_sc.borrow();
            for (i, node) in nodes.iter().enumerate() {
                let nx = node.x - node.w / 2.0;
                let ny = node.y - node.h / 2.0;
                if mx >= nx && mx <= nx + node.w && my >= ny && my <= ny + node.h {
                    let mut sel = sel_sc.borrow_mut();
                    if sel.contains(&i) {
                        sel.remove(&i);
                    } else {
                        sel.insert(i);
                    }
                    da_sc.queue_draw();
                    return;
                }
            }
        });
        drawing_area.add_controller(sel_click);
    }

    // Double-click → open tangle
    if node_count > 0 {
        let dbl_click = gtk4::GestureClick::builder().button(1).build();
        dbl_click.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let nodes_click = nodes.clone();
        let zoom_c = zoom.clone();
        let pan_cx = pan_x.clone();
        let pan_cy = pan_y.clone();
        let db_click = db.clone();
        let app_click = app.clone();
        dbl_click.connect_pressed(move |_, n_press, x, y| {
            if n_press != 2 { return; }
            let z = zoom_c.get();
            if z == 0.0 { return; }
            let mx = (x - pan_cx.get()) / z;
            let my = (y - pan_cy.get()) / z;
            let nodes = nodes_click.borrow();
            for node in nodes.iter() {
                let nx = node.x - node.w / 2.0;
                let ny = node.y - node.h / 2.0;
                if mx >= nx && mx <= nx + node.w && my >= ny && my <= ny + node.h {
                    let title = node.title.clone();
                    drop(nodes); // Release borrow before calling out
                    crate::rich_editor::open_tangle_note(&db_click, &app_click, &title);
                    return;
                }
            }
        });
        drawing_area.add_controller(dbl_click);
    }

    // Search entry
    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text("Search nodes...")
        .hexpand(true)
        .build();
    search_entry.add_css_class("tangle-map-search");

    let sq = search_query.clone();
    let da_search = drawing_area.clone();
    search_entry.connect_search_changed(move |entry| {
        *sq.borrow_mut() = entry.text().to_string();
        da_search.queue_draw();
    });

    let hint_bar = gtk4::Label::builder()
        .label("Drag node: Move   Drag empty: Pan   Scroll: Zoom   Ctrl+Click: Select   Shift+Alt+Drag: Lasso   Right-Drag: Link   Dbl-click: Open")
        .css_classes(["tangle-map-hints"])
        .xalign(0.5)
        .build();

    let bottom_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    bottom_bar.add_css_class("tangle-map-bottom");
    bottom_bar.append(&search_entry);
    bottom_bar.append(&hint_bar);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.append(&drawing_area);
    vbox.append(&bottom_bar);

    dialog.set_child(Some(&vbox));
    dialog.present();
}
