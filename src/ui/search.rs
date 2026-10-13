use super::*;

type SearchDocument = (PathBuf, bool, String, Option<String>);

impl Workspace {
    fn search_documents(&self, project: bool) -> Vec<SearchDocument> {
        let active_path = self.active().path.borrow().clone();
        let mut documents = Vec::new();
        for chapter in self.chapters.borrow().chapters() {
            if project || active_path.as_ref() == Some(&chapter.path) {
                let content = (self.chapter.path.borrow().as_ref() == Some(&chapter.path))
                    .then(|| text(&self.chapter.view));
                documents.push((chapter.path.clone(), true, chapter.title.clone(), content));
            }
        }
        for entry in self.lores.borrow().entries() {
            if project || active_path.as_ref() == Some(&entry.path) {
                let content = (self.lore.path.borrow().as_ref() == Some(&entry.path))
                    .then(|| text(&self.lore.view));
                documents.push((entry.path.clone(), false, entry.title.clone(), content));
            }
        }
        documents
    }

    pub(super) fn search_dialog(self: &Rc<Self>, project: bool) {
        let dialog = gtk::Dialog::with_buttons(
            Some(if project {
                "Find / replace in project"
            } else {
                "Find / replace in document"
            }),
            Some(&self.window),
            gtk::DialogFlags::MODAL,
            &[
                ("Cancel", gtk::ResponseType::Cancel),
                ("Find", gtk::ResponseType::Ok),
                ("Preview replacement", gtk::ResponseType::Apply),
            ],
        );
        let query = Entry::new();
        query.set_placeholder_text(Some("Find literal text"));
        if let Some((previous, _)) = &*self.search_spec.borrow() {
            query.set_text(previous);
        }
        let replacement = Entry::new();
        replacement.set_placeholder_text(Some("Replacement text (may be empty)"));
        let case = CheckButton::with_label("Case sensitive");
        let word = CheckButton::with_label("Whole words only");
        let area = dialog.content_area();
        area.set_margin(16);
        area.set_spacing(10);
        for widget in [
            query.clone().upcast::<gtk::Widget>(),
            replacement.clone().upcast(),
            case.clone().upcast(),
            word.clone().upcast(),
        ] {
            area.add(&widget);
        }
        dialog.set_response_sensitive(gtk::ResponseType::Ok, !query.text().is_empty());
        dialog.set_response_sensitive(gtk::ResponseType::Apply, !query.text().is_empty());
        query.connect_changed(glib::clone!(@weak dialog => move |entry| {
            for response in [gtk::ResponseType::Ok, gtk::ResponseType::Apply] {
                dialog.set_response_sensitive(response, !entry.text().is_empty());
            }
        }));
        dialog.show_all();
        query.grab_focus();
        let response = dialog.run();
        let query = query.text().to_string();
        let replacement = replacement.text().to_string();
        let options = SearchOptions {
            case_sensitive: case.is_active(),
            whole_word: word.is_active(),
        };
        dialog.close();
        if query.is_empty() {
            return;
        }
        if response == gtk::ResponseType::Ok {
            self.find(project, query, options);
        } else if response == gtk::ResponseType::Apply {
            if project {
                let w = self.clone();
                self.with_saved(move || w.replace_project(query, replacement, options));
            } else {
                let original = text(&self.active().view);
                let (changed, count) =
                    features::replace_matches(&original, &query, &replacement, options);
                if count == 0 {
                    self.notify("No matches in this document.");
                    return;
                }
                if preview(
                    &self.window,
                    &format!("Replace {count} matches"),
                    &format!("Before:\n{original}\n\nAfter:\n{changed}"),
                ) {
                    self.observe(self.active_is_chapter());
                    self.active().view.buffer().unwrap().set_text(&changed);
                    self.observe(self.active_is_chapter());
                    self.search_hits.borrow_mut().clear();
                    self.update_outline();
                }
            }
        }
    }

