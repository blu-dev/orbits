pub mod tree;

#[test]
fn sample_test() {
    let mut tree = tree::FsTree::new().complete();
    tree.add_path("/mnt/c/Users", "coe_a/Downloads").unwrap();
    tree.walk_paths(|node| {
        println!("{}", node.full_path().display());
    })
}