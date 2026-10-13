use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TRASH: AtomicU64 = AtomicU64::new(0);

fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn real_directory(path: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory must not be a symlink",
        ));
    }
    Ok(())
}

fn trash_dir(book_dir: &str) -> PathBuf {
    PathBuf::from(book_dir).join("Trash")
}

#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

pub fn project_index(projects: &[Project], path: &Path) -> Option<usize> {
    projects.iter().position(|project| project.path == path)
}

pub fn load_projects(book_dir: &str) -> io::Result<Vec<Project>> {
    let root = PathBuf::from(book_dir);
    fs::create_dir_all(root.join("projects"))?;
    real_directory(&root.join("projects"))?;
    let mut projects = vec![Project {
        name: "Legacy Book".to_string(),
        path: root.clone(),
    }];
    let mut dirs: Vec<PathBuf> = fs::read_dir(root.join("projects"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| {
            fs::symlink_metadata(p)
                .map(|m| m.file_type().is_dir())
                .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    projects.extend(dirs.into_iter().map(|path| {
        Project {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled")
                .to_string(),
            path,
        }
    }));
    Ok(projects)
}

pub fn create_project(book_dir: &str, name: &str) -> io::Result<Project> {
    let name = name.trim();
    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project name cannot be empty",
        ));
    }
    if name == "." || name == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid project name",
        ));
    }
    let projects_dir = PathBuf::from(book_dir).join("projects");
    fs::create_dir_all(&projects_dir)?;
    real_directory(&projects_dir)?;
    let path = projects_dir.join(sanitize(name));
    if occupied(&path) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "project already exists",
        ));
    }
    fs::create_dir(&path)?;
    Ok(Project {
        name: name.to_string(),
        path,
    })
}

pub fn remove_project(book_dir: &str, project: &Project) -> io::Result<()> {
    let projects_dir = PathBuf::from(book_dir).join("projects");
    real_directory(&projects_dir)?;
    if project.path.parent() != Some(projects_dir.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only projects inside the projects directory can be removed",
        ));
    }

    real_directory(&project.path)?;
    let name = project
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid project name"))?;
    let trash = trash_dir(book_dir);
    fs::create_dir_all(&trash)?;
    real_directory(&trash)?;
    for _ in 0..10 {
        let id = format!(
            "{}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id(),
            NEXT_TRASH.fetch_add(1, Ordering::Relaxed)
        );
        let destination = trash.join(format!("{name}--trash-{id}"));
        if !occupied(&destination) {
            return fs::rename(&project.path, destination);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate trash destination",
    ))
}

/// Lists removed projects in the separate `Trash` directory.
pub fn list_trashed_projects(book_dir: &str) -> io::Result<Vec<Project>> {
    let trash = trash_dir(book_dir);
    if !occupied(&trash) {
        return Ok(Vec::new());
    }
    real_directory(&trash)?;
    let mut projects = Vec::new();
    for entry in fs::read_dir(trash)? {
        let path = entry?.path();
        if !fs::symlink_metadata(&path)?.file_type().is_dir() {
            continue;
        }
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some((name, _id)) = filename.rsplit_once("--trash-") {
            projects.push(Project {
                name: name.to_string(),
                path,
            });
        }
    }
    projects.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(projects)
}

/// Restores a project listed by `list_trashed_projects`, rejecting name collisions.
pub fn restore_project(book_dir: &str, trashed: &Project) -> io::Result<Project> {
    let trash = trash_dir(book_dir);
    real_directory(&trash)?;
    if trashed.path.parent() != Some(trash.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project is not in Trash",
        ));
    }
    let filename = trashed
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let (name, id) = filename
        .rsplit_once("--trash-")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid trashed project"))?;
    if id.is_empty() || name.is_empty() || name != trashed.name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid trashed project",
        ));
    }
    real_directory(&trashed.path)?;
    let projects = PathBuf::from(book_dir).join("projects");
    real_directory(&projects)?;
    let destination = projects.join(name);
    if occupied(&destination) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "project name already exists",
        ));
    }
    fs::rename(&trashed.path, &destination)?;
    Ok(Project {
        name: name.to_string(),
        path: destination,
    })
}

