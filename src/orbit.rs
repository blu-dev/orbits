use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

use crate::{FileEntryType, ConflictHandler};
use crate::loader::FileLoader;
use crate::tree::{Tree, node::Node};

use jwalk::{Parallelism, WalkDir};

pub struct DiscoverSystem<A: FileLoader> {
    pub tree: Tree<A>,
    pub no_root: HashSet<PathBuf>,
    pub handler: ConflictHandler,
    pub ignore: HashSet<PathBuf>,
    pub collect: HashSet<PathBuf>,
    pub collected: Vec<(PathBuf, PathBuf)>
}

pub enum ConflictKind {
    StandardConflict(PathBuf, PathBuf),
    RootConflict(Vec<PathBuf>, PathBuf)
}

impl<A: FileLoader> DiscoverSystem<A> where <A as FileLoader>::ErrorType: Debug {
    fn handle_conflict(&mut self, root_path: &Path, local_path: &Path) -> Option<ConflictKind> {
        match self.handler {
            ConflictHandler::Strict => {
                if let Some(root) = self.tree.get_root_for_path(local_path) {
                    panic!("File conflict: path '{}' found when path '{}' is already included!", root_path.join(local_path).display(), root.join(local_path).display());
                } else {
                    panic!("File conflict: path '{}' found when path '{}' is already included!", root_path.join(local_path).display(), local_path.display());
                }
            },
            ConflictHandler::NoRoot => {
                let removed_files = self.tree.remove_paths_by_root(local_path);
                if let Some(root) = self.tree.get_root_for_path(local_path) {
                    Some(ConflictKind::RootConflict(removed_files, root.join(local_path)))
                } else {
                    Some(ConflictKind::RootConflict(removed_files, local_path.to_path_buf()))
                }
            },
            ConflictHandler::First =>  {
                if let Some(root) = self.tree.get_root_for_path(local_path) {
                    Some(ConflictKind::StandardConflict(root_path.join(local_path), root.join(local_path)))
                } else {
                    Some(ConflictKind::StandardConflict(root_path.join(local_path), local_path.to_path_buf()))
                }
            },
            ConflictHandler::Last => None
        }
    }

    pub fn new(loader: A, handler: ConflictHandler) -> Self {
        Self {
            tree: Tree::new(loader),
            no_root: HashSet::new(),
            handler,
            ignore: HashSet::new(),
            collect: HashSet::new(),
            collected: Vec::new()
        }
    }

    pub fn discover_in_root<P: AsRef<Path>>(&mut self, root: P) -> Vec<ConflictKind> {
        let root = root.as_ref();
        let mut conflicts = Vec::new();
        for entry in WalkDir::new(root)
            .min_depth(1)
            .parallelism(Parallelism::Serial)
            .into_iter() {
            if let Ok(entry) = entry {
                let path = entry.path();
                let local_path = path.strip_prefix(root).expect("Path found in root is not physically in root! Possible symlink?");
                let local_pathbuf = local_path.to_path_buf();
                if self.ignore.contains(&local_pathbuf) {
                    continue;
                }
                if self.collect.contains(&local_pathbuf) {
                    self.collected.push((root.to_path_buf(), local_pathbuf));
                    continue;
                }
                drop(local_pathbuf);
                if entry.file_type().is_dir() {
                    if !self.tree.contains_path(local_path) {
                        self.tree.insert_directory(root, local_path);
                    }
                } else if entry.file_type().is_file() {
                    if self.tree.contains_path(local_path) {
                        if let Some(conflict) = self.handle_conflict(root, local_path) {
                            match conflict {
                                ConflictKind::RootConflict(files, conflict_file) => {
                                    return vec![ConflictKind::RootConflict(files, conflict_file)];
                                },
                                ConflictKind::StandardConflict(source, replacement) => {
                                    conflicts.push(ConflictKind::StandardConflict(source, replacement));
                                }
                            }
                        } else if let Some((source, replacement)) = self.tree.insert_file(root, local_path) {
                            conflicts.push(ConflictKind::StandardConflict(source, replacement));
                        }
                    } else {
                        if self.tree.insert_file(root, local_path).is_some() {
                            panic!("Entry found without finding it first!");
                        }
                    }
                }
            }
        }
        conflicts
    }

