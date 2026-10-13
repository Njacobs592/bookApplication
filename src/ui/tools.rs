use super::*;

impl Workspace {
    pub(super) fn apply_appearance(&self) -> io::Result<()> {
        let prefs = &self.state.borrow().preferences;
        let dark = match prefs.theme.as_str() {
            "light" => false,
            "dark" => true,
            _ => gtk::Settings::default()
                .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme()),
        };
        let (background, panel, foreground, border, selection) = if dark {
            ("#14171b", "#1d2229", "#e5e7eb", "#424b59", "#483b68")
        } else {
            ("#fffdf8", "#eeeae2", "#242424", "#8c8780", "#d8cbf5")
        };
        let font = prefs.font.replace('\\', "\\\\").replace('"', "\\\"");
        let css = format!(
            r#"
            window, dialog, popover {{ background: {background}; color: {foreground}; }}
            headerbar, .sidebar, notebook > header {{ background: {panel}; color: {foreground}; }}
            label, entry, textview text {{ color: {foreground}; }}
            button, combobox button {{ background: {panel}; color: {foreground}; border: 1px solid {border}; padding: 6px; }}
            button:disabled {{ opacity: 0.5; }}
            button:focus, entry:focus {{ border: 2px solid #9a86d8; }}
            entry, textview, textview text {{ background: {background}; color: {foreground}; }}
            list row {{ padding: 2px; }}
            list row:selected {{ background: {selection}; color: {foreground}; }}
            textview.editor, textview.editor text {{ font-family: "{font}"; font-size: {}pt; }}
            .chapter-delete, .lore-delete, .remove-project {{ color: {}; }}
        "#,
            prefs.font_size,
            if dark { "#ffaaaa" } else { "#a02020" }
        );
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(css.as_bytes())
            .map_err(|e| io::Error::other(e.to_string()))?;
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().ok_or_else(|| io::Error::other("No display screen"))?,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        for view in [&self.chapter.view, &self.lore.view, &self.reference] {
            view.set_pixels_above_lines(prefs.line_spacing as i32);
            view.set_pixels_inside_wrap(prefs.line_spacing as i32);
            let margin = ((view.allocated_width() - prefs.text_width) / 2).max(24);
            view.set_left_margin(margin);
            view.set_right_margin(margin);
        }
        Ok(())
    }

    pub(super) fn settings(&self) {
        let prefs = self.state.borrow().preferences.clone();
        let dialog = gtk::Dialog::with_buttons(
            Some("Appearance and writing goals"),
            Some(&self.window),
            gtk::DialogFlags::MODAL,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Apply", gtk::ResponseType::Ok),
            ],
        );
        let grid = gtk::Grid::new();
        grid.set_row_spacing(12);
        grid.set_column_spacing(16);
        grid.set_margin(16);
        let font = gtk::FontButton::new();
        font.set_font(&format!("{} {}", prefs.font, prefs.font_size));
        let spacing = gtk::SpinButton::with_range(0.0, 32.0, 1.0);
        spacing.set_value(prefs.line_spacing as f64);
        let width = gtk::SpinButton::with_range(320.0, 1600.0, 40.0);
        width.set_value(prefs.text_width as f64);
        let theme = ComboBoxText::new();
        for value in ["system", "dark", "light"] {
            theme.append(Some(value), value);
        }
        theme.set_active_id(Some(&prefs.theme));
        let include = CheckButton::with_label("Include lore in writing goals");
        include.set_active(prefs.include_lore);
        let goal = gtk::SpinButton::with_range(0.0, 1_000_000.0, 100.0);
        goal.set_value(prefs.daily_goal as f64);
        for (row, (label, widget)) in [
            ("Editor font and size", font.clone().upcast::<gtk::Widget>()),
            ("Extra line spacing", spacing.clone().upcast()),
            ("Maximum text width (pixels)", width.clone().upcast()),
            ("Theme", theme.clone().upcast()),
            ("Daily net-word goal (0 disables)", goal.clone().upcast()),
            ("Goal scope", include.clone().upcast()),
        ]
        .into_iter()
        .enumerate()
        {
            grid.attach(&Label::new(Some(label)), 0, row as i32, 1, 1);
            grid.attach(&widget, 1, row as i32, 1, 1);
        }
        dialog.content_area().add(&grid);
        dialog.show_all();
        if dialog.run() == gtk::ResponseType::Ok {
            let desc = gtk::pango::FontDescription::from_string(
                &font.font().unwrap_or_else(|| "Serif 13".into()),
            );
            let mut state = self.state.borrow_mut();
            state.preferences.font = desc
                .family()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Serif".into());
            state.preferences.font_size = (desc.size() / gtk::pango::SCALE).clamp(8, 72) as u32;
            state.preferences.line_spacing = spacing.value_as_int() as u32;
            state.preferences.text_width = width.value_as_int();
            state.preferences.theme = theme.active_id().unwrap().to_string();
            state.preferences.include_lore = include.is_active();
            state.preferences.daily_goal = goal.value_as_int() as usize;
            drop(state);
            if let Err(cause) = self.apply_appearance().and_then(|_| self.persist()) {
                self.report("Could not apply preferences", &cause);
            }
            self.update_words();
        }
        dialog.close();
    }

