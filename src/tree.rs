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
    Directory,
    File
}

impl FsEntryType {
    pub fn is_dir(&self) -> bool {
        match self {
            Self::Directory => true,
            Self::File => false
        }
    }

    pub fn is_file(&self) -> bool {
        match self {
            Self::Directory => false,
            Self::File => true
        }
    }
}

// Separate implementations of a hash are important for keeping track of files in different systems
trait TreeNode {
    type TreeKey;
    type ErrorType;

    fn get_key(&self) -> Self::TreeKey;
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

    pub fn children(&self) -> std::collections::hash_map::Iter<T::TreeKey, Node<T>> {
        self.children.iter()
    }

    pub fn children_mut(&mut self) -> std::collections::hash_map::IterMut<T::TreeKey, Node<T>> {
        self.children.iter_mut()
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
            }
            let data = self.children.get(&key).expect("Failed to find child immediately after adding it!");
            let data = &data.data;
        }
        self.children.get_mut(&key).expect("Failed to find child after guaranteeing existence!")
    }

    pub fn add_child(&mut self, data: T, overwrite: bool) -> Result<bool, T::ErrorType> {
        let key = data.get_key();
        // TODO: Change from nightly once rust allows try insert
        if self.children.contains_key(&key) {
            if overwrite {
                let Self { key: _, data: data, children: _ } = self.children.insert(key, Self::new(data)).expect("No original member found despite promise!");
            }
            Ok(overwrite)
        } else {
            self.children.insert(key.clone(), Self::new(data));
            let data = self.children.get(&key).expect("Failed to find child immediately after adding it!");
            let data = &data.data;
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
    maps_physically: bool,
}

impl TreeNode for FsNode {
    type TreeKey = String;
    type ErrorType = TreeError;

    fn get_key(&self) -> Self::TreeKey {
        self.name.clone()
    }
}

impl FsNode {
    fn validate_get_fs_entry(path: &Path, should_be_physical: bool) -> Result<FsEntryType, TreeError> {
        if !path.exists() && should_be_physical {
            Err(TreeError::PathDoesNotExist(path.to_path_buf()))
        } else {
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    if metadata.is_file() {
                        Ok(FsEntryType::File)
                    } else if metadata.is_dir() {
                        Ok(FsEntryType::Directory)
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

    pub(crate) fn empty(maps_physically: bool) -> Self {
        Self {
            name: String::default(),
            local_path: PathBuf::default(),
            root_path: PathBuf::default(),
            entry_type: FsEntryType::Directory,
            maps_physically
        }
    }

    pub fn new<A: AsRef<Path>, B: AsRef<Path>>(local_path: A, root_path: B, maps_physically: bool) -> Result<Self, TreeError> {
        let local_path = local_path.as_ref().to_path_buf();
        let root_path = root_path.as_ref().to_path_buf();

        let entry_type = Self::validate_get_fs_entry(&root_path.join(&local_path), maps_physically)?;
        let name = Self::get_file_name(&local_path)?;
        Ok(Self {
            name,
            local_path,
            root_path,
            entry_type,
            maps_physically
        })
    }

    pub fn change_root<A: AsRef<Path>>(&mut self, new_root: A) -> Result<(), TreeError> {
        let new_root = new_root.as_ref();

        // This is not required to maintain the same entry type, so we refresh
        let entry_type = Self::validate_get_fs_entry(&new_root.join(&self.local_path), self.maps_physically)?;
        self.root_path = new_root.to_path_buf();
        self.entry_type = entry_type;
        Ok(())
    }

    pub fn change_local_path<A: AsRef<Path>>(&mut self, new_local: A) -> Result<(), TreeError> {
        let new_local = new_local.as_ref();

        // This is not required to maintain the same entry type, so we refresh
        let entry_type = Self::validate_get_fs_entry(&new_local.join(&self.local_path), self.maps_physically)?;
        let name = Self::get_file_name(new_local)?;
        self.local_path = new_local.to_path_buf();
        self.entry_type = entry_type;
        Ok(())
    }

    pub fn full_path(&self) -> PathBuf {
        self.root_path.join(&self.local_path)
    }
}

pub struct FsTree{
    config: FsTreeConfig,
    top: Node<FsNode>
}

pub struct FsTreeConfig {
    pub requires_physical: bool
}

impl FsTreeConfig {
    pub fn complete(self) -> FsTree {
        let top = Node::new(FsNode::empty(self.requires_physical));
        FsTree { config: self, top: top }
    }
}

impl FsTree {
    pub fn add_path<P: AsRef<Path>, R: AsRef<Path>>(&mut self, root_path: P, local_path: R) -> Result<(), TreeError> {
        let root_path = root_path.as_ref();
        let local_path = local_path.as_ref();
        let mut components = local_path.components();
        let mut current_path = PathBuf::new();

        let mut current_node = Some(&mut self.top);
        while let Some(node) = current_node.take() {
            if let Some(next) = components.next() {
                if node.data.entry_type.is_file() {
                    return Err(TreeError::FileChild(root_path.join(&current_path)));
                }
                let next = next.as_os_str();
                current_path = current_path.join(next);
                current_node = Some(node.try_add_child(FsNode::new(&current_path, root_path, self.config.requires_physical)?, false));
            }
        }

        Ok(())
    }

    pub fn walk_paths(&self, mut f: impl FnMut(&FsNode)) {
        fn internal(node: &Node<FsNode>, f: &mut impl FnMut(&FsNode)) {
            for (_, child) in node.children() {
                f(&child.data);
                if child.data.entry_type.is_dir() {
                    internal(child, f);
                }
            }
        }

        internal(&self.top, &mut f);
    }

    pub fn new() -> FsTreeConfig {
        FsTreeConfig { 
            requires_physical: false,
        }
    }
}