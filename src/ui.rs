use crate::chapters::ChapterStore;
use crate::documents;
use crate::features::{self, Metadata, SearchOptions, TextMatch, UndoHistory, WorkspaceState};
use crate::lore::{Category, LoreStore};
use crate::projects::{self, Project};
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, ComboBoxText, Entry, Label,
    ListBox, ListBoxRow, Notebook, Orientation, Paned, ScrolledWindow, SearchEntry, TextView,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

mod files;
mod search;
#[cfg(test)]
mod tests;
mod tools;
#[cfg(test)]
thread_local! {
    static TEST_WORKSPACE: RefCell<Option<Rc<Workspace>>> = const { RefCell::new(None) };
}

fn text(view: &TextView) -> String {
    let buffer = view.buffer().expect("TextView always has a buffer");
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).unwrap().to_string()
}

fn scroll(view: &TextView) -> ScrolledWindow {
    let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.add(view);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll
}

fn error(window: &ApplicationWindow, context: &str, cause: &dyn std::fmt::Display) {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::Close,
        &format!("{context}: {cause}"),
    );
    dialog.run();
    dialog.close();
}

fn confirm(window: &ApplicationWindow, title: &str, message: &str, action: &str) -> bool {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Question,
        gtk::ButtonsType::None,
        message,
    );
    dialog.set_title(title);
    dialog.add_buttons(&[
        ("Cancel", gtk::ResponseType::Cancel),
        (action, gtk::ResponseType::Ok),
    ]);
    let accepted = dialog.run() == gtk::ResponseType::Ok;
    dialog.close();
    accepted
}

fn prompt(
    window: &ApplicationWindow,
    title: &str,
    initial: &str,
    action: &str,
    validate: impl Fn(&str) -> Result<(), String> + 'static,
) -> Option<String> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            (action, gtk::ResponseType::Ok),
        ],
    );
    let entry = Entry::new();
    entry.set_text(initial);
    entry.set_placeholder_text(Some(title));
    entry.set_activates_default(true);
    let message = Label::new(None);
    message.set_line_wrap(true);
    let area = dialog.content_area();
    area.set_margin(16);
    area.set_spacing(8);
    area.add(&entry);
    area.add(&message);
    dialog.set_default_response(gtk::ResponseType::Ok);
    let check = move |entry: &Entry, dialog: &gtk::Dialog, message: &Label| {
        let result = validate(entry.text().trim());
        dialog.set_response_sensitive(gtk::ResponseType::Ok, result.is_ok());
        message.set_text(result.err().as_deref().unwrap_or(""));
    };
    check(&entry, &dialog, &message);
    entry.connect_changed(glib::clone!(@weak dialog, @weak message => move |entry| {
        check(entry, &dialog, &message);
    }));
    dialog.show_all();
    entry.grab_focus();
    let result = (dialog.run() == gtk::ResponseType::Ok).then(|| entry.text().trim().to_string());
    dialog.close();
    result
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| c.is_control()) {
        Err("Enter a nonempty name without control characters.".into())
    } else if name.contains('/') || name.contains('\\') {
        Err("Names cannot contain path separators.".into())
    } else {
        Ok(())
    }
}

fn choose(
    window: &ApplicationWindow,
    title: &str,
    action: &str,
    labels: &[String],
) -> Option<usize> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            (action, gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(650, 440);
    let list = ListBox::new();
    for label in labels {
        let row = ListBoxRow::new();
        let label = Label::new(Some(label));
        label.set_xalign(0.0);
        label.set_line_wrap(true);
        label.set_margin(12);
        row.add(&label);
        list.add(&row);
    }
    let area = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    area.add(&list);
    area.set_vexpand(true);
    dialog.content_area().add(&area);
    dialog.set_response_sensitive(gtk::ResponseType::Ok, false);
    list.connect_row_selected(glib::clone!(@weak dialog => move |_, row| {
        dialog.set_response_sensitive(gtk::ResponseType::Ok, row.is_some());
    }));
    list.connect_row_activated(glib::clone!(@weak dialog => move |_, _| {
        dialog.response(gtk::ResponseType::Ok);
    }));
    dialog.show_all();
    let result = if dialog.run() == gtk::ResponseType::Ok {
        list.selected_row().map(|row| row.index() as usize)
    } else {
        None
    };
    dialog.close();
    result
}

fn file_dialog(
    window: &ApplicationWindow,
    title: &str,
    action: gtk::FileChooserAction,
) -> Option<PathBuf> {
    let dialog = gtk::FileChooserDialog::new(Some(title), Some(window), action);
    dialog.add_buttons(&[
        ("Cancel", gtk::ResponseType::Cancel),
        ("Select", gtk::ResponseType::Ok),
    ]);
    dialog.set_do_overwrite_confirmation(true);
    let path = if dialog.run() == gtk::ResponseType::Ok {
        dialog.filename()
    } else {
        None
    };
    dialog.close();
    path
}

// Workers own only plain data; GTK objects and completion callbacks stay on the UI thread.
fn job<T: Send + 'static>(
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
    complete: impl FnOnce(io::Result<T>) + 'static,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    let mut complete = Some(complete);
    glib::timeout_add_local(Duration::from_millis(30), move || {
        match receiver.try_recv() {
            Ok(result) => {
                complete.take().unwrap()(result);
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                complete.take().unwrap()(Err(io::Error::other(
                    "background operation stopped unexpectedly",
                )));
                glib::ControlFlow::Break
            }
        }
    });
}

struct Editor {
    view: TextView,
    stack: gtk::Stack,
    scroll: ScrolledWindow,
    path: RefCell<Option<PathBuf>>,
    id: RefCell<String>,
    saved: RefCell<String>,
    observed: RefCell<String>,
    histories: RefCell<HashMap<String, UndoHistory>>,
    loading: Cell<bool>,
    generation: Cell<u64>,
    observed_generation: Cell<u64>,
    draft_written: Cell<u64>,
    dirty: Cell<bool>,
    last_edit: Cell<std::time::Instant>,
}

impl Editor {
    fn new(empty_message: &str, create: &Button) -> Self {
        let view = TextView::new();
        view.set_wrap_mode(gtk::WrapMode::Word);
        view.set_top_margin(24);
        view.set_bottom_margin(24);
        view.style_context().add_class("editor");
        let scroll = scroll(&view);
        let empty = GtkBox::new(Orientation::Vertical, 12);
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        empty.add(&Label::new(Some(empty_message)));
        let shortcut = Button::with_label("Create document");
        shortcut.connect_clicked(glib::clone!(@weak create => move |_| create.emit_clicked()));
        empty.add(&shortcut);
        let stack = gtk::Stack::new();
        stack.add_named(&scroll, "editor");
        stack.add_named(&empty, "empty");
        stack.set_visible_child_name("empty");
        view.set_editable(false);
        Self {
            view,
            stack,
            scroll,
            path: RefCell::new(None),
            id: RefCell::new(String::new()),
            saved: RefCell::new(String::new()),
            observed: RefCell::new(String::new()),
            histories: RefCell::new(HashMap::new()),
            loading: Cell::new(false),
            generation: Cell::new(0),
            observed_generation: Cell::new(0),
            draft_written: Cell::new(0),
            dirty: Cell::new(false),
            last_edit: Cell::new(std::time::Instant::now()),
        }
    }

    fn clear(&self) {
        self.loading.set(true);
        self.view.buffer().unwrap().set_text("");
        self.path.replace(None);
        self.id.replace(String::new());
        self.saved.replace(String::new());
        self.observed.replace(String::new());
        self.dirty.set(false);
        self.view.set_editable(false);
        self.stack.set_visible_child_name("empty");
        self.loading.set(false);
    }

    fn display(&self, path: PathBuf, metadata: &Metadata, saved: String, content: String) {
        self.loading.set(true);
        self.view.buffer().unwrap().set_text(&content);
        self.path.replace(Some(path));
        self.id.replace(metadata.id.clone());
        self.saved.replace(saved.clone());
        self.observed.replace(content.clone());
        self.histories
            .borrow_mut()
            .entry(metadata.id.clone())
            .or_insert_with(|| UndoHistory::new(&content))
            .record(&content);
        self.dirty.set(content != saved);
        self.generation.set(self.generation.get() + 1);
        self.observed_generation.set(self.generation.get());
        self.draft_written.set(0);
        self.view.set_editable(true);
        self.stack.set_visible_child_name("editor");
        self.loading.set(false);
    }
}

#[derive(Clone)]
struct SearchHit {
    path: PathBuf,
    chapter: bool,
    title: String,
    found: TextMatch,
}

