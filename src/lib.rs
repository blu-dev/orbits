pub mod tree;
pub mod loader;

#[derive(Copy, Clone)]
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

#[test]
fn basic_add_test() {
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
}

#[test]
fn multi_add_test() {
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
    assert!(tree.insert_file("/mnt/c/Users", "coe_a/Downloads/some_file.txt").is_none());
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Documents").is_none());
    let (root, local) = tree.insert_directory("/mnt/c/Users/something_else", "coe_a/Downloads").unwrap();
    assert!(root == std::path::Path::new(""));
    assert!(local == std::path::Path::new("coe_a/Downloads"));
}

#[test]
fn remove_test() {
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
    assert!(tree.insert_file("/mnt/c/Users", "coe_a/Downloads/some_file.txt").is_none());
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Documents").is_none());
    assert!(tree.remove_path("coe_a/Documents").is_some());
    tree.walk_paths(|node, _| {
        assert!(node.local_path != std::path::Path::new("coe_a/Documents"));
    });
}

#[test]
fn remove_root_test() {
    
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
    assert!(tree.insert_file("/mnt/c/Users", "coe_a/Downloads/some_file.txt").is_none());
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Documents").is_none());
    assert!(tree.insert_directory("/mnt/c/Users/test", "coe_a/Documents/test").is_none());
    assert!(tree.remove_paths_by_root("/mnt/c/Users").len() == 1);
    tree.walk_paths(|node, _| {
        assert!(node.root_path != std::path::Path::new("/mnt/c/Users"));
    });
}

#[test]
fn filter_walk_paths_test() {
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
    assert!(tree.insert_file("/mnt/c/Users", "coe_a/Downloads/some_file.txt").is_none());
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Documents").is_none());
    assert!(tree.insert_directory("/mnt/c/Users/test", "coe_a/Documents/test").is_none());
    assert!(tree.filter_walk_paths(|_, entry_type| {
        if entry_type.is_file() {
            Some(())
        } else {
            None
        }
    }).len() == 1);
    tree.walk_paths(|_, entry_type| {
        assert!(entry_type.is_dir());
    });
}

#[test]
fn purify_test() {
    let mut tree = tree::Tree::new(tree::loader::StandardLoader {});
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Downloads").is_none());
    assert!(tree.insert_file("/mnt/c/Users", "coe_a/Downloads/some_file.txt").is_none());
    assert!(tree.insert_path("/mnt/c/Users", "coe_a/Documents").is_none());
    assert!(tree.insert_directory("/mnt/c/Users/test", "coe_a/Documents/test").is_none());
    tree.purify();
    tree.walk_paths(|node, _| {
        assert!(node.local_path != std::path::Path::new("coe_a/Documents/test"));
        assert!(node.local_path != std::path::Path::new("coe_a/Downloads/some_file.txt"));
    })
}