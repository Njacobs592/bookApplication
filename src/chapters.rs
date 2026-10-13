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
}

impl ChapterStore {
    /// Loads chapters from `dir`, creating the directory (and a first
    /// chapter) if it doesn't already exist.
    pub fn load_or_init(dir: &str) -> io::Result<Self> {
        let dir = PathBuf::from(dir);
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
        entries.sort();

        let mut chapters: Vec<Chapter> = entries
            .into_iter()
            .map(|path| Chapter {
                title: title_from_path(&path),
                path,
            })
            .collect();

        if chapters.is_empty() {
            let path = dir.join("01_Chapter 1.txt");
            fs::write(&path, "")?;
            chapters.push(Chapter {
                title: title_from_path(&path),
                path,
            });
        }

        Ok(ChapterStore { dir, chapters })
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// Persists `content` to the chapter at `index`.
    pub fn save_chapter_content(&mut self, index: usize, content: &str) -> io::Result<()> {
        let chapter = &self.chapters[index];
        save_document(&chapter.path, content)
    }

    /// Newest first; at most ten previous versions of this chapter.
    pub fn list_revisions(&self, index: usize) -> io::Result<Vec<PathBuf>> {
        list_revisions_for(&self.chapters[index].path)
    }

    /// Restoring is itself a save, so the current content can be recovered.
    pub fn restore_revision(&mut self, index: usize, revision: &Path) -> io::Result<()> {
        restore_document(&self.chapters[index].path, revision)
    }

    /// Creates a new chapter file titled `title` at the end of the book.
    pub fn add_chapter(&mut self, title: &str) -> io::Result<()> {
        let next_num = self.chapters.len() + 1;
        let file_name = format!("{:02}_{}.txt", next_num, sanitize(title));
        let path = self.dir.join(file_name);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.chapters.push(Chapter {
            title: title.to_string(),
            path,
        });
        Ok(())
    }

    /// Renames the chapter at `index` and updates its file on disk.
    pub fn rename_chapter(&mut self, index: usize, new_title: &str) -> io::Result<()> {
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
        let order: Vec<_> = (0..self.chapters.len()).filter(|i| *i != index).collect();
        let titles = order
            .iter()
            .map(|&i| self.chapters[i].title.clone())
            .collect();
        self.update_chapters(titles, order, Some(index))
    }

    /// Moves the chapter at `index` one position earlier in the book.
    pub fn move_up(&mut self, index: usize) -> io::Result<()> {
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
            return Err(error);
        }
        if let Some(index) = deleted {
            // The removed chapter stays in staging until all other moves succeed.
            if let Err(error) = fs::remove_file(stage.join(format!("{index}.txt"))) {
                eprintln!(
                    "Could not clean up removed chapter in {}: {}",
                    stage.display(),
                    error
                );
            }
            let revisions = stage.join(format!("{index}.revisions"));
            if occupied(&revisions) {
                if let Err(error) = fs::remove_dir_all(&revisions) {
                    eprintln!(
                        "Could not clean up removed chapter revisions in {}: {}",
                        revisions.display(),
                        error
                    );
                }
            }
        }
        if let Err(error) = fs::remove_dir(&stage) {
            eprintln!(
                "Could not clean up chapter staging directory {}: {}",
                stage.display(),
                error
            );
        }
        self.chapters = order
            .into_iter()
            .enumerate()
            .map(|(i, _)| Chapter {
                title: titles[i].clone(),
                path: new_paths[i].clone(),
            })
            .collect();
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