    pub(super) fn update_outline(&self) {
        for child in self.outline.children() {
            self.outline.remove(&child);
        }
        let headings = features::headings(&text(&self.chapter.view));
        for heading in &headings {
            let row = ListBoxRow::new();
            let label = Label::new(Some(&heading.title));
            label.set_margin_start(12 + (heading.level.saturating_sub(1) * 14) as i32);
            label.set_margin_top(6);
            label.set_margin_bottom(6);
            label.set_xalign(0.0);
            label.set_line_wrap(true);
            row.add(&label);
            self.outline.add(&row);
        }
        if headings.is_empty() {
            let label = Label::new(Some(
                "Use # headings for chapters and ## headings for scenes.",
            ));
            label.set_line_wrap(true);
            label.set_margin(12);
            self.outline.add(&label);
        }
        self.outline.show_all();
        self.outline_headings.replace(headings);
        self.highlight_outline();
    }

    pub(super) fn highlight_outline(&self) {
        if self.chapter.loading.get() {
            return;
        }
        let buffer = self.chapter.view.buffer().unwrap();
        let line = buffer.iter_at_mark(&buffer.get_insert().unwrap()).line();
        let headings = self.outline_headings.borrow();
        let selected = headings.iter().rposition(|h| h.line <= line);
        self.outline.select_row(
            selected
                .and_then(|i| self.outline.row_at_index(i as i32))
                .as_ref(),
        );
    }

    pub(super) fn update_plan(&self) {
        let path = self.active().path.borrow().clone();
        if *self.plan_document.borrow() == path {
            return;
        }
        self.plan_document.replace(path.clone());
        let metadata = path
            .as_ref()
            .and_then(|path| self.metadata.borrow().get(path).cloned());
        let metadata = metadata.unwrap_or_default();
        for (key, value) in [
            ("pov", metadata.pov),
            ("timeline", metadata.timeline),
            ("status", metadata.status),
            ("tags", metadata.tags.join(", ")),
            ("links", metadata.links.join(", ")),
            ("target", metadata.target.to_string()),
        ] {
            self.plan[key].set_text(&value);
            self.plan[key].set_sensitive(path.is_some());
        }
        self.summary.buffer().unwrap().set_text(&metadata.summary);
        self.summary.set_editable(path.is_some());
    }

