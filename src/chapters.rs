use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|ext| ext == "txt").unwrap_or(false))
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
        fs::write(&chapter.path, content)
    }

    /// Creates a new chapter file titled `title` at the end of the book.
    pub fn add_chapter(&mut self, title: &str) -> io::Result<()> {
        let next_num = self.chapters.len() + 1;
        let file_name = format!("{:02}_{}.txt", next_num, sanitize(title));
        let path = self.dir.join(file_name);
        fs::write(&path, "")?;
        self.chapters.push(Chapter {
            title: title.to_string(),
            path,
        });
        Ok(())
    }

    /// Renames the chapter at `index` and updates its file on disk.
    pub fn rename_chapter(&mut self, index: usize, new_title: &str) -> io::Result<()> {
        self.chapters[index].title = new_title.to_string();
        self.renumber_all()
    }

    /// Deletes the chapter at `index`, removing its file from disk.
    pub fn delete_chapter(&mut self, index: usize) -> io::Result<()> {
        let chapter = self.chapters.remove(index);
        fs::remove_file(&chapter.path)?;
        self.renumber_all()
    }

    /// Moves the chapter at `index` one position earlier in the book.
    pub fn move_up(&mut self, index: usize) -> io::Result<()> {
        if index == 0 {
            return Ok(());
        }
        self.chapters.swap(index, index - 1);
        self.renumber_all()
    }

    /// Moves the chapter at `index` one position later in the book.
    pub fn move_down(&mut self, index: usize) -> io::Result<()> {
        if index + 1 >= self.chapters.len() {
            return Ok(());
        }
        self.chapters.swap(index, index + 1);
        self.renumber_all()
    }

    /// Renames every chapter file on disk to match the current in-memory
    /// order and titles, keeping the numeric prefix in sync.
    fn renumber_all(&mut self) -> io::Result<()> {
        for (i, chapter) in self.chapters.iter_mut().enumerate() {
            let file_name = format!("{:02}_{}.txt", i + 1, sanitize(&chapter.title));
            let new_path = self.dir.join(file_name);
            if new_path != chapter.path {
                fs::rename(&chapter.path, &new_path)?;
                chapter.path = new_path;
            }
        }
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
