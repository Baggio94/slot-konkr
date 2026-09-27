use slot_store::Platform;

/// Every platform has a directory, GBA included.
#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC"]);
}

/// `ALL` is the shelf ring, in the order the shoulders walk it.
#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(Platform::ALL, [Platform::Gba, Platform::Gb, Platform::Gbc]);
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
}

/// The extensions a folder will take. A `.gba` in `GB/` is not a Game Boy cart.
#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    // Case is the dumper's business, not ours.
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
}
