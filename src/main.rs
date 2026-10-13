mod chapters;
mod lore;
mod projects;

use chapters::ChapterStore;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Entry, HeaderBar, Label,
    ListBox, ListBoxRow, MenuButton, Notebook, Orientation, Paned, ScrolledWindow, SearchEntry,
    TextView,
};
use lore::{Category, LoreStore};
use projects::{
    create_project, list_trashed_projects, load_projects, project_index, remove_project,
    restore_project, Project,
};
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;

const BOOK_DIR: &str = "book";

const IDE_CSS: &str = r#"
@define-color app_bg #101214;
@define-color panel_bg #171a1f;
@define-color elevated_bg #1d2229;
@define-color editor_bg #14171b;
@define-color border #2a3139;
@define-color text #e5e7eb;
@define-color muted #aab2bd;
@define-color accent #9a86d8;
@define-color accent_hover #aa96e8;

window {
    background-color: @app_bg;
    color: @text;
    font-family: sans-serif;
    font-size: 11pt;
}

headerbar.ide-header {
    min-height: 54px;
    padding: 7px 16px;
    background-color: #14171b;
    border-bottom: 1px solid @border;
}

headerbar.ide-header button,
headerbar.ide-header combobox button {
    min-height: 32px;
    padding: 0 12px;
    font-size: 11pt;
    border: 1px solid @border;
    border-radius: 6px;
    background-color: @elevated_bg;
    color: @text;
    box-shadow: none;
}

headerbar.ide-header button label,
headerbar.ide-header combobox button label {
    color: @text;
}

headerbar.ide-header button:hover,
headerbar.ide-header combobox button:hover {
    background-color: #252b33;
    border-color: #3a424c;
}

headerbar.ide-header combobox.project-selector button {
    color: #f4f5f7;
    background-color: #303740;
    background-image: none;
    border-color: #59636f;
    text-shadow: none;
}

headerbar.ide-header combobox.project-selector button label {
    color: #f4f5f7;
    text-shadow: none;
}

headerbar.ide-header combobox.project-selector button:hover {
    background-color: #3b4551;
    border-color: #758292;
}

headerbar.ide-header button.header-action {
    color: #f8f5ff;
    background-color: #4f3f69;
    background-image: none;
    border: 1px solid #927dbb;
    text-shadow: none;
    font-weight: bold;
}

headerbar.ide-header button.header-action label {
    color: #f8f5ff;
    text-shadow: none;
    font-weight: bold;
}

headerbar.ide-header button.header-action:hover {
    color: #ffffff;
    background-color: #5d4c78;
    border-color: #aa96d8;
}

headerbar.ide-header button.header-action:hover label {
    color: #ffffff;
}

headerbar.ide-header button.header-action:active {
    background-color: #413351;
    border-color: #8d78b4;
}

headerbar.ide-header button.header-action:focus {
    border-color: #c0aff0;
}

headerbar.ide-header button.header-action:disabled {
    color: #aab2bd;
    background-color: #292d33;
    border-color: @border;
}

headerbar.ide-header button.destructive,
headerbar.ide-header button.destructive label {
    color: #e88989;
}

combobox.project-selector {
    min-width: 210px;
}

headerbar.ide-header label {
    color: @text;
    font-weight: bold;
}

notebook.workspace {
    background-color: @app_bg;
}

notebook.workspace > header {
    background-color: @panel_bg;
    border-bottom: 1px solid @border;
    padding: 0 8px;
}

notebook.workspace tab {
    min-height: 40px;
    padding: 0 18px;
    color: @muted;
    font-size: 11pt;
    font-weight: bold;
}

notebook.workspace tab:checked {
    color: @text;
    border-bottom: 2px solid @accent;
}

.sidebar {
    background-color: @panel_bg;
    border-right: 1px solid @border;
}

.sidebar-title {
    padding: 16px 16px 9px;
    color: @text;
    font-size: 11pt;
    font-weight: bold;
}

.list-panel {
    background-color: @panel_bg;
}

.list-panel row {
    min-height: 42px;
    margin: 2px 8px;
    padding: 0 10px;
    border-radius: 6px;
    border-left: 3px solid transparent;
}

.list-panel row:hover {
    background-color: @elevated_bg;
}

.list-panel row:selected {
    color: #ffffff;
    background-color: #302a45;
    border-left-color: @accent;
}

.toolbar {
    padding: 10px;
    background-color: @elevated_bg;
    border-top: 1px solid @border;
}

.toolbar button {
    min-height: 30px;
    font-size: 10.5pt;
    border: 1px solid #4c5662;
    border-radius: 5px;
    background-color: #303740;
    background-image: none;
    color: #f4f5f7;
    box-shadow: none;
    text-shadow: none;
}

.toolbar button:hover {
    background-color: #3b4551;
    border-color: #758292;
}

.toolbar button,
.toolbar button label {
    color: #f4f5f7;
    text-shadow: none;
}

.search-field {
    margin: 8px 10px 4px;
    padding: 5px 8px;
    font-size: 11pt;
    border: 1px solid @border;
    border-radius: 5px;
    background-color: @app_bg;
    color: @text;
}

.status-bar {
    padding: 6px 12px;
    background-color: @panel_bg;
    color: @text;
    border-top: 1px solid @border;
    font-size: 10pt;
}

.status-bar label {
    color: @text;
}

.toolbar button.icon-button {
    min-width: 30px;
    padding: 0;
}

.toolbar button.destructive {
    color: #ffaaaa;
}

.toolbar button.destructive label {
    color: #ffaaaa;
}

.toolbar combobox button {
    min-height: 30px;
    border-radius: 5px;
    background-color: #252a31;
    color: @text;
}

popover {
    background-color: @panel_bg;
    border: 1px solid @border;
}

popover button {
    min-height: 32px;
    padding: 4px 12px;
    background-color: #303740;
    background-image: none;
    border: 1px solid #4c5662;
    border-radius: 5px;
    color: #f4f5f7;
    text-shadow: none;
}

popover button label {
    color: #f4f5f7;
}

popover button:hover {
    background-color: #3b4551;
    border-color: #758292;
}

.editor-surface {
    background-color: @editor_bg;
}

textview.editor {
    padding: 28px 34px;
    background-color: @editor_bg;
    color: @text;
    caret-color: @accent;
    font-family: serif;
    font-size: 13pt;
}

textview.editor text {
    background-color: @editor_bg;
    color: @text;
    font-family: serif;
    font-size: 13pt;
}

scrollbar slider {
    min-width: 7px;
    min-height: 7px;
    background-color: #3b424c;
    border-radius: 4px;
}

scrollbar trough {
    background-color: @editor_bg;
}
"#;

fn apply_ide_theme() {
    let provider = gtk::CssProvider::new();
    provider
        .load_from_data(IDE_CSS.as_bytes())
        .expect("failed to load application theme");
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("no default display screen"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn prompt(
    window: &ApplicationWindow,
    title: &str,
    initial: &str,
    placeholder: &str,
) -> Option<String> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("OK", gtk::ResponseType::Ok),
        ],
    );
    let entry = Entry::new();
    entry.set_text(initial);
    entry.set_placeholder_text(Some(placeholder));
    entry.set_margin(8);
    dialog.content_area().add(&entry);
    dialog.set_default_response(gtk::ResponseType::Ok);
    dialog.content_area().show_all();
    let result = (dialog.run() == gtk::ResponseType::Ok)
        .then(|| entry.text().to_string())
        .filter(|s| !s.trim().is_empty());
    dialog.close();
    result
}

fn show_error(window: &ApplicationWindow, context: &str, error: &dyn std::fmt::Display) {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::Close,
        &format!("{}: {}", context, error),
    );
    dialog.run();
    dialog.close();
}

fn show_info(window: &ApplicationWindow, message: &str) {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Info,
        gtk::ButtonsType::Close,
        message,
    );
    dialog.run();
    dialog.close();
}

