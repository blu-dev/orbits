use std::path::{Path, PathBuf};

use super::FileEntryType;

pub trait FileLoader {
    type ErrorType; // ErrorType to facilitate integration into the user's system
    
    fn path_exists(&self, root_path: &Path, local_path: &Path) -> bool;
    fn get_file_size(&self, root_path: &Path, local_path: &Path) -> Option<usize>;
    fn get_path_type(&self, root_path: &Path, local_path: &Path) -> Result<FileEntryType, Self::ErrorType>;
    fn load_path(&self, root_path: &Path, local_path: &Path) -> Result<Vec<u8>, Self::ErrorType>;
    fn get_actual_path(&self, root_path: &Path, local_path: &Path) -> Option<PathBuf> {
        Some(root_path.join(local_path))
    }
}