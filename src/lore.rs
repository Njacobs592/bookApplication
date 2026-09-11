use std::fs;
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
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e == "txt").unwrap_or(false))
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
        fs::write(&path, "")?;
        self.entries.push(LoreEntry {
            title: title.to_string(),
            category,
            path,
        });
        Ok(())
    }
    pub fn save_content(&mut self, index: usize, content: &str) -> io::Result<()> {
        fs::write(&self.entries[index].path, content)
    }
    pub fn rename(&mut self, index: usize, title: &str) -> io::Result<()> {
        let old = self.entries[index].path.clone();
        let new = self
            .dir
            .join(self.entries[index].category.directory())
            .join(format!("{}.txt", sanitize(title)));
        fs::rename(old, &new)?;
        self.entries[index].title = title.to_string();
        self.entries[index].path = new;
        Ok(())
    }
    pub fn delete(&mut self, index: usize) -> io::Result<()> {
        fs::remove_file(&self.entries[index].path)?;
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
}
