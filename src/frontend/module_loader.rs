// src/frontend/module_loader.rs - HARDENED
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ModuleLoader {
    loaded_files: HashMap<PathBuf, ModuleInfo>,
    import_stack: Vec<PathBuf>,
    search_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct ModuleInfo {
    source: String,
}

impl ModuleLoader {
    pub fn new() -> Self {
        ModuleLoader {
            loaded_files: HashMap::new(),
            import_stack: Vec::new(),
            search_paths: vec![PathBuf::from(".")],
        }
    }

    pub fn add_search_path(&mut self, path: &Path) {
        self.search_paths.push(path.to_path_buf());
    }

    pub fn resolve_import(&self, import_path: &str, current_file: &str) -> Result<PathBuf> {
        let mut candidates = Vec::new();

        if import_path.ends_with(".gol") {
            candidates.push(PathBuf::from(import_path));
        } else {
            candidates.push(PathBuf::from(format!("{}.gol", import_path)));
        }

        let current_dir = Path::new(current_file).parent().unwrap_or(Path::new("."));
        for candidate in &candidates {
            let full_path = current_dir.join(candidate);
            if full_path.exists() {
                return Ok(full_path);
            }
        }

        for search_path in &self.search_paths {
            for candidate in &candidates {
                let full_path = search_path.join(candidate);
                if full_path.exists() {
                    return Ok(full_path);
                }
            }
        }

        Err(CompileError::simple(
            &format!("Module '{}' not found", import_path),
            0,
            0,
            "",
            ErrorCode::E0001,
        )
        .with_suggestion(&format!(
            "Check if '{}.gol' exists in the current directory or search paths",
            import_path
        )))
    }

    pub fn load_file(&mut self, path: &Path) -> Result<String> {
        let canonical = path.canonicalize().map_err(|e| {
            CompileError::simple(
                &format!("Failed to canonicalize path '{}': {}", path.display(), e),
                0, 0, "", ErrorCode::E0001,
            )
        })?;

        // Check for circular imports
        if self.import_stack.contains(&canonical) {
            let cycle: Vec<String> = self
                .import_stack
                .iter()
                .chain(std::iter::once(&canonical))
                .map(|p| p.display().to_string())
                .collect();
            return Err(CompileError::simple(
                &format!("Circular import detected: {}", cycle.join(" -> ")),
                0, 0, "", ErrorCode::E0001,
            )
            .with_suggestion("Break the import cycle by restructuring your modules"));
        }

        if let Some(info) = self.loaded_files.get(&canonical) {
            return Ok(info.source.clone());
        }

        let source = std::fs::read_to_string(&canonical).map_err(|e| {
            CompileError::simple(
                &format!("Failed to read module '{}': {}", canonical.display(), e),
                0, 0, "", ErrorCode::E0001,
            )
        })?;

        self.loaded_files.insert(
            canonical.clone(),
            ModuleInfo {
                source: source.clone(),
            },
        );

        Ok(source)
    }

    pub fn begin_import(&mut self, path: &Path) -> Result<()> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.import_stack.push(canonical);
        Ok(())
    }

    pub fn end_import(&mut self) {
        self.import_stack.pop();
    }

    pub fn is_loaded(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.loaded_files.contains_key(&canonical)
    }

    pub fn get_loaded_modules(&self) -> Vec<PathBuf> {
        self.loaded_files.keys().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.loaded_files.clear();
        self.import_stack.clear();
    }
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_import_detection() {
        let mut loader = ModuleLoader::new();
        let dir = std::env::temp_dir().join("algol26_test");
        std::fs::create_dir_all(&dir).unwrap();

        let file_a = dir.join("a.gol");
        let file_b = dir.join("b.gol");
        std::fs::write(&file_a, "import b").unwrap();
        std::fs::write(&file_b, "import a").unwrap();

        // Simulate circular: push a, then try to load b (which imports a)
        loader.begin_import(&file_a).unwrap();
        loader.begin_import(&file_b).unwrap();

        let result = loader.load_file(&file_a);
        assert!(result.is_err());

        loader.end_import();
        loader.end_import();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_module_caching() {
        let mut loader = ModuleLoader::new();
        let dir = std::env::temp_dir().join("algol26_test2");
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.gol");
        std::fs::write(&file, "procedure main\n    print(\"test\")").unwrap();

        let source1 = loader.load_file(&file).unwrap();
        let source2 = loader.load_file(&file).unwrap();
        assert_eq!(source1, source2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}