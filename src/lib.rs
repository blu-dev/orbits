pub mod tree;
pub mod loader;

pub enum FileEntryType {
    Directory,
    File
}

impl FileEntryType {
    pub fn is_dir(&self) -> bool {
        match self {
            Self::Directory => true,
            Self::File => false
        }
    }

    pub fn is_file(&self) -> bool {
        !self.is_dir()
    }
}

/// Determines how `orbits` will react to files which have file conflicts.
/// - `Strict` will cause `orbits` to return an `Err(PathBuf)` when a conflict is encountered. The `PathBuf` will be the full path to the conflicting file
/// - `NoRoot` will cause all of the files in the conflciting root to be removed from the tree if a child is encountered. Note that this is first come first serve **only**.
/// - `First` will keep the first file in all file conflicts.
/// - `Last` will cause all files to overwrite the previous file in all file conflicts
pub enum ConflictHandler {
    Strict,
    NoRoot,
    First,
    Last
}