fn editor_content(view: &TextView) -> io::Result<String> {
    let buffer = view
        .buffer()
        .ok_or_else(|| io::Error::other("editor buffer is unavailable"))?;
    let (start, end) = buffer.bounds();
    buffer
        .text(&start, &end, false)
        .map(|text| text.to_string())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "could not read editor text"))
}

fn save_chapter(
    view: &TextView,
    store: &Rc<RefCell<ChapterStore>>,
    current: &Rc<RefCell<Option<usize>>>,
    dirty: &Cell<bool>,
) -> io::Result<()> {
    if dirty.get() {
        if let Some(index) = *current.borrow() {
            store
                .borrow_mut()
                .save_chapter_content(index, &editor_content(view)?)?;
            dirty.set(false);
        }
    }
    Ok(())
}

fn save_lore(
    view: &TextView,
    store: &Rc<RefCell<LoreStore>>,
    current: &Rc<RefCell<Option<usize>>>,
    dirty: &Cell<bool>,
) -> io::Result<()> {
    if dirty.get() {
        if let Some(index) = *current.borrow() {
            store
                .borrow_mut()
                .save_content(index, &editor_content(view)?)?;
            dirty.set(false);
        }
    }
    Ok(())
}

fn update_dirty_indicator(
    window: &ApplicationWindow,
    status: &Label,
    chapter_dirty: &Cell<bool>,
    lore_dirty: &Cell<bool>,
) {
    let dirty = chapter_dirty.get() || lore_dirty.get();
    let title = window
        .title()
        .map(|title| title.to_string())
        .unwrap_or_else(|| "Book App".to_string());
    let base = title.trim_end_matches(" •");
    status.set_text(&format!(
        "{}  ·  {}",
        base,
        if dirty { "Unsaved changes" } else { "Saved" }
    ));
    if dirty {
        window.set_title(&format!("{} •", base));
    } else {
        window.set_title(base);
    }
}

fn save_chapter_with_feedback(
    window: &ApplicationWindow,
    status: &Label,
    view: &TextView,
    store: &Rc<RefCell<ChapterStore>>,
    current: &Rc<RefCell<Option<usize>>>,
    dirty: &Cell<bool>,
    other_dirty: &Cell<bool>,
) -> bool {
    match save_chapter(view, store, current, dirty) {
        Ok(()) => {
            update_dirty_indicator(window, status, dirty, other_dirty);
            true
        }
        Err(error) => {
            show_error(window, "Could not save chapter", &error);
            false
        }
    }
}

fn save_lore_with_feedback(
    window: &ApplicationWindow,
    status: &Label,
    view: &TextView,
    store: &Rc<RefCell<LoreStore>>,
    current: &Rc<RefCell<Option<usize>>>,
    dirty: &Cell<bool>,
    other_dirty: &Cell<bool>,
) -> bool {
    match save_lore(view, store, current, dirty) {
        Ok(()) => {
            update_dirty_indicator(window, status, other_dirty, dirty);
            true
        }
        Err(error) => {
            show_error(window, "Could not save lore entry", &error);
            false
        }
    }
}

fn set_window_title(
    window: &ApplicationWindow,
    project: &Project,
    document: Option<&str>,
    dirty: bool,
) {
    let mut title = project.name.clone();
    if let Some(document) = document {
        title.push_str(" — ");
        title.push_str(document);
    }
    if dirty {
        title.push_str(" •");
    }
    window.set_title(&title);
}

fn row_matches_query(row: &ListBoxRow, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    row.child()
        .and_then(|child| child.downcast::<Label>().ok())
        .map(|label| matches_search(&label.text(), query))
        .unwrap_or(false)
}

fn matches_search(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(&query.to_lowercase())
}

fn category_for_row(row: &ListBoxRow) -> Option<String> {
    let text = row.child()?.downcast::<Label>().ok()?.text().to_string();
    Category::all()
        .into_iter()
        .find(|category| text.ends_with(&format!(" ({})", category.as_str())))
        .map(Category::as_str)
        .map(str::to_string)
}

fn refresh_chapters(list: &ListBox, store: &ChapterStore) {
    for row in list.children() {
        list.remove(&row);
    }
    for chapter in store.chapters() {
        let row = ListBoxRow::new();
        let label = Label::new(Some(&chapter.title));
        label.set_xalign(0.0);
        label.set_margin(6);
        row.add(&label);
        list.add(&row);
    }
    list.show_all();
}

fn refresh_lore(list: &ListBox, store: &LoreStore) {
    for row in list.children() {
        list.remove(&row);
    }
    for entry in store.entries() {
        let row = ListBoxRow::new();
        let label = Label::new(Some(&format!(
            "{} ({})",
            entry.title,
            entry.category.as_str()
        )));
        label.set_xalign(0.0);
        label.set_margin(6);
        row.add(&label);
        list.add(&row);
    }
    list.show_all();
}

fn choose_item(window: &ApplicationWindow, title: &str, labels: &[String]) -> Option<usize> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Open", gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(540, 360);
    let list = ListBox::new();
    list.style_context().add_class("list-panel");
    for label in labels {
        let row = ListBoxRow::new();
        let text = Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_margin(12);
        text.set_line_wrap(true);
        row.add(&text);
        list.add(&row);
    }
    let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.add(&list);
    dialog.content_area().pack_start(&scroll, true, true, 0);
    list.connect_row_activated(glib::clone!(@weak dialog => move |_, _| {
        dialog.response(gtk::ResponseType::Ok);
    }));
    dialog.show_all();
    let selected = if dialog.run() == gtk::ResponseType::Ok {
        list.selected_row().map(|row| row.index() as usize)
    } else {
        None
    };
    dialog.close();
    selected
}

struct DocumentMatch {
    chapter: bool,
    index: usize,
    label: String,
}

fn match_excerpt(title: &str, content: &str, query: &str) -> Option<String> {
    if let Some((index, line)) = content
        .lines()
        .enumerate()
        .find(|(_, line)| matches_search(line, query))
    {
        return Some(format!(
            "line {}: {}",
            index + 1,
            line.trim().chars().take(100).collect::<String>()
        ));
    }
    matches_search(title, query).then(|| "title match".to_string())
}

fn search_project(
    query: &str,
    chapters: &ChapterStore,
    lore: &LoreStore,
    current_chapter: Option<usize>,
    current_lore: Option<usize>,
    chapter_editor: &TextView,
    lore_editor: &TextView,
) -> io::Result<Vec<DocumentMatch>> {
    let mut results = Vec::new();
    for (index, chapter) in chapters.chapters().iter().enumerate() {
        let content = if current_chapter == Some(index) {
            editor_content(chapter_editor)?
        } else {
            chapter.load_content()?
        };
        if let Some(excerpt) = match_excerpt(&chapter.title, &content, query) {
            results.push(DocumentMatch {
                chapter: true,
                index,
                label: format!("Chapter: {} — {}", chapter.title, excerpt),
            });
        }
    }
    for (index, entry) in lore.entries().iter().enumerate() {
        let content = if current_lore == Some(index) {
            editor_content(lore_editor)?
        } else {
            lore.load_content(index)?
        };
        if let Some(excerpt) = match_excerpt(&entry.title, &content, query) {
            results.push(DocumentMatch {
                chapter: false,
                index,
                label: format!("{}: {} — {}", entry.category.as_str(), entry.title, excerpt),
            });
        }
    }
    Ok(results)
}

fn heading_lines(content: &str) -> Vec<(usize, String)> {
    content
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            let trimmed = text.trim_start();
            let title = trimmed.trim_start_matches('#');
            (title != trimmed && title.starts_with(char::is_whitespace) && !title.trim().is_empty())
                .then(|| (line, title.trim().to_string()))
        })
        .collect()
}

fn word_count(content: &str) -> usize {
    content.split_whitespace().count()
}

