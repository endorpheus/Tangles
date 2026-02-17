# Tangles

A rich-text note-taking app with inter-note linking ("tangles"), per-note theming, and a force-directed tangle map visualization. Built with Rust and GTK4.

## Features

- **Rich text editing** — Bold, italic, underline, strikethrough, headings (H1-H4), bullet/numbered lists, code blocks
- **Tangle links** — Link notes to each other with `tangle://` references; auto-link detection scans for note title matches
- **Per-note theming** — Background, text, and accent color customization per tangle via HSV color picker
- **Global dark theme** — Explicit dark defaults (bg: #1a1a2e, fg: #e0e0e0, accent: #b388ff) with global override settings
- **Color star labels** — Tag tangles with colored stars (red, yellow, green, blue, purple) for quick visual categorization
- **Chromeless mode** — Per-tangle borderless window toggle with edge-resize and visible resize grip
- **Code blocks** — Monospace-styled code regions serialized as `<pre><code>` in HTML
- **Tangle map** — Force-directed graph visualization of all tangles and their links; zoom, pan, double-click to open
- **Image embedding** — Insert images from file picker, system icons, or drag-and-drop; EXIF-aware rotation
- **Web links** — Insert hyperlinks; click to open in system browser; hover tooltips
- **Origin tangles** — Backlinks pane shows which tangles reference the current one
- **HTML source view** — Toggle beautified HTML source editing
- **Brain icon launcher** — Floating, draggable, scroll-to-resize brain icon with right-click context menu
- **Stay on Top** — Pin the brain icon above all windows via wmctrl
- **Always on Top** — Pin individual tangles above other windows
- **Autosave** — Debounced 5-second autosave with background-thread DB writes
- **SQLite backend** — WAL mode, prepared statements, word indexing for search
- **Toolbar hamburger** — Collapse/expand the formatting toolbar

## Build

### Dependencies

- Rust (stable)
- GTK4 development libraries
- `wmctrl` (for window positioning and stay-on-top)
- `xprop` (optional, for shadowless brain icon on X11)

#### Arch Linux

```sh
sudo pacman -S gtk4 wmctrl xorg-xprop
```

#### Ubuntu/Debian

```sh
sudo apt install libgtk-4-dev wmctrl x11-utils
```

### Compile

```sh
cargo build --release
```

The binary is at `target/release/tangles`. Copy `assets/` alongside it or run from the project root.

### Run

```sh
cargo run --release
```

## Architecture

| File | Purpose |
|------|---------|
| `src/main.rs` | App entry, brain icon, context menu, note list dialogs |
| `src/note_window.rs` | Per-tangle window: title bar, theme picker, chromeless, star labels, backlinks |
| `src/rich_editor.rs` | Rich text editor: toolbar, formatting, serialization, tangle/web links, drag-drop |
| `src/database.rs` | SQLite wrapper: notes CRUD, word indexing, settings, migrations |
| `src/pickers.rs` | Emoji picker, icon picker, image file browser, resizable picture widget |
| `src/theme.rs` | Global theme dialog with HSV color picker, CSS generation |
| `src/tangle_map.rs` | Force-directed graph visualization of tangle relationships |
| `assets/style.css` | Base dark theme CSS |
| `assets/brain.svg` | Brain icon SVG |
