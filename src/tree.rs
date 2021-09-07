use std::fmt::Debug;
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

struct RawTreeNode {
    raw: Node,
    entry_type: FileEntryType
}

impl RawTreeNode {
    pub fn new(raw: Node, entry_type: FileEntryType) -> Self {
        Self {
            raw,
            entry_type
        }
    }
}

impl TreeNode for RawTreeNode {
    type ErrorType = <Node as TreeNode>::ErrorType;
    type TreeKey = <Node as TreeNode>::TreeKey;

    fn get_key(&self) -> Self::TreeKey {
        self.raw.get_key()
    }
}

pub struct Tree<L: FileLoader> {
    loader: L,
    root: RawNode<RawTreeNode>
}

impl<L: FileLoader> Tree<L> where <L as FileLoader>::ErrorType: Debug {
    fn get_path(&self, path: &Path) -> Option<&RawNode<RawTreeNode>> {
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

    fn get_path_mut(&mut self, path: &Path) -> Option<&mut RawNode<RawTreeNode>> {
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

    pub fn new(loader: L) -> Self {
        Self {
            root: RawNode::new(RawTreeNode::new(Node::root(), FileEntryType::Directory)),
            loader
        }
    }

    /// Attempts to load the specified local path with the loader. If the path is not contained inside of the tree, then `Ok(None)` is returned.
    /// The loader is responsible for returning valid data. If it can't load valid data, it is expected to return an `Err(L::ErrorType)`
    pub fn load<P: AsRef<Path>>(&self, path: P) -> Result<Option<Vec<u8>>, L::ErrorType> {
        let path = path.as_ref();
        if let Some(node) = self.get_path(path) {
            Ok(Some(self.loader.load_path(&node.data.raw.root_path, &node.data.raw.local_path)?))
        } else {
            Ok(None)
        }
    }

    /// Checks the filesystem to see if a file exists
    pub fn contains_path<P: AsRef<Path>>(&self, path: P) -> bool {
        self.get_path(path.as_ref()).is_some()
    }

    fn insert_path_unchecked(&mut self, root_path: &Path, local_path: &Path, entry_type: FileEntryType) -> Option<(PathBuf, PathBuf)> {
        let parent_node = if let Some(parent_path) = local_path.parent() {
            if parent_path == Path::new("/") || parent_path == Path::new("") {
                &mut self.root
            } else if let Some(parent) = self.get_path_mut(parent_path) {
                parent
            } else {
                assert!(self.insert_path_unchecked(&Path::new(""), parent_path, FileEntryType::Directory).is_none());
                match self.get_path_mut(parent_path) {
                    Some(node) => node,
                    None => panic!("Failed to find parent node '{}' immediately after adding it", parent_path.display())
                }
            }
        } else {
            &mut self.root 
        };

        let node = match entry_type {
            FileEntryType::Directory => Node::new(Path::new(""), local_path).unwrap(),
            FileEntryType::File => Node::new(root_path, local_path).unwrap()
        };

        if let Some(RawTreeNode{ raw: Node { local_path: local, root_path: root, .. }, .. }) = parent_node.add_child(RawTreeNode::new(node, entry_type), true) {
            Some((root, local))
        } else {
            None
        }
    }

    /// Inserts a file into the file tree.
    /// This operation is unchecked, and the loader does not confirm that this file exists when adding it to the file tree.
    pub fn insert_file<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        self.insert_path_unchecked(root_path.as_ref(), local_path.as_ref(), FileEntryType::File)
    }

    /// Inserts a directory into the file tree.
    /// This operation is unchecked, and the loader does not confirm that this file exists when adding it to the file tree.
    pub fn insert_directory<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        self.insert_path_unchecked(root_path.as_ref(), local_path.as_ref(), FileEntryType::Directory)
    }