    pub fn discover_roots<P: AsRef<Path>, F: Fn(&Path) -> bool>(&mut self, path: P, depth: usize, filter: F) -> Vec<ConflictKind> {
        let path = path.as_ref();
        let mut conflicts = Vec::new();
        for entry in WalkDir::new(path)
            .min_depth(depth)
            .max_depth(depth)
            .parallelism(Parallelism::RayonNewPool(6))
            .into_iter() {
            if let Ok(entry) = entry {
                let path = entry.path();
                if filter(&path) {
                    conflicts.append(&mut self.discover_in_root(path));
                }
            }
        }
        conflicts
    }

    pub fn into_tree(self) -> Tree<A> {
        let Self { tree, .. } = self;
        tree
    }

    pub fn ignore<P: AsRef<Path>>(&mut self, local_path: P) {
        self.ignore.insert(local_path.as_ref().to_path_buf());
    }

    pub fn collect<P: AsRef<Path>>(&mut self, local_path: P) {
        self.collect.insert(local_path.as_ref().to_path_buf());
    }
}

/// LaunchPad<P, V> does not need any information about the physical loader.
/// This is setup in such a way that the patch and virtual sections can be configured and initialized
/// without the need for the physical loader to be prepared, since sometimes it can be faster to initialize things
/// in multiple threads, and a physical loader is inteded to be used with an archive
pub struct LaunchPad<P: FileLoader, V: FileLoader> {
    pub patch: DiscoverSystem<P>,
    pub virt: DiscoverSystem<V>
}

impl<P: FileLoader, V: FileLoader> LaunchPad<P, V> where
    <P as FileLoader>::ErrorType: Debug,
    <V as FileLoader>::ErrorType: Debug
{
    pub fn new(patch: DiscoverSystem<P>, virt: DiscoverSystem<V>) -> Self {
        Self {
            patch,
            virt
        }
    }

    pub fn launch<A: FileLoader>(self, physical: A) -> Orbit<A, P, V> where <A as FileLoader>::ErrorType: Debug {
        let Self { patch, virt } = self;
        Orbit {
            physical: Tree::new(physical),
            patch: patch.into_tree(),
            virt: virt.into_tree()
        }
    }
}

/// Orbit<A, B, C> handles the loading of files in the priority of C -> B -> A
pub struct Orbit<A: FileLoader, B: FileLoader, C: FileLoader> {
    physical: Tree<A>,
    patch: Tree<B>,
    virt: Tree<C>
}

/// OrbitError<A, B, C> is an enum type for the FileLoader::ErrorType of the Orbit loaders
#[derive(Debug)]
pub enum Error<A: Debug, B: Debug, C: Debug> {
    Physical(A),
    Patch(B),
    Virtual(C)
}

impl<A: FileLoader, B: FileLoader, C: FileLoader> Orbit<A, B, C> where
    <A as FileLoader>::ErrorType: Debug,
    <B as FileLoader>::ErrorType: Debug,
    <C as FileLoader>::ErrorType: Debug
{
    pub fn load<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, Error<A::ErrorType, B::ErrorType, C::ErrorType>> {
        let path = path.as_ref();
        match self.virt.load(path) {
            Ok(Some(data)) => return Ok(data),
            Ok(_) => {},
            Err(e) => return Err(Error::Virtual(e))
        }
        self.load_patch(path)
    }

    pub fn load_patch<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, Error<A::ErrorType, B::ErrorType, C::ErrorType>> {
        let path = path.as_ref();
        match self.patch.load(path) {
            Ok(Some(data)) => return Ok(data),
            Ok(_) => {},
            Err(e) => return Err(Error::Patch(e))
        }
        self.load_physical(path)
    }

    pub fn load_physical<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, Error<A::ErrorType, B::ErrorType, C::ErrorType>> {
        let path = path.as_ref();
        match self.physical.load(path) {
            Ok(data) => Ok(data.expect("Physical loader did not return valid file data!")),
            Err(e) => Err(Error::Physical(e))
        }
    }

    pub fn insert_virtual_file<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        self.virt.insert_file(root_path, local_path)
    }

    pub fn insert_virtual_directory<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        self.virt.insert_directory(root_path, local_path)
    }

    pub fn insert_virtual_path<P: AsRef<Path>, Q: AsRef<Path>>(&mut self, root_path: P, local_path: Q) -> Option<(PathBuf, PathBuf)> {
        self.virt.insert_path(root_path, local_path)
    }

    pub fn walk_patch<F: FnMut(&Node, FileEntryType)>(&self, f: F) {
        self.patch.walk_paths(f);
    }

    pub fn walk_virtual<F: FnMut(&Node, FileEntryType)>(&self, f: F) {
        self.virt.walk_paths(f);
    }
}