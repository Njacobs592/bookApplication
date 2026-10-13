# How to Use Book App

Book App is a desktop workspace for writing chapters and keeping characters,
environments, and world-building notes alongside your book. Everything is stored
locally; there is no cloud synchronization.

## Start the application

Install Rust, Cargo, and the GTK 3 development libraries if they are not already
available. On Debian or Ubuntu, install GTK with:

```bash
sudo apt install libgtk-3-dev
```

Open a terminal in the folder containing `Cargo.toml` and run:

```bash
cargo run
```

On the first launch, Book App uses the `book/` folder in that directory. It
remembers the library location and your workspace for future launches.

## Switch to a dark appearance

1. Open the **Writing** menu at the top of the window.
2. Select **Appearance and goals**.
3. In the **Appearance and writing goals** dialog, find **Theme**.
4. Select **dark**.
5. Click **Apply**.

The appearance changes immediately, and the preference is saved for future
launches. You do not need to edit source code or configuration files.

The theme choices are:

| Theme | Appearance |
| --- | --- |
| `dark` | Dark backgrounds with light text, regardless of the GTK preference. |
| `light` | Light backgrounds with dark text. |
| `system` | Uses GTK's preference for a dark appearance when settings are applied or the app starts. |

If **system** gives you a light appearance when you want a dark one, select
**dark** explicitly.

You can also adjust **Editor font and size**, **Extra line spacing**, and
**Maximum text width (pixels)** in the same dialog. A narrower text width makes
long passages easier to read on a wide screen. Click **Apply** to save changes,
or **Cancel** to leave the previous settings unchanged.

## Create or open a book project

1. Choose **Project > New project**.
2. Enter a unique project name and click **Create**.
3. Use the project selector at the top-left to switch between books.

**Legacy Book** contains chapter files from the original `book/` layout. It is
available alongside newer projects and cannot be removed through the application.

To use a different library location, choose **Project > Choose library folder**.
This opens and remembers the selected library; it does not move or delete the
books in your previous library. The library's full path appears at the bottom of
the window.

## Write and organize chapters

Select the **Chapters** tab, then select a chapter in the left sidebar to edit it.

- Click **New**, enter a title, and click **Create** to add a chapter.
- Select a chapter and click **Rename** to change its title.
- Use **Move up**, **Move down**, or drag-and-drop to change chapter order.
- Use the sidebar search field to filter chapter titles.
- Right-click the chapter list for document actions.

When no chapter is available, use the empty workspace's **Create document**
button. The blank workspace is not an editable document until you create one.

### Save and undo

Click **Save** or press `Ctrl+S` to save both open chapter and lore editors.
Selecting another document, switching projects, and closing the application also
preserve edits. Check the status bar for unsaved changes or operation errors.

Use `Ctrl+Z` to undo and `Ctrl+Shift+Z` to redo. Undo history stays with each
document while you switch between documents during the current application
session. It does not survive restarting the application.

### Add headings and scenes

The editor stores plain text. Markdown-style headings provide structure for
navigation; they are not a rendered rich-text preview.

For example:

```markdown
# Chapter opening

Write your opening here.

## Scene: The arrival

Write the first scene here.

## Scene: The discovery

Write the next scene here.
```

Choose **Writing > Outline**, or press `Ctrl+Shift+O`, to show the live outline.
Activate a heading to jump to that section. Edit or reorder scene text directly
in the chapter; scenes are heading-based sections, not separate scene files.

### Use focus mode

Choose **Writing > Focus mode**, or press `F11`, to hide navigation and reference
panels. Press `F11` again to restore them.

## Keep characters, environments, and lore

1. Select the **Lore & characters** tab.
2. Choose a category under **New entry category**.
3. Click **New**, enter a title, and click **Create**.
4. Select the entry and write its notes.

Use the category checkboxes to show or hide categories, and the search field to
filter titles and category names.

For an empty entry, choose **Document > Insert lore template** to insert a
category-appropriate starting structure. Templates do not overwrite existing
notes. To move an entry to another category, select it and choose
**Document > Change lore category**.

To consult notes while drafting, choose **Writing > Reference pane**. Select a
lore entry in the reference selector. This pane is read-only; edit the entry in
the **Lore & characters** tab.

## Add planning notes and document links

Select a document and choose **Writing > Planning and links**. Chapter planning
appears in the side panel; lore planning opens in a dialog.

You can record a POV character, timeline, draft status, comma-separated tags,
word target, and summary or scene-planning notes. A word target of `0` means no
document target.

To link another document:

1. Choose it under **Link a document**.
2. Click **Add selected link**.
3. Click **Save planning**.

Use **Show links and backlinks** to navigate links and see documents that refer
back to the selected document. Links use stable IDs so they can survive renames
and reordering. Missing or deleted linked documents are identified as such.

Use **Save planning** to explicitly persist planning edits. For lore, the dialog's
**Save planning** button saves and closes the dialog.

