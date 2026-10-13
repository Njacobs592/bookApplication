use crate::chapters::{list_revisions_for, restore_document, save_document};
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Character,
    Environment,
    Lore,
}

impl Category {
    pub fn all() -> [Category; 3] {
        [Category::Character, Category::Environment, Category::Lore]
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Character => "Character",
            Category::Environment => "Environment",
            Category::Lore => "Lore",
        }
    }
    fn directory(self) -> &'static str {
        match self {
            Category::Character => "characters",
            Category::Environment => "environments",
            Category::Lore => "lore",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoreEntry {
    pub title: String,
    pub category: Category,
    pub path: PathBuf,
}

pub struct LoreStore {
    dir: PathBuf,
    entries: Vec<LoreEntry>,
}

impl LoreStore {
    pub fn load(project_dir: &Path) -> io::Result<Self> {
        let dir = project_dir.join("lore");
        let mut entries = Vec::new();
        for category in Category::all() {
            let category_dir = dir.join(category.directory());
            fs::create_dir_all(&category_dir)?;
            let mut files: Vec<PathBuf> = fs::read_dir(&category_dir)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| {
                    p.extension().map(|e| e == "txt").unwrap_or(false)
                        && fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file())
                })
                .collect();
            files.sort();
            entries.extend(files.into_iter().map(|path| LoreEntry {
                title: title_from_path(&path),
                category,
                path,
            }));
        }
        Ok(Self { dir, entries })
    }

    pub fn entries(&self) -> &[LoreEntry] {
        &self.entries
    }
    pub fn load_content(&self, index: usize) -> io::Result<String> {
        fs::read_to_string(&self.entries[index].path)
    }
    pub fn add(&mut self, category: Category, title: &str) -> io::Result<()> {
        let path = self
            .dir
            .join(category.directory())
            .join(format!("{}.txt", sanitize(title)));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.entries.push(LoreEntry {
            title: title.to_string(),
            category,
            path,
        });
        Ok(())
    }
    pub fn save_content(&mut self, index: usize, content: &str) -> io::Result<()> {
        save_document(&self.entries[index].path, content)
    }
    /// Newest first; at most ten previous versions.
    pub fn list_revisions(&self, index: usize) -> io::Result<Vec<PathBuf>> {
        list_revisions_for(&self.entries[index].path)
    }
    /// Restore a revision returned by `list_revisions`.
    pub fn restore_revision(&mut self, index: usize, revision: &Path) -> io::Result<()> {
        restore_document(&self.entries[index].path, revision)
    }
    pub fn rename(&mut self, index: usize, title: &str) -> io::Result<()> {
        let old = self.entries[index].path.clone();
        let new = self
            .dir
            .join(self.entries[index].category.directory())
            .join(format!("{}.txt", sanitize(title)));
        if old != new {
            if !fs::symlink_metadata(&old)?.file_type().is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a regular lore file",
                ));
            }
            if fs::symlink_metadata(&new).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "a lore entry with that title already exists",
                ));
            }
            let old_versions = old
                .parent()
                .unwrap()
                .join(".revisions")
                .join(old.file_name().unwrap());
            let new_versions = new
                .parent()
                .unwrap()
                .join(".revisions")
                .join(new.file_name().unwrap());
            if fs::symlink_metadata(old_versions.parent().unwrap()).is_ok()
                && !fs::symlink_metadata(old_versions.parent().unwrap())?
                    .file_type()
                    .is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid revisions directory",
                ));
            }
            if fs::symlink_metadata(&old_versions).is_ok()
                && !fs::symlink_metadata(&old_versions)?.file_type().is_dir()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid revisions directory",
                ));
            }
            if fs::symlink_metadata(&new_versions).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "lore revisions already exist",
                ));
            }
            fs::rename(&old, &new)?;
            if fs::symlink_metadata(&old_versions).is_ok() {
                if let Err(error) = fs::rename(&old_versions, &new_versions) {
                    if let Err(rollback) = fs::rename(&new, &old) {
                        return Err(io::Error::new(
                            rollback.kind(),
                            format!("{error}; rollback failed: {rollback}"),
                        ));
                    }
                    return Err(error);
                }
            }
        }
        self.entries[index].title = title.to_string();
        self.entries[index].path = new;
        Ok(())
    }
    pub fn delete(&mut self, index: usize) -> io::Result<()> {
        fs::remove_file(&self.entries[index].path)?;
        let _ = fs::remove_dir_all(
            self.entries[index]
                .path
                .parent()
                .unwrap()
                .join(".revisions")
                .join(self.entries[index].path.file_name().unwrap()),
        );
        self.entries.remove(index);
        Ok(())
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}
fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entries_persist_by_category() {
        let dir = PathBuf::from(format!(".bookapp_test_lore_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut store = LoreStore::load(&dir).unwrap();
        store.add(Category::Character, "Ada / Hero").unwrap();
        store.save_content(0, "brave").unwrap();
        store.rename(0, "Ada").unwrap();
        assert_eq!(
            LoreStore::load(&dir).unwrap().load_content(0).unwrap(),
            "brave"
        );
        let loaded = LoreStore::load(&dir).unwrap();
        assert_eq!(loaded.entries()[0].category, Category::Character);
        let mut loaded = loaded;
        loaded.delete(0).unwrap();
        assert!(loaded.entries().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_titles_do_not_overwrite_existing_entries() {
        let dir = PathBuf::from(format!(
            ".bookapp_test_lore_collision_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let mut store = LoreStore::load(&dir).unwrap();
        store.add(Category::Character, "Ada").unwrap();
        store.save_content(0, "keep this").unwrap();
        store.add(Category::Character, "Other").unwrap();
        store.save_content(1, "keep this too").unwrap();

        assert!(store.add(Category::Character, "Ada").is_err());
        assert!(store.rename(1, "Ada").is_err());
        assert_eq!(store.entries()[1].title, "Other");
        assert_eq!(store.load_content(0).unwrap(), "keep this");
        assert_eq!(store.load_content(1).unwrap(), "keep this too");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lore_revisions_survive_rename_and_restore() {
        let dir = PathBuf::from(format!(
            ".bookapp_test_lore_revisions_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let mut store = LoreStore::load(&dir).unwrap();
        store.add(Category::Lore, "Earlier").unwrap();
        store.save_content(0, "one").unwrap();
        store.save_content(0, "two").unwrap();
        store.rename(0, "Later").unwrap();
        let revisions = store.list_revisions(0).unwrap();
        assert_eq!(revisions.len(), 2);
        store.restore_revision(0, &revisions[0]).unwrap();
        assert_eq!(store.load_content(0).unwrap(), "one");
        assert_eq!(
            store
                .restore_revision(0, &dir.join("forged.txt"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        fs::remove_file(&store.entries()[0].path).unwrap();
        assert!(store.save_content(0, "three").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
