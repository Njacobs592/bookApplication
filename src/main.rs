mod chapters;
mod lore;
mod projects;

use chapters::ChapterStore;
use gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Entry, HeaderBar, Label,
    ListBox, ListBoxRow, Notebook, Orientation, Paned, ScrolledWindow, TextView,
};
use lore::{Category, LoreStore};
use projects::{create_project, load_projects};
use std::cell::RefCell;
use std::rc::Rc;

const BOOK_DIR: &str = "book";

const IDE_CSS: &str = r#"
@define-color app_bg #1e1f22;
@define-color panel_bg #252629;
@define-color elevated_bg #2b2d30;
@define-color border #3b3d41;
@define-color text #d7dae0;
@define-color muted #8f939b;
@define-color accent #4f8cc9;

window {
    background-color: @app_bg;
    color: @text;
}

headerbar.ide-header {
    min-height: 46px;
    padding: 5px 10px;
    background-color: @elevated_bg;
    border-bottom: 1px solid @border;
}

headerbar.ide-header button,
headerbar.ide-header combobox button {
    min-height: 30px;
    padding: 0 10px;
}

headerbar.ide-header button.primary {
    color: #ffffff;
    background-color: @accent;
}

combobox.project-selector {
    min-width: 190px;
}

notebook.workspace {
    background-color: @app_bg;
}

notebook.workspace > header {
    background-color: @panel_bg;
    border-bottom: 1px solid @border;
}

notebook.workspace tab {
    min-height: 34px;
    padding: 0 16px;
    color: @muted;
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
    padding: 14px 16px 8px;
    color: @muted;
    font-size: 0.9em;
    font-weight: bold;
}

.list-panel {
    background-color: @panel_bg;
}

.list-panel row {
    min-height: 36px;
    padding: 0 8px;
    border-left: 3px solid transparent;
}

.list-panel row:hover {
    background-color: @elevated_bg;
}

.list-panel row:selected {
    color: #ffffff;
    background-color: #365a8a;
    border-left-color: @accent;
}

.toolbar {
    padding: 8px;
    background-color: @elevated_bg;
    border-top: 1px solid @border;
}

.toolbar button {
    min-height: 30px;
}

.toolbar button.icon-button {
    min-width: 30px;
    padding: 0;
}

.toolbar button.destructive {
    color: #e88989;
}

.editor-surface {
    background-color: @app_bg;
}

textview.editor {
    padding: 20px;
    background-color: @app_bg;
    color: @text;
    caret-color: @accent;
}

textview.editor text {
    background-color: @app_bg;
    color: @text;
}