fn set_editor_text(view: &TextView, text: &str, loading: &Cell<bool>, previous: &Cell<usize>) {
    loading.set(true);
    view.buffer().unwrap().set_text(text);
    previous.set(word_count(text));
    loading.set(false);
}

fn update_word_status(
    label: &Label,
    view: &TextView,
    session_words: &Cell<usize>,
    goal: &Cell<Option<usize>>,
) {
    let count = editor_content(view).map(|text| word_count(&text));
    let session = session_words.get();
    let goal_text = goal
        .get()
        .map(|target| format!(" / {} goal", target))
        .unwrap_or_default();
    match count {
        Ok(count) => label.set_text(&format!(
            "{} words  ·  Session: {}{}",
            count, session, goal_text
        )),
        Err(error) => label.set_text(&format!("Word count unavailable: {}", error)),
    }
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Book App")
        .default_width(1000)
        .default_height(650)
        .build();
    apply_ide_theme();
    window.style_context().add_class("ide-window");
    let loaded_projects = match load_projects(BOOK_DIR) {
        Ok(projects) => projects,
        Err(error) => {
            window.show_all();
            show_error(&window, "Could not load projects", &error);
            return;
        }
    };
    let first_project = loaded_projects[0].clone();
    let loaded_chapters = match ChapterStore::load_or_init(&first_project.path.to_string_lossy()) {
        Ok(chapters) => chapters,
        Err(error) => {
            window.show_all();
            show_error(&window, "Could not load chapters", &error);
            return;
        }
    };
    let loaded_lore = match LoreStore::load(&first_project.path) {
        Ok(lore) => lore,
        Err(error) => {
            window.show_all();
            show_error(&window, "Could not load lore", &error);
            return;
        }
    };
    let projects = Rc::new(RefCell::new(loaded_projects));
    let chapter_store = Rc::new(RefCell::new(loaded_chapters));
    let lore_store = Rc::new(RefCell::new(loaded_lore));
    let current_project = Rc::new(RefCell::new(0usize));
    let current_chapter = Rc::new(RefCell::new(None::<usize>));
    let current_lore = Rc::new(RefCell::new(None::<usize>));
    let header = HeaderBar::builder().show_close_button(true).build();
    header.style_context().add_class("ide-header");
    let project_selector = ComboBoxText::new();
    project_selector
        .style_context()
        .add_class("project-selector");
    for project in projects.borrow().iter() {
        project_selector.append_text(&project.name);
    }
    project_selector.set_active(Some(0));
    let new_project = Button::with_label("New Project");
    new_project.style_context().add_class("header-action");
    let delete_project = Button::with_label("Remove Project");
    delete_project.style_context().add_class("destructive");
    delete_project.set_sensitive(false);
    let writing_menu = MenuButton::new();
    writing_menu.set_label("Writing Tools");
    let popover = gtk::Popover::new(Some(&writing_menu));
    let menu_items = GtkBox::new(Orientation::Vertical, 4);
    menu_items.set_margin(8);
    let find_all = Button::with_label("Find in Project");
    let outline = Button::with_label("Chapter Outline");
    let goal_button = Button::with_label("Set Session Goal");
    let focus_button = Button::with_label("Focus Mode");
    let revisions_button = Button::with_label("Recover Revision");
    let restore_button = Button::with_label("Restore Project");
    for button in [
        &find_all,
        &outline,
        &goal_button,
        &focus_button,
        &revisions_button,
        &restore_button,
    ] {
        menu_items.pack_start(button, false, false, 0);
    }
    popover.add(&menu_items);
    menu_items.show_all();
    writing_menu.set_popover(Some(&popover));
    header.pack_start(&project_selector);
    header.pack_start(&new_project);
    header.pack_start(&delete_project);
    header.pack_end(&writing_menu);
    window.set_titlebar(Some(&header));

    let notebook = Notebook::new();
    notebook.style_context().add_class("workspace");
    let chapter_list = ListBox::new();
    chapter_list.style_context().add_class("list-panel");
    let chapter_sidebar = GtkBox::new(Orientation::Vertical, 4);
    chapter_sidebar.style_context().add_class("sidebar");
    let chapter_title = Label::new(Some("Chapters"));
    chapter_title.set_xalign(0.0);
    chapter_title.style_context().add_class("sidebar-title");
    chapter_sidebar.pack_start(&chapter_title, false, false, 0);
    let chapter_search = SearchEntry::new();
    chapter_search.style_context().add_class("search-field");
    chapter_sidebar.pack_start(&chapter_search, false, false, 0);
    let chapter_scroll = ScrolledWindow::builder().min_content_width(220).build();
    chapter_scroll.add(&chapter_list);
    chapter_sidebar.pack_start(&chapter_scroll, true, true, 0);
    let chapter_toolbar = GtkBox::new(Orientation::Horizontal, 4);
    chapter_toolbar.style_context().add_class("toolbar");
    let new_chapter = Button::with_label("New");
    let rename_chapter = Button::with_label("Rename");
    let delete_chapter = Button::with_label("Delete");
    let up = Button::with_label("↑");
    let down = Button::with_label("↓");
    delete_chapter.style_context().add_class("destructive");
    up.style_context().add_class("icon-button");
    down.style_context().add_class("icon-button");
    for button in [&new_chapter, &rename_chapter, &delete_chapter, &up, &down] {
        chapter_toolbar.pack_start(button, true, true, 0);
    }
    chapter_sidebar.pack_start(&chapter_toolbar, false, false, 0);
    let chapter_editor = TextView::new();
    chapter_editor.style_context().add_class("editor");
    chapter_editor.set_wrap_mode(gtk::WrapMode::Word);
    let chapter_pane = Paned::new(Orientation::Horizontal);
    let editor_scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    editor_scroll.style_context().add_class("editor-surface");
    editor_scroll.add(&chapter_editor);
    chapter_pane.pack1(&chapter_sidebar, false, false);
    chapter_pane.pack2(&editor_scroll, true, false);
    notebook.append_page(&chapter_pane, Some(&Label::new(Some("Chapters"))));

    let lore_list = ListBox::new();
    lore_list.style_context().add_class("list-panel");
    let lore_sidebar = GtkBox::new(Orientation::Vertical, 4);
    lore_sidebar.style_context().add_class("sidebar");
    let lore_title = Label::new(Some("Lore & Characters"));
    lore_title.set_xalign(0.0);
    lore_title.style_context().add_class("sidebar-title");
    lore_sidebar.pack_start(&lore_title, false, false, 0);
    let lore_search = SearchEntry::new();
    lore_search.style_context().add_class("search-field");
    lore_sidebar.pack_start(&lore_search, false, false, 0);
    let lore_scroll = ScrolledWindow::builder().min_content_width(260).build();
    lore_scroll.add(&lore_list);
    lore_sidebar.pack_start(&lore_scroll, true, true, 0);
    let lore_toolbar = GtkBox::new(Orientation::Horizontal, 4);
    lore_toolbar.style_context().add_class("toolbar");
    let new_lore = Button::with_label("New");
    let rename_lore = Button::with_label("Rename");
    let delete_lore = Button::with_label("Delete");
    delete_lore.style_context().add_class("destructive");
    for button in [&new_lore, &rename_lore, &delete_lore] {
        lore_toolbar.pack_start(button, true, true, 0);
    }
    lore_sidebar.pack_start(&lore_toolbar, false, false, 0);
    let lore_editor = TextView::new();
    lore_editor.style_context().add_class("editor");
    lore_editor.set_wrap_mode(gtk::WrapMode::Word);
    let lore_pane = Paned::new(Orientation::Horizontal);
    let lore_editor_scroll =
        ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    lore_editor_scroll
        .style_context()
        .add_class("editor-surface");
    lore_editor_scroll.add(&lore_editor);
    lore_pane.pack1(&lore_sidebar, false, false);
    lore_pane.pack2(&lore_editor_scroll, true, false);
    notebook.append_page(&lore_pane, Some(&Label::new(Some("Lore & Characters"))));

    let save = Button::with_label("Save");
    save.style_context().add_class("header-action");
    header.pack_end(&save);
    let status = Label::new(Some("Saved"));
    status.set_xalign(0.0);
    let words = Label::new(None);
    words.set_xalign(1.0);
    let status_row = GtkBox::new(Orientation::Horizontal, 12);
    status_row.style_context().add_class("status-bar");
    status_row.pack_start(&status, true, true, 0);
    status_row.pack_end(&words, false, false, 0);
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.pack_start(&notebook, true, true, 0);
    root.pack_start(&status_row, false, false, 0);
    window.add(&root);
    refresh_chapters(&chapter_list, &chapter_store.borrow());
    refresh_lore(&lore_list, &lore_store.borrow());

    let chapter_dirty = Rc::new(Cell::new(false));
    let lore_dirty = Rc::new(Cell::new(false));
    let session_words = Rc::new(Cell::new(0usize));
    let session_goal = Rc::new(Cell::new(None::<usize>));
    let chapter_loading = Rc::new(Cell::new(false));
    let lore_loading = Rc::new(Cell::new(false));
    let previous_chapter_words = Rc::new(Cell::new(0usize));
    let previous_lore_words = Rc::new(Cell::new(0usize));
    let chapter_query = Rc::new(RefCell::new(String::new()));
    let lore_query = Rc::new(RefCell::new(String::new()));
    chapter_list.set_filter_func(Some(Box::new(glib::clone!(@strong chapter_query =>
        move |row| row_matches_query(row, &chapter_query.borrow())
    ))));
    lore_list.set_filter_func(Some(Box::new(glib::clone!(@strong lore_query =>
        move |row| row_matches_query(row, &lore_query.borrow())
    ))));
    lore_list.set_header_func(Some(Box::new(|row, before| {
        let category = category_for_row(row);
        let previous_category = before.and_then(category_for_row);
        if category.is_some() && category != previous_category {
            let heading = Label::new(category.as_deref());
            heading.set_xalign(0.0);
            heading.style_context().add_class("sidebar-title");
            row.set_header(Some(&heading));
        } else {
            row.set_header(None::<&Label>);
        }
    })));
    chapter_search.connect_search_changed(
        glib::clone!(@strong chapter_query, @weak chapter_list =>
        move |search| {
            chapter_query.replace(search.text().to_lowercase());
            chapter_list.invalidate_filter();
        }),
    );
    lore_search.connect_search_changed(glib::clone!(@strong lore_query, @weak lore_list =>
    move |search| {
        lore_query.replace(search.text().to_lowercase());
        lore_list.invalidate_filter();
    }));
    chapter_editor.buffer().unwrap().connect_changed(
        glib::clone!(@strong chapter_dirty, @weak window, @weak status, @strong lore_dirty,
        @strong chapter_loading, @strong previous_chapter_words, @strong session_words,
        @strong session_goal, @weak chapter_editor, @weak words, @weak notebook =>
        move |_| {
            if chapter_loading.get() { return; }
            chapter_dirty.set(true);
            update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
            let count = editor_content(&chapter_editor).map(|text| word_count(&text));
            if let Ok(count) = count {
                session_words.set(session_words.get() + count.saturating_sub(previous_chapter_words.get()));
                previous_chapter_words.set(count);
            }
            if notebook.current_page() == Some(0) {
                update_word_status(&words, &chapter_editor, &session_words, &session_goal);
            }
        }),
    );
    lore_editor.buffer().unwrap().connect_changed(
        glib::clone!(@strong lore_dirty, @weak window, @weak status, @strong chapter_dirty,
        @strong lore_loading, @strong previous_lore_words, @strong session_words,
        @strong session_goal, @weak lore_editor, @weak words, @weak notebook =>
        move |_| {
            if lore_loading.get() { return; }
            lore_dirty.set(true);
            update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
            let count = editor_content(&lore_editor).map(|text| word_count(&text));
            if let Ok(count) = count {
                session_words.set(session_words.get() + count.saturating_sub(previous_lore_words.get()));
                previous_lore_words.set(count);
            }
            if notebook.current_page() == Some(1) {
                update_word_status(&words, &lore_editor, &session_words, &session_goal);
            }
        }),
    );

    let category = ComboBoxText::new();
    for c in Category::all() {
        category.append_text(c.as_str());
    }
    category.set_active(Some(0));
    lore_toolbar.pack_start(&category, true, true, 0);

    chapter_list.connect_row_selected(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @strong projects, @strong current_project,
    @strong chapter_loading, @strong previous_chapter_words, @strong session_words,
    @strong session_goal, @weak words, @weak chapter_editor, @weak chapter_list, @weak window, @weak status =>
    move |_, row| {
        let previous = *current_chapter.borrow();
        if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
            &current_chapter, &chapter_dirty, &lore_dirty) {
            if let Some(index) = previous {
                chapter_list.select_row(chapter_list.row_at_index(index as i32).as_ref());
            }
            return;
        }
        if let Some(row) = row {
            let index = row.index() as usize;
            let loaded = chapter_store.borrow().chapters()[index].load_content();
            match loaded {
                Ok(text) => {
                    set_editor_text(&chapter_editor, &text, &chapter_loading, &previous_chapter_words);
                    chapter_dirty.set(false);
                    *current_chapter.borrow_mut() = Some(index);
                    let project = projects.borrow()[*current_project.borrow()].clone();
                    let title = chapter_store.borrow().chapters()[index].title.clone();
                    set_window_title(&window, &project, Some(&title),
                        chapter_dirty.get() || lore_dirty.get());
                    update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
                    update_word_status(&words, &chapter_editor, &session_words, &session_goal);
                }
                Err(error) => {
                    show_error(&window, "Could not load chapter", &error);
                    if let Some(index) = previous {
                        chapter_list.select_row(chapter_list.row_at_index(index as i32).as_ref());
                    }
                }
            }
        }
    }));
    lore_list.connect_row_selected(glib::clone!(@strong lore_store, @strong current_lore,
    @strong chapter_dirty, @strong lore_dirty, @strong projects, @strong current_project,
    @strong lore_loading, @strong previous_lore_words, @strong session_words, @strong session_goal,
    @weak words, @weak lore_editor, @weak lore_list, @weak window, @weak status =>
    move |_, row| {
        let previous = *current_lore.borrow();
        if !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
            &current_lore, &lore_dirty, &chapter_dirty) {
            if let Some(index) = previous {
                lore_list.select_row(lore_list.row_at_index(index as i32).as_ref());
            }
            return;
        }
        if let Some(row) = row {
            let index = row.index() as usize;
            let loaded = lore_store.borrow().load_content(index);
            match loaded {
                Ok(text) => {
                    set_editor_text(&lore_editor, &text, &lore_loading, &previous_lore_words);
                    lore_dirty.set(false);
                    *current_lore.borrow_mut() = Some(index);
                    let project = projects.borrow()[*current_project.borrow()].clone();
                    let title = lore_store.borrow().entries()[index].title.clone();
                    set_window_title(&window, &project, Some(&title),
                        chapter_dirty.get() || lore_dirty.get());
                    update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
                    update_word_status(&words, &lore_editor, &session_words, &session_goal);
                }
                Err(error) => {
                    show_error(&window, "Could not load lore entry", &error);
                    if let Some(index) = previous {
                        lore_list.select_row(lore_list.row_at_index(index as i32).as_ref());
                    }
                }
            }
        }
    }));

    new_project.connect_clicked(glib::clone!(@strong projects, @strong chapter_store, @strong lore_store,
        @strong current_project, @strong current_chapter, @strong current_lore, @strong chapter_dirty, @strong lore_dirty,
        @weak chapter_editor, @weak lore_editor, @weak project_selector, @weak window, @weak status =>
        move |_| {
            if let Some(name) = prompt(&window, "New Project", "", "Project name") {
                if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                    &current_chapter, &chapter_dirty, &lore_dirty)
                    || !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                        &current_lore, &lore_dirty, &chapter_dirty) {
                    return;
                }
                let created = match create_project(BOOK_DIR, &name) {
                    Ok(project) => project,
                    Err(error) => {
                        show_error(&window, "Could not create project", &error);
                        return;
                    }
                };
                let updated = match load_projects(BOOK_DIR) {
                    Ok(projects) => projects,
                    Err(error) => {
                        show_error(&window, "Project was created but could not be reloaded", &error);
                        return;
                    }
                };
                let current_path = projects.borrow()[*current_project.borrow()].path.clone();
                let Some(current_index) = project_index(&updated, &current_path) else {
                    let error = io::Error::new(
                        io::ErrorKind::NotFound,
                        "the current project is missing from the refreshed project list",
                    );
                    show_error(&window, "Could not refresh projects", &error);
                    return;
                };
                let Some(created_index) = project_index(&updated, &created.path) else {
                    let error = io::Error::new(
                        io::ErrorKind::NotFound,
                        "the new project is missing from the refreshed project list",
                    );
                    show_error(&window, "Could not select new project", &error);
                    return;
                };
                current_project.replace(current_index);
                projects.replace(updated);
                project_selector.remove_all();
                for project in projects.borrow().iter() {
                    project_selector.append_text(&project.name);
                }
                project_selector.set_active(Some(created_index as u32));
            }
        }));
    project_selector.connect_changed(
        glib::clone!(@strong projects, @strong chapter_store, @strong lore_store,
        @strong current_project, @strong current_chapter, @strong current_lore,
        @strong chapter_dirty, @strong lore_dirty, @strong chapter_query, @strong lore_query,
        @strong chapter_loading, @strong lore_loading, @strong previous_chapter_words, @strong previous_lore_words,
        @strong session_words, @strong session_goal, @weak words,
        @weak chapter_list, @weak lore_list, @weak chapter_editor, @weak lore_editor,
        @weak chapter_search, @weak lore_search, @weak project_selector, @weak notebook,
        @weak delete_project, @weak window, @weak status =>
        move |selector| {
            let Some(index) = selector.active().map(|index| index as usize) else { return };
            if index >= projects.borrow().len() || index == *current_project.borrow() { return; }
            let previous_project = *current_project.borrow();
            if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                &current_chapter, &chapter_dirty, &lore_dirty)
                || !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                    &current_lore, &lore_dirty, &chapter_dirty) {
                selector.set_active(Some(previous_project as u32));
                return;
            }
            let project = projects.borrow()[index].clone();
            let loaded_chapters = ChapterStore::load_or_init(&project.path.to_string_lossy());
            let loaded_lore = LoreStore::load(&project.path);
            let (loaded_chapters, loaded_lore) = match (loaded_chapters, loaded_lore) {
                (Ok(chapters), Ok(lore)) => (chapters, lore),
                (Err(error), _) | (_, Err(error)) => {
                    show_error(&window, "Could not open project", &error);
                    selector.set_active(Some(previous_project as u32));
                    return;
                }
            };
            *current_project.borrow_mut() = index;
            delete_project.set_sensitive(index != 0);
            *current_chapter.borrow_mut() = None;
            *current_lore.borrow_mut() = None;
            chapter_store.replace(loaded_chapters);
            lore_store.replace(loaded_lore);
            refresh_chapters(&chapter_list, &chapter_store.borrow());
            refresh_lore(&lore_list, &lore_store.borrow());
            chapter_search.set_text("");
            lore_search.set_text("");
            chapter_query.replace(String::new());
            lore_query.replace(String::new());
            chapter_list.invalidate_filter();
            lore_list.invalidate_filter();
            set_editor_text(&chapter_editor, "", &chapter_loading, &previous_chapter_words);
            set_editor_text(&lore_editor, "", &lore_loading, &previous_lore_words);
            chapter_dirty.set(false);
            lore_dirty.set(false);
            if let Some(row) = chapter_list.row_at_index(0) {
                chapter_list.select_row(Some(&row));
            }
            if let Some(row) = lore_list.row_at_index(0) {
                lore_list.select_row(Some(&row));
            }
            let active_project = projects.borrow()[index].clone();
            let document = if notebook.current_page() == Some(0) {
                current_chapter
                    .borrow()
                    .and_then(|i| chapter_store.borrow().chapters().get(i)
                        .map(|chapter| chapter.title.clone()))
            } else {
                current_lore
                    .borrow()
                    .and_then(|i| lore_store.borrow().entries().get(i)
                        .map(|entry| entry.title.clone()))
            };
            set_window_title(&window, &active_project, document.as_deref(), false);
            update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
            let view = if notebook.current_page() == Some(0) { &chapter_editor } else { &lore_editor };
            update_word_status(&words, view, &session_words, &session_goal);
        }),
    );

    new_chapter.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @weak chapter_editor, @weak status,
    @weak chapter_list, @weak chapter_search, @strong chapter_query, @weak window =>
    move |_| {
        if let Some(title) = prompt(&window, "New Chapter", "", "Chapter title") {
            if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                &current_chapter, &chapter_dirty, &lore_dirty) { return; }
            if let Err(error) = chapter_store.borrow_mut().add_chapter(&title) {
                show_error(&window, "Could not create chapter", &error);
                return;
            }
            chapter_search.set_text("");
            chapter_query.replace(String::new());
            chapter_list.invalidate_filter();
            refresh_chapters(&chapter_list, &chapter_store.borrow());
            let i = chapter_store.borrow().chapters().len() - 1;
            chapter_list.select_row(chapter_list.row_at_index(i as i32).as_ref());
        }
    }));
    rename_chapter.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @weak chapter_list, @weak chapter_search,
    @strong chapter_query, @weak chapter_editor, @weak window, @weak status =>
    move |_| {
        if let Some(i) = *current_chapter.borrow() {
            let old = chapter_store.borrow().chapters()[i].title.clone();
            if let Some(title) = prompt(&window, "Rename Chapter", &old, "Chapter title") {
                if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                    &current_chapter, &chapter_dirty, &lore_dirty) { return; }
                if let Err(error) = chapter_store.borrow_mut().rename_chapter(i, &title) {
                    show_error(&window, "Could not rename chapter", &error);
                    return;
                }
                chapter_search.set_text("");
                chapter_query.replace(String::new());
                chapter_list.invalidate_filter();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                chapter_list.select_row(chapter_list.row_at_index(i as i32).as_ref());
            }
        }
    }));
    delete_chapter.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @weak chapter_list, @weak chapter_search,
    @strong chapter_query, @strong chapter_loading, @strong previous_chapter_words,
    @strong session_words, @strong session_goal, @weak words, @weak chapter_editor, @weak window, @weak status =>
    move |_| {
        if let Some(i) = *current_chapter.borrow() {
            let title = chapter_store.borrow().chapters()[i].title.clone();
            if prompt(&window, &format!("Type '{}' to delete", title), "", &title).as_deref()
                == Some(title.as_str()) {
                if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                    &current_chapter, &chapter_dirty, &lore_dirty) { return; }
                if let Err(error) = chapter_store.borrow_mut().delete_chapter(i) {
                    show_error(&window, "Could not delete chapter", &error);
                    return;
                }
                *current_chapter.borrow_mut() = None;
                set_editor_text(&chapter_editor, "", &chapter_loading, &previous_chapter_words);
                chapter_dirty.set(false);
                chapter_search.set_text("");
                chapter_query.replace(String::new());
                chapter_list.invalidate_filter();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                let remaining = chapter_store.borrow().chapters().len();
                if remaining > 0 {
                    let next = i.min(remaining - 1);
                    chapter_list.select_row(chapter_list.row_at_index(next as i32).as_ref());
                }
                update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
                update_word_status(&words, &chapter_editor, &session_words, &session_goal);
            }
        }
    }));
    up.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @weak chapter_list, @weak chapter_search,
    @strong chapter_query, @weak chapter_editor, @weak window, @weak status =>
    move |_| {
        if let Some(i) = *current_chapter.borrow() {
            if i > 0 {
                if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                    &current_chapter, &chapter_dirty, &lore_dirty) { return; }
                if let Err(error) = chapter_store.borrow_mut().move_up(i) {
                    show_error(&window, "Could not move chapter", &error);
                    return;
                }
                *current_chapter.borrow_mut() = None;
                chapter_search.set_text("");
                chapter_query.replace(String::new());
                chapter_list.invalidate_filter();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                chapter_list.select_row(chapter_list.row_at_index((i - 1) as i32).as_ref());
            }
        }
    }));
    down.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong chapter_dirty, @strong lore_dirty, @weak chapter_list, @weak chapter_search,
    @strong chapter_query, @weak chapter_editor, @weak window, @weak status =>
    move |_| {
        let i = match *current_chapter.borrow() { Some(i) => i, None => return };
        if i + 1 < chapter_store.borrow().chapters().len() {
            if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                &current_chapter, &chapter_dirty, &lore_dirty) { return; }
            if let Err(error) = chapter_store.borrow_mut().move_down(i) {
                show_error(&window, "Could not move chapter", &error);
                return;
            }
            *current_chapter.borrow_mut() = None;
            chapter_search.set_text("");
            chapter_query.replace(String::new());
            chapter_list.invalidate_filter();
            refresh_chapters(&chapter_list, &chapter_store.borrow());
            chapter_list.select_row(chapter_list.row_at_index((i + 1) as i32).as_ref());
        }
    }));

    new_lore.connect_clicked(
        glib::clone!(@strong lore_store, @weak category, @weak lore_list,
        @weak lore_search, @strong lore_query, @strong current_lore, @strong lore_dirty,
        @strong chapter_dirty, @weak lore_editor, @weak status, @weak window =>
        move |_| {
            if let Some(title) = prompt(&window, "New Lore Entry", "", "Entry title") {
                if !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                    &current_lore, &lore_dirty, &chapter_dirty) { return; }
                let c = Category::all()[category.active().unwrap_or(0) as usize];
                if let Err(error) = lore_store.borrow_mut().add(c, &title) {
                    show_error(&window, "Could not create lore entry", &error);
                    return;
                }
                lore_search.set_text("");
                lore_query.replace(String::new());
                lore_list.invalidate_filter();
                refresh_lore(&lore_list, &lore_store.borrow());
                if let Some(index) = lore_store.borrow().entries().iter()
                    .position(|entry| entry.category == c && entry.title == title) {
                    lore_list.select_row(lore_list.row_at_index(index as i32).as_ref());
                }
            }
        }),
    );
    rename_lore.connect_clicked(glib::clone!(@strong lore_store, @strong current_lore,
    @strong chapter_dirty, @strong lore_dirty, @weak lore_list, @weak lore_search,
    @strong lore_query, @weak lore_editor,
    @weak window, @weak status =>
    move |_| {
        if let Some(i) = *current_lore.borrow() {
            let old = lore_store.borrow().entries()[i].title.clone();
            if let Some(title) = prompt(&window, "Rename Lore Entry", &old, "Entry title") {
                if !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                    &current_lore, &lore_dirty, &chapter_dirty) { return; }
                if let Err(error) = lore_store.borrow_mut().rename(i, &title) {
                    show_error(&window, "Could not rename lore entry", &error);
                    return;
                }
                lore_search.set_text("");
                lore_query.replace(String::new());
                lore_list.invalidate_filter();
                refresh_lore(&lore_list, &lore_store.borrow());
                lore_list.select_row(lore_list.row_at_index(i as i32).as_ref());
            }
        }
    }));
    delete_lore.connect_clicked(glib::clone!(@strong lore_store, @strong current_lore,
    @strong chapter_dirty, @strong lore_dirty, @weak lore_list, @weak lore_search,
    @strong lore_query, @strong lore_loading, @strong previous_lore_words,
    @strong session_words, @strong session_goal, @weak words, @weak lore_editor,
    @weak window, @weak status =>
    move |_| {
        if let Some(i) = *current_lore.borrow() {
            if !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                &current_lore, &lore_dirty, &chapter_dirty) { return; }
            if let Err(error) = lore_store.borrow_mut().delete(i) {
                show_error(&window, "Could not delete lore entry", &error);
                return;
            }
            *current_lore.borrow_mut() = None;
            set_editor_text(&lore_editor, "", &lore_loading, &previous_lore_words);
            lore_dirty.set(false);
            lore_search.set_text("");
            lore_query.replace(String::new());
            lore_list.invalidate_filter();
            refresh_lore(&lore_list, &lore_store.borrow());
            if !lore_store.borrow().entries().is_empty() {
                let next = i.min(lore_store.borrow().entries().len() - 1);
                lore_list.select_row(lore_list.row_at_index(next as i32).as_ref());
            }
            update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
            update_word_status(&words, &lore_editor, &session_words, &session_goal);
        }
    }));
    save.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong lore_store, @strong current_lore, @strong chapter_dirty, @strong lore_dirty,
    @weak chapter_editor, @weak lore_editor, @weak window, @weak status =>
    move |_| {
        save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
            &current_chapter, &chapter_dirty, &lore_dirty);
        save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
            &current_lore, &lore_dirty, &chapter_dirty);
    }));
    window.connect_key_press_event(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong lore_store, @strong current_lore, @strong chapter_dirty, @strong lore_dirty,
    @weak chapter_editor, @weak lore_editor, @weak window, @weak status =>
    @default-return glib::Propagation::Proceed, move |_, key| {
        if key.state().contains(gdk::ModifierType::CONTROL_MASK)
            && key.keyval() == gdk::keys::constants::s {
            save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
                &current_chapter, &chapter_dirty, &lore_dirty);
            save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                &current_lore, &lore_dirty, &chapter_dirty);
            glib::Propagation::Stop
        } else { glib::Propagation::Proceed }
    }));
    window.connect_delete_event(glib::clone!(@strong chapter_store, @strong current_chapter,
    @strong lore_store, @strong current_lore, @strong chapter_dirty, @strong lore_dirty,
    @weak chapter_editor, @weak lore_editor, @weak window, @weak status =>
    @default-return glib::Propagation::Proceed,
    move |_, _| {
        let chapter_saved = save_chapter_with_feedback(&window, &status, &chapter_editor,
            &chapter_store, &current_chapter, &chapter_dirty, &lore_dirty);
        let lore_saved = save_lore_with_feedback(&window, &status, &lore_editor,
            &lore_store, &current_lore, &lore_dirty, &chapter_dirty);
        if chapter_saved && lore_saved {
            glib::Propagation::Proceed
        } else {
            glib::Propagation::Stop
        }
    }));
    notebook.connect_switch_page(glib::clone!(@strong projects, @strong current_project,
    @strong chapter_store, @strong current_chapter, @strong lore_store, @strong current_lore,
    @strong chapter_dirty, @strong lore_dirty, @strong session_words, @strong session_goal,
    @weak chapter_editor, @weak lore_editor, @weak words, @weak window, @weak status =>
    move |_, _, page| {
        let project = projects.borrow()[*current_project.borrow()].clone();
        let document = if page == 0 {
            current_chapter.borrow().and_then(|i| chapter_store.borrow().chapters()
                .get(i).map(|chapter| chapter.title.clone()))
        } else {
            current_lore.borrow().and_then(|i| lore_store.borrow().entries()
                .get(i).map(|entry| entry.title.clone()))
        };
        set_window_title(&window, &project, document.as_deref(),
            chapter_dirty.get() || lore_dirty.get());
        update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
        let view = if page == 0 { &chapter_editor } else { &lore_editor };
        update_word_status(&words, view, &session_words, &session_goal);
    }));

    find_all.connect_clicked(glib::clone!(@strong chapter_store, @strong lore_store,
    @strong current_chapter, @strong current_lore, @strong chapter_query, @strong lore_query,
    @weak chapter_search, @weak lore_search, @weak chapter_list, @weak lore_list,
    @weak chapter_editor, @weak lore_editor, @weak notebook, @weak popover, @weak window =>
    move |_| {
        popover.popdown();
        let Some(query) = prompt(&window, "Find in Project", "", "Search titles and contents") else {
            return;
        };
        let results = match search_project(query.trim(), &chapter_store.borrow(), &lore_store.borrow(),
            *current_chapter.borrow(), *current_lore.borrow(), &chapter_editor, &lore_editor) {
            Ok(results) => results,
            Err(error) => {
                show_error(&window, "Could not search project", &error);
                return;
            }
        };
        if results.is_empty() {
            show_info(&window, "No matches in this project.");
            return;
        }
        let labels: Vec<String> = results.iter().map(|result| result.label.clone()).collect();
        if let Some(result) = choose_item(&window, "Search Results", &labels).and_then(|index| results.get(index)) {
            if result.chapter {
                chapter_search.set_text("");
                chapter_query.replace(String::new());
                chapter_list.invalidate_filter();
                notebook.set_current_page(Some(0));
                chapter_list.select_row(chapter_list.row_at_index(result.index as i32).as_ref());
                chapter_editor.grab_focus();
            } else {
                lore_search.set_text("");
                lore_query.replace(String::new());
                lore_list.invalidate_filter();
                notebook.set_current_page(Some(1));
                lore_list.select_row(lore_list.row_at_index(result.index as i32).as_ref());
                lore_editor.grab_focus();
            }
        }
    }));
    outline.connect_clicked(glib::clone!(@strong current_chapter, @weak chapter_editor,
    @weak notebook, @weak popover, @weak window =>
    move |_| {
        popover.popdown();
        if current_chapter.borrow().is_none() {
            show_info(&window, "Select a chapter to view its outline.");
            return;
        }
        let content = match editor_content(&chapter_editor) {
            Ok(text) => text,
            Err(error) => {
                show_error(&window, "Could not read chapter", &error);
                return;
            }
        };
        let headings = heading_lines(&content);
        if headings.is_empty() {
            show_info(&window, "Add Markdown-style headings (for example, # Scene) to this chapter to build an outline.");
            return;
        }
        let labels: Vec<String> = headings.iter()
            .map(|(line, title)| format!("{}: {}", line + 1, title)).collect();
        if let Some((line, _)) = choose_item(&window, "Chapter Outline", &labels)
            .and_then(|index| headings.get(index)) {
            notebook.set_current_page(Some(0));
            let buffer = chapter_editor.buffer().unwrap();
            let mut iter = buffer.iter_at_line(*line as i32);
            buffer.place_cursor(&iter);
            chapter_editor.scroll_to_iter(&mut iter, 0.15, false, 0.0, 0.0);
            chapter_editor.grab_focus();
        }
    }));
    goal_button.connect_clicked(glib::clone!(@strong session_words, @strong session_goal,
    @weak chapter_editor, @weak lore_editor, @weak notebook, @weak words, @weak popover, @weak window =>
    move |_| {
        popover.popdown();
        let Some(text) = prompt(&window, "Session Goal", "", "Words to add this session") else {
            return;
        };
        match text.trim().parse::<usize>() {
            Ok(target) if target > 0 => {
                session_words.set(0);
                session_goal.set(Some(target));
                let view = if notebook.current_page() == Some(0) { &chapter_editor } else { &lore_editor };
                update_word_status(&words, view, &session_words, &session_goal);
            }
            _ => show_error(&window, "Invalid session goal", &"Enter a positive whole number"),
        }
    }));
    focus_button.connect_clicked(glib::clone!(@weak chapter_sidebar, @weak lore_sidebar,
    @weak popover => move |button| {
        popover.popdown();
        if chapter_sidebar.is_visible() {
            chapter_sidebar.hide();
            lore_sidebar.hide();
            button.set_label("Show Sidebars");
        } else {
            chapter_sidebar.show();
            lore_sidebar.show();
            button.set_label("Focus Mode");
        }
    }));
    revisions_button.connect_clicked(glib::clone!(@strong chapter_store, @strong lore_store,
    @strong current_chapter, @strong current_lore, @strong chapter_dirty, @strong lore_dirty,
    @strong chapter_loading, @strong lore_loading, @strong previous_chapter_words,
    @strong previous_lore_words, @strong session_words, @strong session_goal,
    @weak chapter_editor, @weak lore_editor, @weak notebook, @weak words,
    @weak popover, @weak window, @weak status =>
    move |_| {
        popover.popdown();
        let chapter = notebook.current_page() == Some(0);
        let index = if chapter { *current_chapter.borrow() } else { *current_lore.borrow() };
        let Some(index) = index else {
            show_info(&window, "Select a document to recover a revision.");
            return;
        };
        let saved = if chapter {
            save_chapter_with_feedback(
                &window,
                &status,
                &chapter_editor,
                &chapter_store,
                &current_chapter,
                &chapter_dirty,
                &lore_dirty,
            )
        } else {
            save_lore_with_feedback(
                &window,
                &status,
                &lore_editor,
                &lore_store,
                &current_lore,
                &lore_dirty,
                &chapter_dirty,
            )
        };
        if !saved {
            return;
        }
        let revisions = if chapter {
            chapter_store.borrow().list_revisions(index)
        } else {
            lore_store.borrow().list_revisions(index)
        };
        let revisions = match revisions {
            Ok(revisions) => revisions,
            Err(error) => {
                show_error(&window, "Could not list revisions", &error);
                return;
            }
        };
        if revisions.is_empty() {
            show_info(&window, "This document has no saved revisions yet.");
            return;
        }
        let labels: Vec<_> = revisions.iter().enumerate()
            .map(|(index, path)| format!(
                "Revision {}{} — {}",
                index + 1,
                if index == 0 { " (newest)" } else { "" },
                path.file_name().unwrap_or_default().to_string_lossy()
            )).collect();
        let Some(revision) = choose_item(&window, "Recover Revision", &labels)
            .and_then(|selected| revisions.get(selected)) else {
            return;
        };
        let restored = if chapter {
            chapter_store.borrow_mut().restore_revision(index, revision)
        } else {
            lore_store.borrow_mut().restore_revision(index, revision)
        };
        if let Err(error) = restored {
            show_error(&window, "Could not recover revision", &error);
            return;
        }
        let content = if chapter {
            chapter_store.borrow().chapters()[index].load_content()
        } else {
            lore_store.borrow().load_content(index)
        };
        match content {
            Ok(content) => {
                if chapter {
                    set_editor_text(&chapter_editor, &content, &chapter_loading, &previous_chapter_words);
                    chapter_dirty.set(false);
                } else {
                    set_editor_text(&lore_editor, &content, &lore_loading, &previous_lore_words);
                    lore_dirty.set(false);
                }
                update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
                let view = if chapter { &chapter_editor } else { &lore_editor };
                update_word_status(&words, view, &session_words, &session_goal);
            }
            Err(error) => show_error(&window, "Revision was restored but could not be opened", &error),
        }
    }));
    restore_button.connect_clicked(glib::clone!(@strong projects, @strong current_project,
    @weak project_selector, @weak popover, @weak window => move |_| {
        popover.popdown();
        let trashed = match list_trashed_projects(BOOK_DIR) {
            Ok(projects) => projects,
            Err(error) => {
                show_error(&window, "Could not list deleted projects", &error);
                return;
            }
        };
        if trashed.is_empty() {
            show_info(&window, "There are no deleted projects to restore.");
            return;
        }
        let labels: Vec<_> = trashed.iter()
            .map(|project| format!(
                "{} — {}",
                project.name,
                project.path.file_name().unwrap_or_default().to_string_lossy()
            )).collect();
        let Some(project) = choose_item(&window, "Restore Project", &labels)
            .and_then(|index| trashed.get(index)) else {
            return;
        };
        if let Err(error) = restore_project(BOOK_DIR, project) {
            show_error(&window, "Could not restore project", &error);
            return;
        }
        let current_path = projects.borrow()[*current_project.borrow()].path.clone();
        let updated = match load_projects(BOOK_DIR) {
            Ok(updated) => updated,
            Err(error) => {
                show_error(&window, "Project was restored but could not be reloaded", &error);
                return;
            }
        };
        let Some(index) = project_index(&updated, &current_path) else {
            show_error(&window, "Could not refresh projects", &"Current project is missing");
            return;
        };
        projects.replace(updated);
        current_project.replace(index);
        project_selector.remove_all();
        for project in projects.borrow().iter() {
            project_selector.append_text(&project.name);
        }
        project_selector.set_active(Some(index as u32));
    }));

    delete_project.connect_clicked(glib::clone!(@strong projects, @strong current_project,
    @strong chapter_store, @strong lore_store, @strong current_chapter, @strong current_lore,
    @strong chapter_dirty, @strong lore_dirty, @weak project_selector,
    @weak chapter_editor, @weak lore_editor, @weak window, @weak status =>
    move |_| {
        let selected_index = *current_project.borrow();
        if selected_index == 0 {
            return;
        }
        let project = projects.borrow()[selected_index].clone();
        let confirmation = prompt(
            &window,
            &format!("Type '{}' to move this project to Trash", project.name),
            "",
            "Project name",
        );
        if confirmation.as_deref().map(str::trim) != Some(project.name.as_str()) {
            return;
        }
        if !save_chapter_with_feedback(&window, &status, &chapter_editor, &chapter_store,
            &current_chapter, &chapter_dirty, &lore_dirty)
            || !save_lore_with_feedback(&window, &status, &lore_editor, &lore_store,
                &current_lore, &lore_dirty, &chapter_dirty) {
            return;
        }

        project_selector.set_active(Some(0));
        if *current_project.borrow() != 0 {
            return;
        }
        if let Err(error) = remove_project(BOOK_DIR, &project) {
            show_error(&window, "Could not remove project", &error);
            return;
        }

        let updated: Vec<Project> = projects
            .borrow()
            .iter()
            .filter(|candidate| candidate.path != project.path)
            .cloned()
            .collect();
        current_project.replace(0);
        projects.replace(updated);
        project_selector.remove_all();
        for project in projects.borrow().iter() {
            project_selector.append_text(&project.name);
        }
        project_selector.set_active(Some(0));
    }));
    if let Some(row) = chapter_list.row_at_index(0) {
        chapter_list.select_row(Some(&row));
    }
    if let Some(row) = lore_list.row_at_index(0) {
        lore_list.select_row(Some(&row));
    }
    let active_project = projects.borrow()[*current_project.borrow()].clone();
    let document = if notebook.current_page() == Some(0) {
        current_chapter.borrow().and_then(|i| {
            chapter_store
                .borrow()
                .chapters()
                .get(i)
                .map(|chapter| chapter.title.clone())
        })
    } else {
        current_lore.borrow().and_then(|i| {
            lore_store
                .borrow()
                .entries()
                .get(i)
                .map(|entry| entry.title.clone())
        })
    };
    set_window_title(
        &window,
        &active_project,
        document.as_deref(),
        chapter_dirty.get() || lore_dirty.get(),
    );
    update_dirty_indicator(&window, &status, &chapter_dirty, &lore_dirty);
    update_word_status(&words, &chapter_editor, &session_words, &session_goal);
    window.show_all();
}

