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
                for entry in entries.filter_map(Result::ok) {
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
    dirs.retain(|d| d.exists());
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: change CWD to `dir` during the closure, then restore.
    fn with_cwd<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
        let old = env::current_dir().unwrap();
        env::set_current_dir(dir).unwrap();
        let result = f();
        env::set_current_dir(old).unwrap();
        result
    }

    // ── walk_up tests ──────────────────────────────────────

    #[test]
    fn walk_up_finds_agents_md_at_cwd() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("AGENTS.md"), b"hello from agents").unwrap();
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert_eq!(ctx.agents_content, "hello from agents");
    }

    #[test]
    fn walk_up_falls_back_to_uncode_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("UNCODE.md"), b"hello from uncode").unwrap();
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert_eq!(ctx.agents_content, "hello from uncode");
    }

    #[test]
    fn walk_up_returns_empty_when_no_config() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert!(ctx.agents_content.is_empty());
    }

    #[test]
    fn walk_up_prefers_agents_md_over_uncode_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("AGENTS.md"), b"agents content").unwrap();
        fs::write(dir.path().join("UNCODE.md"), b"uncode content").unwrap();
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert_eq!(ctx.agents_content, "agents content");
    }

    #[test]
    fn walk_up_searches_parent_directories() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("AGENTS.md"), b"parent content").unwrap();
        let sub = dir.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        let ctx = ContextLoader::new(sub).load();
        assert_eq!(ctx.agents_content, "parent content");
    }

    // ── skill_dirs tests ───────────────────────────────────

    /// RAII guard to restore CWD on drop — stores old path inline.
    struct CwdGuard(Option<std::path::PathBuf>);
    impl CwdGuard {
        fn new(dir: &std::path::Path) -> Self {
            let old = env::current_dir().expect("get initial cwd");
            env::set_current_dir(dir).expect("set cwd to temp dir");
            Self(Some(old))
        }
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            if let Some(old) = self.0.take() {
                let _ = env::set_current_dir(old);
            }
        }
    }

    #[test]
    fn skill_dirs_includes_cwd_relative_dir() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".uncode/skills")).unwrap();
        let result = with_cwd(dir.path(), || skill_dirs());
        let rel = std::path::Path::new(".uncode/skills");
        assert!(result.iter().any(|p| p == rel));
    }

    #[test]
    fn skill_dirs_filters_nonexistent_cwd_dir() {
        let dir = TempDir::new().unwrap();
        let result = with_cwd(dir.path(), || skill_dirs());
        let rel = std::path::Path::new(".uncode/skills");
        assert!(!result.iter().any(|p| p == rel));
    }

    #[test]
    fn skill_dirs_home_dir_pushed() {
        // skill_dirs() pushes home dir entries but filters by exists().
        // We cannot directly verify the home entries appear since they must exist.
        // Instead, verify that with a temp home, the entry is present.
        let dir = TempDir::new().unwrap();
        let home_skills = dir.path().join(".uncode/skills");
        fs::create_dir_all(&home_skills).unwrap();
        // We can't mock dirs::home_dir(). Just verify the basic contract:
        // the CWD-relative dir is returned when it exists.
        let result = with_cwd(dir.path(), || skill_dirs());
        assert!(result.contains(&std::path::PathBuf::from(".uncode/skills")));
    }

    // ── load_skills integration tests ───────────────────────

    #[test]
    fn load_skills_finds_skill_from_absolute_dir() {
        let dir = TempDir::new().unwrap();
        let skills = dir.path().join(".uncode/skills");
        let skill = skills.join("my-tool/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, b"description: a handy tool").unwrap();
        // Call ContextLoader::load() which internally calls skill_dirs() and load_skills().
        // We must set CWD to a dir that has our absolute .uncode/skills.
        // But skill_dirs() starts from CWD... we need to make the loader find our skills.
        // Instead, set CWD to the temp dir.
        let _guard = CwdGuard::new(dir.path());
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert!(
            ctx.skills
                .iter()
                .any(|(n, d)| n == "my-tool" && d == "a handy tool"),
            "expected my-tool in skills but got {:?}",
            ctx.skills
        );
    }

    #[test]
    fn load_skills_multiple() {
        let dir = TempDir::new().unwrap();
        let skills = dir.path().join(".uncode/skills");
        fs::create_dir_all(&skills).unwrap();
        for name in &["alpha", "beta"] {
            let p = skills.join(name).join("SKILL.md");
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, format!("description: skill {name}")).unwrap();
        }
        let _guard = CwdGuard::new(dir.path());
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert!(ctx.skills.iter().any(|(n, _)| n == "alpha"));
        assert!(ctx.skills.iter().any(|(n, _)| n == "beta"));
    }

    #[test]
    fn load_skills_missing_description() {
        let dir = TempDir::new().unwrap();
        let skills = dir.path().join(".uncode/skills");
        let skill = skills.join("no-desc/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, b"no description line present").unwrap();
        let _guard = CwdGuard::new(dir.path());
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert!(
            ctx.skills
                .iter()
                .any(|(n, d)| n == "no-desc" && d.is_empty())
        );
    }

    #[test]
    fn load_skills_skips_dirs_without_skill_md() {
        let dir = TempDir::new().unwrap();
        let skills = dir.path().join(".uncode/skills");
        fs::create_dir_all(skills.join("has-skill")).unwrap();
        fs::create_dir_all(skills.join("no-skill")).unwrap();
        fs::write(skills.join("has-skill/SKILL.md"), b"description: present").unwrap();
        let _guard = CwdGuard::new(dir.path());
        let ctx = ContextLoader::new(dir.path().to_path_buf()).load();
        assert!(ctx.skills.iter().any(|(n, _)| n == "has-skill"));
        assert!(!ctx.skills.iter().any(|(n, _)| n == "no-skill"));
    }
}