scrollbar slider {
    min-width: 8px;
    min-height: 8px;
    background-color: #555960;
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

fn build_ui(app: &Application) {
    let projects = Rc::new(RefCell::new(
        load_projects(BOOK_DIR).expect("failed to load projects"),
    ));
    let chapter_store = Rc::new(RefCell::new(
        ChapterStore::load_or_init(projects.borrow()[0].path.to_str().unwrap())
            .expect("failed to load chapters"),
    ));
    let lore_store = Rc::new(RefCell::new(
        LoreStore::load(&projects.borrow()[0].path).expect("failed to load lore"),
    ));
    let current_project = Rc::new(RefCell::new(0usize));
    let current_chapter = Rc::new(RefCell::new(None::<usize>));
    let current_lore = Rc::new(RefCell::new(None::<usize>));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Book App")
        .default_width(1000)
        .default_height(650)
        .build();
    apply_ide_theme();
    window.style_context().add_class("ide-window");
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
    new_project.style_context().add_class("primary");
    header.pack_start(&project_selector);
    header.pack_start(&new_project);
    window.set_titlebar(Some(&header));

    let notebook = Notebook::new();
    notebook.style_context().add_class("workspace");
    let chapter_list = ListBox::new();
    chapter_list.style_context().add_class("list-panel");
    let chapter_sidebar = GtkBox::new(Orientation::Vertical, 4);
    chapter_sidebar.style_context().add_class("sidebar");
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
    save.style_context().add_class("primary");
    header.pack_end(&save);
    window.add(&notebook);
    refresh_chapters(&chapter_list, &chapter_store.borrow());
    refresh_lore(&lore_list, &lore_store.borrow());

    fn save_chapter(
        view: &TextView,
        store: &Rc<RefCell<ChapterStore>>,
        current: &Rc<RefCell<Option<usize>>>,
    ) {
        if let Some(index) = *current.borrow() {
            let buffer = view.buffer().unwrap();
            let (start, end) = buffer.bounds();
            let text = buffer
                .text(&start, &end, false)
                .map(|s| s.to_string())
                .unwrap_or_default();
            store
                .borrow_mut()
                .save_chapter_content(index, &text)
                .expect("failed to save chapter");
        }
    }
    fn save_lore(
        view: &TextView,
        store: &Rc<RefCell<LoreStore>>,
        current: &Rc<RefCell<Option<usize>>>,
    ) {
        if let Some(index) = *current.borrow() {
            let buffer = view.buffer().unwrap();
            let (start, end) = buffer.bounds();
            let text = buffer
                .text(&start, &end, false)
                .map(|s| s.to_string())
                .unwrap_or_default();
            store
                .borrow_mut()
                .save_content(index, &text)
                .expect("failed to save lore entry");
        }
    }

    chapter_list.connect_row_selected(glib::clone!(@strong chapter_store, @strong current_chapter, @weak chapter_editor =>
        move |_, row| {
            save_chapter(&chapter_editor, &chapter_store, &current_chapter);
            if let Some(row) = row {
                let index = row.index() as usize;
                let text = chapter_store.borrow().chapters()[index].load_content().unwrap_or_default();
                chapter_editor.buffer().unwrap().set_text(&text);
                *current_chapter.borrow_mut() = Some(index);
            }
        }));
    lore_list.connect_row_selected(
        glib::clone!(@strong lore_store, @strong current_lore, @weak lore_editor =>
        move |_, row| {
            save_lore(&lore_editor, &lore_store, &current_lore);
            if let Some(row) = row {
                let index = row.index() as usize;
                let text = lore_store.borrow().load_content(index).unwrap_or_default();
                lore_editor.buffer().unwrap().set_text(&text);
                *current_lore.borrow_mut() = Some(index);
            }
        }),
    );

    new_project.connect_clicked(glib::clone!(@strong projects, @weak project_selector, @weak window =>
        move |_| {
            if let Some(name) = prompt(&window, "New Project", "", "Project name") {
                if create_project(BOOK_DIR, &name).is_ok() {
                    let updated = load_projects(BOOK_DIR).expect("failed to reload projects");
                    projects.replace(updated);
                    project_selector.remove_all();
                    for project in projects.borrow().iter() { project_selector.append_text(&project.name); }
                    project_selector.set_active(Some((projects.borrow().len() - 1) as u32));
                }
            }
        }));
    project_selector.connect_changed(glib::clone!(@strong projects, @strong chapter_store, @strong lore_store,
        @strong current_project, @strong current_chapter, @strong current_lore, @weak chapter_list, @weak lore_list,
        @weak chapter_editor, @weak lore_editor => move |selector| {
            let index = selector.active().unwrap_or(0) as usize;
            if index >= projects.borrow().len() { return; }
            save_chapter(&chapter_editor, &chapter_store, &current_chapter);
            save_lore(&lore_editor, &lore_store, &current_lore);
            let project = projects.borrow()[index].clone();
            *current_project.borrow_mut() = index;
            *current_chapter.borrow_mut() = None;
            *current_lore.borrow_mut() = None;
            *chapter_store.borrow_mut() = ChapterStore::load_or_init(project.path.to_str().unwrap()).unwrap();
            *lore_store.borrow_mut() = LoreStore::load(&project.path).unwrap();
            refresh_chapters(&chapter_list, &chapter_store.borrow());
            refresh_lore(&lore_list, &lore_store.borrow());
            chapter_editor.buffer().unwrap().set_text("");
            lore_editor.buffer().unwrap().set_text("");
            if let Some(row) = chapter_list.row_at_index(0) { chapter_list.select_row(Some(&row)); }
        }));

    new_chapter.connect_clicked(
        glib::clone!(@strong chapter_store, @weak chapter_list, @weak window =>
        move |_| if let Some(title) = prompt(&window, "New Chapter", "", "Chapter title") {
            chapter_store.borrow_mut().add_chapter(&title).unwrap();
            refresh_chapters(&chapter_list, &chapter_store.borrow());
            let i = chapter_store.borrow().chapters().len() - 1;
            chapter_list.select_row(chapter_list.row_at_index(i as i32).as_ref());
        }),
    );
    rename_chapter.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter, @weak chapter_list, @weak chapter_editor, @weak window =>
        move |_| if let Some(i) = *current_chapter.borrow() {
            let old = chapter_store.borrow().chapters()[i].title.clone();
            if let Some(title) = prompt(&window, "Rename Chapter", &old, "Chapter title") {
                save_chapter(&chapter_editor, &chapter_store, &current_chapter);
                *current_chapter.borrow_mut() = None;
                chapter_store.borrow_mut().rename_chapter(i, &title).unwrap();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                chapter_list.select_row(chapter_list.row_at_index(i as i32).as_ref());
            }
        }));
    delete_chapter.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter, @weak chapter_list, @weak chapter_editor, @weak window =>
        move |_| if let Some(i) = *current_chapter.borrow() {
            let title = chapter_store.borrow().chapters()[i].title.clone();
            if prompt(&window, &format!("Type '{}' to delete", title), "", &title).as_deref() == Some(title.as_str()) {
                save_chapter(&chapter_editor, &chapter_store, &current_chapter);
                *current_chapter.borrow_mut() = None;
                chapter_store.borrow_mut().delete_chapter(i).unwrap();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                chapter_editor.buffer().unwrap().set_text("");
                let remaining = chapter_store.borrow().chapters().len();
                if let Some(row) = chapter_list.row_at_index(i.min(remaining.saturating_sub(1)) as i32) {
                    chapter_list.select_row(Some(&row));
                }
            }
        }));
    up.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter, @weak chapter_list, @weak chapter_editor =>
        move |_| if let Some(i) = *current_chapter.borrow() { if i > 0 {
            save_chapter(&chapter_editor, &chapter_store, &current_chapter);
            *current_chapter.borrow_mut() = None; chapter_store.borrow_mut().move_up(i).unwrap();
            refresh_chapters(&chapter_list, &chapter_store.borrow()); chapter_list.select_row(chapter_list.row_at_index((i - 1) as i32).as_ref());
        }}));
    down.connect_clicked(
        glib::clone!(@strong chapter_store, @strong current_chapter, @weak chapter_list, @weak chapter_editor =>
        move |_| {
            let i = match *current_chapter.borrow() { Some(i) => i, None => return };
            if i + 1 < chapter_store.borrow().chapters().len() {
                save_chapter(&chapter_editor, &chapter_store, &current_chapter);
                *current_chapter.borrow_mut() = None;
                chapter_store.borrow_mut().move_down(i).unwrap();
                refresh_chapters(&chapter_list, &chapter_store.borrow());
                chapter_list.select_row(chapter_list.row_at_index((i + 1) as i32).as_ref());
            }
        }),
    );

    let category = ComboBoxText::new();
    for c in Category::all() {
        category.append_text(c.as_str());
    }
    category.set_active(Some(0));
    lore_toolbar.pack_start(&category, true, true, 0);
    new_lore.connect_clicked(glib::clone!(@strong lore_store, @weak category, @weak lore_list, @weak window =>
        move |_| if let Some(title) = prompt(&window, "New Lore Entry", "", "Entry title") {
            let c = Category::all()[category.active().unwrap_or(0) as usize];
            lore_store.borrow_mut().add(c, &title).unwrap(); refresh_lore(&lore_list, &lore_store.borrow());
        }));
    rename_lore.connect_clicked(glib::clone!(@strong lore_store, @strong current_lore, @weak lore_list, @weak lore_editor, @weak window =>
        move |_| if let Some(i) = *current_lore.borrow() {
            let old = lore_store.borrow().entries()[i].title.clone();
            if let Some(title) = prompt(&window, "Rename Lore Entry", &old, "Entry title") {
                save_lore(&lore_editor, &lore_store, &current_lore);
                *current_lore.borrow_mut() = None; lore_store.borrow_mut().rename(i, &title).unwrap();
                refresh_lore(&lore_list, &lore_store.borrow()); lore_list.select_row(lore_list.row_at_index(i as i32).as_ref());
            }
        }));
    delete_lore.connect_clicked(glib::clone!(@strong lore_store, @strong current_lore, @weak lore_list, @weak lore_editor =>
        move |_| if let Some(i) = *current_lore.borrow() {
            save_lore(&lore_editor, &lore_store, &current_lore);
            *current_lore.borrow_mut() = None; lore_store.borrow_mut().delete(i).unwrap(); refresh_lore(&lore_list, &lore_store.borrow());
            lore_editor.buffer().unwrap().set_text("");
            if let Some(row) = lore_list.row_at_index(i.min(lore_store.borrow().entries().len().saturating_sub(1)) as i32) {
                lore_list.select_row(Some(&row));
            }
        }));
    save.connect_clicked(glib::clone!(@strong chapter_store, @strong current_chapter, @strong lore_store, @strong current_lore,
        @weak chapter_editor, @weak lore_editor, @weak notebook => move |_| {
            if notebook.current_page() == Some(0) { save_chapter(&chapter_editor, &chapter_store, &current_chapter); }
            else { save_lore(&lore_editor, &lore_store, &current_lore); }
        }));
    window.connect_key_press_event(glib::clone!(@strong chapter_store, @strong current_chapter, @strong lore_store, @strong current_lore,
        @weak chapter_editor, @weak lore_editor, @weak notebook => @default-return glib::Propagation::Proceed, move |_, key| {
            if key.state().contains(gdk::ModifierType::CONTROL_MASK) && key.keyval() == gdk::keys::constants::s {
                if notebook.current_page() == Some(0) { save_chapter(&chapter_editor, &chapter_store, &current_chapter); }
                else { save_lore(&lore_editor, &lore_store, &current_lore); }
                glib::Propagation::Stop
            } else { glib::Propagation::Proceed }
        }));
    if let Some(row) = chapter_list.row_at_index(0) {
        chapter_list.select_row(Some(&row));
    }
    window.show_all();
}

fn main() {
    let app = Application::builder()
        .application_id("com.njacobs.bookapp")
        .build();
    app.connect_activate(build_ui);
    app.run();
}
