use std::path::{Path, PathBuf};

pub struct ContextLoader {
    cwd: PathBuf,
}

pub struct LoadedContext {
    pub agents_content: String,
    pub skills: Vec<(String, String)>,
}

impl ContextLoader {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    pub fn load(&self) -> LoadedContext {
        let agents = Self::walk_up(&self.cwd, "AGENTS.md")
            .or_else(|| Self::walk_up(&self.cwd, "UNCODE.md"))
            .unwrap_or_default();

        let skills = Self::load_skills();

        LoadedContext {
            agents_content: agents,
            skills,
        }
    }

    fn walk_up(start: &Path, filename: &str) -> Option<String> {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join(filename);
            if candidate.exists()
                && let Ok(content) = std::fs::read_to_string(&candidate)
            {
                return Some(content);
            }
            if !current.pop() {
                break;
            }
        }
        None
    }

    fn load_skills() -> Vec<(String, String)> {
        let mut skills = Vec::new();
        for dir in skill_dirs() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path().join("SKILL.md");
                    if path.exists()
                        && let Ok(content) = std::fs::read_to_string(&path)
                    {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let desc = content
                            .lines()
                            .find_map(|l| l.strip_prefix("description:"))
                            .map(|s| s.trim().to_string())
                            .unwrap_or_default();
                        skills.push((name, desc));
                    }
                }
            }
        }
        skills
    }
}

fn skill_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from(".uncode/skills")];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".uncode/skills"));
        dirs.push(home.join(".config/opencode/skills"));
    }
    dirs.into_iter().filter(|d| d.exists()).collect()
}
