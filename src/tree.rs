use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::io;

use thiserror::Error;

use crate::loader::FileLoader;

pub mod node;
pub mod loader;

use node::Node;
use loader::StandardLoader;

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

    pub fn get_child<A: AsRef<T::TreeKey>>(&self, key: A) -> Option<&Self> {
        self.children.get(key.as_ref())
    }

    pub fn get_child_mut<A: AsRef<T::TreeKey>>(&mut self, key: A) -> Option<&mut Self> {
        self.children.get_mut(key.as_ref())
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

impl<T: TreeNode> Hash for RawNode<T> where <T as TreeNode>::TreeKey: Hash {
    fn hash<A: Hasher>(&self, state: &mut A) {
        self.key.hash(state);
    }
}

pub enum TreeFailure<T> {
    MissingNode(T)
}

impl<T> Deref for TreeFailure<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::MissingNode(e) => e
        }
    }
}

pub struct Tree<L: FileLoader> {
    loader: L,
    root: RawNode<Node>
}

impl<L: FileLoader> Tree<L> {
    fn get_path(&self, path: &Path) -> Option<&Node> {
        let keys = path
            .components()
            .map(|x| x.as_os_str().to_str().expect("Unable to get str from OsStr").to_string())
            .collect::<Vec<String>>()
            .into_iter();

        let mut current_node = Some(&self.root);
        
        while let Some(node) = current_node.take() {
            if let Some(next_key) = keys.next() {
                for (key, node) in node.children() {
                    if next_key == key.as_str() {
                        current_node = Some(node);
                        break;
                    }
                }
            } else {
                return Some(&node.data);
            }
        }
        None
    }

    fn get_path_mut(&mut self, path: &Path) -> Option<&mut Node> {
        let keys = path
            .components()
            .map(|x| x.as_os_str().to_str().expect("Unable to get str from OsStr").to_string())
            .collect::<Vec<String>>()
            .into_iter();

        let mut current_node = Some(&mut self.root);

        while let Some(node) = current_node.take() {
            if let Some(next_key) = keys.next() {
                for (key, node) in node.children_mut() {
                    if next_key == key.as_str() {
                        current_node = Some(node);
                        break;
                    }
                }
            } else {
                return Some(&mut node.data);
            }
        }
        None
    }

    /// Attempts to load the specified local path with the loader. If the path is not contained inside of the tree, then `Ok(None)` is returned.
    /// The loader is responsible for returning valid data. If it can't load valid data, it is expected to return an `Err(L::ErrorType)`
    pub fn load<P: AsRef<Path>>(&self, path: P) -> Result<Option<Vec<u8>>, L::ErrorType> {
        let path = path.as_ref();
        if let Some(node) = self.get_path(path) {
            Ok(Some(self.loader.load_path(&node.root_path, &node.local_path)?))
        } else {
            Ok(None)
        }
    }

    /// Checks the filesystem to see if a file exists
    pub fn contains_path<P: AsRef<Path>>(&self, path: P) -> bool {
        self.get_path(path.as_ref()).is_some()
    }

    pub fn insert_path<P: AsRef<Path>>(&mut self, path: P) -> Option<PathBuf> {

    }

    pub fn remove_path<P: AsRef<Path>>(&mut self, path: P) -> Option<PathBuf> {

    }

    pub fn remove_paths_by_root<P: AsRef<Path>>(&mut self, root: P) -> Vec<PathBuf> {

    }

    pub fn walk_paths<F: FnMut(&Node)>(&self, mut func: F) {

    }

    pub fn filter_walk_paths<C, F: FnMut(&Node) -> Option<C>>(&mut self, mut func: F) {

    }
}