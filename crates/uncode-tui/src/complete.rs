use std::path::Path;

pub struct CompletionEngine {
    slash_commands: Vec<String>,
}

impl CompletionEngine {
    pub fn new(slash_commands: Vec<String>) -> Self {
        Self { slash_commands }
    }

    pub fn complete(&self, input: &str) -> Vec<String> {
        if input.starts_with('/') {
            self.complete_slash(input)
        } else {
            self.complete_path(input)
        }
    }

    fn complete_slash(&self, input: &str) -> Vec<String> {
        let prefix = input.trim_start_matches('/');
        if prefix.is_empty() {
            return self
                .slash_commands
                .iter()
                .map(|c| format!("/{c}"))
                .collect();
        }
        self.slash_commands
            .iter()
            .filter(|c| c.starts_with(prefix))
            .map(|c| format!("/{c}"))
            .collect()
    }

    fn complete_path(&self, input: &str) -> Vec<String> {
        let (dir, prefix) = match input.rsplit_once(' ') {
            Some((_, last))
                if last.starts_with('/') || last.starts_with("./") || last.starts_with("../") =>
            {
                let path = Path::new(last);
                let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let prefix = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                (dir, prefix)
            }
            _ => return vec![],
        };

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return vec![],
        };

        let mut matches: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                if is_dir { format!("{name}/") } else { name }
            })
            .collect();
        matches.sort_unstable();
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_slash_all() {
        let engine = CompletionEngine::new(vec!["help".into(), "quit".into()]);
        let completions = engine.complete("/");
        assert_eq!(completions.len(), 2);
        assert!(completions.contains(&"/help".to_owned()));
    }

    #[test]
    fn test_complete_slash_prefix() {
        let engine = CompletionEngine::new(vec!["help".into(), "quit".into()]);
        let completions = engine.complete("/h");
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0], "/help");
    }

    #[test]
    fn test_complete_slash_no_match() {
        let engine = CompletionEngine::new(vec!["help".into()]);
        let completions = engine.complete("/xyz");
        assert!(completions.is_empty());
    }

    #[test]
    fn test_complete_non_path() {
        let engine = CompletionEngine::new(vec!["help".into()]);
        let completions = engine.complete("hello world");
        assert!(completions.is_empty());
    }
}
