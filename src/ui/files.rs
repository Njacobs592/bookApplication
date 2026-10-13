use super::*;

impl Workspace {
    pub(super) fn import_chapter(self: &Rc<Self>) {
        let Some(path) = file_dialog(
            &self.window,
            "Import UTF-8 text or Markdown",
            gtk::FileChooserAction::Open,
        ) else {
            return;
        };
        let initial = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported chapter".into());
        let Some(title) = prompt(
            &self.window,
            "Imported chapter title",
            &initial,
            "Import",
            valid_name,
        ) else {
            return;
        };
        let w = self.clone();
        self.with_saved(move || {
            w.edit_chapters(move |store| {
                Ok(Some(
                    store.import_chapter(&title, &fs::read_to_string(path)?)?,
                ))
            })
        });
    }

    pub(super) fn export(self: &Rc<Self>) {
        let Some(path) = file_dialog(
            &self.window,
            "Export manuscript (.txt or .md)",
            gtk::FileChooserAction::Save,
        ) else {
            return;
        };
        if !path
            .extension()
            .is_some_and(|ext| ext == "txt" || ext == "md")
        {
            self.report(
                "Invalid export filename",
                &"Choose a .txt or .md extension.",
            );
            return;
        }
        let project = self.state.borrow().project.clone();
        if path.starts_with(&project) {
            self.report("Unsafe export location", &"Export outside the active project to avoid replacing source documents or importing the manuscript as a chapter.");
            return;
        }
        let w = self.clone();
        self.with_saved(move || {
            w.set_busy(true, "Exporting manuscript…");
            let w2 = w.clone();
            job(
                move || {
                    let chapters = ChapterStore::load_or_init(&project.to_string_lossy())?;
                    let markdown = path.extension().is_some_and(|ext| ext == "md");
                    let manuscript = chapters.export_manuscript_as(&[], markdown)?;
                    documents::atomic_write(&path, manuscript.as_bytes())?;
                    Ok(path)
                },
                move |result| {
                    w2.set_busy(false, "");
                    match result {
                        Ok(path) => {
                            w2.notify(&format!("Manuscript exported to {}", path.display()))
                        }
                        Err(cause) => w2.report("Could not export manuscript", &cause),
                    }
                },
            );
        });
    }

    pub(super) fn backup(self: &Rc<Self>) {
        let Some(parent) = file_dialog(
            &self.window,
            "Select backup parent folder",
            gtk::FileChooserAction::SelectFolder,
        ) else {
            return;
        };
        let initial = format!(
            "Book backup {}",
            chrono::Local::now().format("%Y-%m-%d-%H%M%S")
        );
        let Some(name) = prompt(
            &self.window,
            "Backup folder name",
            &initial,
            "Back up",
            valid_name,
        ) else {
            return;
        };
        let destination = parent.join(name);
        let source = self.state.borrow().project.clone();
        let w = self.clone();
        self.with_saved(move || {
            w.set_busy(true, "Backing up project…");
            let w2 = w.clone();
            job(
                move || {
                    documents::backup_project(&source, &destination)?;
                    Ok(destination)
                },
                move |result| {
                    w2.set_busy(false, "");
                    match result {
                        Ok(path) => {
                            w2.notify(&format!("Portable backup created at {}", path.display()))
                        }
                        Err(cause) => w2.report("Could not back up project", &cause),
                    }
                },
            );
        });
    }

    pub(super) fn import_backup(self: &Rc<Self>) {
        let Some(source) = file_dialog(
            &self.window,
            "Open portable project backup",
            gtk::FileChooserAction::SelectFolder,
        ) else {
            return;
        };
        let initial = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported book".into());
        let Some(name) = prompt(
            &self.window,
            "Imported project name",
            &initial,
            "Import",
            valid_name,
        ) else {
            return;
        };
        let root = self.state.borrow().storage.join("projects");
        let w = self.clone();
        self.with_saved(move || {
            w.set_busy(true, "Importing project backup…");
            let w2 = w.clone();
            job(
                move || documents::import_backup(&source, &root, &name),
                move |result| {
                    w2.set_busy(false, "");
                    match result {
                        Ok(path) => {
                            if let Err(cause) = w2.reload_projects() {
                                w2.report("Could not reload imported project", &cause);
                                return;
                            }
                            w2.open_project(path.path);
                        }
                        Err(cause) => w2.report("Could not import backup", &cause),
                    }
                },
            );
        });
    }

