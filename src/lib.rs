mod tree;

#[test]
fn sample_test() {
    let mut tree = tree::FsTree::new();
    tree.add_path("/mnt/c/Users", "coe_a/Downloads").unwrap();
}