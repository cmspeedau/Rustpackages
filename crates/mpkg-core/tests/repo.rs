use mpkg_core::{PackageManifest, RepoIndex, Repository};
use tempfile::tempdir;

#[test]
fn load_repo_from_disk() {
    let td = tempdir().unwrap();
    let repo_root = td.path().join("test-repo");
    std::fs::create_dir_all(&repo_root).unwrap();

    let idx = RepoIndex {
        packages: vec![
            PackageManifest {
                name: "mpkg-test-pkg".into(),
                version: "1.0.0".into(),
                description: Some("test".into()),
                license: Some("MIT".into()),
                provides: vec![],
                depends: vec![],
            },
        ],
    };

    let mut repo = Repository {
        name: String::new(),
        index: idx,
    };
    repo.write_index(&repo_root).unwrap();

    let loaded = Repository::load(&repo_root, "test-repo").unwrap();
    assert_eq!(loaded.name, "test-repo");
    assert_eq!(loaded.index.packages.len(), 1);
    assert_eq!(loaded.find("mpkg-test-pkg").unwrap().version, "1.0.0");
}