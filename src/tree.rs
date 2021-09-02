use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io;

use std::borrow::Borrow;
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
    InvalidFileName(PathBuf),
    #[error("Attempting to add child entry to file at '{0}'!")]
    FileChild(PathBuf),
    #[error("Path already exists, consumed by '{0}'")]
    OwnedPath(PathBuf),
    #[error("Failed to add child '{0}' to node at '{1}' despite it not existing!")]
    PhantomNode(String, PathBuf),
}

enum FsEntryType {
    Directory(usize),
    File(usize)
}

impl FsEntryType {
    pub fn is_dir(&self) -> bool {
        match self {
            Self::Directory(_) => true,
            Self::File(_) => false
        }
    }

    pub fn is_file(&self) -> bool {
        match self {
            Self::Directory(_) => false,
            Self::File(_) => true
        }
    }
}

// Separate implementations of a hash are important for keeping track of files in different systems
trait TreeNode {
    type TreeKey;
    type ErrorType;

    fn get_key(&self) -> Self::TreeKey;

    fn on_child_added(&mut self, child: &Self) -> Result<(), Self::ErrorType>;
    fn on_child_removed(&mut self, child: Self) -> Result<(), Self::ErrorType>;
}

struct Node<T: TreeNode> {
    key: T::TreeKey,
    data: T,
    children: HashMap<T::TreeKey, Node<T>>
}

impl<T: TreeNode> Node<T> where <T as TreeNode>::TreeKey: Hash + Eq + Clone{
    pub fn new(base: T) -> Self {
        let key = base.get_key();
        Self {
            key,
            data: base,
            children: HashMap::new()
        }
    }

    pub fn get_child<A: Borrow<T::TreeKey>>(&self, key: A) -> Option<&Self> {
        self.children.get(key.borrow())
    }

    pub fn get_child_mut<A: Borrow<T::TreeKey>>(&mut self, key: A) -> Option<&mut Self> {
        self.children.get_mut(key.borrow())
    }

    pub fn try_add_child(&mut self, data: T, overwrite: bool) -> &mut Self {
        let key = data.get_key();
        if !self.children.contains_key(&key) | overwrite {
            if let Some(original) = self.children.insert(key.clone(), Self::new(data)) {
                let Self { key: _, data: data, children: _ } = original;
                self.data.on_child_removed(data);
            }
            let data = self.children.get(&key).expect("Failed to find child immediately after adding it!");
            let data = &data.data;
            self.data.on_child_added(data);
        }
        self.children.get_mut(&key).expect("Failed to find child after guaranteeing existence!")
    }

    pub fn add_child(&mut self, data: T, overwrite: bool) -> Result<bool, T::ErrorType> {
        let key = data.get_key();
        // TODO: Change from nightly once rust allows try insert
        if self.children.contains_key(&key) {
            if overwrite {
                let Self { key: _, data: data, children: _ } = self.children.insert(key, Self::new(data)).expect("No original member found despite promise!");
                self.data.on_child_removed(data);
            }
            Ok(overwrite)
        } else {
            self.children.insert(key.clone(), Self::new(data));
            let data = self.children.get(&key).expect("Failed to find child immediately after adding it!");
            let data = &data.data;
            self.data.on_child_added(data);
            Ok(true)
        }
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
    type ErrorType = TreeError;

    fn get_key(&self) -> Self::TreeKey {
        self.name.clone()
    }

    fn on_child_added(&mut self, _child: &Self) -> Result<(), Self::ErrorType> {
        if let FsEntryType::Directory(count) = &mut self.entry_type {
            *count += 1;
            Ok(())
        } else {
            Err(TreeError::FileChild(self.local_path.clone()))
        }
    }

    fn on_child_removed(&mut self, _child: Self) -> Result<(), Self::ErrorType> {
        if let FsEntryType::Directory(count) = &mut self.entry_type {
            *count -= 1;
        }
        Ok(())
    }
}

impl FsNode {
    fn validate_get_fs_entry(path: &Path) -> Result<FsEntryType, TreeError> {
        if !path.exists() {
            Err(TreeError::PathDoesNotExist(path.to_path_buf()))
        } else {
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    if metadata.is_file() {
                        Ok(FsEntryType::File(metadata.len() as usize))
                    } else if metadata.is_dir() {
                        Ok(FsEntryType::Directory(0))
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

    pub(crate) fn empty() -> Self {
        Self {
            name: String::default(),
            local_path: PathBuf::default(),
            root_path: PathBuf::default(),
            entry_type: FsEntryType::Directory(0)
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
pub struct FsTree(Node<FsNode>);

impl FsTree {
    pub fn add_path<P: AsRef<Path>, R: AsRef<Path>>(&mut self, root_path: P, local_path: R) -> Result<(), TreeError> {
        let root_path = root_path.as_ref();
        let local_path = local_path.as_ref();
        let mut components = local_path.components();
        let mut current_path = PathBuf::new();

        let mut current_node = Some(&mut self.0);
        while let Some(node) = current_node.take() {
            if let Some(next) = components.next() {
                if node.data.entry_type.is_file() {
                    return Err(TreeError::FileChild(root_path.join(&current_path)));
                }
                let next = next.as_os_str();
                current_path = current_path.join(next);
                current_node = Some(node.try_add_child(FsNode::new(&current_path, root_path)?, false));
            }
        }

        Ok(())
    }

    pub fn new() -> Self {
        Self(Node::new(FsNode::empty()))
    }
}