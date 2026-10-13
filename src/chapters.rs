use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_REVISIONS: usize = 10;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> String {
    format!(
        "{}-{}-{:016}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn revisions_dir(path: &Path) -> PathBuf {
    path.parent()
        .unwrap()
        .join(".revisions")
        .join(path.file_name().unwrap())
}

pub(crate) fn list_revisions_for(path: &Path) -> io::Result<Vec<PathBuf>> {
    let dir = revisions_dir(path);
    if !occupied(&dir) {
        return Ok(Vec::new());
    }
    if !fs::symlink_metadata(&dir)?.file_type().is_dir()
        || !fs::symlink_metadata(dir.parent().unwrap())?
            .file_type()
            .is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid revisions directory",
        ));
    }
    let mut files = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    files.retain(|p| {
        p.extension().is_some_and(|e| e == "txt")
            && fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
    });
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(files)
}

/// Writes alongside the target and atomically replaces it only after the new
/// contents (and the previous revision, if any) have been flushed.
pub(crate) fn save_document(path: &Path, content: &str) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular document",
        ));
    }
    let temp = path.with_file_name(format!(".save-{}", unique_id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        fs::set_permissions(&temp, fs::metadata(path)?.permissions())?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        if occupied(path) {
            let prior = fs::read(path)?;
            let dir = revisions_dir(path);
            if occupied(dir.parent().unwrap())
                && !fs::symlink_metadata(dir.parent().unwrap())?
                    .file_type()
                    .is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid revisions root",
                ));
            }
            fs::create_dir_all(&dir)?;
            if !fs::symlink_metadata(dir.parent().unwrap())?
                .file_type()
                .is_dir()
                || !fs::symlink_metadata(&dir)?.file_type().is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid revisions directory",
                ));
            }
            let revision = dir.join(format!("{}.txt", unique_id()));
            let mut backup = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&revision)?;
            if let Err(error) = backup.write_all(&prior).and_then(|_| backup.sync_all()) {
                let _ = fs::remove_file(&revision);
                return Err(error);
            }
            let files = list_revisions_for(path)?;
            for old in files.iter().skip(MAX_REVISIONS) {
                fs::remove_file(old)?;
            }
        }
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn restore_document(path: &Path, revision: &Path) -> io::Result<()> {
    if !list_revisions_for(path)?
        .iter()
        .any(|candidate| candidate == revision)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown revision",
        ));
    }
    let content = fs::read_to_string(revision)?;
    save_document(path, &content)
}

/// A single chapter of the book, backed by a text file on disk.
pub struct Chapter {
    pub title: String,
    pub path: PathBuf,
}

impl Chapter {
    pub fn load_content(&self) -> io::Result<String> {
        fs::read_to_string(&self.path)
    }
}

/// Manages the collection of chapters that make up a book, stored as one
/// text file per chapter inside a book directory.
pub struct ChapterStore {
    dir: PathBuf,
    chapters: Vec<Chapter>,
    loaded: Vec<String>,
}

impl ChapterStore {
    /// Loads chapters from `dir`, creating the directory (and a first
    /// chapter) if it doesn't already exist.
    pub fn load_or_init(dir: &str) -> io::Result<Self> {
        let dir = PathBuf::from(dir);
        crate::documents::real_path(&dir)?;
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| {
                p.extension().map(|ext| ext == "txt").unwrap_or(false)
                    && fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
            })
            .collect();
        entries.sort_by(|a, b| {
            chapter_number(a)
                .cmp(&chapter_number(b))
                .then_with(|| a.cmp(b))
        });

        let mut chapters: Vec<Chapter> = entries
            .into_iter()
            .map(|path| Chapter {
                title: title_from_path(&path),
                path,
            })
            .collect();