    pub(super) fn planning_dialog(&self) {
        self.update_plan();
        let page = self.tools.nth_page(Some(2)).unwrap();
        self.tools.remove_page(Some(2));
        let dialog = gtk::Dialog::with_buttons(
            Some("Lore planning, tags and links"),
            Some(&self.window),
            gtk::DialogFlags::MODAL,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Save planning", gtk::ResponseType::Ok),
            ],
        );
        dialog.set_default_size(550, 650);
        dialog.content_area().add(&page);
        dialog.show_all();
        loop {
            if dialog.run() != gtk::ResponseType::Ok {
                self.plan_document.replace(None);
                self.update_plan();
                break;
            }
            match self.save_plan() {
                Ok(()) => break,
                Err(cause) => self.report("Could not save planning", &cause),
            }
        }
        dialog.content_area().remove(&page);
        self.tools
            .append_page(&page, Some(&Label::new(Some("Plan"))));
        dialog.close();
    }

    pub(super) fn save_plan(&self) -> io::Result<()> {
        let Some(path) = self.plan_document.borrow().clone() else {
            return Ok(());
        };
        let mut metadata = self
            .metadata
            .borrow()
            .get(&path)
            .cloned()
            .ok_or_else(|| io::Error::other("The planning document is no longer available"))?;
        let target = self.plan["target"].text();
        metadata.target = target.trim().parse::<usize>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Word target must be a nonnegative whole number",
            )
        })?;
        metadata.pov = self.plan["pov"].text().to_string();
        metadata.timeline = self.plan["timeline"].text().to_string();
        metadata.status = self.plan["status"].text().to_string();
        metadata.summary = text(&self.summary);
        metadata.tags = comma_values(&self.plan["tags"].text());
        metadata.links = comma_values(&self.plan["links"].text());
        metadata.links.retain(|id| id != &metadata.id);
        features::save_metadata(&path, &metadata)?;
        self.metadata.borrow_mut().insert(path, metadata);
        Ok(())
    }

    pub(super) fn show_links(self: &Rc<Self>) {
        if let Err(cause) = self.save_plan() {
            self.report("Could not save links", &cause);
            return;
        }
        let id = self.active().id.borrow().clone();
        let path = self.active().path.borrow().clone();
        let Some(metadata) = path.and_then(|p| self.metadata.borrow().get(&p).cloned()) else {
            return;
        };
        let all = self.metadata.borrow();
        let mut targets: Vec<(Option<PathBuf>, String)> = Vec::new();
        for linked in &metadata.links {
            let target = all
                .iter()
                .find(|(_, meta)| &meta.id == linked)
                .map(|(path, _)| path.clone());
            let label = target
                .as_ref()
                .map(|p| format!("Link: {}", p.file_name().unwrap().to_string_lossy()))
                .unwrap_or_else(|| format!("Missing/deleted link: {linked}"));
            targets.push((target, label));
        }
        for (path, meta) in all.iter().filter(|(_, meta)| meta.links.contains(&id)) {
            let _ = meta;
            targets.push((
                Some(path.clone()),
                format!("Backlink: {}", path.file_name().unwrap().to_string_lossy()),
            ));
        }
        drop(all);
        if targets.is_empty() {
            self.notify("This document has no links or backlinks.");
            return;
        }
        let labels: Vec<_> = targets.iter().map(|(_, label)| label.clone()).collect();
        if let Some(target) = choose(&self.window, "Links and backlinks", "Open", &labels)
            .and_then(|i| targets.get(i))
        {
            if let Some(path) = &target.0 {
                let chapter = self
                    .chapters
                    .borrow()
                    .chapters()
                    .iter()
                    .any(|c| &c.path == path);
                self.navigate(chapter, path.clone(), None);
            } else {
                self.notify(
                    "The linked document is missing. Restore it from Trash or remove its link.",
                );
            }
        }
    }

    pub(super) fn count_book(self: &Rc<Self>) {
        let project = self.state.borrow().project.clone();
        let catalog_generation = self.catalog_generation.get();
        let generation = self.chapter.generation.get();
        let documents: Vec<_> = self
            .chapters
            .borrow()
            .chapters()
            .iter()
            .filter_map(|c| {
                self.metadata
                    .borrow()
                    .get(&c.path)
                    .map(|meta| (c.path.clone(), meta.id.clone()))
            })
            .collect();
        let w = self.clone();
        job(
            move || {
                let mut counts = HashMap::new();
                for (path, id) in documents {
                    counts.insert(id, features::word_count(&fs::read_to_string(path)?));
                }
                Ok(counts)
            },
            move |result| {
                if project != w.state.borrow().project
                    || catalog_generation != w.catalog_generation.get()
                {
                    return;
                }
                match result {
                    Ok(mut counts) => {
                        if w.chapter.path.borrow().is_some()
                            && w.chapter.generation.get() >= generation
                        {
                            counts.insert(
                                w.chapter.id.borrow().clone(),
                                features::word_count(&text(&w.chapter.view)),
                            );
                        }
                        w.book_counts.replace(counts);
                        w.update_words();
                    }
                    Err(cause) => w.report("Could not count manuscript", &cause),
                }
            },
        );
    }

    pub(super) fn update_words(&self) {
        let count = features::word_count(&self.active().observed.borrow());
        let state = self.state.borrow();
        let include = state.preferences.include_lore;
        let target = self
            .active()
            .path
            .borrow()
            .as_ref()
            .and_then(|p| self.metadata.borrow().get(p).map(|m| m.target))
            .unwrap_or(0);
        let book: usize = self.book_counts.borrow().values().sum();
        let target_text = if target > 0 {
            format!("/{target}")
        } else {
            String::new()
        };
        let goal = self.session_goal.get();
        self.words.set_text(&format!(
            "{count}{target_text} words · Book {book} · Session {:+}{} ({} added) · Today {:+}{}",
            state.progress.net(include),
            if goal > 0 {
                format!("/{goal}")
            } else {
                String::new()
            },
            state.progress.gross(include),
            state.progress.daily_net(include),
            if state.preferences.daily_goal > 0 {
                format!("/{}", state.preferences.daily_goal)
            } else {
                String::new()
            },
        ));
        self.words.set_tooltip_text(Some("Goals use net growth. Gross additions also count pasted and retyped words. Lore inclusion is configurable."));
    }
}

pub(super) fn comma_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    for value in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        if !values.iter().any(|v| v == value) {
            values.push(value.to_string());
        }
    }
    values
}
