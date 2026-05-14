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
        } else if input.contains(' ') {
            self.complete_path(input)
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
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir { format!("{name}/") } else { name }
            })
            .collect();
        matches.sort();
        matches
    }
}