struct Workspace {
    window: ApplicationWindow,
    root: GtkBox,
    notebook: Notebook,
    selector: ComboBoxText,
    chapter_list: ListBox,
    lore_list: ListBox,
    chapter_search: SearchEntry,
    lore_search: SearchEntry,
    category: ComboBoxText,
    categories: Vec<CheckButton>,
    chapter_sidebar: GtkBox,
    lore_sidebar: GtkBox,
    chapter_pane: Paned,
    lore_pane: Paned,
    reference_pane: Paned,
    tools: Notebook,
    reference_selector: ComboBoxText,
    reference: TextView,
    outline: ListBox,
    plan: BTreeMap<String, Entry>,
    summary: TextView,
    link_selector: ComboBoxText,
    links: RefCell<Vec<(String, String)>>,
    plan_document: RefCell<Option<PathBuf>>,
    chapter: Editor,
    lore: Editor,
    status: Label,
    words: Label,
    buttons: HashMap<String, Button>,
    state: RefCell<WorkspaceState>,
    config: PathBuf,
    projects: RefCell<Vec<Project>>,
    chapters: RefCell<ChapterStore>,
    lores: RefCell<LoreStore>,
    switching: Cell<bool>,
    busy: Cell<bool>,
    closing: Cell<bool>,
    snapshot_busy: Cell<bool>,
    focus: Cell<bool>,
    session_goal: Cell<usize>,
    search_hits: RefCell<Vec<SearchHit>>,
    search_index: Cell<usize>,
    last_trash: RefCell<Option<(bool, PathBuf)>>,
    storage_label: Label,
    metadata: RefCell<HashMap<PathBuf, Metadata>>,
    book_counts: RefCell<HashMap<String, usize>>,
    search_spec: RefCell<Option<(String, SearchOptions)>>,
    outline_headings: RefCell<Vec<features::Heading>>,
    catalog_generation: Cell<u64>,
}

pub fn build_ui(app: &Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let config_dir = std::env::var_os("BOOKAPP_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("bookapp")))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/bookapp")));
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Book App")
        .build();
    let result = (|| -> io::Result<Rc<Workspace>> {
        let config = config_dir
            .ok_or_else(|| io::Error::other("No configuration directory is available"))?;
        let initial = std::env::var_os("BOOKAPP_STORAGE_DIR")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?.join("book"));
        let mut state = features::load_workspace(&config, &initial)?;
        state.progress.start_session();
        if let Some(path) = std::env::var_os("BOOKAPP_STORAGE_DIR") {
            state.storage = PathBuf::from(path);
        }

        fs::create_dir_all(&state.storage)?;
        state.storage = fs::canonicalize(&state.storage)?;
        let projects = projects::load_projects(&state.storage.to_string_lossy())?;
        let project_index = projects::project_index(&projects, &state.project).unwrap_or(0);
        state.project = projects[project_index].path.clone();
        let chapters = ChapterStore::load_or_init(&state.project.to_string_lossy())?;
        let lores = LoreStore::load(&state.project)?;
        window.set_default_size(state.width.max(760), state.height.max(500));
        let root = GtkBox::new(Orientation::Vertical, 0);
        let header = gtk::HeaderBar::builder()
            .show_close_button(true)
            .title("Book App")
            .build();
        let selector = ComboBoxText::new();
        selector.set_tooltip_text(Some("Active book project"));
        for project in &projects {
            selector.append_text(&project.name);
        }
        selector.set_active(Some(project_index as u32));
        header.pack_start(&selector);
        let mut buttons = HashMap::new();
        let mut make = |key: &str, label: &str, tooltip: &str| {
            let button = Button::with_label(label);
            button.set_tooltip_text(Some(tooltip));
            button.style_context().add_class(key);
            buttons.insert(key.to_string(), button.clone());
            button
        };
        let save = make("save", "Save", "Save both open documents · Ctrl+S");
        header.pack_end(&save);
        for (title, items) in [
            (
                "Project",
                vec![
                    ("new-project", "New project", "Create a book project"),
                    (
                        "remove-project",
                        "Move project to Trash",
                        "Remove the active project safely",
                    ),
                    (
                        "restore-project",
                        "Restore project",
                        "Restore a project from Trash",
                    ),
                    (
                        "storage",
                        "Choose library folder",
                        "Open a different book library",
                    ),
                    (
                        "backup",
                        "Back up project",
                        "Copy a portable project backup",
                    ),
                    (
                        "import-backup",
                        "Import project backup",
                        "Import a copied project folder",
                    ),
                    (
                        "import",
                        "Import chapter",
                        "Import a UTF-8 text or Markdown file",
                    ),
                    (
                        "export",
                        "Export manuscript",
                        "Export ordered chapters as text or Markdown",
                    ),
                ],
            ),
            (
                "Document",
                vec![
                    ("undo", "Undo · Ctrl+Z", "Undo editor changes"),
                    ("redo", "Redo · Ctrl+Shift+Z", "Redo editor changes"),
                    (
                        "revisions",
                        "Compare / recover revision",
                        "Preview and restore a saved version",
                    ),
                    (
                        "restore-document",
                        "Restore deleted document",
                        "Recover a chapter or lore entry",
                    ),
                    (
                        "undo-delete",
                        "Undo last deletion",
                        "Restore the last deleted document",
                    ),
                    (
                        "recategorize",
                        "Change lore category",
                        "Move the selected lore entry to another category",
                    ),
                    (
                        "template",
                        "Insert lore template",
                        "Insert a reusable template into an empty entry",
                    ),
                ],
            ),
            (
                "Writing",
                vec![
                    (
                        "find",
                        "Find / replace · Ctrl+F",
                        "Search or replace in the active document",
                    ),
                    (
                        "find-project",
                        "Find / replace in project · Ctrl+Shift+F",
                        "Search all chapters and lore",
                    ),
                    (
                        "next-match",
                        "Next match · F3",
                        "Jump to the next search result",
                    ),
                    (
                        "previous-match",
                        "Previous match · Shift+F3",
                        "Jump to the previous search result",
                    ),
                    (
                        "outline",
                        "Outline · Ctrl+Shift+O",
                        "Show live chapter headings",
                    ),
                    (
                        "reference",
                        "Reference pane",
                        "Consult lore beside your chapter",
                    ),
                    (
                        "planning",
                        "Planning and links",
                        "Edit summaries, scene details, tags and links",
                    ),
                    (
                        "goal",
                        "New writing session",
                        "Reset session totals and set a goal",
                    ),
                    (
                        "focus",
                        "Focus mode · F11",
                        "Hide navigation and reference panels",
                    ),
                    (
                        "settings",
                        "Appearance and goals",
                        "Configure font, theme, line spacing and goals",
                    ),
                ],
            ),
        ] {
            let menu = gtk::MenuButton::new();
            menu.set_label(title);
            let popover = gtk::Popover::new(Some(&menu));
            let box_ = GtkBox::new(Orientation::Vertical, 4);
            box_.set_margin(8);
            for (key, label, tooltip) in items {
                let button = make(key, label, tooltip);
                button.connect_clicked(glib::clone!(@weak popover => move |_| popover.popdown()));
                box_.add(&button);
            }
            popover.add(&box_);
            box_.show_all();
            menu.set_popover(Some(&popover));
            header.pack_end(&menu);
        }
        window.set_titlebar(Some(&header));
        let notebook = Notebook::new();
        let chapter_list = ListBox::new();
        let lore_list = ListBox::new();
        let chapter_search = SearchEntry::new();
        chapter_search.set_placeholder_text(Some("Filter chapter titles"));
        let lore_search = SearchEntry::new();
        lore_search.set_placeholder_text(Some("Filter lore titles and categories"));
        let category = ComboBoxText::new();
        for category_ in Category::all() {
            category.append_text(category_.as_str());
        }
        category.set_active(Some(0));
        category.set_tooltip_text(Some("Category for a new lore entry"));
        let mut sidebars = Vec::new();
        let mut categories = Vec::new();
        for (is_chapter, title, list, search) in [
            (true, "Chapters", &chapter_list, &chapter_search),
            (false, "Lore & characters", &lore_list, &lore_search),
        ] {
            let sidebar = GtkBox::new(Orientation::Vertical, 6);
            sidebar.style_context().add_class("sidebar");
            sidebar.set_size_request(220, -1);
            sidebar.add(&Label::new(Some(title)));
            sidebar.add(search);
            if !is_chapter {
                for category_ in Category::all() {
                    let toggle = CheckButton::with_label(category_.as_str());
                    toggle.set_active(true);
                    toggle.set_tooltip_text(Some("Show or hide this category"));
                    sidebar.add(&toggle);
                    categories.push(toggle);
                }
            }
            let list_scroll =
                ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
            list_scroll.add(list);
            sidebar.pack_start(&list_scroll, true, true, 0);
            if !is_chapter {
                sidebar.add(&Label::new(Some("New entry category")));
                sidebar.add(&category);
            }
            let toolbar = GtkBox::new(Orientation::Horizontal, 4);
            let prefix = if is_chapter { "chapter" } else { "lore" };
            for (action, label) in [("new", "New"), ("rename", "Rename"), ("delete", "Trash")] {
                let key = format!("{prefix}-{action}");
                toolbar.add(&make(&key, label, &format!("{action} {prefix}")));
            }
            sidebar.add(&toolbar);
            if is_chapter {
                let movement = GtkBox::new(Orientation::Horizontal, 4);
                movement.add(&make("up", "Move up", "Move chapter one position earlier"));
                movement.add(&make(
                    "down",
                    "Move down",
                    "Move chapter one position later",
                ));
                sidebar.add(&movement);
            }
            sidebars.push(sidebar);
        }
        let planning_buttons = [
            make(
                "add-link",
                "Add selected link",
                "Link the selected document using a stable ID",
            ),
            make(
                "backlinks",
                "Show links and backlinks",
                "Navigate linked documents and incoming references",
            ),
            make(
                "save-plan",
                "Save planning",
                "Persist optional document metadata",
            ),
        ];
        let chapter = Editor::new(
            "No chapter selected. Create a chapter to begin writing.",
            &buttons["chapter-new"],
        );
        let lore = Editor::new(
            "No lore entry selected. Create a character, environment, or lore entry.",
            &buttons["lore-new"],
        );
        let chapter_pane = Paned::new(Orientation::Horizontal);
        chapter_pane.pack1(&sidebars[0], false, false);
        let reference_pane = Paned::new(Orientation::Horizontal);
        reference_pane.pack1(&chapter.stack, true, false);
        let tools = Notebook::new();
        tools.set_size_request(240, -1);
        let reference = TextView::new();
        reference.set_editable(false);
        reference.set_cursor_visible(false);
        reference.set_wrap_mode(gtk::WrapMode::Word);
        reference.style_context().add_class("editor");
        let reference_selector = ComboBoxText::new();
        let reference_box = GtkBox::new(Orientation::Vertical, 8);
        reference_box.add(&reference_selector);
        reference_box.pack_start(&scroll(&reference), true, true, 0);
        tools.append_page(&reference_box, Some(&Label::new(Some("Reference"))));
        let outline = ListBox::new();
        let outline_scroll =
            ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        outline_scroll.add(&outline);
        tools.append_page(&outline_scroll, Some(&Label::new(Some("Outline"))));
        let planning = GtkBox::new(Orientation::Vertical, 6);
        planning.set_margin(10);
        let mut plan = BTreeMap::new();
        for (key, label) in [
            ("pov", "POV character"),
            ("timeline", "Timeline"),
            ("status", "Draft status"),
            ("tags", "Tags (comma-separated)"),
            ("target", "Chapter word target"),
            ("links", "Linked document IDs (comma-separated)"),
        ] {
            planning.add(&Label::new(Some(label)));
            let entry = Entry::new();
            entry.set_placeholder_text(Some(label));
            planning.add(&entry);
            plan.insert(key.to_string(), entry);
        }
        planning.add(&Label::new(Some("Summary / scene planning notes")));
        let summary = TextView::new();
        summary.set_wrap_mode(gtk::WrapMode::Word);
        summary.set_size_request(-1, 90);
        planning.add(&scroll(&summary));
        let link_selector = ComboBoxText::new();
        planning.add(&Label::new(Some("Link a document")));
        planning.add(&link_selector);
        for button in planning_buttons {
            planning.add(&button);
        }
        let plan_scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        plan_scroll.add(&planning);
        tools.append_page(&plan_scroll, Some(&Label::new(Some("Plan"))));
        reference_pane.pack2(&tools, false, false);
        chapter_pane.pack2(&reference_pane, true, false);
        chapter_pane.set_position(state.chapter_sidebar);
        let lore_pane = Paned::new(Orientation::Horizontal);
        lore_pane.pack1(&sidebars[1], false, false);
        lore_pane.pack2(&lore.stack, true, false);
        lore_pane.set_position(state.lore_sidebar);
        notebook.append_page(&chapter_pane, Some(&Label::new(Some("Chapters"))));
        notebook.append_page(&lore_pane, Some(&Label::new(Some("Lore & characters"))));
        notebook.set_current_page(Some(state.tab.min(1)));
        let status = Label::new(None);
        status.set_xalign(0.0);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let words = Label::new(None);
        words.set_xalign(1.0);
        words.set_ellipsize(gtk::pango::EllipsizeMode::End);
        words.set_max_width_chars(72);
        let status_box = GtkBox::new(Orientation::Horizontal, 12);
        status_box.set_margin(8);
        status_box.pack_start(&status, true, true, 0);
        status_box.pack_end(&words, false, false, 0);
        let storage_label = Label::new(Some(&format!("Library: {}", state.storage.display())));
        storage_label.set_xalign(0.0);
        storage_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        storage_label.set_margin_start(8);
        root.pack_start(&notebook, true, true, 0);
        root.add(&status_box);
        root.add(&storage_label);
        window.add(&root);
        let workspace = Rc::new(Workspace {
            window: window.clone(),
            root,
            notebook,
            selector,
            chapter_list,
            lore_list,
            chapter_search,
            lore_search,
            category,
            categories,
            chapter_sidebar: sidebars[0].clone(),
            lore_sidebar: sidebars[1].clone(),
            chapter_pane,
            lore_pane,
            reference_pane,
            tools,
            reference_selector,
            reference,
            outline,
            plan,
            summary,
            link_selector,
            links: RefCell::new(Vec::new()),
            plan_document: RefCell::new(None),
            chapter,
            lore,
            status,
            words,
            buttons,
            state: RefCell::new(state),
            config,
            projects: RefCell::new(projects),
            chapters: RefCell::new(chapters),
            lores: RefCell::new(lores),
            switching: Cell::new(false),
            busy: Cell::new(false),
            snapshot_busy: Cell::new(false),
            closing: Cell::new(false),
            focus: Cell::new(false),
            session_goal: Cell::new(0),
            search_hits: RefCell::new(Vec::new()),
            search_index: Cell::new(0),
            last_trash: RefCell::new(None),
            storage_label,
            metadata: RefCell::new(HashMap::new()),
            book_counts: RefCell::new(HashMap::new()),
            search_spec: RefCell::new(None),
            outline_headings: RefCell::new(Vec::new()),
            catalog_generation: Cell::new(0),
        });
        workspace.wire();
        workspace.apply_appearance()?;
        workspace.refresh()?;
        Ok(workspace)
    })();
    match result {
        Ok(workspace) => {
            #[cfg(test)]
            TEST_WORKSPACE.with(|slot| slot.replace(Some(workspace.clone())));
            let tab = workspace.state.borrow().tab.min(1);
            workspace.switching.set(true);
            window.show_all();
            workspace.chapter.clear();
            workspace.lore.clear();
            workspace.notebook.set_current_page(Some(tab));
            workspace.switching.set(false);
            workspace.tools.hide();
            workspace.restore_selection();
            // The window owns the callbacks; no detached process or global workspace is needed.
        }
        Err(cause) => {
            window.show_all();
            error(&window, "Could not open the writing workspace", &cause);
        }
    }
}

