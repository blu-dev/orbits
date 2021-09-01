use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io;

use std::cmp::PartialEq;
use std::ops::{Deref, DerefMut};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TreeError {
    #[error("The path '{0}' does not exist!")]
    PathDoesNotExist(PathBuf),
    #[error("The path '{0}' has invalid metadata!")]
    InvalidMetadata(PathBuf),
    #[error("Failed to get metadata for path '{0}'! io::Errror: {1:?}")]
    FailedMetadata(PathBuf, io::Error),
    #[error("The path '{0}' is missing a file name!")]
    NoFileName(PathBuf),
    #[error("The path '{0}' does not contain a valid filename!")]
    InvalidFileName(PathBuf)
}

enum FsEntryType {
    Directory(usize),
    File(usize)
}

// Separate implementations of a hash are important for keeping track of files in different systems
trait TreeNode {
    type TreeKey;

    fn get_key(&self) -> Self::TreeKey;
}

struct Node<T: TreeNode> {
    key: T::TreeKey,
    data: T,
    children: HashMap<T::TreeKey, Node<T>>
}

impl<T: TreeNode> Node<T> where <T as TreeNode>::TreeKey: Hash + Eq {
    pub fn get_child<A: AsRef<T::TreeKey>>(&self, key: A) -> Option<&Node<T>> {
        self.children.get(key.as_ref())
    }
} 

impl<T: TreeNode> Hash for Node<T> where <T as TreeNode>::TreeKey: Hash {
    fn hash<A: Hasher>(&self, state: &mut A) {
        self.key.hash(state);
    }
}

pub struct FsNode {
    name: String,
    local_path: PathBuf,
    root_path: PathBuf,
    entry_type: FsEntryType,
}

impl TreeNode for FsNode {
    type TreeKey = String;

    fn get_key(&self) -> Self::TreeKey {
        self.name.clone()
    }
}

impl FsNode {
    fn validate_get_fs_entry(path: &Path) -> Result<FsEntryType, TreeError> {
        if !path.exists() {
            Err(TreeError::PathDoesNotExist(path.to_path_buf()))
        } else {
            match std::fs::metadata(&path) { // probably a better way to do this but for now this is fine
                Ok(metadata) => {
                    if metadata.is_file() {
                        Ok(FsEntryType::File(metadata.len() as usize))
                    } else if metadata.is_dir() {
                        Ok(FsEntryType::Directory(std::fs::read_dir(path).unwrap().count()))
                    } else {
                        Err(TreeError::InvalidMetadata(path.to_path_buf()))
                    }
                },
                Err(err) => Err(TreeError::FailedMetadata(path.to_path_buf(), err))
            }
        }
    }

    fn get_file_name(path: &Path) -> Result<String, TreeError> {
        match path.file_name() {
            Some(name) => {
                if let Some(name) = name.to_str() {
                    Ok(String::from(name))
                } else {
                    Err(TreeError::InvalidFileName(path.to_path_buf()))
                }
            },
            None => {
                Err(TreeError::NoFileName(path.to_path_buf()))
            }
        }
    }

    pub fn new<A: AsRef<Path>, B: AsRef<Path>>(local_path: A, root_path: B) -> Result<Self, TreeError> {
        let local_path = local_path.as_ref().to_path_buf();
        let root_path = root_path.as_ref().to_path_buf();

        let entry_type = Self::validate_get_fs_entry(&root_path.join(&local_path))?;
        let name = Self::get_file_name(&local_path)?;
        Ok(Self {
            name,
            local_path,
            root_path,
            entry_type
        })
    }

    pub fn change_root<A: AsRef<Path>>(&mut self, new_root: A) -> Result<(), TreeError> {
        let new_root = new_root.as_ref();

        // This is not required to maintain the same entry type, so we refresh
        let entry_type = Self::validate_get_fs_entry(&new_root.join(&self.local_path))?;
        self.root_path = new_root.to_path_buf();
        self.entry_type = entry_type;
        Ok(())
    }

    pub fn change_local_path<A: AsRef<Path>>(&mut self, new_local: A) -> Result<(), TreeError> {
        let new_local = new_local.as_ref();

        // This is not required to maintain the same entry type, so we refresh
        let entry_type = Self::validate_get_fs_entry(&new_local.join(&self.local_path))?;
        let name = Self::get_file_name(new_local)?;
        self.local_path = new_local.to_path_buf();
        self.entry_type = entry_type;
        Ok(())
    }

    pub fn full_path(&self) -> PathBuf {
        self.root_path.join(&self.local_path)
    }
}

#[repr(transparent)]
pub struct FsTree(FsNode);

impl Deref for FsTree {
    type Target = FsNode;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}


impl DerefMut for FsTree {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FsTree {
    pub fn has_entry() {

    }
}