fn main() {
    let app = Application::builder()
        .application_id("com.njacobs.bookapp")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

#[cfg(test)]
mod tests {
    use super::{
        heading_lines, match_excerpt, matches_search, refresh_chapters, row_matches_query,
        save_chapter, search_project, word_count, IDE_CSS,
    };
    use crate::chapters::ChapterStore;
    use crate::lore::{Category, LoreStore};
    use gtk::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;

    #[test]
    fn search_is_case_insensitive_and_matches_lore_category_text() {
        assert!(matches_search("The Silver Forest", "silver"));
        assert!(matches_search("Ada (Character)", "character"));
        assert!(!matches_search("Ada (Character)", "environment"));
        assert!(matches_search("anything", ""));
    }

    #[test]
    fn gtk_stylesheet_parses_successfully() {
        gtk::init().expect("GTK initialization is required to test the stylesheet");
        let provider = gtk::CssProvider::new();
        assert!(provider.load_from_data(IDE_CSS.as_bytes()).is_ok());

        let dir = PathBuf::from(format!(".bookapp_test_ui_search_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let chapters = Rc::new(RefCell::new(
            ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap(),
        ));
        chapters.borrow_mut().add_chapter("Next").unwrap();
        chapters
            .borrow_mut()
            .save_chapter_content(1, "A meteor fell")
            .unwrap();
        let list = gtk::ListBox::new();
        refresh_chapters(&list, &chapters.borrow());
        list.set_filter_func(Some(Box::new(|row| row_matches_query(row, "next"))));
        list.invalidate_filter();
        assert!(!list.row_at_index(0).unwrap().is_child_visible());
        list.set_filter_func(Some(Box::new(|_| true)));
        list.invalidate_filter();
        list.select_row(list.row_at_index(1).as_ref());
        assert_eq!(list.selected_row().unwrap().index(), 1);
        let mut lore = LoreStore::load(&dir).unwrap();
        lore.add(Category::Character, "Ada").unwrap();
        lore.save_content(0, "The traveler").unwrap();
        let chapter_editor = gtk::TextView::new();
        let lore_editor = gtk::TextView::new();
        chapter_editor.buffer().unwrap().set_text("Unsaved draft");
        lore_editor.buffer().unwrap().set_text("");

        let results = search_project(
            "DRAFT",
            &chapters.borrow(),
            &lore,
            Some(0),
            None,
            &chapter_editor,
            &lore_editor,
        )
        .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].chapter);
        assert_eq!(results[0].index, 0);
        let results = search_project(
            "METEOR",
            &chapters.borrow(),
            &lore,
            Some(0),
            None,
            &chapter_editor,
            &lore_editor,
        )
        .unwrap();
        assert_eq!(results[0].index, 1);
        let results = search_project(
            "TRAVELER",
            &chapters.borrow(),
            &lore,
            Some(0),
            None,
            &chapter_editor,
            &lore_editor,
        )
        .unwrap();
        assert!(!results[0].chapter);

        fs::remove_dir_all(&dir).unwrap();
        assert!(search_project(
            "anything",
            &chapters.borrow(),
            &lore,
            Some(0),
            None,
            &chapter_editor,
            &lore_editor,
        )
        .is_err());
        let dirty = Cell::new(true);
        let current = Rc::new(RefCell::new(Some(0)));
        assert!(save_chapter(&chapter_editor, &chapters, &current, &dirty).is_err());
        assert!(
            dirty.get(),
            "failed save must preserve the unsaved indicator"
        );
    }

    #[test]
    fn project_search_finds_text_and_title() {
        assert_eq!(
            match_excerpt("Opening", "A secret passage\nHidden stairs", "HIDDEN"),
            Some("line 2: Hidden stairs".to_string())
        );
        assert_eq!(
            match_excerpt("Opening", "No match", "OPEN"),
            Some("title match".to_string())
        );
        assert_eq!(match_excerpt("Opening", "No match", "missing"), None);
    }

    #[test]
    fn outline_and_word_count_follow_editor_text() {
        assert_eq!(
            heading_lines("# Opening\nBody text\n  ## Second scene\n#\nNot a heading"),
            vec![(0, "Opening".to_string()), (2, "Second scene".to_string())]
        );
        assert_eq!(word_count("one  two\nthree"), 3);
    }
}
