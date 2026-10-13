use super::*;
use std::time::Instant;

fn wait_until(predicate: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        while context.pending() {
            context.iteration(false);
        }
        if predicate() {
            break;
        }
        assert!(Instant::now() < deadline, "GTK operation timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn gtk_workspace_writing_recovery_navigation_and_preferences() {
    gtk::init().expect("Run GTK interaction tests under xvfb-run");
    let errors = Rc::new(RefCell::new(Vec::<String>::new()));
    let captured = errors.clone();
    let dialog_monitor = glib::timeout_add_local(Duration::from_millis(20), move || {
        for window in gtk::Window::list_toplevels() {
            if let Ok(dialog) = window.downcast::<gtk::MessageDialog>() {
                if dialog.is_visible() {
                    captured
                        .borrow_mut()
                        .push(dialog.property::<String>("text"));
                    dialog.response(gtk::ResponseType::Close);
                }
            }
        }
        glib::ControlFlow::Continue
    });
    let dir = std::env::current_dir()
        .unwrap()
        .join(format!(".bookapp_test_workspace_{}", std::process::id()));
    assert!(
        !dir.exists(),
        "Test fixture already exists; do not overwrite it"
    );
    fs::create_dir_all(&dir).unwrap();
    let library = dir.join("library");
    let config = dir.join("config");
    std::env::set_var("BOOKAPP_STORAGE_DIR", &library);
    std::env::set_var("BOOKAPP_CONFIG_DIR", &config);
    let app = Application::builder()
        .application_id("com.njacobs.bookapp.tests")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    build_ui(&app);
    let w = TEST_WORKSPACE.with(|slot| slot.borrow().as_ref().unwrap().clone());
    wait_until(|| !w.busy.get() && w.chapter.path.borrow().is_some());
    assert!(w.chapter.view.is_editable());
    assert!(!w.buttons["up"].is_sensitive());
    assert!(!w.buttons["down"].is_sensitive());
    let first = w.chapter.path.borrow().clone().unwrap();
    let first_id = w.chapter.id.borrow().clone();
    let content = "# Opening\none two\n## Second scene\nthree";
    w.chapter.view.buffer().unwrap().set_text(content);
    w.observe(true);
    w.update_outline();
    assert_eq!(w.outline_headings.borrow().len(), 2);
    assert!(w.chapter.dirty.get());
    w.undo(false);
    assert_eq!(text(&w.chapter.view), "");
    w.undo(true);
    assert_eq!(text(&w.chapter.view), content);
    let large = format!("{content}\n{}", "word ".repeat(20_000));
    for suffix in ["a", "b", "c", "d", "final"] {
        w.chapter
            .view
            .buffer()
            .unwrap()
            .set_text(&format!("{large}{suffix}"));
    }
    wait_until(|| w.chapter.observed.borrow().ends_with("final"));
    assert!(features::word_count(&w.chapter.observed.borrow()) > 20_000);
    w.undo(false);
    assert_eq!(
        text(&w.chapter.view),
        content,
        "Rapid edits are grouped after idle"
    );

    w.chapter
        .last_edit
        .set(Instant::now() - Duration::from_secs(3));
    w.write_drafts();
    wait_until(|| !w.snapshot_busy.get());
    assert_eq!(
        documents::load_draft(&first).unwrap().unwrap().content,
        content
    );
    assert_eq!(fs::read_to_string(&first).unwrap(), "");
    assert!(crate::chapters::list_revisions_for(&first)
        .unwrap()
        .is_empty());
    let saved = Rc::new(Cell::new(false));
    let flag = saved.clone();
    w.with_saved(move || flag.set(true));
    wait_until(|| saved.get());
    assert_eq!(fs::read_to_string(&first).unwrap(), content);
    assert!(!w.chapter.dirty.get());
    assert!(documents::load_draft(&first).unwrap().is_none());
    w.find(false, "scene".into(), SearchOptions::default());
    wait_until(|| !w.busy.get() && !w.search_hits.borrow().is_empty());
    let buffer = w.chapter.view.buffer().unwrap();
    let (start, end) = buffer.selection_bounds().unwrap();
    assert_eq!(buffer.text(&start, &end, false).unwrap().as_str(), "scene");
    assert!(start
        .tags()
        .iter()
        .any(|tag| tag.name().as_deref() == Some("search-match")));
    w.next_match(false);
    let approve_preview = glib::timeout_add_local(Duration::from_millis(10), || {
        for window in gtk::Window::list_toplevels() {
            if let Ok(dialog) = window.downcast::<gtk::Dialog>() {
                if dialog.is_visible()
                    && dialog
                        .title()
                        .is_some_and(|title| title.starts_with("Replace "))
                {
                    dialog.response(gtk::ResponseType::Ok);
                }
            }
        }
        glib::ControlFlow::Continue
    });
    w.replace_project("one".into(), "once".into(), SearchOptions::default());
    wait_until(|| !w.busy.get() && fs::read_to_string(&first).unwrap().contains("once"));
    approve_preview.remove();
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        content.replace("one", "once")
    );
    assert_eq!(text(&w.chapter.view), content.replace("one", "once"));
    assert!(crate::chapters::list_revisions_for(&first).unwrap().len() >= 2);
    w.undo(false);
    assert_eq!(text(&w.chapter.view), content);
    let saved = Rc::new(Cell::new(false));
    let flag = saved.clone();
    w.with_saved(move || flag.set(true));
    wait_until(|| saved.get());

    w.edit_chapters(|store| Ok(Some(store.import_chapter("Second", "another scene")?)));
    wait_until(|| !w.busy.get() && w.chapters.borrow().chapters().len() == 2);
    w.navigate(true, first.clone(), None);
    wait_until(|| !w.busy.get() && w.chapter.path.borrow().as_ref() == Some(&first));
    w.undo(false);
    assert_eq!(
        text(&w.chapter.view),
        "",
        "Undo history survives document switches"
    );
    w.undo(true);
    let saved = Rc::new(Cell::new(false));
    let flag = saved.clone();
    w.with_saved(move || flag.set(true));
    wait_until(|| saved.get());
    let buffer = w.chapter.view.buffer().unwrap();
    buffer.place_cursor(&buffer.iter_at_offset(7));
    w.edit_chapters(|store| {
        store.rename_chapter(0, "Renamed")?;
        Ok(Some(0))
    });
    wait_until(|| !w.busy.get() && w.chapters.borrow().chapters()[0].title == "Renamed");
    assert_eq!(*w.chapter.id.borrow(), first_id);
    let buffer = w.chapter.view.buffer().unwrap();
    assert_eq!(
        buffer.iter_at_mark(&buffer.get_insert().unwrap()).offset(),
        7
    );
    assert_eq!(text(&w.chapter.view), content);
    w.move_chapter(0, 1);
    wait_until(|| !w.busy.get() && w.chapters.borrow().chapters()[1].title == "Renamed");
    assert_eq!(*w.chapter.id.borrow(), first_id);

    w.notebook.set_current_page(Some(1));
    wait_until(|| !w.busy.get());
    assert!(w.lore.path.borrow().is_none());
    assert!(!w.lore.view.is_editable());
    assert_eq!(w.lore.stack.visible_child_name().as_deref(), Some("empty"));
    w.edit_lore(|store| {
        store.add(Category::Character, "Ada")?;
        Ok(Some(0))
    });
    wait_until(|| !w.busy.get() && w.lore.path.borrow().is_some());
    w.action("template");
    assert!(text(&w.lore.view).contains('#'));
    w.plan["tags"].set_text("hero, protagonist, hero");
    w.plan["target"].set_text("100");
    w.save_plan().unwrap();
    let lore_path = w.lore.path.borrow().clone().unwrap();
    let metadata = features::load_metadata(&lore_path).unwrap();
    assert_eq!(metadata.tags, ["hero", "protagonist"]);
    assert_eq!(metadata.target, 100);
    let lore_id = metadata.id;
    let saved = Rc::new(Cell::new(false));
    let flag = saved.clone();
    w.with_saved(move || flag.set(true));
    wait_until(|| saved.get());
    w.edit_lore(|store| {
        store.recategorize(0, Category::Lore)?;
        Ok(Some(0))
    });
    wait_until(|| !w.busy.get() && w.lores.borrow().entries()[0].category == Category::Lore);
    assert_eq!(*w.lore.id.borrow(), lore_id);
    w.edit_lore(|store| {
        store.delete(0)?;
        Ok(None)
    });
    wait_until(|| !w.busy.get() && w.lores.borrow().entries().is_empty());
    assert!(!w.lore.view.is_editable());
    let trashed = w.lores.borrow().list_deleted().unwrap()[0].clone();
    w.edit_lore(move |store| Ok(Some(store.restore_deleted(&trashed)?)));
    wait_until(|| !w.busy.get() && w.lore.path.borrow().is_some());
    assert_eq!(*w.lore.id.borrow(), lore_id);
    assert!(text(&w.lore.view).contains('#'));

    w.state.borrow_mut().preferences.theme = "light".into();
    w.state.borrow_mut().preferences.font_size = 18;
    w.apply_appearance().unwrap();
    assert_eq!(
        w.chapter
            .view
            .pango_context()
            .font_description()
            .unwrap()
            .size()
            / gtk::pango::SCALE,
        18,
        "Configured font size must reach the actual editor, not just saved preferences"
    );
    w.persist().unwrap();
    assert!(config.join("workspace.json").is_file());
    let restored = features::load_workspace(&config, &library).unwrap();
    assert_eq!(restored.preferences.font_size, 18);
    assert_eq!(restored.preferences.theme, "light");
    assert_eq!(restored.lore, w.lore.path.borrow().clone());
    assert!(restored
        .cursors
        .contains_key(&w.lore.path.borrow().clone().unwrap()));
    w.closing.set(true);
    w.window.close();
    wait_until(|| !w.window.is_visible());
    assert!(app.active_window().is_none());
    build_ui(&app);
    let w = TEST_WORKSPACE.with(|slot| slot.borrow().as_ref().unwrap().clone());
    wait_until(|| !w.busy.get() && w.lore.path.borrow().is_some());
    assert_eq!(w.notebook.current_page(), Some(1));
    assert_eq!(*w.lore.id.borrow(), lore_id);
    assert_eq!(w.state.borrow().preferences.font_size, 18);
    assert_eq!(w.state.borrow().preferences.theme, "light");
    assert_eq!(w.plan["target"].text().as_str(), "100");

    // A failed save must leave edits in the editor, not clear or replace them.
    let path = w.lore.path.borrow().clone().unwrap();
    fs::write(&path, "external edits").unwrap();
    w.lore.view.buffer().unwrap().set_text("my edits");
    w.observe(false);
    assert!(documents::save_checked(&path, &w.lore.saved.borrow(), &text(&w.lore.view)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "external edits");
    assert!(w.lore.dirty.get());
    assert_eq!(text(&w.lore.view), "my edits");
    let completed = Rc::new(Cell::new(false));
    let flag = completed.clone();
    w.with_saved(move || flag.set(true));
    wait_until(|| !w.busy.get() && !errors.borrow().is_empty());
    assert!(
        !completed.get(),
        "Failed saves must block navigation and close"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "external edits");
    assert_eq!(text(&w.lore.view), "my edits");
    assert!(w.lore.dirty.get());
    assert_eq!(errors.borrow().len(), 1);
    assert!(errors
        .borrow_mut()
        .pop()
        .unwrap()
        .contains("Could not save"));
    // Keep shutdown noninteractive; save conflict handling itself was checked above.
    w.lore.saved.replace("my edits".into());
    w.lore.dirty.set(false);
    w.chapter.dirty.set(false);
    w.closing.set(true);
    w.window.close();
    TEST_WORKSPACE.with(|slot| slot.replace(None));
    std::env::remove_var("BOOKAPP_STORAGE_DIR");
    std::env::remove_var("BOOKAPP_CONFIG_DIR");
    fs::remove_dir_all(dir).unwrap();
    dialog_monitor.remove();
    assert!(
        errors.borrow().is_empty(),
        "Unexpected GTK errors: {:?}",
        errors.borrow()
    );
}

#[test]
fn dialog_names_and_tag_values_are_validated() {
    assert!(valid_name("").is_err());
    assert!(valid_name("../chapter").is_err());
    assert!(valid_name("line\nbreak").is_err());
    assert!(valid_name("A chapter — café").is_ok());
    assert_eq!(
        super::tools::comma_values("hero, hero, location, "),
        ["hero", "location"]
    );
}
