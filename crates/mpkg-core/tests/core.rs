use tempfile::TempDir;

use mpkg_core::archive;
use mpkg_core::store::{HistoryEntry, Store};

fn make_pkg_src(dir: &std::path::Path, name: &str, version: &str, file_body: &str) {
    std::fs::create_dir_all(dir.join("usr/share")).unwrap();
    std::fs::write(
        dir.join("pkg.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\ndescription = \"test pkg\"\n",
            name, version
        ),
    )
    .unwrap();
    std::fs::write(dir.join("usr/share/hello.txt"), file_body).unwrap();
}

#[test]
fn install_list_verify_remove_rollback() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let store = Store::new(root);

    // Build and install a package.
    let src = tmp.path().join("src");
    make_pkg_src(&src, "hello", "1.0.0", "hi there");
    let pkg = tmp.path().join("hello-1.0.0.mpk");
    archive::build_package(&src, &pkg).unwrap();

    let rec = store.install(&pkg).unwrap();
    assert_eq!(rec.manifest.package.name, "hello");
    assert!(root.join("usr/share/hello.txt").is_file());

    // List + verify pass.
    assert_eq!(store.list().unwrap().len(), 1);
    assert!(store.verify("hello").unwrap().is_empty());

    // Tamper with the file -> verify catches it.
    std::fs::write(root.join("usr/share/hello.txt"), "tampered").unwrap();
    assert_eq!(store.verify("hello").unwrap().len(), 1);

    // Remove -> file gone, history recorded.
    store.remove("hello").unwrap();
    assert!(!root.join("usr/share/hello.txt").exists());
    assert_eq!(store.history().unwrap().len(), 2); // install + remove

    // Rollback undoes the remove and restores file contents from the trash.
    let e = store.rollback().unwrap();
    assert!(matches!(e, HistoryEntry::Remove { .. }));
    assert!(root.join("usr/share/hello.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("usr/share/hello.txt")).unwrap(),
        "tampered" // trash preserved the tampered content; hashes still recorded
    );
    assert!(store.verify("hello").unwrap().is_empty());
}

#[test]
fn manifest_validation_rejects_bad_names() {
    let m: mpkg_core::manifest::PkgManifest = toml::from_str(
        "[package]\nname = \"Bad Name!\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    assert!(m.validate().is_err());
}

#[test]
fn resolver_orders_dependencies_first_and_detects_cycles() {
    use mpkg_core::manifest::Dependency;
    use mpkg_core::repo::{RepoEntry, RepoIndex};
    use mpkg_core::resolve::resolve;

    let mk = |name: &str, deps: Vec<Dependency>| RepoEntry {
        name: name.into(),
        version: "1.0.0".into(),
        description: String::new(),
        arch: "x86_64".into(),
        sha256: "00".into(),
        url: format!("file:///pkgs/{}.mpk", name),
        dependencies: deps,
        desktop: None,
    };
    let index = RepoIndex {
        name: "test".into(),
        packages: vec![
            mk("app", vec![Dependency { name: "lib".into(), version: "*".into() }]),
            mk("lib", vec![]),
        ],
    };
    let order = resolve(&index, &["app".into()]).unwrap();
    assert_eq!(order[0].name, "lib");
    assert_eq!(order[1].name, "app");

    let cyc = RepoIndex {
        name: "t".into(),
        packages: vec![
            mk("a", vec![Dependency { name: "b".into(), version: "*".into() }]),
            mk("b", vec![Dependency { name: "a".into(), version: "*".into() }]),
        ],
    };
    assert!(resolve(&cyc, &["a".into()]).is_err());
}

#[test]
fn sign_and_verify_roundtrip() {
    let (sk, pk) = mpkg_core::sign::generate_keypair();
    let sig = mpkg_core::sign::sign(&sk, b"index data").unwrap();
    assert!(mpkg_core::sign::verify(&pk, b"index data", &sig).unwrap());
    assert!(!mpkg_core::sign::verify(&pk, b"other data", &sig).unwrap());
}