    /// Inserts a path into the file tree. If a previous entry existed, it gets replaced and the root/local path is returned.
    /// If you use `insert_path`, it is required that the path "exists" such that the `FileLoader` can get it's entry type
    pub fn insert_path<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        let root_path = root_path.as_ref();
        let local_path = local_path.as_ref();
        let entry_type = self.loader.get_path_type(root_path, local_path).unwrap();
        self.insert_path_unchecked(root_path, local_path, entry_type)
    }

    pub fn remove_path<P: AsRef<Path>>(&mut self, path: P) -> Option<(PathBuf, PathBuf)> {
        let path = path.as_ref();
        println!("{}", path.display());
        let name = path
            .file_name()
            .expect("Path does not contain file name!")
            .to_str()
            .expect("Unable to convert OsStr to str")
            .to_string();
        let parent_node = if let Some(parent_path) = path.parent() {
            if parent_path == Path::new("/") || parent_path == Path::new("") {
                &mut self.root
            } else if let Some(parent) = self.get_path_mut(parent_path) {
                parent
            } else {
                return None;
            }
        } else {
            &mut self.root
        };

        if let Some(RawNode { data: RawTreeNode { raw: Node { local_path: local, root_path: root, .. }, .. }, .. }) = parent_node.children.remove(&name) {
            Some((root, local))
        } else {
            None
        }
    }

    pub fn remove_paths_by_root<P: AsRef<Path>>(&mut self, root: P) -> Vec<PathBuf> {
        let remove = root.as_ref();
        let mut to_remove = Vec::new();
        self.walk_paths(|node, _| {
            if node.root_path == remove {
                to_remove.push(node.local_path.clone());
            }
        });
        to_remove
            .into_iter()
            .filter_map(|local_path| {
                if let Some((root, local)) = self.remove_path(&local_path) {
                    Some(local)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn walk_paths<F: FnMut(&Node, FileEntryType)>(&self, mut f: F) {
        fn internal<F: FnMut(&Node, FileEntryType)>(node: &RawNode<RawTreeNode>, f: &mut F, depth: usize) {
            if depth != 0 {
                f(&node.data.raw, node.data.entry_type);
            }
            for (_, child) in node.children() {
                internal(child, f, depth + 1);
            }
        }
        internal(&self.root, &mut f, 0);
    }

    pub fn filter_walk_paths<C, F: FnMut(&Node, FileEntryType) -> Option<C>>(&mut self, mut f: F) -> Vec<(PathBuf, PathBuf, C)> {
        fn internal<C, F: FnMut(&Node, FileEntryType) -> Option<C>>(node: &mut RawNode<RawTreeNode>, f: &mut F, rejected: &mut Vec<(PathBuf, C)>, depth: usize) {
            if depth != 0 {
                if let Some(complaint) = f(&node.data.raw, node.data.entry_type) {
                    rejected.push((node.data.raw.local_path.clone(), complaint));
                    return;
                }
            }
            for (_, child) in node.children_mut() {
                internal(child, f, rejected, depth + 1);
            }
        }
        let mut rejected: Vec<(PathBuf, C)> = Vec::new();
        internal(&mut self.root, &mut f, &mut rejected, 0);
        rejected.into_iter().filter_map(|(local_path, reason)| {
            if let Some((root, local)) = self.remove_path(&local_path) {
                Some((root, local, reason))
            } else {
                None
            }
        }).collect()
    }

    pub fn purify(&mut self) {
        let mut to_remove = Vec::new();
        self.walk_paths(|node, _| {
            if !self.loader.path_exists(&node.root_path, &node.local_path) {
                to_remove.push(node.local_path.clone());
            }
        });
        for local_path in to_remove.into_iter() {
            let _ = self.remove_path(&local_path);
        }
    }

    pub fn get_root_for_path<P: AsRef<Path>>(&self, path: P) -> Option<PathBuf> {
        if let Some(node) = self.get_path(path.as_ref()) {
            Some(node.data.raw.root_path.clone())
        } else {
            None
        }
    }
}