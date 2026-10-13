# Book App

Book App is a local-first desktop writing application built with Rust and GTK 3.
It provides a focused workspace for drafting chapters while keeping book projects,
characters, environments, and world-building notes organized as plain text.

For step-by-step usage and instructions for enabling a dark appearance, see
[HOWTO.md](HOWTO.md).

## Writing workspace

- Create, rename, and reorder numbered chapters using movement buttons or
  drag-and-drop. Right-click document lists for contextual actions.
- Organize characters, environments, and lore with category visibility controls,
  title filters, reusable templates, and category changes.
- Use **Writing → Reference pane** to consult lore beside a chapter.
- Use **Writing → Outline** for live, hierarchical Markdown heading navigation.
  Use `#` headings and `##` scene headings to structure a plain-text chapter.
- Use **Writing → Planning and links** for optional summaries, POV, timeline,
  draft status, tags, word targets, stable document links, and backlinks. Scene
  planning notes can accompany the heading-based outline in the summary.
- Configure font and size, line spacing, text width, theme, daily goals, and
  whether lore contributes to goals under **Writing → Appearance and goals**.
- Restore the previous project, document, tab, cursor, scroll position, window
  size, and sidebar widths at startup. Missing documents fall back to available
  documents; empty collections display creation actions instead of editable blanks.
- Track manuscript totals, document targets, net session growth, gross additions,
  and daily net growth. Goals default to net manuscript growth excluding lore.
  Daily progress rolls over on the local calendar date. Starting a new session
  resets session counts without resetting daily progress.

## Editing and recovery

- Save both open editors with **Save** or `Ctrl+S`. Switching documents/projects
  and closing also preserve changes. Save failures keep edits open.
- Undo/redo with `Ctrl+Z` and `Ctrl+Shift+Z`. Per-document history survives
  document switches and renames during the application session, not restarts.
- Recovery drafts are written in the background after idle. They do not replace
  source files or consume saved revisions. Reopening a document with a draft
  offers recovery; a conflict warning explains if the disk version changed.
- Explicit saves atomically replace source documents and retain up to ten earlier
  revisions. **Document → Compare / recover revision** shows readable timestamps,
  word counts, and a side-by-side preview. Restoration preserves the current version.
- Chapters and lore entries, including their revisions and metadata, go to
  recoverable document Trash after confirmation. Use **Document → Undo last
  deletion** or **Restore deleted document**. Name collisions are rejected.
- Project removal requires its exact name and moves it to project Trash.
  Restore it through the **Project** menu. Legacy Book cannot be removed.
- External edits are checked before saving. Conflicts are reported rather than
  silently overwriting another application's changes.

## Search, exchange, and backups

- **Writing → Find / replace** searches the active editor; **Find / replace in
  project** searches chapter and lore titles and text, including open unsaved text.
- Search supports literal text, case sensitivity, whole words, Unicode character
  positions, highlighting, exact-match navigation, and next/previous results.
- Replacements require a before/after preview and confirmation. Project-wide
  replacement saves open edits first, checks source files again before applying,
  retains recoverable originals as revisions, and attempts rollback on failure.
- **Project → Import chapter** imports a UTF-8 text/Markdown file as one chapter.
  **Export manuscript** combines chapters in book order, using chapter titles and
  separators, and excludes lore. Choose `.txt` or `.md` and a location outside the
  active project. DOCX, PDF, and EPUB are deferred extensions, not supported formats.
- **Project → Back up project** creates a portable directory copy; **Import project
  backup** copies it into the library as a new project without overwriting an
  existing project. Backups include document history and sidecars.
- **Project → Choose library folder** opens a different library without moving
  existing projects. The status area displays the current library's full path.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+S` | Save open documents |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+N` | Create a document in the active tab |
| `Ctrl+F` / `Ctrl+Shift+F` | Find/replace in document / project |
| `F3` / `Shift+F3` | Next / previous search match |
| `Ctrl+Shift+O` | Show outline |
| `Ctrl+Tab` | Switch between chapters and lore |
| `F11` | Toggle focus mode |

## Requirements

The application requires:

- Rust and Cargo
- GTK 3 development libraries

On Debian or Ubuntu, install GTK 3 development files with:

```bash
sudo apt install libgtk-3-dev
```

## Running

From the project directory:

```bash
cargo run
```

On first run, the application uses the absolute path to `book/` in the launch
directory. It remembers that path afterward, so subsequent launches do not depend
on the working directory. Choosing another library does not migrate or delete
existing data.

Preferences, workspace positions, and writing progress are stored in
`$XDG_CONFIG_HOME/bookapp/workspace.json`, or
`$HOME/.config/bookapp/workspace.json`. Invalid configuration is reported rather
than silently replaced. For isolated testing or a separate installation,
`BOOKAPP_CONFIG_DIR` selects the configuration directory and
`BOOKAPP_STORAGE_DIR` selects the library.

## Storage Layout

The original flat chapter files in `book/` are retained as the **Legacy Book** project. New projects are stored separately:

```text
book/
├── 01_Chapter 1.txt
├── 02_Another Chapter.txt
└── projects/
    └── My Novel/
        ├── 01_Chapter 1.txt
        └── lore/
            ├── characters/
            ├── environments/
            └── lore/
```

Chapter and lore entries are individual UTF-8 text files. Their parent directories
also hold `.metadata/` JSON sidecars with stable document IDs and optional planning
data, `.drafts/` recovery snapshots, and `.revisions/` saved versions. Renaming,
reordering, recategorizing, and trash/restoration preserve these related files.
Keep hidden sidecar directories when copying a project manually.

Project Trash and document Trash are separate. Removed content remains recoverable
until you deliberately remove its Trash files. An intentionally empty chapter
collection stays empty after reopening.

## Development

Format the source:

```bash
cargo fmt --check
```

Check the project:

```bash
cargo check
```

Run the test suite:

```bash
GDK_BACKEND=x11 xvfb-run -a cargo test -- --test-threads=1
```

GTK interaction tests require a display and run on a single test thread. The suite
also covers storage operations, revision preservation, recovery drafts, conflicts,
Unicode search, undo/redo, metadata, configuration, goals, and portable backups.
Setting `GDK_BACKEND=x11` ensures GTK uses the isolated Xvfb display even when the
desktop environment normally selects Wayland.

## Project Structure

```text
src/
├── main.rs      # Application entry point
├── ui.rs        # Workspace state, widgets, navigation, and editing actions
├── ui/          # Search, file/revision actions, settings/planning, and GTK tests
├── features.rs  # Undo, search, metadata, preferences, workspace, and progress
├── documents.rs # Drafts, safe writes, document Trash, and portable backups
├── chapters.rs  # Chapter storage and ordering
├── lore.rs      # Character, environment, and lore storage
└── projects.rs  # Project discovery, creation, removal, and restoration
```

## Notes

The application currently uses GTK 3 through the `gtk`, `gdk`, and `gio` crates. Content is saved locally and is not synchronized to a remote service.
