use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

pub fn load_projects(book_dir: &str) -> io::Result<Vec<Project>> {
    let root = PathBuf::from(book_dir);
    fs::create_dir_all(root.join("projects"))?;
    let mut projects = vec![Project {
        name: "Legacy Book".to_string(),
        path: root.clone(),
    }];
    let mut dirs: Vec<PathBuf> = fs::read_dir(root.join("projects"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
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
    let path = PathBuf::from(book_dir)
        .join("projects")
        .join(sanitize(name));
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "project already exists",
        ));
    }
    fs::create_dir_all(&path)?;
    Ok(Project {
        name: name.to_string(),
        path,
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
}
