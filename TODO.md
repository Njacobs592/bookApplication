# Application Improvement TODO

The implementation below uses the confirmed design choices: idle recovery
snapshots, session-only per-document undo history, Markdown heading-based scenes
with JSON planning sidecars, previewed literal replacement, net manuscript goals,
and text/Markdown import/export. DOCX, PDF, and EPUB remain deferred extensions.
Existing plain-text projects remain supported.

## Highest priority: writing comfort and safety

- [x] **1. Editor undo/redo**
  - Support undo and redo in chapter and lore editors with `Ctrl+Z` and
    `Ctrl+Shift+Z`.
  - Decide whether edit history should survive document switches.
  - Test typing, deletion, paste, undo/redo, document switches, and saves.
- [x] **2. Idle autosave and crash recovery**
  - Add debounced autosave or recoverable draft snapshots.
  - Show saving, saved, and failure states without clearing unsaved edits on failure.
  - Keep frequent autosaves from exhausting the ten-revision history.
  - Test interrupted sessions, recovery, save failures, and revision retention.
- [x] **3. Recoverable chapter and lore deletion**
  - Extend Trash to individual documents and their revisions.
  - Add immediate Undo and consistent deletion confirmation.
  - Test restoration, title collisions, chapter ordering, and revision preservation.
- [x] **4. Explicit empty workspaces**
  - Disable editing when no document is selected.
  - Show helpful empty states with create-document actions.
  - Test empty lore collections, deletion of the last chapter, and project switches.
- [x] **5. Configurable editor appearance**
  - Add font, text size, line spacing, and light/dark/system theme preferences.
  - Add a centered, configurable maximum text width.
  - Test preference persistence, wrapping, readability, and different window sizes.
- [x] **6. Restore the previous workspace**
  - Remember project, document, tab, cursor position, and scroll position.
  - Remember window size and sidebar widths.
  - Test restart restoration and safe handling of missing or renamed documents.

## UI/UX improvements

- [x] **7. Lore reference beside the draft**
  - Add an optional reference pane for characters, environments, and lore.
  - Keep the current chapter open while consulting reference material.
  - Test selection, project changes, focus behavior, and narrow windows.
- [x] **8. Persistent chapter outline**
  - Replace or supplement the modal outline with a live, collapsible panel.
  - Preserve heading hierarchy, highlight the current section, and support jumping.
  - Test heading edits, nested headings, empty outlines, and cursor navigation.
- [x] **9. Simpler navigation and controls**
  - Add chapter numbers, drag-and-drop ordering, and contextual menus.
  - Add collapsible lore categories and clarify the creation category selector.
  - Disable unavailable document actions and movement controls at list boundaries.
  - Test filtered lists, selection preservation, reordering, and category grouping.
- [x] **10. Separate writing and maintenance actions**
  - Move project removal/restoration into project actions.
  - Place revision recovery with document actions.
  - Keep everyday drafting tools prominent.
  - Test action availability and correct project/document targeting.
- [x] **11. Better dialogs and feedback**
  - Use action-specific labels such as Create, Rename, and Restore.
  - Validate names and explain collisions inline.
  - Use nonmodal feedback for routine information while keeping failures visible.
  - Test invalid names, duplicate names, cancellation, and operation failures.
- [x] **12. Keyboard and accessibility support**
  - Add and display shortcuts for find, outline, focus mode, creation, and view changes.
  - Add tooltips, accessible button names, and clear search placeholders.
  - Support visible keyboard focus and larger system text.
  - Test keyboard-only workflows, shortcut conflicts, and accessibility labels.

## Functional improvements

- [x] **13. Complete search and replacement workflows**
  - Show multiple matches per document and highlight matching text.
  - Jump to the exact match and support next/previous navigation.
  - Add in-document find/replace.
  - Confirm project-wide replacement behavior, including preview and recovery.
  - Test unsaved text, title-only matches, Unicode, replacement, and match positions.
- [x] **14. Manuscript import and export**
  - Start with UTF-8 text/Markdown import and ordered, combined manuscript export.
  - Exclude lore from manuscript export by default.
  - Define chapter separators, title handling, and overwrite confirmation.
  - Consider DOCX, PDF, EPUB, chapter breaks, and front matter as later extensions.
  - Test chapter order, Unicode, empty chapters, round trips, and filesystem failures.
- [x] **15. Scenes and planning metadata**
  - Confirm a storage design for optional scenes, summaries, POV, timeline, and status.
  - Add a simple outline or index-card planning view.
  - Keep planning optional and the drafting editor straightforward.
  - Test persistence, scene ordering, chapter moves, and existing project compatibility.
- [x] **16. Lore templates and links**
  - Add reusable character and environment templates.
  - Support recategorization, tags, chapter links, and backlinks.
  - Confirm link identity and behavior when linked documents are renamed or deleted.
  - Test templates, category moves, link persistence, and missing references.
- [x] **17. Meaningful progress tracking**
  - Distinguish gross words added from net manuscript growth.
  - Allow writers to choose whether lore contributes to goals.
  - Add whole-book totals, chapter targets, and persistent daily goals.
  - Define session boundaries and daily rollover behavior.
  - Test deletion/retyping, undo/redo, pasted text, project switches, and restarts.
- [x] **18. Understandable revision recovery**
  - Show readable timestamps, word counts, and revision previews.
  - Compare revisions with the current document before restoring.
  - Preserve the current version during recovery.
  - Test revision ordering, comparisons, restore failures, and subsequent recovery.
- [x] **19. User-controlled storage and backup**
  - Use a stable default location rather than relying on the working directory.
  - Allow users to choose or open project folders and show their actual paths.
  - Provide portable project backups.
  - Detect external changes before overwriting a document.
  - Confirm migration and conflict-resolution behavior.
  - Test existing projects, folder permissions, backup restoration, and external edits.

## Supporting engineering improvements

- [x] **20. Responsiveness, maintainability, and interaction tests**
  - Debounce expensive work, including full-document word counting.
  - Move longer storage and search operations off the GTK UI thread.
  - Separate UI state and actions from the large `build_ui` function.
  - Add interaction tests for empty editors, failed saves, deletion recovery,
    and restoring the previous workspace.
  - Test large documents/projects, rapid edits, stale operation results,
    and preservation of unsaved edits during failures.

## Validation for implemented changes

- [x] Run `cargo fmt --check`.
- [x] Run `cargo check`.
- [x] Run `GDK_BACKEND=x11 xvfb-run -a cargo test -- --test-threads=1`
  (41 tests passed).
- [ ] Manually exercise the affected GTK workflows and their failure cases.
- [x] Confirm existing project files and unrelated working-tree changes remain intact.

Automated GTK interactions cover rapid large-document edits, undo across document
switches, recovery drafts, saving, rename/reorder identity and cursor retention,
empty editors, templates, metadata, category changes, deletion/restoration,
workspace reopening, rendered font size, exact-match highlighting, previewed
project replacement with revision recovery, and failed-save preservation. The built
executable also passed isolated startup and visual smoke checks. Full hands-on
review of every menu, accessibility behavior, and desktop theme remains a review
step; no compiler-warning cleanup was hidden or suppressed in the source.

## Original first milestone (completed)

Items **1-6** and the basic text/Markdown export portion of **14** are implemented,
along with the remaining agreed improvements. The unchecked manual validation
step above is intentionally left for hands-on review rather than reported as
automatically completed.