        let marker = dir.join(".chapters-initialized");
        crate::documents::real_path(&marker)?;
        if chapters.is_empty() && !occupied(&marker) {
            let path = dir.join("01_Chapter 1.txt");
            fs::write(&path, "")?;
            chapters.push(Chapter {
                title: title_from_path(&path),
                path,
            });
        }

        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(marker)?;
        let loaded = chapters
            .iter()
            .map(Chapter::load_content)
            .collect::<io::Result<Vec<_>>>()?;
        Ok(ChapterStore {
            dir,
            chapters,
            loaded,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// Persists `content` to the chapter at `index`.
    pub fn save_chapter_content(&mut self, index: usize, content: &str) -> io::Result<()> {
        let chapter = self
            .chapters
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown chapter"))?;
        crate::documents::save_checked(&chapter.path, &self.loaded[index], content)?;
        self.loaded[index] = content.to_string();
        Ok(())
    }

    /// Newest first; at most ten previous versions of this chapter.
    pub fn list_revisions(&self, index: usize) -> io::Result<Vec<PathBuf>> {
        let chapter = self
            .chapters
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown chapter"))?;
        list_revisions_for(&chapter.path)
    }

    /// Restoring is itself a save, so the current content can be recovered.
    pub fn restore_revision(&mut self, index: usize, revision: &Path) -> io::Result<()> {
        if !self
            .list_revisions(index)?
            .iter()
            .any(|candidate| candidate == revision)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown revision",
            ));
        }
        self.save_chapter_content(index, &fs::read_to_string(revision)?)
    }

    /// Creates a new chapter file titled `title` at the end of the book.
    pub fn add_chapter(&mut self, title: &str) -> io::Result<()> {
        let next_num = self.chapters.len() + 1;
        let file_name = format!("{:02}_{}.txt", next_num, sanitize(title));
        let path = self.dir.join(file_name);
        crate::documents::unused_document(&path)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.chapters.push(Chapter {
            title: title.to_string(),
            path,
        });
        self.loaded.push(String::new());
        Ok(())
    }

    pub fn import_chapter(&mut self, title: &str, content: &str) -> io::Result<usize> {
        let index = self.chapters.len();
        let path = self
            .dir
            .join(format!("{:02}_{}.txt", index + 1, sanitize(title)));
        crate::documents::unused_document(&path)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(error) = file
            .write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
        {
            fs::remove_file(&path)?;
            return Err(error);
        }
        self.chapters.push(Chapter {
            title: title.into(),
            path,
        });
        self.loaded.push(content.into());
        Ok(index)
    }

    pub fn import_file(&mut self, source: &Path) -> io::Result<usize> {
        crate::documents::real_path(source)?;
        if !source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "txt" | "md" | "markdown"
                )
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chapter import accepts text or Markdown files",
            ));
        }
        let title = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid import filename")
            })?;
        self.import_chapter(title, &fs::read_to_string(source)?)
    }

    /// Overrides contain unsaved caller buffers, indexed by chapter, never lore.
    pub fn export_manuscript(&self, overrides: &[(usize, String)]) -> io::Result<String> {
        self.export_manuscript_as(overrides, true)
    }

    pub fn export_manuscript_as(
        &self,
        overrides: &[(usize, String)],
        markdown: bool,
    ) -> io::Result<String> {
        if overrides.iter().any(|(i, _)| *i >= self.chapters.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter override",
            ));
        }
        let mut output = String::new();
        for (index, chapter) in self.chapters.iter().enumerate() {
            let content = match overrides.iter().find(|(i, _)| *i == index) {
                Some((_, content)) => content.clone(),
                None => chapter.load_content()?,
            };
            if !output.is_empty() {
                output.push_str(if markdown { "\n\n---\n\n" } else { "\n\n" });
            }
            output.push_str(&format!(
                "{}{}\n\n{}",
                if markdown { "# " } else { "" },
                chapter.title,
                content
            ));
        }
        Ok(output)
    }

    pub fn move_to(&mut self, from: usize, to: usize) -> io::Result<()> {
        if from >= self.chapters.len() || to >= self.chapters.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter position",
            ));
        }
        let mut order: Vec<_> = (0..self.chapters.len()).collect();
        let original = order.remove(from);
        order.insert(to, original);
        let titles = order
            .iter()
            .map(|&i| self.chapters[i].title.clone())
            .collect();
        self.update_chapters(titles, order, None)
    }

    pub fn list_deleted(&self) -> io::Result<Vec<PathBuf>> {
        crate::documents::list_deleted(&self.dir, "chapter")
    }

    pub fn restore_deleted(&mut self, trash: &Path) -> io::Result<usize> {
        let manifest = crate::documents::read_deleted(&self.dir, trash, "chapter")?;
        if self
            .chapters
            .iter()
            .any(|chapter| chapter.title == manifest.title)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "chapter title already exists",
            ));
        }
        let source = trash.join(&manifest.original_file);
        crate::documents::validate_document(&source)?;
        let content = fs::read_to_string(&source)?;
        let temporary = self.dir.join(format!(".restore-{}.txt", unique_id()));
        let moves = crate::documents::bundle_moves(&source, &temporary);
        crate::documents::move_many(&self.dir, &moves)?;
        let original = self.chapters.len();
        self.chapters.push(Chapter {
            title: manifest.title,
            path: temporary.clone(),
        });
        self.loaded.push(content);
        let index = manifest.index.min(original);
        if let Err(error) = self.move_to(original, index) {
            self.chapters.pop();
            self.loaded.pop();
            let reverse: Vec<_> = moves.into_iter().map(|(a, b)| (b, a)).collect();
            crate::documents::move_many(&self.dir, &reverse)?;
            return Err(error);
        }
        fs::remove_file(trash.join("manifest.json"))?;
        fs::remove_dir_all(trash)?;
        Ok(index)
    }

    /// Renames the chapter at `index` and updates its file on disk.
    pub fn rename_chapter(&mut self, index: usize, new_title: &str) -> io::Result<()> {
        if index >= self.chapters.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter",
            ));
        }
        let new_path = self
            .dir
            .join(format!("{:02}_{}.txt", index + 1, sanitize(new_title)));
        let titles: Vec<String> = self
            .chapters
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == index {
                    new_title.to_string()
                } else {
                    c.title.clone()
                }
            })
            .collect();
        self.update_chapters(titles, (0..self.chapters.len()).collect(), None)?;
        debug_assert_eq!(self.chapters[index].path, new_path);
        Ok(())
    }

    /// Deletes the chapter at `index`, removing its file from disk.
    pub fn delete_chapter(&mut self, index: usize) -> io::Result<()> {
        if index >= self.chapters.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter",
            ));
        }
        let order: Vec<_> = (0..self.chapters.len()).filter(|i| *i != index).collect();
        let titles = order
            .iter()
            .map(|&i| self.chapters[i].title.clone())
            .collect();
        self.update_chapters(titles, order, Some(index))
    }

    /// Moves the chapter at `index` one position earlier in the book.
    pub fn move_up(&mut self, index: usize) -> io::Result<()> {
        if index >= self.chapters.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter",
            ));
        }
        if index == 0 {
            return Ok(());
        }
        let mut order: Vec<_> = (0..self.chapters.len()).collect();
        order.swap(index, index - 1);
        let titles = order
            .iter()
            .map(|&i| self.chapters[i].title.clone())
            .collect();
        self.update_chapters(titles, order, None)
    }

    /// Moves the chapter at `index` one position later in the book.
    pub fn move_down(&mut self, index: usize) -> io::Result<()> {
        if index >= self.chapters.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown chapter",
            ));
        }
        if index + 1 >= self.chapters.len() {
            return Ok(());
        }
        let mut order: Vec<_> = (0..self.chapters.len()).collect();
        order.swap(index, index + 1);
        let titles = order
            .iter()
            .map(|&i| self.chapters[i].title.clone())
            .collect();
        self.update_chapters(titles, order, None)
    }

    /// Renames every chapter file on disk to match the current in-memory
    /// order and titles, keeping the numeric prefix in sync.
    fn update_chapters(
        &mut self,
        titles: Vec<String>,
        order: Vec<usize>,
        deleted: Option<usize>,
    ) -> io::Result<()> {
        let old_paths: Vec<_> = self.chapters.iter().map(|c| c.path.clone()).collect();
        let new_paths: Vec<_> = titles
            .iter()
            .enumerate()
            .map(|(i, title)| {
                self.dir
                    .join(format!("{:02}_{}.txt", i + 1, sanitize(title)))
            })
            .collect();
        let mut destinations = std::collections::HashSet::new();
        for path in &new_paths {
            if !destinations.insert(path) || (occupied(path) && !old_paths.contains(path)) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "chapter file already exists",
                ));
            }
        }
        for path in &old_paths {
            crate::documents::real_path(path)?;
            if !fs::symlink_metadata(path)?.file_type().is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a regular chapter",
                ));
            }
        }
        let old_revisions: Vec<_> = old_paths.iter().map(|p| revisions_dir(p)).collect();
        let new_revisions: Vec<_> = new_paths.iter().map(|p| revisions_dir(p)).collect();
        for dir in old_revisions.iter().filter(|p| occupied(p)) {
            if !fs::symlink_metadata(dir)?.file_type().is_dir()
                || !fs::symlink_metadata(dir.parent().unwrap())?
                    .file_type()
                    .is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid revisions directory",
                ));
            }
        }
        for path in &new_revisions {
            if occupied(path) && !old_revisions.contains(path) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "chapter revisions already exist",
                ));
            }
        }
        let sidecars: Vec<_> = old_paths
            .iter()
            .map(|p| {
                crate::documents::auxiliaries(p)
                    .into_iter()
                    .skip(1)
                    .collect::<Vec<_>>()
            })
            .collect();
        let new_sidecars: Vec<_> = new_paths
            .iter()
            .map(|p| {
                crate::documents::auxiliaries(p)
                    .into_iter()
                    .skip(1)
                    .collect::<Vec<_>>()
            })
            .collect();
        for paths in &sidecars {
            for path in paths {
                crate::documents::real_path(path)?;
                if occupied(path) && !fs::symlink_metadata(path)?.file_type().is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid document sidecar",
                    ));
                }
            }
        }
        for paths in &new_sidecars {
            for path in paths {
                crate::documents::real_path(path)?;
                if occupied(path) && !sidecars.iter().flatten().any(|old| old == path) {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "document sidecar already exists",
                    ));
                }
            }
        }
        let trash = if let Some(index) = deleted {
            Some(crate::documents::prepare_trash(
                &self.dir,
                &crate::documents::DeletedDocument {
                    kind: "chapter".into(),
                    title: self.chapters[index].title.clone(),
                    category: None,
                    index,
                    original_file: old_paths[index]
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                },
            )?)
        } else {
            None
        };
        let stage = self.dir.join(format!(".chapter-stage-{}", unique_id()));
        fs::create_dir(&stage)?;
        let mut staged = Vec::new();
        let mut committed = Vec::new();
        let result = (|| {
            for (i, old) in old_paths.iter().enumerate() {
                let temp = stage.join(format!("{i}.txt"));
                fs::rename(old, &temp)?;
                staged.push((old.clone(), temp));
            }
            for (i, old) in old_revisions.iter().enumerate() {
                if occupied(old) {
                    let temp = stage.join(format!("{i}.revisions"));
                    fs::rename(old, &temp)?;
                    staged.push((old.clone(), temp));
                }
                for (i, paths) in sidecars.iter().enumerate() {
                    for (kind, old) in paths.iter().enumerate() {
                        if occupied(old) {
                            let temp = stage.join(format!("{i}.sidecar-{kind}"));
                            fs::rename(old, &temp)?;
                            staged.push((old.clone(), temp));
                        }
                    }
                }
            }
            for (pos, &original) in order.iter().enumerate() {
                let temp = stage.join(format!("{original}.txt"));
                fs::rename(&temp, &new_paths[pos])?;
                committed.push((new_paths[pos].clone(), temp));
                let revisions = stage.join(format!("{original}.revisions"));
                if occupied(&revisions) {
                    fs::create_dir_all(new_revisions[pos].parent().unwrap())?;
                    fs::rename(&revisions, &new_revisions[pos])?;
                    committed.push((new_revisions[pos].clone(), revisions));
                }
                for (kind, destination) in new_sidecars[pos].iter().enumerate() {
                    let temp = stage.join(format!("{original}.sidecar-{kind}"));
                    if occupied(&temp) {
                        fs::create_dir_all(destination.parent().unwrap())?;
                        fs::rename(&temp, destination)?;
                        committed.push((destination.clone(), temp));
                    }
                }
            }
            if let (Some(index), Some(trash)) = (deleted, &trash) {
                let destination = trash.join(old_paths[index].file_name().unwrap());
                let auxiliary = crate::documents::auxiliaries(&destination);
                let mut bundle = vec![
                    (stage.join(format!("{index}.txt")), destination),
                    (
                        stage.join(format!("{index}.revisions")),
                        auxiliary[0].clone(),
                    ),
                ];
                for kind in 0..2 {
                    bundle.push((
                        stage.join(format!("{index}.sidecar-{kind}")),
                        auxiliary[kind + 1].clone(),
                    ));
                }
                for (temp, destination) in bundle {
                    if occupied(&temp) {
                        fs::create_dir_all(destination.parent().unwrap())?;
                        fs::rename(&temp, &destination)?;
                        committed.push((destination, temp));
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            for (destination, temp) in committed.iter().rev() {
                if let Err(rollback) = fs::rename(destination, temp) {
                    return Err(io::Error::new(
                        rollback.kind(),
                        format!(
                            "{error}; rollback failed ({rollback}); recovery files in {}",
                            stage.display()
                        ),
                    ));
                }
            }
            for (old, temp) in staged.iter().rev() {
                if let Err(rollback) = fs::rename(temp, old) {
                    return Err(io::Error::new(
                        rollback.kind(),
                        format!(
                            "{error}; rollback failed ({rollback}); recovery files in {}",
                            stage.display()
                        ),
                    ));
                }
            }
            let _ = fs::remove_dir(&stage);
            if let Some(trash) = &trash {
                fs::remove_dir_all(trash)?;
            }
            return Err(error);
        }
        self.loaded = order.iter().map(|&i| self.loaded[i].clone()).collect();
        self.chapters = order
            .into_iter()
            .enumerate()
            .map(|(i, _)| Chapter {
                title: titles[i].clone(),
                path: new_paths[i].clone(),
            })
            .collect();
        fs::remove_dir(stage)
    }
}

/// Replaces characters that are unsafe or awkward in file names.
fn sanitize(title: &str) -> String {
    title
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}

/// Derives a display title from a chapter file name, e.g.
/// "01_Chapter 1.txt" -> "Chapter 1".
fn title_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    match stem.split_once('_') {
        Some((prefix, rest)) if prefix.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => stem.to_string(),
    }
}

fn chapter_number(path: &Path) -> u64 {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.split_once('_'))
        .and_then(|(prefix, _)| prefix.parse().ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_than_one_hundred_chapters_reload_in_numeric_order() {
        let dir = temp_dir("numeric_order");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        for number in 2..=105 {
            store
                .import_chapter(&format!("Chapter {number}"), &format!("body {number}"))
                .unwrap();
        }
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        for (index, chapter) in store.chapters().iter().enumerate() {
            assert_eq!(chapter.title, format!("Chapter {}", index + 1));
        }
        let path = store.chapters()[104].path.clone();
        crate::documents::write_draft(&path, "body 105", "unsaved 105").unwrap();
        let metadata = crate::documents::auxiliaries(&path)[1].clone();
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        fs::write(metadata, r#"{"id":"chapter-105"}"#).unwrap();
        assert!(store.move_to(0, 105).is_err());
        store.move_to(104, 0).unwrap();
        store.delete_chapter(0).unwrap();
        let trash = store.list_deleted().unwrap()[0].clone();
        assert_eq!(store.restore_deleted(&trash).unwrap(), 0);
        let store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        assert_eq!(store.chapters()[0].title, "Chapter 105");
        assert_eq!(store.chapters()[100].title, "Chapter 100");
        assert_eq!(
            crate::documents::load_draft(&store.chapters()[0].path)
                .unwrap()
                .unwrap()
                .content,
            "unsaved 105"
        );
        assert_eq!(
            fs::read_to_string(&crate::documents::auxiliaries(&store.chapters()[0].path)[1])
                .unwrap(),
            r#"{"id":"chapter-105"}"#
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recoverable_delete_restores_order_history_sidecars_and_empty_book() {
        let dir = temp_dir("recoverable");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        store.add_chapter("Second").unwrap();
        store.save_chapter_content(0, "first").unwrap();
        let original = store.chapters()[0].path.clone();
        crate::documents::write_draft(&original, "first", "unsaved").unwrap();
        let metadata = crate::documents::auxiliaries(&original)[1].clone();
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        fs::write(&metadata, r#"{"id":"stable"}"#).unwrap();
        store.delete_chapter(0).unwrap();
        let trash = store.list_deleted().unwrap()[0].clone();
        assert!(!original.exists());
        assert_eq!(store.restore_deleted(&trash).unwrap(), 0);
        assert_eq!(store.chapters()[0].title, "Chapter 1");
        assert_eq!(store.chapters()[1].title, "Second");
        assert_eq!(store.chapters()[0].load_content().unwrap(), "first");
        assert_eq!(store.list_revisions(0).unwrap().len(), 1);
        assert_eq!(
            crate::documents::load_draft(&store.chapters()[0].path)
                .unwrap()
                .unwrap()
                .content,
            "unsaved"
        );
        assert_eq!(fs::read_to_string(metadata).unwrap(), r#"{"id":"stable"}"#);
        store.move_to(0, 1).unwrap();
        assert!(crate::documents::load_draft(&store.chapters()[1].path)
            .unwrap()
            .is_some());
        store.delete_chapter(1).unwrap();
        store.delete_chapter(0).unwrap();
        assert!(ChapterStore::load_or_init(dir.to_str().unwrap())
            .unwrap()
            .chapters()
            .is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_collisions_and_external_edits_do_not_overwrite() {
        let dir = temp_dir("restore_conflict");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        let original = store.chapters()[0].path.clone();
        fs::write(&original, "external").unwrap();
        assert_eq!(
            store.save_chapter_content(0, "mine").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        store.delete_chapter(0).unwrap();
        let trash = store.list_deleted().unwrap()[0].clone();
        store.add_chapter("Chapter 1").unwrap();
        assert_eq!(
            store.restore_deleted(&trash).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(trash.exists());
        assert_eq!(store.chapters()[0].load_content().unwrap(), "");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_and_export_follow_chapter_order_and_unsaved_overrides() {
        let dir = temp_dir("import_export");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        let source = dir.join("Imported.md");
        fs::write(&source, "imported body").unwrap();
        assert_eq!(store.import_file(&source).unwrap(), 1);
        store.move_to(1, 0).unwrap();
        let manuscript = store
            .export_manuscript(&[(1, "unsaved body".into())])
            .unwrap();
        assert_eq!(
            manuscript,
            "# Imported\n\nimported body\n\n---\n\n# Chapter 1\n\nunsaved body"
        );
        assert_eq!(
            store
                .export_manuscript_as(&[(1, "café 猫".into())], false)
                .unwrap(),
            "Imported\n\nimported body\n\nChapter 1\n\ncafé 猫"
        );
        assert!(store
            .export_manuscript_as(&[(99, "invalid".into())], false)
            .is_err());
        assert_eq!(store.chapters()[1].load_content().unwrap(), "");
        fs::remove_dir_all(dir).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(format!(".bookapp_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn rename_delete_and_reorder() {
        let dir = temp_dir("basic");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        store.add_chapter("Second").unwrap();
        store.add_chapter("Third").unwrap();
        assert_eq!(
            store
                .chapters()
                .iter()
                .map(|c| c.title.clone())
                .collect::<Vec<_>>(),
            vec!["Chapter 1", "Second", "Third"]
        );

        // Reorder: move "Third" up twice to become first.
        store.move_up(2).unwrap();
        store.move_up(1).unwrap();
        assert_eq!(
            store
                .chapters()
                .iter()
                .map(|c| c.title.clone())
                .collect::<Vec<_>>(),
            vec!["Third", "Chapter 1", "Second"]
        );

        // Rename the first chapter and confirm content is preserved after the file rename.
        store.save_chapter_content(0, "hello world").unwrap();
        store.rename_chapter(0, "Renamed").unwrap();
        assert_eq!(store.chapters()[0].title, "Renamed");
        assert_eq!(store.chapters()[0].load_content().unwrap(), "hello world");

        // Delete the middle chapter and confirm the rest survive with correct numbering.
        store.delete_chapter(1).unwrap();
        assert_eq!(
            store
                .chapters()
                .iter()
                .map(|c| c.title.clone())
                .collect::<Vec<_>>(),
            vec!["Renamed", "Second"]
        );
        assert_eq!(
            store.chapters()[0]
                .path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            "01_Renamed.txt"
        );
        assert_eq!(
            store.chapters()[1]
                .path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            "02_Second.txt"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_and_rename_do_not_overwrite_existing_files() {
        let dir = temp_dir("chapter_collision");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        store.save_chapter_content(0, "keep this").unwrap();

        let occupied = dir.join("01_Reserved.txt");
        fs::write(&occupied, "do not replace").unwrap();
        assert!(store.rename_chapter(0, "Reserved").is_err());
        assert_eq!(store.chapters()[0].title, "Chapter 1");
        assert_eq!(store.chapters()[0].load_content().unwrap(), "keep this");
        assert_eq!(fs::read_to_string(&occupied).unwrap(), "do not replace");

        let occupied = dir.join("02_Second.txt");
        fs::write(&occupied, "do not replace").unwrap();
        assert!(store.add_chapter("Second").is_err());
        assert_eq!(fs::read_to_string(occupied).unwrap(), "do not replace");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reorder_preserves_content_and_revisions() {
        let dir = temp_dir("reorder_revisions");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        store.add_chapter("Second").unwrap();
        store.save_chapter_content(0, "first").unwrap();
        store.save_chapter_content(1, "second").unwrap();
        let revision = store.list_revisions(1).unwrap()[0].clone();
        store.move_up(1).unwrap();
        assert_eq!(store.chapters()[0].load_content().unwrap(), "second");
        assert_eq!(store.chapters()[1].load_content().unwrap(), "first");
        assert_eq!(store.list_revisions(0).unwrap().len(), 1);
        assert!(!revision.exists());
        let revision = store.list_revisions(0).unwrap()[0].clone();
        store.restore_revision(0, &revision).unwrap();
        assert_eq!(store.chapters()[0].load_content().unwrap(), "");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_operations_leave_store_unchanged() {
        let dir = temp_dir("failed_reorder");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        store.add_chapter("Second").unwrap();
        store.save_chapter_content(0, "first").unwrap();
        store.save_chapter_content(1, "second").unwrap();
        let collision = dir.join("01_Second.txt");
        fs::write(&collision, "occupied").unwrap();
        assert!(store.move_up(1).is_err());
        assert_eq!(store.chapters()[0].load_content().unwrap(), "first");
        assert_eq!(store.chapters()[1].load_content().unwrap(), "second");
        assert_eq!(fs::read_to_string(collision).unwrap(), "occupied");
        let missing = store.chapters()[1].path.clone();
        fs::remove_file(missing).unwrap();
        assert!(store.delete_chapter(0).is_err());
        assert_eq!(store.chapters().len(), 2);
        assert_eq!(store.chapters()[0].load_content().unwrap(), "first");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_failure_and_bounded_revisions() {
        let dir = temp_dir("revisions");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        for i in 0..13 {
            store
                .save_chapter_content(0, &format!("version {i}"))
                .unwrap();
        }
        let revisions = store.list_revisions(0).unwrap();
        assert_eq!(revisions.len(), 10);
        assert_eq!(
            fs::read_to_string(revisions.last().unwrap()).unwrap(),
            "version 2"
        );
        let current = store.chapters()[0].path.clone();
        fs::remove_file(&current).unwrap();
        assert!(store.save_chapter_content(0, "lost").is_err());
        assert!(!current.exists());
        assert!(fs::read_dir(&dir).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".save-")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_failure_keeps_existing_content() {
        let dir = temp_dir("backup_failure");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        let revisions_root = dir.join(".revisions");
        fs::write(&revisions_root, "blocked").unwrap();
        assert!(store.save_chapter_content(0, "new").is_err());
        assert_eq!(store.chapters()[0].load_content().unwrap(), "");
        assert_eq!(fs::read_to_string(revisions_root).unwrap(), "blocked");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_save_preserves_document_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("permissions");
        let mut store = ChapterStore::load_or_init(dir.to_str().unwrap()).unwrap();
        let path = store.chapters()[0].path.clone();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        store.save_chapter_content(0, "private draft").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