pub(crate) fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(format!(".bookapp_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn legacy_is_preserved_and_projects_are_discovered() {
        let dir = test_dir("projects");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("01_Existing.txt"), "keep").unwrap();
        let created = create_project(dir.to_str().unwrap(), "New Book").unwrap();
        assert!(dir.join("01_Existing.txt").exists());
        assert_eq!(created.path, dir.join("projects/New Book"));
        assert_eq!(load_projects(dir.to_str().unwrap()).unwrap().len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn project_index_finds_new_projects_independent_of_sort_order() {
        let dir = test_dir("project_order");
        let created = create_project(dir.to_str().unwrap(), "Alpha").unwrap();
        create_project(dir.to_str().unwrap(), "Zebra").unwrap();
        let projects = load_projects(dir.to_str().unwrap()).unwrap();

        let index = project_index(&projects, &created.path).unwrap();
        assert_eq!(projects[index].name, "Alpha");
        assert_ne!(index, projects.len() - 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removing_project_preserves_legacy_book_and_other_projects() {
        let dir = test_dir("remove_project");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("01_Legacy.txt"), "keep legacy").unwrap();
        let removed = create_project(dir.to_str().unwrap(), "Remove me").unwrap();
        let remaining = create_project(dir.to_str().unwrap(), "Keep me").unwrap();
        fs::write(removed.path.join("notes.txt"), "project content").unwrap();

        remove_project(dir.to_str().unwrap(), &removed).unwrap();

        assert!(!removed.path.exists());
        assert!(remaining.path.exists());
        assert_eq!(
            fs::read_to_string(dir.join("01_Legacy.txt")).unwrap(),
            "keep legacy"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removing_legacy_path_is_rejected() {
        let dir = test_dir("remove_legacy");
        fs::create_dir_all(&dir).unwrap();
        let legacy = Project {
            name: "Legacy Book".to_string(),
            path: dir.clone(),
        };

        assert!(remove_project(dir.to_str().unwrap(), &legacy).is_err());
        assert!(dir.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn removing_project_symlink_does_not_follow_target() {
        use std::os::unix::fs::symlink;

        let dir = test_dir("remove_project_symlink");
        let outside = test_dir("project_symlink_target");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        let link_path = dir.join("projects/linked");
        fs::create_dir_all(link_path.parent().unwrap()).unwrap();
        symlink(&outside, &link_path).unwrap();
        let linked = Project {
            name: "linked".to_string(),
            path: link_path.clone(),
        };

        assert!(remove_project(dir.to_str().unwrap(), &linked).is_err());

        assert!(fs::symlink_metadata(&link_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).unwrap(),
            "keep"
        );
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn trash_restore_and_collision() {
        let dir = test_dir("trash_restore");
        let original = create_project(dir.to_str().unwrap(), "Draft").unwrap();
        fs::write(original.path.join("chapter.txt"), "kept").unwrap();
        remove_project(dir.to_str().unwrap(), &original).unwrap();
        let trashed = list_trashed_projects(dir.to_str().unwrap()).unwrap();
        assert_eq!(trashed.len(), 1);
        assert_eq!(trashed[0].name, "Draft");
        let replacement = create_project(dir.to_str().unwrap(), "Draft").unwrap();
        remove_project(dir.to_str().unwrap(), &replacement).unwrap();
        assert_eq!(
            list_trashed_projects(dir.to_str().unwrap()).unwrap().len(),
            2
        );
        let replacement = create_project(dir.to_str().unwrap(), "Draft").unwrap();
        assert_eq!(
            restore_project(dir.to_str().unwrap(), &trashed[0])
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        fs::remove_dir(replacement.path).unwrap();
        let restored = restore_project(dir.to_str().unwrap(), &trashed[0]).unwrap();
        assert_eq!(
            fs::read_to_string(restored.path.join("chapter.txt")).unwrap(),
            "kept"
        );
        assert_eq!(
            list_trashed_projects(dir.to_str().unwrap()).unwrap().len(),
            1
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn trash_symlink_and_forged_path_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = test_dir("trash_symlinks");
        let outside = test_dir("trash_outside");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, dir.join("Trash")).unwrap_or_else(|_| {
            fs::create_dir_all(&dir).unwrap();
            symlink(&outside, dir.join("Trash")).unwrap();
        });
        let project = create_project(dir.to_str().unwrap(), "Draft").unwrap();
        assert!(remove_project(dir.to_str().unwrap(), &project).is_err());
        assert!(project.path.exists());
        let forged = Project {
            name: "Draft".into(),
            path: project.path.clone(),
        };
        assert!(restore_project(dir.to_str().unwrap(), &forged).is_err());
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