## Find and replace text

Choose **Writing > Find / replace** for the active document, or
**Writing > Find / replace in project** for all chapters and lore.

1. Enter the literal text to find.
2. Optionally enable **Case sensitive** or **Whole words only**.
3. Click **Find**.
4. For a project search, select a result and click **Jump to match**.

Matches are highlighted in the editor. Use `F3` for the next match and
`Shift+F3` for the previous match.

To replace text, enter the replacement and click **Preview replacement**.
Review the before-and-after text, then click **Apply replacement** or **Cancel**.
An empty replacement removes the matching text. Searches are literal, not regular
expressions.

Project-wide replacement saves open edits before preparing the preview. Applied
changes retain recoverable originals as saved revisions.

## Set writing goals

The status bar shows the current document's word count, manuscript total,
session net growth, gross additions, and daily net growth.

- **Net growth** accounts for additions and deletions.
- **Gross additions** also count pasted, retyped, and redone words.
- Goals use net growth by default, with lore excluded.

Choose **Writing > New writing session**, enter a positive whole-number word
goal, and click **Start session**. This resets session totals, not daily progress.
A new application session also starts a fresh session count.

Set the daily goal and choose whether to include lore under
**Writing > Appearance and goals**. A daily goal of `0` disables the daily target.
Daily progress uses the local calendar date and rolls over at midnight.

## Recover drafts, revisions, and deleted content

### Unsaved recovery drafts

After an idle period, the application writes recovery snapshots in the
background. These snapshots do not replace source documents or consume saved
revision history, so continue using **Save** for normal saves.

If a reopened document has a recovery draft, the application offers to recover
it into the editor. Review recovered text and save it when you are satisfied.
Declining recovery discards that recovery draft. If the disk version changed
outside the app, the recovery prompt warns you about the conflict.

### Earlier saved versions

1. Select the chapter or lore entry.
2. Choose **Document > Compare / recover revision**.
3. Select a version and click **Compare**.
4. Review the current and earlier versions side by side.
5. Click **Restore revision** to recover it, or **Cancel** to keep the current text.

The application keeps up to ten earlier saved versions per document. Restoration
preserves the current version as a revision.

### Deleted documents and projects

Select a chapter or lore entry and click **Trash** to move it, its metadata, and
its revisions to recoverable document Trash after confirmation.

- Choose **Document > Undo last deletion** to restore the last deletion from the
  current session.
- Choose **Document > Restore deleted document** to select an item from document
  Trash.
- Choose **Project > Move project to Trash** to remove a book. Enter its exact
  name to confirm.
- Choose **Project > Restore project** to recover a removed book.

Restoration rejects name collisions instead of overwriting an existing document
or project.

If saving fails, your edits remain in the editor. If another program has changed
the source file, Book App reports the conflict rather than silently overwriting
it. Copy important unsaved text into a separate file before resolving the conflict.

## Import, export, and back up your work

### Import a chapter

Choose **Project > Import chapter**, select a UTF-8 text or Markdown file, and
confirm its chapter title. The file is imported as one chapter; headings do not
automatically split it into multiple chapters.

### Export a manuscript

Choose **Project > Export manuscript** and select a destination filename ending
in `.txt` or `.md`. Use a location outside the active project folder.

Export saves open edits first and combines chapters in book order, with their
titles. Markdown export includes heading markers and chapter separators. Lore is
not included. DOCX, PDF, and EPUB export are not currently supported.

### Create and restore a portable backup

1. Choose **Project > Back up project**.
2. Select a parent folder and enter a new backup folder name.
3. Keep that backup separately from your active project.

The backup is a directory copy, not a ZIP archive. It includes document history
and sidecar files.

To restore a copy as a new project, choose **Project > Import project backup**,
select the backup directory, and enter a unique project name.

If copying a project manually, include hidden directories such as `.metadata/`,
`.drafts/`, and `.revisions/`. These hold planning data, recovery drafts, and saved
versions. Backups are still important: local recovery history is not an
independent backup or cloud synchronization.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+S` | Save both open editors |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+N` | Create a document in the active tab |
| `Ctrl+F` | Find/replace in the active document |
| `Ctrl+Shift+F` | Find/replace in the project |
| `F3` / `Shift+F3` | Next / previous search match |
| `Ctrl+Shift+O` | Show the chapter outline |
| `Ctrl+Tab` | Switch between chapters and lore |
| `F11` | Toggle focus mode |

## Where preferences are saved

Normally, settings and workspace information are saved in:

```text
$XDG_CONFIG_HOME/bookapp/workspace.json
```

If `XDG_CONFIG_HOME` is not set, the location is:

```text
$HOME/.config/bookapp/workspace.json
```

If the application was launched with `BOOKAPP_CONFIG_DIR`, that directory is
used instead. Theme preferences are application-wide, not per book project.
Use the appearance dialog rather than manually editing this file.

See [README.md](README.md) for storage layout and development/test commands.