impl Workspace {
    fn active(&self) -> &Editor {
        if self.notebook.current_page() == Some(1) {
            &self.lore
        } else {
            &self.chapter
        }
    }

    fn active_is_chapter(&self) -> bool {
        self.notebook.current_page() != Some(1)
    }

    fn active_index(&self) -> Option<usize> {
        let path = self.active().path.borrow().clone()?;
        if self.active_is_chapter() {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .position(|c| c.path == path)
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .position(|e| e.path == path)
        }
    }

    fn notify(&self, message: &str) {
        self.status.set_text(message);
    }

    fn set_busy(&self, busy: bool, message: &str) {
        self.busy.set(busy);
        self.root.set_sensitive(!busy);
        for button in self.buttons.values() {
            button.set_sensitive(!busy);
        }
        self.selector.set_sensitive(!busy);
        self.notify(message);
        if !busy {
            self.update_controls();
        }
    }

    fn report(&self, context: &str, cause: &dyn std::fmt::Display) {
        self.notify(&format!("{context}: {cause}"));
        error(&self.window, context, cause);
    }

    fn wire(self: &Rc<Self>) {
        for (key, button) in &self.buttons {
            let workspace = self.clone();
            let key = key.clone();
            button.connect_clicked(move |_| workspace.action(&key));
        }
        for (is_chapter, editor) in [(true, &self.chapter), (false, &self.lore)] {
            let workspace = self.clone();
            editor.view.buffer().unwrap().connect_changed(move |_| {
                let editor = if is_chapter {
                    &workspace.chapter
                } else {
                    &workspace.lore
                };
                if editor.loading.get() || editor.path.borrow().is_none() {
                    return;
                }
                editor.generation.set(editor.generation.get() + 1);
                editor.last_edit.set(std::time::Instant::now());
                editor.dirty.set(true);
                workspace.update_controls();
                let generation = editor.generation.get();
                let weak = Rc::downgrade(&workspace);
                glib::timeout_add_local_once(Duration::from_millis(250), move || {
                    if let Some(workspace) = weak.upgrade() {
                        let editor = if is_chapter {
                            &workspace.chapter
                        } else {
                            &workspace.lore
                        };
                        if generation == editor.generation.get() {
                            workspace.observe(is_chapter);
                            workspace.update_outline();
                            workspace.highlight_matches();
                        }
                    }
                });
            });
            let workspace = self.clone();
            editor
                .view
                .buffer()
                .unwrap()
                .connect_mark_set(move |_, _, mark| {
                    if mark.name().as_deref() == Some("insert") && is_chapter {
                        workspace.highlight_outline();
                    }
                });
            let workspace = self.clone();
            editor.view.connect_size_allocate(move |view, allocation| {
                let width = workspace.state.borrow().preferences.text_width;
                let margin = ((allocation.width() - width) / 2).max(24);
                view.set_left_margin(margin);
                view.set_right_margin(margin);
            });
        }
        for (chapter, list) in [(true, &self.chapter_list), (false, &self.lore_list)] {
            let workspace = self.clone();
            list.connect_row_selected(move |_, row| {
                if workspace.switching.get() || workspace.busy.get() {
                    return;
                }
                if let Some(row) = row {
                    let index = row.index() as usize;
                    let path = if chapter {
                        workspace
                            .chapters
                            .borrow()
                            .chapters()
                            .get(index)
                            .map(|c| c.path.clone())
                    } else {
                        workspace
                            .lores
                            .borrow()
                            .entries()
                            .get(index)
                            .map(|e| e.path.clone())
                    };
                    if let Some(path) = path {
                        workspace.navigate(chapter, path, None);
                    }
                }
            });
            list.add_events(gdk::EventMask::BUTTON_PRESS_MASK);
            let workspace = self.clone();
            list.connect_button_press_event(move |list, event| {
                if event.button() != 3 || workspace.busy.get() {
                    return glib::Propagation::Proceed;
                }
                if let Some(row) = list.row_at_y(event.position().1 as i32) {
                    list.select_row(Some(&row));
                }
                let menu = gtk::Menu::new();
                let prefix = if chapter { "chapter" } else { "lore" };
                for (action, label) in [
                    ("new", "New"),
                    ("rename", "Rename"),
                    ("delete", "Move to Trash"),
                ] {
                    let item = gtk::MenuItem::with_label(label);
                    let workspace = workspace.clone();
                    let key = format!("{prefix}-{action}");
                    item.connect_activate(move |_| workspace.action(&key));
                    menu.append(&item);
                }
                menu.show_all();
                menu.popup_easy(event.button(), event.time());
                glib::Propagation::Stop
            });
        }
        let workspace = self.clone();
        self.chapter_list.set_filter_func(Some(Box::new(move |row| {
            let query = workspace.chapter_search.text().to_lowercase();
            workspace
                .chapters
                .borrow()
                .chapters()
                .get(row.index() as usize)
                .is_some_and(|chapter| chapter.title.to_lowercase().contains(&query))
        })));
        let workspace = self.clone();
        self.lore_list.set_filter_func(Some(Box::new(move |row| {
            workspace
                .lores
                .borrow()
                .entries()
                .get(row.index() as usize)
                .is_some_and(|entry| {
                    let category = Category::all()
                        .iter()
                        .position(|c| *c == entry.category)
                        .unwrap();
                    workspace.categories[category].is_active()
                        && format!("{} {}", entry.title, entry.category.as_str())
                            .to_lowercase()
                            .contains(&workspace.lore_search.text().to_lowercase())
                })
        })));
        for search in [&self.chapter_search, &self.lore_search] {
            let workspace = self.clone();
            search.connect_search_changed(move |_| {
                workspace.chapter_list.invalidate_filter();
                workspace.lore_list.invalidate_filter();
            });
        }
        for toggle in &self.categories {
            let workspace = self.clone();
            toggle.connect_toggled(move |_| workspace.lore_list.invalidate_filter());
        }
        let workspace = self.clone();
        self.selector.connect_changed(move |selector| {
            if workspace.switching.get() || workspace.busy.get() {
                return;
            }
            if let Some(project) = selector
                .active()
                .and_then(|i| workspace.projects.borrow().get(i as usize).cloned())
            {
                if project.path != workspace.state.borrow().project {
                    let w = workspace.clone();
                    workspace.with_saved(move || w.open_project(project.path));
                }
            }
        });
        let workspace = self.clone();
        self.notebook.connect_switch_page(move |_, _, page| {
            if workspace.switching.get() || workspace.busy.get() {
                return;
            }
            workspace.state.borrow_mut().tab = page;
            let w = workspace.clone();
            glib::idle_add_local_once(move || {
                if w.notebook.current_page() == Some(page) {
                    w.restore_selection();
                    w.update_controls();
                }
            });
        });
        let workspace = self.clone();
        self.reference_selector.connect_changed(move |selector| {
            if workspace.switching.get() {
                return;
            }
            let entry = selector
                .active()
                .and_then(|i| workspace.lores.borrow().entries().get(i as usize).cloned());
            if let Some(entry) = entry {
                let w = workspace.clone();
                let generation = workspace.catalog_generation.get();
                let current = workspace.lore.path.borrow().as_ref() == Some(&entry.path);
                if current {
                    workspace
                        .reference
                        .buffer()
                        .unwrap()
                        .set_text(&text(&workspace.lore.view));
                } else {
                    let selected = entry.path.clone();
                    job(
                        move || fs::read_to_string(&entry.path),
                        move |result| {
                            if w.catalog_generation.get() != generation {
                                return;
                            }
                            let still_selected = w.reference_selector.active().and_then(|i| {
                                w.lores
                                    .borrow()
                                    .entries()
                                    .get(i as usize)
                                    .map(|e| e.path.clone())
                            });
                            if still_selected.as_ref() != Some(&selected) {
                                return;
                            }
                            match result {
                                Ok(content) => w.reference.buffer().unwrap().set_text(&content),
                                Err(cause) => w.report("Could not read reference", &cause),
                            }
                        },
                    );
                }
            }
        });
        let workspace = self.clone();
        self.outline.connect_row_activated(move |_, row| {
            if let Some(heading) =
                features::headings(&text(&workspace.chapter.view)).get(row.index() as usize)
            {
                workspace.jump(&workspace.chapter.view, heading.line, None);
            }
        });
        let workspace = self.clone();
        self.window.connect_key_press_event(move |_, event| {
            let ctrl = event.state().contains(gdk::ModifierType::CONTROL_MASK);
            let shift = event.state().contains(gdk::ModifierType::SHIFT_MASK);
            let key = event.keyval();
            let action = if ctrl && key == gdk::keys::constants::s {
                Some("save")
            } else if ctrl && (key == gdk::keys::constants::z || key == gdk::keys::constants::Z) {
                Some(if shift { "redo" } else { "undo" })
            } else if ctrl && (key == gdk::keys::constants::f || key == gdk::keys::constants::F) {
                Some(if shift { "find-project" } else { "find" })
            } else if ctrl
                && shift
                && (key == gdk::keys::constants::o || key == gdk::keys::constants::O)
            {
                Some("outline")
            } else if ctrl && key == gdk::keys::constants::n {
                Some(if workspace.active_is_chapter() {
                    "chapter-new"
                } else {
                    "lore-new"
                })
            } else if key == gdk::keys::constants::F11 {
                Some("focus")
            } else if key == gdk::keys::constants::F3 {
                Some(if shift {
                    "previous-match"
                } else {
                    "next-match"
                })
            } else {
                None
            };
            if let Some(action) = action {
                if gtk::prelude::GtkWindowExt::focused_widget(&workspace.window).is_some_and(
                    |widget| {
                        widget.is::<Entry>()
                            || widget == workspace.summary.clone().upcast::<gtk::Widget>()
                    },
                ) && matches!(action, "undo" | "redo")
                {
                    return glib::Propagation::Proceed;
                }
                workspace.action(action);
                glib::Propagation::Stop
            } else if ctrl && key == gdk::keys::constants::Tab {
                workspace
                    .notebook
                    .set_current_page(Some(if workspace.active_is_chapter() { 1 } else { 0 }));
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let workspace = self.clone();
        self.window.connect_delete_event(move |_, _| {
            if workspace.closing.get() {
                return glib::Propagation::Proceed;
            }
            if workspace.busy.get() {
                return glib::Propagation::Stop;
            }
            let w = workspace.clone();
            workspace.with_saved(move || {
                if let Err(cause) = w.persist() {
                    w.report("Could not save workspace", &cause);
                    return;
                }
                w.closing.set(true);
                w.window.close();
            });
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(2), move || {
            let Some(workspace) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !workspace.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            workspace.write_drafts();
            glib::ControlFlow::Continue
        });
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_secs(60), move || {
            let Some(workspace) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !workspace.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            workspace.update_words();
            if let Err(cause) = workspace.persist() {
                workspace.report("Could not save progress", &cause);
            }
            glib::ControlFlow::Continue
        });
    }

    fn observe(&self, chapter: bool) {
        let editor = if chapter { &self.chapter } else { &self.lore };
        if editor.path.borrow().is_none() {
            return;
        }
        if editor.observed_generation.get() == editor.generation.get() {
            return;
        }
        let content = text(&editor.view);
        let before = features::word_count(&editor.observed.borrow());
        let after = features::word_count(&content);
        let include_lore = self.state.borrow().preferences.include_lore;
        self.state.borrow_mut().progress.record(
            &editor.id.borrow(),
            before,
            after,
            !chapter,
            include_lore,
        );
        if chapter {
            self.book_counts
                .borrow_mut()
                .insert(editor.id.borrow().clone(), after);
        }
        if let Some(history) = editor.histories.borrow_mut().get_mut(&*editor.id.borrow()) {
            history.record(&content);
        }
        editor.observed.replace(content.clone());
        editor.observed_generation.set(editor.generation.get());
        if !chapter {
            let selected = self.reference_selector.active().and_then(|i| {
                self.lores
                    .borrow()
                    .entries()
                    .get(i as usize)
                    .map(|e| e.path.clone())
            });
            if selected.as_ref() == editor.path.borrow().as_ref() {
                self.reference.buffer().unwrap().set_text(&content);
            }
        }
        editor.dirty.set(content != *editor.saved.borrow());
        self.update_controls();
        self.update_words();
    }

    fn capture_position(&self, editor: &Editor) {
        if let Some(path) = editor.path.borrow().clone() {
            let buffer = editor.view.buffer().unwrap();
            let offset = buffer.iter_at_mark(&buffer.get_insert().unwrap()).offset();
            let mut state = self.state.borrow_mut();
            state.cursors.insert(path.clone(), offset);
            state
                .scrolls
                .insert(path, editor.scroll.vadjustment().value());
        }
    }

    fn persist(&self) -> io::Result<()> {
        self.observe(true);
        self.observe(false);
        self.capture_position(&self.chapter);
        self.capture_position(&self.lore);
        let mut state = self.state.borrow_mut();
        state.width = self.window.size().0;
        state.height = self.window.size().1;
        state.chapter_sidebar = self.chapter_pane.position();
        state.lore_sidebar = self.lore_pane.position();
        state.tab = self.notebook.current_page().unwrap_or(0);
        if let Some(path) = self.chapter.path.borrow().clone() {
            state.chapter = Some(path);
        }
        if let Some(path) = self.lore.path.borrow().clone() {
            state.lore = Some(path);
        }
        features::save_workspace(&self.config, &state)
    }

    fn with_saved(self: &Rc<Self>, complete: impl FnOnce() + 'static) {
        if self.busy.get() {
            return;
        }
        if self.snapshot_busy.get() {
            let workspace = self.clone();
            glib::timeout_add_local_once(Duration::from_millis(80), move || {
                workspace.with_saved(complete)
            });
            return;
        }
        if let Err(cause) = self.save_plan() {
            self.report("Could not save planning", &cause);
            return;
        }
        self.observe(true);
        self.observe(false);
        let mut saves = Vec::new();
        for editor in [&self.chapter, &self.lore] {
            if editor.dirty.get() {
                if let Some(path) = editor.path.borrow().clone() {
                    saves.push((path, editor.saved.borrow().clone(), text(&editor.view)));
                }
            }
        }
        if saves.is_empty() {
            complete();
            return;
        }
        self.set_busy(true, "Saving documents…");
        let w = self.clone();
        job(
            move || {
                let mut outcomes = Vec::new();
                for (path, base, content) in saves {
                    let result = documents::save_checked(&path, &base, &content);
                    let cleanup = if result.is_ok() {
                        documents::clear_draft(&path)
                    } else {
                        Ok(())
                    };
                    let failed = result.is_err();
                    outcomes.push((path, content, result, cleanup));
                    if failed {
                        break;
                    }
                }
                Ok(outcomes)
            },
            move |result| {
                w.set_busy(false, "Saved");
                match result {
                    Ok(outcomes) => {
                        for (path, content, outcome, cleanup) in outcomes {
                            match outcome {
                                Ok(()) => {
                                    for editor in [&w.chapter, &w.lore] {
                                        if editor.path.borrow().as_ref() == Some(&path) {
                                            editor.saved.replace(content.clone());
                                            editor.dirty.set(false);
                                        }
                                    }
                                }
                                Err(cause) => {
                                    w.reselect();
                                    w.report("Could not save; edits remain open", &cause);
                                    return;
                                }
                            }
                            if let Err(cause) = cleanup {
                                w.reselect();
                                w.report(
                                    "Document saved, but recovery draft cleanup failed",
                                    &cause,
                                );
                                return;
                            }
                        }
                        if let Err(cause) = w.persist() {
                            w.report("Could not save workspace", &cause);
                            return;
                        }
                        w.update_controls();
                        complete();
                    }
                    Err(cause) => w.report("Could not save", &cause),
                }
            },
        );
    }

    fn write_drafts(self: &Rc<Self>) {
        if self.busy.get() || self.snapshot_busy.get() {
            return;
        }
        self.observe(true);
        self.observe(false);
        let mut drafts = Vec::new();
        for editor in [&self.chapter, &self.lore] {
            if editor.dirty.get()
                && editor.generation.get() != editor.draft_written.get()
                && editor.last_edit.get().elapsed() >= Duration::from_secs(2)
            {
                if let Some(path) = editor.path.borrow().clone() {
                    drafts.push((
                        path,
                        editor.saved.borrow().clone(),
                        text(&editor.view),
                        editor.generation.get(),
                    ));
                }
            }
        }
        if drafts.is_empty() {
            return;
        }
        self.snapshot_busy.set(true);
        let w = self.clone();
        job(
            move || {
                let mut written = Vec::new();
                for (path, base, content, generation) in drafts {
                    documents::write_draft(&path, &base, &content)?;
                    written.push((path, generation));
                }
                Ok(written)
            },
            move |result| {
                w.snapshot_busy.set(false);
                match result {
                    Ok(written) => {
                        for (path, generation) in written {
                            for editor in [&w.chapter, &w.lore] {
                                if editor.path.borrow().as_ref() == Some(&path) {
                                    editor.draft_written.set(generation);
                                }
                            }
                        }
                        w.notify("Recovery draft saved · document has unsaved changes");
                        if let Err(cause) = w.persist() {
                            w.report("Could not save progress", &cause);
                        }
                    }
                    Err(cause) => w.report("Could not write recovery draft", &cause),
                }
            },
        );
    }

    fn refresh(self: &Rc<Self>) -> io::Result<()> {
        self.catalog_generation
            .set(self.catalog_generation.get() + 1);
        self.switching.set(true);
        let result = (|| {
            for list in [&self.chapter_list, &self.lore_list, &self.outline] {
                for child in list.children() {
                    list.remove(&child);
                }
            }
            self.reference_selector.remove_all();
            self.link_selector.remove_all();
            self.metadata.borrow_mut().clear();
            self.links.borrow_mut().clear();
            let chapter_rows: Vec<_> = self
                .chapters
                .borrow()
                .chapters()
                .iter()
                .enumerate()
                .map(|(i, chapter)| {
                    (
                        chapter.path.clone(),
                        format!("{:02}  {}", i + 1, chapter.title),
                    )
                })
                .collect();
            let lore_rows: Vec<_> = self
                .lores
                .borrow()
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.path.clone(),
                        format!("{} ({})", entry.title, entry.category.as_str()),
                    )
                })
                .collect();
            for (chapter, rows, list) in [
                (true, chapter_rows, &self.chapter_list),
                (false, lore_rows, &self.lore_list),
            ] {
                for (index, (path, title)) in rows.into_iter().enumerate() {
                    let metadata = features::load_metadata(&path)?;
                    features::save_metadata(&path, &metadata)?;
                    self.links
                        .borrow_mut()
                        .push((metadata.id.clone(), title.clone()));
                    self.link_selector.append_text(&title);
                    self.metadata.borrow_mut().insert(path, metadata);
                    let row = ListBoxRow::new();
                    let label = Label::new(Some(&title));
                    label.set_xalign(0.0);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    label.set_margin(10);
                    row.add(&label);
                    list.add(&row);
                    if chapter {
                        let targets = [gtk::TargetEntry::new(
                            "BOOKAPP_CHAPTER",
                            gtk::TargetFlags::SAME_APP,
                            0,
                        )];
                        row.drag_source_set(
                            gdk::ModifierType::BUTTON1_MASK,
                            &targets,
                            gdk::DragAction::MOVE,
                        );
                        row.drag_dest_set(gtk::DestDefaults::ALL, &targets, gdk::DragAction::MOVE);
                        row.connect_drag_data_get(move |_, _, data, _, _| {
                            data.set_text(&index.to_string());
                        });
                        let w = self.clone();
                        row.connect_drag_data_received(move |_, _, _, _, data, _, _| {
                            if let Some(from) = data.text().and_then(|s| s.parse::<usize>().ok()) {
                                let w2 = w.clone();
                                w.with_saved(move || w2.move_chapter(from, index));
                            }
                        });
                    } else {
                        self.reference_selector.append_text(&title);
                    }
                }
                list.show_all();
            }
            self.link_selector.set_active(Some(0));
            self.reference_selector.set_active(Some(0));
            Ok(())
        })();
        self.switching.set(false);
        self.reference.buffer().unwrap().set_text("");
        if self.reference_selector.active().is_some() {
            self.reference_selector.emit_by_name::<()>("changed", &[]);
        }
        self.update_controls();
        if result.is_ok() {
            self.count_book();
        }
        result
    }