    pub(super) fn revisions(self: &Rc<Self>) {
        let Some(path) = self.active().path.borrow().clone() else {
            return;
        };
        let chapter = self.active_is_chapter();
        let w = self.clone();
        self.with_saved(move || {
            w.set_busy(true, "Loading revisions…");
            let target = path.clone();
            let w2 = w.clone();
            job(
                move || {
                    let revisions = crate::chapters::list_revisions_for(&target)?;
                    let mut versions = Vec::new();
                    for revision in revisions {
                        let content = fs::read_to_string(&revision)?;
                        let label = format!(
                            "{} · {} words",
                            revision_date(&revision),
                            features::word_count(&content)
                        );
                        versions.push((revision, label, content));
                    }
                    Ok(versions)
                },
                move |result| {
                    w2.set_busy(false, "");
                    match result {
                        Ok(versions) => {
                            if versions.is_empty() {
                                w2.notify("This document has no saved revisions yet.");
                                return;
                            }
                            let labels: Vec<_> =
                                versions.iter().map(|(_, label, _)| label.clone()).collect();
                            let Some((_, label, revision)) =
                                choose(&w2.window, "Saved revisions", "Compare", &labels)
                                    .and_then(|i| versions.get(i).cloned())
                            else {
                                return;
                            };
                            let current = text(&w2.active().view);
                            if !compare(&w2.window, &label, &current, &revision) {
                                return;
                            }
                            w2.set_busy(true, "Restoring revision; preserving current version…");
                            let w3 = w2.clone();
                            let target = path.clone();
                            job(
                                move || {
                                    documents::save_checked(&target, &current, &revision)?;
                                    documents::clear_draft(&target)
                                },
                                move |result| {
                                    w3.set_busy(false, "");
                                    match result {
                                        Ok(()) => w3.open_document(chapter, path, None),
                                        Err(cause) => {
                                            w3.report("Could not restore revision", &cause)
                                        }
                                    }
                                },
                            );
                        }
                        Err(cause) => w2.report("Could not list revisions", &cause),
                    }
                },
            );
        });
    }
}

fn revision_date(path: &Path) -> String {
    let timestamp = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse::<u128>().ok())
        .and_then(|ns| {
            i64::try_from(ns / 1_000_000_000).ok().and_then(|secs| {
                chrono::DateTime::from_timestamp(secs, (ns % 1_000_000_000) as u32)
            })
        });
    timestamp
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string()
        })
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

fn compare(window: &ApplicationWindow, title: &str, current: &str, revision: &str) -> bool {
    let dialog = gtk::Dialog::with_buttons(
        Some(&format!("Compare revision · {title}")),
        Some(window),
        gtk::DialogFlags::MODAL,
        &[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Restore revision", gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(950, 600);
    let box_ = GtkBox::new(Orientation::Horizontal, 12);
    for (label, content) in [
        ("Current version (preserved on restore)", current),
        ("Selected revision", revision),
    ] {
        let panel = GtkBox::new(Orientation::Vertical, 8);
        panel.add(&Label::new(Some(label)));
        let view = TextView::new();
        view.set_editable(false);
        view.set_wrap_mode(gtk::WrapMode::Word);
        view.buffer().unwrap().set_text(content);
        panel.pack_start(&scroll(&view), true, true, 0);
        box_.pack_start(&panel, true, true, 0);
    }
    dialog.content_area().add(&box_);
    dialog.show_all();
    let accepted = dialog.run() == gtk::ResponseType::Ok;
    dialog.close();
    accepted
}
