//! Shared document persistence, recoverable deletion, and portable project copies.
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn unique_id() -> String {
    format!(
        "{:039}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn real_path(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        if !ancestor.as_os_str().is_empty()
            && exists(ancestor)
            && fs::symlink_metadata(ancestor)?.file_type().is_symlink()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "symlinks are not supported",
            ));
        }
    }
    Ok(())
}

pub(crate) fn auxiliaries(path: &Path) -> Vec<PathBuf> {
    let parent = path.parent().unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    vec![
        parent.join(".revisions").join(path.file_name().unwrap()),
        parent.join(".metadata").join(format!("{name}.json")),
        parent.join(".drafts").join(format!("{name}.json")),
    ]
}

pub(crate) fn bundle_moves(old: &Path, new: &Path) -> Vec<(PathBuf, PathBuf)> {
    std::iter::once((old.to_path_buf(), new.to_path_buf()))
        .chain(auxiliaries(old).into_iter().zip(auxiliaries(new)))
        .collect()
}

/// Staging permits cycles (chapter reordering) without overwriting any unrelated file.
pub(crate) fn move_many(root: &Path, moves: &[(PathBuf, PathBuf)]) -> io::Result<()> {
    for (old, new) in moves.iter().filter(|(old, new)| old != new) {
        real_path(old)?;
        real_path(new)?;
        if exists(new)
            && !moves
                .iter()
                .any(|(source, _)| source == new && exists(source))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination already exists: {}", new.display()),
            ));
        }
    }
    let moves: Vec<_> = moves.iter().filter(|(a, b)| a != b && exists(a)).collect();
    let mut destinations = std::collections::HashSet::new();
    for (old, new) in &moves {
        real_path(old)?;
        real_path(new)?;
        if !destinations.insert(new)
            || (exists(new) && !moves.iter().any(|(source, _)| source == new))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination already exists: {}", new.display()),
            ));
        }
    }
    real_path(root)?;
    let stage = root.join(format!(".document-stage-{}", unique_id()));
    fs::create_dir(&stage)?;
    let mut staged = Vec::new();
    let mut committed = Vec::new();
    let result = (|| {
        for (i, (old, _)) in moves.iter().enumerate() {
            let temp = stage.join(i.to_string());
            fs::rename(old, &temp)?;
            staged.push((old, temp));
        }
        for (i, (_, new)) in moves.iter().enumerate() {
            fs::create_dir_all(new.parent().unwrap())?;
            real_path(new.parent().unwrap())?;
            let temp = stage.join(i.to_string());
            fs::rename(&temp, new)?;
            committed.push((new, temp));
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (new, temp) in committed.iter().rev() {
            fs::rename(new, temp).map_err(|rollback| {
                io::Error::new(
                    rollback.kind(),
                    format!(
                        "{error}; rollback failed: {rollback}; recovery: {}",
                        stage.display()
                    ),
                )
            })?;
        }
        for (old, temp) in staged.iter().rev() {
            fs::rename(temp, old).map_err(|rollback| {
                io::Error::new(
                    rollback.kind(),
                    format!(
                        "{error}; rollback failed: {rollback}; recovery: {}",
                        stage.display()
                    ),
                )
            })?;
        }
        fs::remove_dir(&stage)?;
        return Err(error);
    }
    fs::remove_dir(stage)
}

pub(crate) fn validate_document(path: &Path) -> io::Result<()> {
    real_path(path)?;
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular document",
        ));
    }
    for (index, auxiliary) in auxiliaries(path).iter().enumerate() {
        real_path(auxiliary)?;
        if exists(auxiliary) {
            let kind = fs::symlink_metadata(auxiliary)?.file_type();
            if (index == 0 && !kind.is_dir()) || (index != 0 && !kind.is_file()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid document auxiliary",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn unused_document(path: &Path) -> io::Result<()> {
    for path in std::iter::once(path.to_path_buf()).chain(auxiliaries(path)) {
        real_path(&path)?;
        if exists(&path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "document or auxiliary already exists",
            ));
        }
    }
    Ok(())
}

pub(crate) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    real_path(path)?;
    fs::create_dir_all(path.parent().unwrap())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    atomic_write(path, &bytes)
}

pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    real_path(path)?;
    let permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output is not a regular file",
            ))
        }
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => None,
        Err(cause) => return Err(cause),
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = parent.join(format!(".export-{}", unique_id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        if let Some(permissions) = permissions {
            fs::set_permissions(&temporary, permissions)?;
        }
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if let Err(cause) = result {
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(cleanup) if cleanup.kind() == io::ErrorKind::NotFound => {}
            Err(cleanup) => {
                return Err(io::Error::other(format!(
                    "{cause}; output cleanup failed: {cleanup}"
                )))
            }
        }
        return Err(cause);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub base: String,
    pub content: String,
}

pub fn write_draft(path: &Path, base: &str, content: &str) -> io::Result<()> {
    atomic_json(
        &auxiliaries(path)[2],
        &Draft {
            base: base.into(),
            content: content.into(),
        },
    )
}

pub fn load_draft(path: &Path) -> io::Result<Option<Draft>> {
    let draft = auxiliaries(path)[2].clone();
    real_path(&draft)?;
    match fs::read(draft) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn clear_draft(path: &Path) -> io::Result<()> {
    let draft = auxiliaries(path)[2].clone();
    real_path(&draft)?;
    match fs::remove_file(draft) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub fn save_checked(path: &Path, expected: &str, content: &str) -> io::Result<()> {
    real_path(path)?;
    if fs::read_to_string(path)? != expected {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "external file conflict: the document changed on disk; reload or recover your draft before saving"));
    }
    crate::chapters::save_document(path, content)
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DeletedDocument {
    pub kind: String,
    pub title: String,
    pub category: Option<String>,
    pub index: usize,
    pub original_file: String,
}

pub(crate) fn prepare_trash(root: &Path, manifest: &DeletedDocument) -> io::Result<PathBuf> {
    let trash = root.join(".document-trash");
    real_path(&trash)?;
    fs::create_dir_all(&trash)?;
    let folder = trash.join(unique_id());
    fs::create_dir(&folder)?;
    if let Err(error) = atomic_json(&folder.join("manifest.json"), manifest) {
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok(folder)
}

pub(crate) fn read_deleted(root: &Path, folder: &Path, kind: &str) -> io::Result<DeletedDocument> {
    if folder.parent() != Some(root.join(".document-trash").as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown deleted document",
        ));
    }
    real_path(folder)?;
    real_path(&folder.join("manifest.json"))?;
    let manifest: DeletedDocument =
        serde_json::from_slice(&fs::read(folder.join("manifest.json"))?)
            .map_err(io::Error::other)?;
    if manifest.kind != kind
        || Path::new(&manifest.original_file).components().count() != 1
        || !matches!(
            Path::new(&manifest.original_file).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid deleted document manifest",
        ));
    }
    Ok(manifest)
}

pub(crate) fn list_deleted(root: &Path, kind: &str) -> io::Result<Vec<PathBuf>> {
    let trash = root.join(".document-trash");
    if !exists(&trash) {
        return Ok(Vec::new());
    }
    real_path(&trash)?;
    let mut result = Vec::new();
    for entry in fs::read_dir(&trash)? {
        let path = entry?.path();
        real_path(&path)?;
        let manifest: DeletedDocument =
            serde_json::from_slice(&fs::read(path.join("manifest.json"))?)
                .map_err(io::Error::other)?;
        if manifest.kind == kind {
            result.push(path);
        }
    }
    result.sort_by(|a, b| b.cmp(a));
    Ok(result)
}

fn copy_tree(source: &Path, dest: &Path, root: bool) -> io::Result<()> {
    real_path(source)?;
    if !fs::symlink_metadata(source)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "backup source must be a directory",
        ));
    }
    fs::create_dir(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if root && (name == "projects" || name == "Trash") {
            continue;
        }
        let path = entry.path();
        let kind = fs::symlink_metadata(&path)?.file_type();
        if kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "backup refuses symlinks",
            ));
        }
        if kind.is_dir() {
            copy_tree(&path, &dest.join(&name), false)?;
        } else if kind.is_file() {
            fs::copy(path, dest.join(name))?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "backup contains a special file",
            ));
        }
    }
    Ok(())
}