    fn restore_selection(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        let chapter = self.active_is_chapter();
        let editor = if chapter { &self.chapter } else { &self.lore };
        if editor.path.borrow().is_some() {
            self.update_plan();
            self.update_words();
            return;
        }
        let preferred = if chapter {
            self.state.borrow().chapter.clone()
        } else {
            self.state.borrow().lore.clone()
        };
        let paths: Vec<_> = if chapter {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .map(|c| c.path.clone())
                .collect()
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .map(|e| e.path.clone())
                .collect()
        };
        let path = preferred
            .filter(|p| paths.contains(p))
            .or_else(|| paths.first().cloned());
        if let Some(path) = path {
            self.open_document(chapter, path, None);
        } else {
            editor.clear();
            self.update_plan();
            self.update_controls();
            self.update_words();
        }
    }

    fn navigate(self: &Rc<Self>, chapter: bool, path: PathBuf, found: Option<TextMatch>) {
        let editor = if chapter { &self.chapter } else { &self.lore };
        if editor.path.borrow().as_ref() == Some(&path) {
            if let Some(found) = found {
                self.jump(
                    &editor.view,
                    found.line as i32,
                    Some((found.start, found.end)),
                );
            }
            return;
        }
        let w = self.clone();
        self.with_saved(move || w.open_document(chapter, path, found));
    }

