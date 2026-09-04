// ALGOL26 - Module Loader
// Handles importing files and resolving modules

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};

pub struct ModuleLoader {
    loaded_files: HashSet<PathBuf>,
    import_stack: Vec<PathBuf>,
}

impl ModuleLoader {
    pub fn new() -> Self {
        ModuleLoader {
            loaded_files: HashSet::new(),
            import_stack: Vec::new(),
        }
    }
    
    pub fn resolve_import(&mut self, import_path: &str, current_file: &str) -> Result<PathBuf> {
        let current_dir = Path::new(current_file).parent().unwrap_or(Path::new("."));
        
        let resolved = if import_path.ends_with(".gol") {
            current_dir.join(import_path)
        } else {
            current_dir.join(format!("{}.gol", import_path))
        };
        
        // Check for circular imports
        let canonical = resolved.canonicalize().unwrap_or(resolved.clone());
        if self.import_stack.contains(&canonical) {
            return Err(CompileError::new(
                &format!("Circular import detected: {}", import_path),
                0, 0, "",
                ErrorCode::E0001,
            ));
        }
        
        Ok(resolved)
    }
    
    pub fn load_file(&mut self, path: &Path) -> Result<String> {
        let canonical = path.canonicalize().unwrap_or(path.to_path_buf());
        
        if self.loaded_files.contains(&canonical) {
            return Ok(String::new()); // Already loaded
        }
        
        let source = std::fs::read_to_string(path).map_err(|e| {
            CompileError::new(
                &format!("Failed to read module '{}': {}", path.display(), e),
                0, 0, "",
                ErrorCode::E0001,
            )
        })?;
        
        self.loaded_files.insert(canonical.clone());
        self.import_stack.push(canonical);
        
        Ok(source)
    }
    
    pub fn pop_import(&mut self) {
        self.import_stack.pop();
    }
    
    pub fn is_loaded(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or(path.to_path_buf());
        self.loaded_files.contains(&canonical)
    }
}

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new()
    }
}