/// A portable directory copy, including revisions, drafts, metadata, and deleted documents.
pub fn backup_project(source: &Path, dest: &Path) -> io::Result<()> {
    real_path(source)?;
    real_path(dest)?;
    if exists(dest) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "backup destination exists",
        ));
    }
    let source_absolute = fs::canonicalize(source)?;
    let parent = fs::canonicalize(
        dest.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    if parent.starts_with(&source_absolute) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "backup destination must be outside the project",
        ));
    }
    let stage = parent.join(format!(".backup-stage-{}", unique_id()));
    let result = copy_tree(source, &stage, true).and_then(|_| {
        if exists(dest) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "backup destination exists",
            ));
        }
        fs::rename(&stage, dest)
    });
    if result.is_err() && exists(&stage) {
        fs::remove_dir_all(&stage)?;
    }
    result
}

pub fn import_backup(
    source: &Path,
    projects_root: &Path,
    name: &str,
) -> io::Result<crate::projects::Project> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| c.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid project name",
        ));
    }
    real_path(projects_root)?;
    fs::create_dir_all(projects_root)?;
    let destination = projects_root.join(crate::projects::sanitize(name));
    backup_project(source, &destination)?;
    Ok(crate::projects::Project {
        name: name.into(),
        path: destination,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let path = PathBuf::from(format!(".bookapp_test_documents_{name}_{}", unique_id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn atomic_export_creates_overwrites_and_rejects_invalid_outputs() {
        let root = test_dir("export");
        let output = root.join("manuscript.md");
        atomic_write(&output, b"first").unwrap();
        atomic_write(&output, b"second").unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "second");
        assert_eq!(
            atomic_write(&root, b"invalid").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(atomic_write(&root.join("missing/output.txt"), b"invalid").is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(0o600)).unwrap();
            atomic_write(&output, b"private").unwrap();
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let link = root.join("link");
            std::os::unix::fs::symlink("manuscript.md", &link).unwrap();
            assert_eq!(
                atomic_write(&link, b"invalid").unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(fs::read_to_string(&output).unwrap(), "private");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drafts_and_conflicts_preserve_the_original() {
        let root = test_dir("draft");
        let path = root.join("chapter.txt");
        fs::write(&path, "base").unwrap();
        write_draft(&path, "base", "unsaved").unwrap();
        assert_eq!(load_draft(&path).unwrap().unwrap().content, "unsaved");
        fs::write(&path, "external").unwrap();
        assert_eq!(
            save_checked(&path, "base", "mine").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        assert!(crate::chapters::list_revisions_for(&path)
            .unwrap()
            .is_empty());
        save_checked(&path, "external", "merged").unwrap();
        assert_eq!(
            fs::read_to_string(crate::chapters::list_revisions_for(&path).unwrap()[0].clone())
                .unwrap(),
            "external"
        );
        clear_draft(&path).unwrap();
        assert!(load_draft(&path).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_backup_excludes_nested_projects_and_checks_collisions() {
        let root = test_dir("backup");
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("01_Chapter.txt"), "book").unwrap();
        for directory in [
            "projects",
            "Trash",
            ".metadata",
            ".drafts",
            ".revisions",
            ".document-trash",
        ] {
            fs::create_dir(source.join(directory)).unwrap();
            fs::write(source.join(directory).join("data"), "keep").unwrap();
        }
        let backup = root.join("backup");
        backup_project(&source, &backup).unwrap();
        assert!(!backup.join("projects").exists());
        assert!(!backup.join("Trash").exists());
        assert!(backup.join(".drafts/data").exists());
        assert!(backup.join(".revisions/data").exists());
        assert!(backup_project(&source, &backup).is_err());
        assert!(backup_project(&source, &source.join("nested")).is_err());
        let imported = import_backup(&backup, &root.join("projects"), "Restored").unwrap();
        assert_eq!(
            fs::read_to_string(imported.path.join("01_Chapter.txt")).unwrap(),
            "book"
        );
        assert!(import_backup(&backup, &root.join("projects"), "Restored").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_refused_and_incomplete_backup_is_removed() {
        let root = test_dir("symlink");
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("chapter.txt"), "book").unwrap();
        std::os::unix::fs::symlink("chapter.txt", source.join("link")).unwrap();
        assert!(backup_project(&source, &root.join("backup")).is_err());
        assert!(!root.join("backup").exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert!(write_draft(&source.join("link"), "", "draft").is_ok());
        assert!(save_checked(&source.join("link"), "book", "new").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