    fn open_document(self: &Rc<Self>, chapter: bool, path: PathBuf, found: Option<TextMatch>) {
        let editor = if chapter { &self.chapter } else { &self.lore };
        self.capture_position(editor);
        self.set_busy(true, "Opening document…");
        let target = path.clone();
        let w = self.clone();
        job(
            move || {
                Ok((
                    fs::read_to_string(&target)?,
                    documents::load_draft(&target)?,
                ))
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok((saved, draft)) => {
                        let mut content = saved.clone();
                        if let Some(draft) = draft {
                            if draft.content != saved {
                                let message = if draft.base == saved {
                                    "A recovery draft contains unsaved edits. Recover it into the editor?"
                                } else {
                                    "The document changed outside the application. Recover the draft into the editor? Review it before saving over the current file."
                                };
                                if confirm(&w.window, "Recover draft", message, "Recover") {
                                    content = draft.content;
                                } else if let Err(cause) = documents::clear_draft(&path) {
                                    w.report("Could not discard recovery draft", &cause);
                                    return;
                                }
                            }
                        }
                        let Some(metadata) = w.metadata.borrow().get(&path).cloned() else {
                            w.report("Could not open document", &"Missing metadata");
                            return;
                        };
                        let editor = if chapter { &w.chapter } else { &w.lore };
                        editor.display(path.clone(), &metadata, saved, content);
                        w.switching.set(true);
                        if chapter {
                            w.state.borrow_mut().chapter = Some(path.clone());
                            let index = w
                                .chapters
                                .borrow()
                                .chapters()
                                .iter()
                                .position(|c| c.path == path)
                                .unwrap();
                            w.chapter_list
                                .select_row(w.chapter_list.row_at_index(index as i32).as_ref());
                        } else {
                            w.state.borrow_mut().lore = Some(path.clone());
                            let index = w
                                .lores
                                .borrow()
                                .entries()
                                .iter()
                                .position(|e| e.path == path)
                                .unwrap();
                            w.lore_list
                                .select_row(w.lore_list.row_at_index(index as i32).as_ref());
                        }
                        w.notebook
                            .set_current_page(Some(if chapter { 0 } else { 1 }));
                        w.switching.set(false);
                        if let Some(found) = found {
                            w.jump(
                                &editor.view,
                                found.line as i32,
                                Some((found.start, found.end)),
                            );
                        } else {
                            let offset = w.state.borrow().cursors.get(&path).copied().unwrap_or(0);
                            let buffer = editor.view.buffer().unwrap();
                            buffer.place_cursor(
                                &buffer.iter_at_offset(offset.clamp(0, buffer.char_count())),
                            );
                            let value = w.state.borrow().scrolls.get(&path).copied().unwrap_or(0.0);
                            let scroll = editor.scroll.clone();
                            glib::idle_add_local_once(move || {
                                scroll.vadjustment().set_value(value);
                            });
                        }
                        w.update_plan();
                        w.update_outline();
                        w.highlight_matches();
                        w.update_controls();
                        w.update_words();
                    }
                    Err(cause) => {
                        w.report("Could not open document", &cause);
                        w.reselect();
                    }
                }
            },
        );
    }

    fn reselect(&self) {
        self.switching.set(true);
        for (chapter, editor, list) in [
            (true, &self.chapter, &self.chapter_list),
            (false, &self.lore, &self.lore_list),
        ] {
            let path = editor.path.borrow().clone();
            let index = if chapter {
                self.chapters
                    .borrow()
                    .chapters()
                    .iter()
                    .position(|c| Some(&c.path) == path.as_ref())
            } else {
                self.lores
                    .borrow()
                    .entries()
                    .iter()
                    .position(|e| Some(&e.path) == path.as_ref())
            };
            list.select_row(index.and_then(|i| list.row_at_index(i as i32)).as_ref());
        }
        let index = projects::project_index(&self.projects.borrow(), &self.state.borrow().project);
        self.selector.set_active(index.map(|i| i as u32));
        self.switching.set(false);
    }

    fn open_project(self: &Rc<Self>, path: PathBuf) {
        self.capture_position(&self.chapter);
        self.capture_position(&self.lore);
        self.set_busy(true, "Opening project…");
        let target = path.clone();
        let w = self.clone();
        job(
            move || {
                Ok((
                    ChapterStore::load_or_init(&target.to_string_lossy())?,
                    LoreStore::load(&target)?,
                ))
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok((chapters, lores)) => {
                        let same = w.state.borrow().project == path;
                        w.chapters.replace(chapters);
                        w.lores.replace(lores);
                        w.chapter.clear();
                        w.lore.clear();
                        w.state.borrow_mut().project = path;
                        if !same {
                            w.state.borrow_mut().chapter = None;
                            w.state.borrow_mut().lore = None;
                        }
                        w.last_trash.replace(None);
                        w.search_hits.borrow_mut().clear();
                        w.plan_document.replace(None);
                        if let Err(cause) = w.refresh() {
                            w.report("Could not refresh project", &cause);
                            return;
                        }
                        w.reselect();
                        w.restore_selection();
                    }
                    Err(cause) => {
                        w.report("Could not open project", &cause);
                        w.reselect();
                    }
                }
            },
        );
    }

    fn update_controls(&self) {
        if self.busy.get() {
            return;
        }
        let chapter = self.chapter.path.borrow().is_some();
        let lore = self.lore.path.borrow().is_some();
        for (key, enabled) in [
            ("chapter-rename", chapter),
            ("chapter-delete", chapter),
            ("lore-rename", lore),
            ("lore-delete", lore),
            ("recategorize", !self.active_is_chapter() && lore),
            ("template", !self.active_is_chapter() && lore),
            ("revisions", self.active().path.borrow().is_some()),
            ("undo", self.active().path.borrow().is_some()),
            ("redo", self.active().path.borrow().is_some()),
            ("find", self.active().path.borrow().is_some()),
            ("save-plan", self.active().path.borrow().is_some()),
            ("undo-delete", self.last_trash.borrow().is_some()),
            (
                "remove-project",
                self.selector.active().is_some_and(|i| i > 0),
            ),
        ] {
            self.buttons[key].set_sensitive(enabled);
        }
        let index = self.chapter.path.borrow().as_ref().and_then(|path| {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .position(|c| &c.path == path)
        });
        self.buttons["up"].set_sensitive(index.is_some_and(|i| i > 0));
        self.buttons["down"]
            .set_sensitive(index.is_some_and(|i| i + 1 < self.chapters.borrow().chapters().len()));
        let project_name = self
            .projects
            .borrow()
            .iter()
            .find(|p| p.path == self.state.borrow().project)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Book".into());
        let path = self.active().path.borrow().clone();
        let document = if self.active_is_chapter() {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .find(|c| Some(&c.path) == path.as_ref())
                .map(|c| c.title.clone())
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .find(|e| Some(&e.path) == path.as_ref())
                .map(|e| e.title.clone())
        }
        .unwrap_or_else(|| "No document".into());
        let dirty = self.chapter.dirty.get() || self.lore.dirty.get();
        self.window.set_title(&format!(
            "{project_name} — {document}{}",
            if dirty { " •" } else { "" }
        ));
        self.status.set_text(if dirty {
            "Unsaved changes · recovery drafts saved after idle"
        } else {
            "Saved"
        });
    }

    fn jump(&self, view: &TextView, line: i32, range: Option<(i32, i32)>) {
        let buffer = view.buffer().unwrap();
        let mut iter = buffer.iter_at_line(line.clamp(0, buffer.line_count() - 1));
        if let Some((start, end)) = range {
            let start = buffer.iter_at_offset(start.clamp(0, buffer.char_count()));
            let end = buffer.iter_at_offset(end.clamp(0, buffer.char_count()));
            buffer.select_range(&start, &end);
            iter = start;
        } else {
            buffer.place_cursor(&iter);
        }
        view.scroll_to_iter(&mut iter, 0.15, false, 0.0, 0.0);
        view.grab_focus();
    }
}

