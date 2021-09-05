use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::borrow::Borrow;
use std::path::{Path, PathBuf};
use std::io;

use thiserror::Error;

use crate::{FileEntryType, loader::FileLoader};

pub mod node;
pub mod loader;

use node::Node;

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


// Separate implementations of a hash are important for keeping track of files in different systems
trait TreeNode {
    type TreeKey;
    type ErrorType;

    fn get_key(&self) -> Self::TreeKey;
}

struct RawNode<T: TreeNode> {
    key: T::TreeKey,
    data: T,
    children: HashMap<T::TreeKey, RawNode<T>>
}

impl<T: TreeNode> RawNode<T> where <T as TreeNode>::TreeKey: Hash + Eq + Clone{
    pub fn new(base: T) -> Self {
        let key = base.get_key();
        Self {
            key,
            data: base,
            children: HashMap::new()
        }
    }

    pub fn children(&self) -> std::collections::hash_map::Iter<T::TreeKey, RawNode<T>> {
        self.children.iter()
    }

    pub fn children_mut(&mut self) -> std::collections::hash_map::IterMut<T::TreeKey, RawNode<T>> {
        self.children.iter_mut()
    }

    pub fn get_child<A: Borrow<T::TreeKey>>(&self, key: A) -> Option<&Self> {
        self.children.get(key.borrow())
    }

    pub fn get_child_mut<A: Borrow<T::TreeKey>>(&mut self, key: A) -> Option<&mut Self> {
        self.children.get_mut(key.borrow())
    }

    pub fn add_child(&mut self, data: T, overwrite: bool) -> Option<T> {
        let key = data.get_key();
        // TODO: Change from nightly once rust allows try insert
        if self.children.contains_key(&key) {
            if overwrite {
                let Self { data, .. } = self.children.insert(key, Self::new(data)).expect("No original member found despite promise!");
                Some(data)
            } else {
                None
            }
        } else {
            self.children.insert(key.clone(), Self::new(data));
            None
        }
    }
} 

impl<T: TreeNode> Hash for RawNode<T> where <T as TreeNode>::TreeKey: Hash {
    fn hash<A: Hasher>(&self, state: &mut A) {
        self.key.hash(state);
    }
}

pub struct Tree<L: FileLoader> {
    loader: L,
    root: RawNode<Node>
}

impl<L: FileLoader> Tree<L> {
    fn get_path(&self, path: &Path) -> Option<&RawNode<Node>> {
        let mut keys = path
            .components()
            .map(|x| x.as_os_str().to_str().expect("Unable to get str from OsStr").to_string())
            .collect::<Vec<String>>()
            .into_iter();

        let mut current_node = Some(&self.root);

        while let Some(next_key) = keys.next() {
            if let Some(node) = current_node.take() {
                if let Some(next_node) = node.get_child(&next_key) {
                    current_node = Some(next_node);
                }
            } else {
                return None;
            }
        }
        current_node
    }

    fn get_path_mut(&mut self, path: &Path) -> Option<&mut RawNode<Node>> {
        let mut keys = path
            .components()
            .map(|x| x.as_os_str().to_str().expect("Unable to get str from OsStr").to_string())
            .collect::<Vec<String>>()
            .into_iter();

        let mut current_node = Some(&mut self.root);

        while let Some(next_key) = keys.next() {
            if let Some(node) = current_node.take() {
                if let Some(next_node) = node.get_child_mut(&next_key) {
                    current_node = Some(next_node);
                }
            } else {
                return None;
            }
        }
        current_node
    }

    /// Attempts to load the specified local path with the loader. If the path is not contained inside of the tree, then `Ok(None)` is returned.
    /// The loader is responsible for returning valid data. If it can't load valid data, it is expected to return an `Err(L::ErrorType)`
    pub fn load<P: AsRef<Path>>(&self, path: P) -> Result<Option<Vec<u8>>, L::ErrorType> {
        let path = path.as_ref();
        if let Some(node) = self.get_path(path) {
            Ok(Some(self.loader.load_path(&node.data.root_path, &node.data.local_path)?))
        } else {
            Ok(None)
        }
    }

    /// Checks the filesystem to see if a file exists
    pub fn contains_path<P: AsRef<Path>>(&self, path: P) -> bool {
        self.get_path(path.as_ref()).is_some()
    }

    /// Inserts a path into the file tree. If a previous entry existed, it gets replaced and the root/local path is returned.
    pub fn insert_path<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        let root_path = root_path.as_ref();
        let local_path = local_path.as_ref();
        let node = Node::new(root_path, local_path).unwrap();
        let parent_node = if let Some(parent_path) = local_path.parent() {
            if let Some(parent) = self.get_path_mut(parent_path) {
                parent
            } else {
                assert!(self.insert_path("", parent_path).is_none());
                self.get_path_mut(parent_path).expect("Failed to find parent node immediately after adding it!")
            }
        } else {
            &mut self.root 
        };
        
        if let Some(Node { local_path: local, root_path: root, .. }) = parent_node.add_child(node, true) {
            Some((root, local))
        } else {
            None
        }
    }

    pub fn remove_path<P: AsRef<Path>>(&mut self, path: P) -> Option<(PathBuf, PathBuf)> {
        let path = path.as_ref();
        let name = path
            .as_os_str()
            .to_str()
            .expect("Unable to convert OsStr to str")
            .to_string();
        let parent_node = if let Some(parent_path) = path.parent() {
            if let Some(parent) = self.get_path_mut(parent_path) {
                parent
            } else {
                return None;
            }
        } else {
            &mut self.root
        };

        if let Some(RawNode { data: Node { name: _, local_path: local, root_path: root }, .. }) = parent_node.children.remove(&name) {
            Some((root, local))
        } else {
            None
        }
    }

    pub fn remove_paths_by_root<P: AsRef<Path>>(&mut self, root: P) -> Vec<(PathBuf, PathBuf)> {
        let remove = root.as_ref();
        let mut to_remove = Vec::new();
        self.walk_paths(|node| {
            if let Ok(FileEntryType::File) = self.loader.get_path_type(&node.root_path, &node.local_path) {
                if node.root_path == remove {
                    to_remove.push(node.local_path.clone());
                }
            }
        });
        to_remove
            .into_iter()
            .filter_map(|local_path| {
                if let Some((root, local)) = self.remove_path(&local_path) {
                    Some((root, local))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn walk_paths<F: FnMut(&Node)>(&self, mut f: F) {
        fn internal<F: FnMut(&Node)>(node: &RawNode<Node>, f: &mut F) {
            f(&node.data);
            for (_, child) in node.children() {
                internal(child, f);
            }
        }
        internal(&self.root, &mut f);
    }

    pub fn filter_walk_paths<C, F: FnMut(&Node) -> Option<C>>(&mut self, mut f: F) -> Vec<(PathBuf, PathBuf, C)> {
        fn internal<C, F: FnMut(&Node) -> Option<C>>(node: &mut RawNode<Node>, f: &mut F, rejected: &mut Vec<(PathBuf, C)>) {
            if let Some(complaint) = f(&node.data) {
                rejected.push((node.data.local_path.clone(), complaint));
            } else {
                for (_, child) in node.children_mut() {
                    internal(child, f, rejected);
                }
            }
        }
        let mut rejected: Vec<(PathBuf, C)> = Vec::new();
        internal(&mut self.root, &mut f, &mut rejected);
        rejected.into_iter().filter_map(|(local_path, reason)| {
            if let Some((root, local)) = self.remove_path(&local_path) {
                Some((root, local, reason))
            } else {
                None
            }
        }).collect()
    }
}