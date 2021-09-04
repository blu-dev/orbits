
fn main() {
    let mut tree = orbits::tree::FsTree::new().complete();
    tree.add_path("/mnt/c/Users", "coe_a/Downloads");
    tree.add_path("/mnt/c/Users", "coe_a/Documents");
    tree.add_path("/mnt/c/Invalid", "coe_a/Downloads/lmao");
    tree.walk_paths(|node| {
        println!("{}", node.full_path().display());
    })
}