impl Workspace {
    fn action(self: &Rc<Self>, key: &str) {
        if self.busy.get() {
            return;
        }
        match key {
            "save" => {
                let w = self.clone();
                self.with_saved(move || w.notify("All open documents saved"));
            }
            "undo" | "redo" => self.undo(key == "redo"),
            "find" | "find-project" => self.search_dialog(key == "find-project"),
            "next-match" | "previous-match" => self.next_match(key == "previous-match"),
            "focus" => {
                let focus = !self.focus.get();
                self.focus.set(focus);
                self.chapter_sidebar.set_visible(!focus);
                self.lore_sidebar.set_visible(!focus);
                self.tools.set_visible(!focus);
                self.buttons["focus"].set_label(if focus {
                    "Leave focus mode · F11"
                } else {
                    "Focus mode · F11"
                });
            }
            "outline" | "reference" | "planning" => {
                if key == "planning" && !self.active_is_chapter() {
                    self.planning_dialog();
                    return;
                }
                self.focus.set(false);
                self.notebook.set_current_page(Some(0));
                self.chapter_sidebar.show();
                self.lore_sidebar.show();
                self.tools.show();
                self.reference_pane
                    .set_position((self.chapter_pane.allocated_width() - 330).max(200));
                self.tools.set_current_page(Some(match key {
                    "reference" => 0,
                    "outline" => 1,
                    _ => 2,
                }));
                self.update_outline();
                self.update_plan();
            }
            "save-plan" => match self.save_plan() {
                Ok(()) => self.notify("Planning saved"),
                Err(cause) => self.report("Could not save planning", &cause),
            },
            "add-link" => {
                if let Some((id, _)) = self
                    .link_selector
                    .active()
                    .and_then(|i| self.links.borrow().get(i as usize).cloned())
                {
                    let entry = &self.plan["links"];
                    let mut ids: Vec<_> = entry
                        .text()
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    if id != *self.active().id.borrow() && !ids.contains(&id) {
                        ids.push(id);
                    }
                    entry.set_text(&ids.join(", "));
                }
            }
            "backlinks" => self.show_links(),
            "settings" => self.settings(),
            "goal" => {
                if let Some(value) = prompt(
                    &self.window,
                    "Session word goal",
                    &self.session_goal.get().to_string(),
                    "Start session",
                    |value| {
                        value
                            .parse::<usize>()
                            .ok()
                            .filter(|n| *n > 0)
                            .map(|_| ())
                            .ok_or_else(|| "Enter a positive whole number.".into())
                    },
                ) {
                    self.observe(true);
                    self.observe(false);
                    self.state.borrow_mut().progress.start_session();
                    self.session_goal.set(value.parse().unwrap());
                    self.update_words();
                    if let Err(cause) = self.persist() {
                        self.report("Could not save session", &cause);
                    }
                }
            }
            "chapter-new" | "lore-new" => self.new_document(key == "chapter-new"),
            "chapter-rename" | "lore-rename" => self.rename_document(key == "chapter-rename"),
            "chapter-delete" | "lore-delete" => self.delete_document(key == "chapter-delete"),
            "up" | "down" => {
                let index = self.chapter.path.borrow().as_ref().and_then(|p| {
                    self.chapters
                        .borrow()
                        .chapters()
                        .iter()
                        .position(|c| &c.path == p)
                });
                if let Some(index) = index {
                    let target = if key == "up" {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(self.chapters.borrow().chapters().len() - 1)
                    };
                    let w = self.clone();
                    self.with_saved(move || w.move_chapter(index, target));
                }
            }
            "recategorize" => self.recategorize(),
            "template" => {
                if !self.active_is_chapter() {
                    if !text(&self.lore.view).trim().is_empty() {
                        self.notify("Templates can only be inserted into an empty lore entry.");
                        return;
                    }
                    if let Some(index) = self.active_index() {
                        let category = self.lores.borrow().entries()[index].category;
                        self.lore
                            .view
                            .buffer()
                            .unwrap()
                            .set_text(features::template(category.as_str()));
                    }
                }
            }
            "restore-document" | "undo-delete" => self.restore_deleted(key == "undo-delete"),
            "revisions" => self.revisions(),
            "new-project" => self.new_project(),
            "remove-project" => self.remove_project(),
            "restore-project" => self.restore_project(),
            "storage" => self.choose_storage(),
            "import" => self.import_chapter(),
            "export" => self.export(),
            "backup" => self.backup(),
            "import-backup" => self.import_backup(),
            _ => self.report("Unknown action", &key),
        }
    }

