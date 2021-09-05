use std::path::Path;

use crate::loader::FileLoader;
use crate::FileEntryType;

pub struct StandardLoader;

impl FileLoader for StandardLoader {
    type ErrorType = std::io::Error;

    fn path_exists(&self, root_path: &Path, local_path: &Path) -> bool {
        let full_path = root_path.join(local_path);

        full_path.exists()
    }

    fn get_path_type(&self, root_path: &Path, local_path: &Path) -> Result<FileEntryType, Self::ErrorType> {
        let full_path = root_path.join(local_path);

        let metadata = std::fs::metadata(&full_path)?;
        if metadata.is_dir() {
            Ok(FileEntryType::Directory)
        } else if metadata.is_file() {
            Ok(FileEntryType::File)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Filepath '{}' has an unsupported entry type.", full_path.display())))
        }
    }

    fn load_path(&self, root_path: &Path, local_path: &Path) -> Result<Vec<u8>, Self::ErrorType> {
        let full_path = root_path.join(local_path);
        if !full_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Filepath '{}' does not exist!", full_path.display())
            ));
        }

        std::fs::read(full_path)
    }
}