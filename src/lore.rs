use crate::chapters::list_revisions_for;
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
    loaded: Vec<String>,
}

impl LoreStore {
    pub fn load(project_dir: &Path) -> io::Result<Self> {
        let dir = project_dir.join("lore");
        crate::documents::real_path(&dir)?;
        let mut entries = Vec::new();
        for category in Category::all() {
            let category_dir = dir.join(category.directory());
            crate::documents::real_path(&category_dir)?;
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
        let loaded = entries
            .iter()
            .map(|entry| fs::read_to_string(&entry.path))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            dir,
            entries,
            loaded,
        })
    }

    pub fn entries(&self) -> &[LoreEntry] {
        &self.entries
    }
    pub fn load_content(&self, index: usize) -> io::Result<String> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        fs::read_to_string(&entry.path)
    }
    pub fn add(&mut self, category: Category, title: &str) -> io::Result<()> {
        let path = self
            .dir
            .join(category.directory())
            .join(format!("{}.txt", sanitize(title)));
        crate::documents::unused_document(&path)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.entries.push(LoreEntry {
            title: title.to_string(),
            category,
            path,
        });
        self.loaded.push(String::new());
        Ok(())
    }
    pub fn save_content(&mut self, index: usize, content: &str) -> io::Result<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        crate::documents::save_checked(&entry.path, &self.loaded[index], content)?;
        self.loaded[index] = content.to_string();
        Ok(())
    }
    /// Newest first; at most ten previous versions.
    pub fn list_revisions(&self, index: usize) -> io::Result<Vec<PathBuf>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        list_revisions_for(&entry.path)
    }
    /// Restore a revision returned by `list_revisions`.
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
        self.save_content(index, &fs::read_to_string(revision)?)
    }
    pub fn rename(&mut self, index: usize, title: &str) -> io::Result<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        let old = entry.path.clone();
        let new = self
            .dir
            .join(self.entries[index].category.directory())
            .join(format!("{}.txt", sanitize(title)));
        crate::documents::validate_document(&old)?;
        crate::documents::move_many(&self.dir, &crate::documents::bundle_moves(&old, &new))?;
        self.entries[index].title = title.to_string();
        self.entries[index].path = new;
        Ok(())
    }
    pub fn delete(&mut self, index: usize) -> io::Result<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        crate::documents::validate_document(&entry.path)?;
        let root = self.dir.parent().unwrap();
        let trash = crate::documents::prepare_trash(
            root,
            &crate::documents::DeletedDocument {
                kind: "lore".into(),
                title: entry.title.clone(),
                category: Some(entry.category.directory().into()),
                index,
                original_file: entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            },
        )?;
        let destination = trash.join(entry.path.file_name().unwrap());
        if let Err(error) = crate::documents::move_many(
            root,
            &crate::documents::bundle_moves(&entry.path, &destination),
        ) {
            fs::remove_dir_all(trash)?;
            return Err(error);
        }
        self.entries.remove(index);
        self.loaded.remove(index);
        Ok(())
    }

    pub fn recategorize(&mut self, index: usize, category: Category) -> io::Result<()> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown lore entry"))?;
        crate::documents::validate_document(&entry.path)?;
        let destination = self
            .dir
            .join(category.directory())
            .join(entry.path.file_name().unwrap());
        crate::documents::move_many(
            &self.dir,
            &crate::documents::bundle_moves(&entry.path, &destination),
        )?;
        self.entries[index].category = category;
        self.entries[index].path = destination;
        Ok(())
    }

    pub fn list_deleted(&self) -> io::Result<Vec<PathBuf>> {
        crate::documents::list_deleted(self.dir.parent().unwrap(), "lore")
    }

    pub fn restore_deleted(&mut self, trash: &Path) -> io::Result<usize> {
        let root = self.dir.parent().unwrap();
        let manifest = crate::documents::read_deleted(root, trash, "lore")?;
        let category = Category::all()
            .into_iter()
            .find(|category| Some(category.directory()) == manifest.category.as_deref())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid lore category"))?;
        let source = trash.join(&manifest.original_file);
        crate::documents::validate_document(&source)?;
        let destination = self
            .dir
            .join(category.directory())
            .join(&manifest.original_file);
        let content = fs::read_to_string(&source)?;
        crate::documents::move_many(root, &crate::documents::bundle_moves(&source, &destination))?;
        let index = manifest.index.min(self.entries.len());
        self.entries.insert(
            index,
            LoreEntry {
                title: manifest.title,
                category,
                path: destination,
            },
        );
        self.loaded.insert(index, content);
        fs::remove_file(trash.join("manifest.json"))?;
        fs::remove_dir_all(trash)?;
        Ok(index)
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
    fn auxiliary_collisions_and_recategorization_collisions_are_rejected() {
        let dir = PathBuf::from(format!(
            ".bookapp_test_lore_aux_collision_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let mut store = LoreStore::load(&dir).unwrap();
        store.add(Category::Character, "Ada").unwrap();
        store.save_content(0, "keep").unwrap();
        store.add(Category::Lore, "Ada").unwrap();
        assert_eq!(
            store.recategorize(0, Category::Lore).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let reserved = dir.join("lore/characters/.metadata/Reserved.txt.json");
        fs::create_dir_all(reserved.parent().unwrap()).unwrap();
        fs::write(&reserved, "orphan").unwrap();
        assert_eq!(
            store.rename(0, "Reserved").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(store.load_content(0).unwrap(), "keep");
        assert_eq!(fs::read_to_string(reserved).unwrap(), "orphan");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recategorization_and_recovery_preserve_auxiliaries() {
        let dir = PathBuf::from(format!(".bookapp_test_lore_recover_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut store = LoreStore::load(&dir).unwrap();
        store.add(Category::Character, "Ada").unwrap();
        store.save_content(0, "base").unwrap();
        crate::documents::write_draft(&store.entries()[0].path, "base", "unsaved").unwrap();
        let metadata = crate::documents::auxiliaries(&store.entries()[0].path)[1].clone();
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        fs::write(metadata, r#"{"id":"stable"}"#).unwrap();
        store.recategorize(0, Category::Lore).unwrap();
        store.rename(0, "Renamed").unwrap();
        store.delete(0).unwrap();
        let trash = store.list_deleted().unwrap()[0].clone();
        assert_eq!(store.restore_deleted(&trash).unwrap(), 0);
        assert_eq!(store.entries()[0].category, Category::Lore);
        assert_eq!(store.load_content(0).unwrap(), "base");
        assert_eq!(store.list_revisions(0).unwrap().len(), 1);
        assert_eq!(
            crate::documents::load_draft(&store.entries()[0].path)
                .unwrap()
                .unwrap()
                .content,
            "unsaved"
        );
        assert_eq!(
            fs::read_to_string(&crate::documents::auxiliaries(&store.entries()[0].path)[1])
                .unwrap(),
            r#"{"id":"stable"}"#
        );
        store.delete(0).unwrap();
        let trash = store.list_deleted().unwrap()[0].clone();
        store.add(Category::Lore, "Renamed").unwrap();
        assert_eq!(
            store.restore_deleted(&trash).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(trash.exists());
        fs::write(&store.entries()[0].path, "external").unwrap();
        assert_eq!(
            store.save_content(0, "mine").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        fs::remove_dir_all(dir).unwrap();
    }
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