    fn undo(&self, redo: bool) {
        let chapter = self.active_is_chapter();
        self.observe(chapter);
        let editor = self.active();
        let result = editor
            .histories
            .borrow_mut()
            .get_mut(&*editor.id.borrow())
            .and_then(|history| if redo { history.redo() } else { history.undo() });
        if let Some(content) = result {
            editor.loading.set(true);
            editor.view.buffer().unwrap().set_text(&content);
            editor.loading.set(false);
            editor.generation.set(editor.generation.get() + 1);
            self.observe(chapter);
            self.update_outline();
        } else {
            self.notify(if redo {
                "Nothing to redo"
            } else {
                "Nothing to undo"
            });
        }
    }

    fn edit_chapters(
        self: &Rc<Self>,
        operation: impl FnOnce(&mut ChapterStore) -> io::Result<Option<usize>> + Send + 'static,
    ) {
        let path = self.state.borrow().project.clone();
        let previous_trash = match self.chapters.borrow().list_deleted() {
            Ok(paths) => paths,
            Err(cause) => {
                self.report("Could not list document Trash", &cause);
                return;
            }
        };
        let old_id = self.chapter.id.borrow().clone();
        let old_metadata = self.metadata.borrow().clone();
        self.capture_position(&self.chapter);
        self.set_busy(true, "Updating chapters…");
        let w = self.clone();
        job(
            move || {
                let mut store = ChapterStore::load_or_init(&path.to_string_lossy())?;
                let selected = operation(&mut store)?;
                Ok((store, selected))
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok((store, selected)) => {
                        w.chapters.replace(store);
                        w.chapter.clear();
                        w.plan_document.replace(None);
                        w.state.borrow_mut().chapter = None;
                        if let Err(cause) = w.refresh() {
                            w.report("Could not refresh chapters", &cause);
                            return;
                        }
                        w.relocate_positions(&old_metadata);
                        let target = selected
                            .and_then(|i| {
                                w.chapters
                                    .borrow()
                                    .chapters()
                                    .get(i)
                                    .map(|c| c.path.clone())
                            })
                            .or_else(|| {
                                w.metadata
                                    .borrow()
                                    .iter()
                                    .find(|(_, meta)| meta.id == old_id)
                                    .map(|(p, _)| p.clone())
                            });
                        if let Some(path) = target {
                            w.open_document(true, path, None);
                        } else {
                            w.restore_selection();
                        }
                        w.remember_last_trash(true, &previous_trash);
                    }
                    Err(cause) => {
                        w.report("Could not update chapters", &cause);
                        w.reselect();
                    }
                }
            },
        );
    }

    fn edit_lore(
        self: &Rc<Self>,
        operation: impl FnOnce(&mut LoreStore) -> io::Result<Option<usize>> + Send + 'static,
    ) {
        let path = self.state.borrow().project.clone();
        let previous_trash = match self.lores.borrow().list_deleted() {
            Ok(paths) => paths,
            Err(cause) => {
                self.report("Could not list document Trash", &cause);
                return;
            }
        };
        let old_id = self.lore.id.borrow().clone();
        let old_metadata = self.metadata.borrow().clone();
        self.capture_position(&self.lore);
        self.set_busy(true, "Updating lore…");
        let w = self.clone();
        job(
            move || {
                let mut store = LoreStore::load(&path)?;
                let selected = operation(&mut store)?;
                Ok((store, selected))
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok((store, selected)) => {
                        w.lores.replace(store);
                        w.lore.clear();
                        w.plan_document.replace(None);
                        w.state.borrow_mut().lore = None;
                        if let Err(cause) = w.refresh() {
                            w.report("Could not refresh lore", &cause);
                            return;
                        }
                        w.relocate_positions(&old_metadata);
                        let target = selected
                            .and_then(|i| w.lores.borrow().entries().get(i).map(|e| e.path.clone()))
                            .or_else(|| {
                                w.metadata
                                    .borrow()
                                    .iter()
                                    .find(|(_, meta)| meta.id == old_id)
                                    .map(|(p, _)| p.clone())
                            });
                        if let Some(path) = target {
                            w.open_document(false, path, None);
                        } else {
                            w.restore_selection();
                        }
                        w.remember_last_trash(false, &previous_trash);
                    }
                    Err(cause) => {
                        w.report("Could not update lore", &cause);
                        w.reselect();
                    }
                }
            },
        );
    }

    fn remember_last_trash(&self, chapter: bool, previous: &[PathBuf]) {
        let list = if chapter {
            self.chapters.borrow().list_deleted()
        } else {
            self.lores.borrow().list_deleted()
        };
        match list {
            Ok(paths) => {
                if let Some(path) = paths.iter().find(|path| !previous.contains(path)) {
                    self.last_trash.replace(Some((chapter, path.clone())));
                } else if self
                    .last_trash
                    .borrow()
                    .as_ref()
                    .is_some_and(|(kind, path)| *kind == chapter && !paths.contains(path))
                {
                    self.last_trash.replace(None);
                }
            }
            Err(cause) => self.report("Could not list deleted documents", &cause),
        }
        self.update_controls();
    }

    fn relocate_positions(&self, previous: &HashMap<PathBuf, Metadata>) {
        let metadata = self.metadata.borrow();
        let mut state = self.state.borrow_mut();
        let cursors = state.cursors.clone();
        let scrolls = state.scrolls.clone();
        for (old_path, old_meta) in previous {
            if let Some((new_path, _)) = metadata.iter().find(|(_, meta)| meta.id == old_meta.id) {
                if let Some(offset) = cursors.get(old_path) {
                    state.cursors.insert(new_path.clone(), *offset);
                }
                if let Some(value) = scrolls.get(old_path) {
                    state.scrolls.insert(new_path.clone(), *value);
                }
            }
        }
    }

    fn new_document(self: &Rc<Self>, chapter: bool) {
        let category = Category::all()[self.category.active().unwrap_or(0) as usize];
        let names: Vec<_> = if chapter {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .map(|c| c.title.clone())
                .collect()
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .filter(|e| e.category == category)
                .map(|e| e.title.clone())
                .collect()
        };
        let Some(title) = prompt(
            &self.window,
            if chapter {
                "New chapter"
            } else {
                "New lore entry"
            },
            "",
            "Create",
            move |name| {
                valid_name(name)?;
                if names.iter().any(|n| n == name) {
                    Err("That name is already in use.".into())
                } else {
                    Ok(())
                }
            },
        ) else {
            return;
        };
        let w = self.clone();
        self.with_saved(move || {
            if chapter {
                w.edit_chapters(move |store| {
                    store.add_chapter(&title)?;
                    Ok(Some(store.chapters().len() - 1))
                });
            } else {
                w.edit_lore(move |store| {
                    store.add(category, &title)?;
                    Ok(store
                        .entries()
                        .iter()
                        .position(|e| e.title == title && e.category == category))
                });
            }
        });
    }

    fn rename_document(self: &Rc<Self>, chapter: bool) {
        let editor = if chapter { &self.chapter } else { &self.lore };
        let path = editor.path.borrow().clone();
        let index = if chapter {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .position(|c| Some(&c.path) == path.as_ref())
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .position(|e| Some(&e.path) == path.as_ref())
        };
        let Some(index) = index else {
            return;
        };
        let title = if chapter {
            self.chapters.borrow().chapters()[index].title.clone()
        } else {
            self.lores.borrow().entries()[index].title.clone()
        };
        let names: Vec<_> = if chapter {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, c)| c.title.clone())
                .collect()
        } else {
            let store = self.lores.borrow();
            let category = store.entries()[index].category;
            store
                .entries()
                .iter()
                .enumerate()
                .filter(|(i, e)| *i != index && e.category == category)
                .map(|(_, e)| e.title.clone())
                .collect()
        };
        let Some(name) = prompt(
            &self.window,
            "Rename document",
            &title,
            "Rename",
            move |name| {
                valid_name(name)?;
                if names.iter().any(|n| n == name) {
                    Err("That name is already in use.".into())
                } else {
                    Ok(())
                }
            },
        ) else {
            return;
        };
        let w = self.clone();
        self.with_saved(move || {
            if chapter {
                w.edit_chapters(move |store| {
                    store.rename_chapter(index, &name)?;
                    Ok(Some(index))
                });
            } else {
                w.edit_lore(move |store| {
                    store.rename(index, &name)?;
                    Ok(Some(index))
                });
            }
        });
    }

    fn delete_document(self: &Rc<Self>, chapter: bool) {
        let editor = if chapter { &self.chapter } else { &self.lore };
        let path = editor.path.borrow().clone();
        let index = if chapter {
            self.chapters
                .borrow()
                .chapters()
                .iter()
                .position(|c| Some(&c.path) == path.as_ref())
        } else {
            self.lores
                .borrow()
                .entries()
                .iter()
                .position(|e| Some(&e.path) == path.as_ref())
        };
        let Some(index) = index else {
            return;
        };
        if !confirm(&self.window, "Move document to Trash", "Move this document and its saved revisions to Trash? You can restore it or undo this deletion.", "Move to Trash") { return; }
        let w = self.clone();
        self.with_saved(move || {
            if chapter {
                w.edit_chapters(move |store| {
                    store.delete_chapter(index)?;
                    Ok((!store.chapters().is_empty())
                        .then(|| index.min(store.chapters().len() - 1)))
                });
            } else {
                w.edit_lore(move |store| {
                    store.delete(index)?;
                    Ok((!store.entries().is_empty()).then(|| index.min(store.entries().len() - 1)))
                });
            }
        });
    }

    fn move_chapter(self: &Rc<Self>, from: usize, to: usize) {
        if from == to {
            return;
        }
        self.edit_chapters(move |store| {
            store.move_to(from, to)?;
            Ok(Some(to))
        });
    }

    fn recategorize(self: &Rc<Self>) {
        if self.active_is_chapter() {
            return;
        }
        let Some(index) = self.active_index() else {
            return;
        };
        let names: Vec<_> = Category::all()
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        if let Some(target) = choose(&self.window, "Move lore to category", "Move", &names) {
            let category = Category::all()[target];
            let id = self.lore.id.borrow().clone();
            let w = self.clone();
            self.with_saved(move || {
                w.edit_lore(move |store| {
                    store.recategorize(index, category)?;
                    for (i, entry) in store.entries().iter().enumerate() {
                        if features::load_metadata(&entry.path)?.id == id {
                            return Ok(Some(i));
                        }
                    }
                    Ok(None)
                })
            });
        }
    }

    fn restore_deleted(self: &Rc<Self>, immediate: bool) {
        let mut paths = Vec::new();
        if immediate {
            if let Some(item) = self.last_trash.borrow().clone() {
                paths.push(item);
            }
        } else {
            for chapter in [true, false] {
                let result = if chapter {
                    self.chapters.borrow().list_deleted()
                } else {
                    self.lores.borrow().list_deleted()
                };
                match result {
                    Ok(list) => paths.extend(list.into_iter().map(|path| (chapter, path))),
                    Err(cause) => {
                        self.report("Could not list Trash", &cause);
                        return;
                    }
                }
            }
        }
        if paths.is_empty() {
            self.notify("There are no deleted documents to restore.");
            return;
        }
        let labels: Vec<_> = paths
            .iter()
            .map(|(chapter, path)| {
                format!(
                    "{} · {}",
                    if *chapter { "Chapter" } else { "Lore" },
                    path.file_name().unwrap().to_string_lossy()
                )
            })
            .collect();
        let selected = if immediate {
            Some(0)
        } else {
            choose(&self.window, "Deleted documents", "Restore", &labels)
        };
        if let Some((chapter, path)) = selected.and_then(|i| paths.get(i).cloned()) {
            let w = self.clone();
            self.with_saved(move || {
                if chapter {
                    w.edit_chapters(move |store| Ok(Some(store.restore_deleted(&path)?)));
                } else {
                    w.edit_lore(move |store| Ok(Some(store.restore_deleted(&path)?)));
                }
            });
        }
    }

    fn reload_projects(&self) -> io::Result<()> {
        let storage = self.state.borrow().storage.clone();
        let projects = projects::load_projects(&storage.to_string_lossy())?;
        self.switching.set(true);
        self.selector.remove_all();
        for project in &projects {
            self.selector.append_text(&project.name);
        }
        let selected =
            projects::project_index(&projects, &self.state.borrow().project).unwrap_or(0);
        self.selector.set_active(Some(selected as u32));
        self.projects.replace(projects);
        self.switching.set(false);
        Ok(())
    }

    fn new_project(self: &Rc<Self>) {
        let names: Vec<_> = self
            .projects
            .borrow()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let Some(name) = prompt(&self.window, "New project", "", "Create", move |name| {
            valid_name(name)?;
            if names.iter().any(|n| n == name) {
                Err("That project already exists.".into())
            } else {
                Ok(())
            }
        }) else {
            return;
        };
        let w = self.clone();
        self.with_saved(move || {
            let result =
                projects::create_project(&w.state.borrow().storage.to_string_lossy(), &name);
            match result {
                Ok(project) => {
                    if let Err(cause) = w.reload_projects() {
                        w.report("Could not reload projects", &cause);
                        return;
                    }
                    w.open_project(project.path);
                }
                Err(cause) => w.report("Could not create project", &cause),
            }
        });
    }

    fn remove_project(self: &Rc<Self>) {
        let Some(index) = self.selector.active() else {
            return;
        };
        if index == 0 {
            return;
        }
        let project = self.projects.borrow()[index as usize].clone();
        let expected = project.name.clone();
        if prompt(
            &self.window,
            &format!("Type '{}' to move project to Trash", project.name),
            "",
            "Move to Trash",
            move |name| {
                if name == expected {
                    Ok(())
                } else {
                    Err("Enter the exact project name.".into())
                }
            },
        )
        .is_none()
        {
            return;
        }
        let w = self.clone();
        self.with_saved(move || {
            let result =
                projects::remove_project(&w.state.borrow().storage.to_string_lossy(), &project);
            match result {
                Ok(()) => {
                    let storage = w.state.borrow().storage.clone();
                    w.state.borrow_mut().project = storage;
                    if let Err(cause) = w.reload_projects() {
                        w.report("Could not reload projects", &cause);
                        return;
                    }
                    let path = w.state.borrow().storage.clone();
                    w.open_project(path);
                }
                Err(cause) => w.report("Could not remove project", &cause),
            }
        });
    }

    fn restore_project(self: &Rc<Self>) {
        let storage = self.state.borrow().storage.clone();
        let trashed = match projects::list_trashed_projects(&storage.to_string_lossy()) {
            Ok(trashed) => trashed,
            Err(cause) => {
                self.report("Could not list project Trash", &cause);
                return;
            }
        };
        if trashed.is_empty() {
            self.notify("No projects in Trash.");
            return;
        }
        let labels: Vec<_> = trashed.iter().map(|p| p.name.clone()).collect();
        if let Some(project) = choose(&self.window, "Deleted projects", "Restore", &labels)
            .and_then(|i| trashed.get(i).cloned())
        {
            let w = self.clone();
            self.with_saved(move || {
                match projects::restore_project(&storage.to_string_lossy(), &project)
                    .and_then(|_| w.reload_projects())
                {
                    Ok(()) => w.notify("Project restored"),
                    Err(cause) => w.report("Could not restore project", &cause),
                }
            });
        }
    }

    fn choose_storage(self: &Rc<Self>) {
        let Some(path) = file_dialog(
            &self.window,
            "Choose book library folder",
            gtk::FileChooserAction::SelectFolder,
        ) else {
            return;
        };
        let w = self.clone();
        self.with_saved(move || {
            let result = fs::canonicalize(&path).and_then(|path| {
                let projects = projects::load_projects(&path.to_string_lossy())?;
                Ok((path, projects))
            });
            match result {
                Ok((path, projects)) => {
                    w.state.borrow_mut().storage = path.clone();
                    w.projects.replace(projects);
                    w.storage_label
                        .set_text(&format!("Library: {}", path.display()));
                    if let Err(cause) = w.reload_projects() {
                        w.report("Could not load library", &cause);
                        return;
                    }
                    w.open_project(path);
                }
                Err(cause) => w.report("Could not open library", &cause),
            }
        });
    }
}