    pub(super) fn find(self: &Rc<Self>, project: bool, query: String, options: SearchOptions) {
        let documents = self.search_documents(project);
        self.search_spec
            .replace(Some((query.clone(), options.clone())));
        self.set_busy(true, "Searching…");
        let w = self.clone();
        job(
            move || {
                let mut hits = Vec::new();
                for (path, chapter, title, current) in documents {
                    let content = match current {
                        Some(content) => content,
                        None => fs::read_to_string(&path)?,
                    };
                    let found = features::find_matches(&content, &query, options);
                    if found.is_empty()
                        && !features::find_matches(&title, &query, options).is_empty()
                    {
                        hits.push(SearchHit {
                            path: path.clone(),
                            chapter,
                            title: title.clone(),
                            found: TextMatch {
                                start: 0,
                                end: 0,
                                line: 0,
                                excerpt: "Title match".into(),
                            },
                        });
                    }
                    hits.extend(found.into_iter().map(|found| SearchHit {
                        path: path.clone(),
                        chapter,
                        title: title.clone(),
                        found,
                    }));
                }
                Ok(hits)
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok(hits) => {
                        if hits.is_empty() {
                            w.notify("No matches found.");
                            w.search_hits.borrow_mut().clear();
                            w.highlight_matches();
                            return;
                        }
                        let labels: Vec<_> = hits
                            .iter()
                            .map(|hit| {
                                format!(
                                    "{} · line {}: {}",
                                    hit.title,
                                    hit.found.line + 1,
                                    hit.found.excerpt
                                )
                            })
                            .collect();
                        w.search_hits.replace(hits);
                        let selected = if project {
                            choose(&w.window, "Search results", "Jump to match", &labels)
                        } else {
                            Some(0)
                        };
                        if let Some(index) = selected {
                            w.open_hit(index);
                        }
                        w.highlight_matches();
                    }
                    Err(cause) => w.report("Could not search", &cause),
                }
            },
        );
    }

    fn open_hit(self: &Rc<Self>, index: usize) {
        let hit = self.search_hits.borrow().get(index).cloned();
        if let Some(hit) = hit {
            self.search_index.set(index);
            if hit.chapter {
                self.chapter_search.set_text("");
            } else {
                self.lore_search.set_text("");
                if let Some(entry) = self
                    .lores
                    .borrow()
                    .entries()
                    .iter()
                    .find(|e| e.path == hit.path)
                {
                    let category = Category::all()
                        .iter()
                        .position(|c| *c == entry.category)
                        .unwrap();
                    self.categories[category].set_active(true);
                }
            }
            self.navigate(hit.chapter, hit.path, Some(hit.found));
            self.highlight_matches();
        }
    }

    pub(super) fn next_match(self: &Rc<Self>, previous: bool) {
        if self.search_hits.borrow().is_empty() {
            self.notify("Run Find first to navigate matches.");
            return;
        }
        // Recompute offsets for the open document after edits, keeping other project results.
        if let Some((query, options)) = self.search_spec.borrow().clone() {
            if let Some(path) = self.active().path.borrow().clone() {
                let chapter = self.active_is_chapter();
                let title = path.file_name().unwrap().to_string_lossy().into_owned();
                let found = features::find_matches(&text(&self.active().view), &query, options);
                let mut hits = self.search_hits.borrow_mut();
                let first = hits
                    .iter()
                    .position(|hit| hit.path == path)
                    .unwrap_or(hits.len());
                hits.retain(|hit| hit.path != path);
                let refreshed = found.into_iter().map(|found| SearchHit {
                    path: path.clone(),
                    chapter,
                    title: title.clone(),
                    found,
                });
                hits.splice(first..first, refreshed);
            }
        }
        let length = self.search_hits.borrow().len();
        if length == 0 {
            self.notify("No remaining matches.");
            self.highlight_matches();
            return;
        }
        let index = self.search_index.get().min(length - 1);
        self.open_hit(if previous {
            (index + length - 1) % length
        } else {
            (index + 1) % length
        });
    }

    pub(super) fn highlight_matches(&self) {
        for editor in [&self.chapter, &self.lore] {
            let buffer = editor.view.buffer().unwrap();
            let tag = buffer
                .tag_table()
                .unwrap()
                .lookup("search-match")
                .unwrap_or_else(|| {
                    let tag = gtk::TextTag::builder()
                        .name("search-match")
                        .background("#827142")
                        .foreground("#ffffff")
                        .build();
                    buffer.tag_table().unwrap().add(&tag);
                    tag
                });
            let (start, end) = buffer.bounds();
            buffer.remove_tag(&tag, &start, &end);
            if let Some((query, options)) = &*self.search_spec.borrow() {
                for found in features::find_matches(&text(&editor.view), query, *options) {
                    buffer.apply_tag(
                        &tag,
                        &buffer.iter_at_offset(found.start),
                        &buffer.iter_at_offset(found.end),
                    );
                }
            }
        }
    }

    pub(super) fn replace_project(
        self: &Rc<Self>,
        query: String,
        replacement: String,
        options: SearchOptions,
    ) {
        let documents = self.search_documents(true);
        self.set_busy(true, "Preparing replacement preview…");
        let w = self.clone();
        job(
            move || {
                let mut changes = Vec::new();
                for (path, _, title, current) in documents {
                    let original = fs::read_to_string(&path)?;
                    let content = current.unwrap_or_else(|| original.clone());
                    let (changed, count) =
                        features::replace_matches(&content, &query, &replacement, options);
                    if count > 0 {
                        changes.push((path, title, original, changed, count));
                    }
                }
                Ok(changes)
            },
            move |result| {
                w.set_busy(false, "");
                match result {
                    Ok(changes) => {
                        if changes.is_empty() {
                            w.notify("No matches to replace.");
                            return;
                        }
                        let total: usize = changes.iter().map(|change| change.4).sum();
                        let preview_text = changes.iter().map(|(_, title, before, after, count)| {
                        format!("=== {title}: {count} matches ===\nBefore:\n{before}\n\nAfter:\n{after}\n")
                    }).collect::<Vec<_>>().join("\n");
                        if !preview(
                            &w.window,
                            &format!("Replace {total} matches in {} documents", changes.len()),
                            &preview_text,
                        ) {
                            return;
                        }
                        w.set_busy(true, "Replacing text; originals retained as revisions…");
                        let w2 = w.clone();
                        job(
                            move || {
                                // Preflight all sources before the first write.
                                for (path, _, expected, _, _) in &changes {
                                    if fs::read_to_string(path)? != *expected {
                                        return Err(io::Error::other(format!(
                                            "{} changed after preview; no replacements were made",
                                            path.display()
                                        )));
                                    }
                                }
                                let mut committed: Vec<(PathBuf, String, String)> = Vec::new();
                                for (path, _, before, after, _) in changes {
                                    if let Err(cause) =
                                        documents::save_checked(&path, &before, &after)
                                    {
                                        let mut rollback_errors = Vec::new();
                                        for (path, before, after) in committed.iter().rev() {
                                            if let Err(rollback) =
                                                documents::save_checked(path, after, before)
                                            {
                                                rollback_errors.push(format!(
                                                    "{}: {rollback}",
                                                    path.display()
                                                ));
                                            }
                                        }
                                        return Err(io::Error::other(format!(
                                            "{cause}; {}",
                                            if rollback_errors.is_empty() {
                                                "completed changes were rolled back".into()
                                            } else {
                                                format!("rollback failed: {}. Recover originals using document revisions.", rollback_errors.join("; "))
                                            }
                                        )));
                                    }
                                    committed.push((path, before, after));
                                }
                                Ok(())
                            },
                            move |result| {
                                w2.set_busy(false, "");
                                match result {
                                    Ok(()) => {
                                        w2.search_hits.borrow_mut().clear();
                                        let path = w2.state.borrow().project.clone();
                                        w2.open_project(path);
                                    }
                                    Err(cause) => w2.report("Project replacement failed", &cause),
                                }
                            },
                        );
                    }
                    Err(cause) => w.report("Could not prepare replacement", &cause),
                }
            },
        );
    }
}

fn preview(window: &ApplicationWindow, title: &str, content: &str) -> bool {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Apply replacement", gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(800, 550);
    let view = TextView::new();
    view.set_editable(false);
    view.set_wrap_mode(gtk::WrapMode::Word);
    view.buffer().unwrap().set_text(content);
    dialog.content_area().add(&scroll(&view));
    dialog.show_all();
    let accepted = dialog.run() == gtk::ResponseType::Ok;
    dialog.close();
    accepted
}
