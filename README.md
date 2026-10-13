# Book App

Book App is a desktop writing application built with Rust and GTK 3. It provides a focused workspace for drafting chapters while keeping book projects, characters, environments, and world-building notes organized on disk.

## Features

- Create, switch between, and remove book projects.
- Switch between projects from the project selector.
- Create, edit, rename, delete, and reorder chapters.
- Save chapter content with the Save button or `Ctrl+S`.
- Search chapters and lore entries by title; lore entries are grouped by category.
- Use **Writing Tools → Find in Project** to search chapter and lore titles and contents,
  including unsaved text in the open editors. Select a result to open that document.
- Use **Writing Tools → Chapter Outline** to navigate headings in the selected
  chapter (`# Heading`, `## Scene`, and so on).
- See the current document's word count and words added during this writing
  session in the status bar. **Set Session Goal** starts a new session count and
  shows progress toward a positive whole-number goal. **Focus Mode** hides the
  sidebars until toggled off.
- Track the active project and document, with an unsaved-changes indicator.
- Preserve edits on project changes and window close, and show errors when file operations fail.
- Maintain lore and character-development entries.
- Categorize development notes as:
  - Character
  - Environment
  - Lore
- Edit, rename, and delete lore entries.
- IDE-inspired dark interface with styled navigation panels, tabs, toolbars, and editor surfaces.
- Plain-text storage so project files remain easy to inspect, copy, and back up.
- Reject duplicate chapter and lore file names rather than overwriting existing content.
- Removing a project requires typing its name and moves it to a recoverable Trash.
  Use **Writing Tools → Restore Project** to bring it back; the Legacy Book cannot be removed.
- Saved document versions retain up to 10 earlier revisions per document.
  Use **Writing Tools → Recover Revision** to restore one for the open chapter or
  lore entry. The current version is preserved as a revision during recovery.

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

The application creates the `book/` storage directory automatically when it starts.

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

Chapter and lore entries are stored as individual UTF-8 text files. File names are derived from their titles, with path separators replaced to keep them safe to store. Saves replace documents atomically and keep up to 10 previous versions; removed projects remain in the Trash until you remove them manually.

## Development

Format the source:

```bash
cargo fmt
```

Check the project:

```bash
cargo check
```

Run the test suite:

```bash
cargo test
```

## Project Structure

```text
src/
├── main.rs      # GTK application and user interface
├── chapters.rs  # Chapter storage and ordering
├── lore.rs      # Character, environment, and lore storage
└── projects.rs  # Project discovery and creation
```

## Notes

The application currently uses GTK 3 through the `gtk`, `gdk`, and `gio` crates. Content is saved locally and is not synchronized to a remote service.
