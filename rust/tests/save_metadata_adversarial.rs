//! Independent authored fixtures; only owned temporary packages are modified.
#![cfg(target_os = "linux")]
#[cfg(feature = "ui-test")]
use omuse::durable_fs;
use omuse::{
    create_project::Project,
    document,
    model::{Document, Layer},
};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
fn chmod(p: &Path, mode: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
}
fn wrappers(parent: &Path) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".omuse-stage-")
        })
        .collect()
}
fn all_writable(root: &Path) {
    let m = fs::symlink_metadata(root).unwrap();
    if m.is_dir() {
        chmod(root, 0o700);
        for e in fs::read_dir(root).unwrap() {
            all_writable(&e.unwrap().path());
        }
    }
}
fn attr(p: &Path, name: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn getxattr(
            p: *const std::ffi::c_char,
            n: *const std::ffi::c_char,
            v: *mut std::ffi::c_void,
            s: usize,
        ) -> isize;
    }
    let p = CString::new(p.as_os_str().as_bytes()).unwrap();
    let n = CString::new(name).unwrap();
    let len = unsafe { getxattr(p.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0) };
    if len < 0 {
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(61));
        return None;
    }
    let mut out = vec![0; len as usize];
    assert_eq!(
        unsafe { getxattr(p.as_ptr(), n.as_ptr(), out.as_mut_ptr().cast(), out.len()) },
        len
    );
    Some(out)
}
fn set_attr(p: &Path, name: &str, value: &[u8]) {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn setxattr(
            p: *const std::ffi::c_char,
            n: *const std::ffi::c_char,
            v: *const std::ffi::c_void,
            s: usize,
            f: i32,
        ) -> i32;
    }
    let p = CString::new(p.as_os_str().as_bytes()).unwrap();
    let n = CString::new(name).unwrap();
    assert_eq!(
        unsafe {
            setxattr(
                p.as_ptr(),
                n.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        },
        0,
        "{}",
        std::io::Error::last_os_error()
    );
}
fn remove_attr_if_present(p: &Path, name: &str) {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn removexattr(p: *const std::ffi::c_char, n: *const std::ffi::c_char) -> i32;
    }
    if attr(p, name).is_some() {
        let p = CString::new(p.as_os_str().as_bytes()).unwrap();
        let n = CString::new(name).unwrap();
        assert_eq!(unsafe { removexattr(p.as_ptr(), n.as_ptr()) }, 0);
    }
}
fn acl(owner: &Path) -> Vec<u8> {
    let uid = fs::metadata(owner).unwrap().uid();
    let mut a = 2u32.to_le_bytes().to_vec();
    // The namespace permits only the own mapped uid; this is an exact ACL-byte
    // inheritance/round-trip fixture, not different-principal access qualification.
    for (tag, p, id) in [
        (1u16, 7u16, u32::MAX),
        (2, 4, uid),
        (4, 5, u32::MAX),
        (16, 5, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        a.extend(tag.to_le_bytes());
        a.extend(p.to_le_bytes());
        a.extend(id.to_le_bytes());
    }
    a
}
#[test]
fn independent_replacement_removes_inherited_acl_absent_from_existing_policy() {
    let tmp = tempfile::tempdir().unwrap();
    set_attr(tmp.path(), "system.posix_acl_default", &acl(tmp.path()));
    let path = tmp.path().join("policy.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    let old_id = doc.layers[0].id.clone();
    let old_image = path.join("images").join(format!("{old_id}.png"));
    for p in [
        &path,
        &path.join("images"),
        &old_image,
        &path.join("manifest.json"),
    ] {
        remove_attr_if_present(p, "system.posix_acl_access");
        if p.is_dir() {
            remove_attr_if_present(p, "system.posix_acl_default");
            chmod(p, 0o2770);
        } else {
            chmod(p, 0o600);
        }
    }
    let added = Layer::paint("added", 6, 4);
    let id = added.id.clone();
    doc.layers.push(added);
    document::save(&doc, &path).unwrap();
    for p in [
        &path,
        &path.join("images"),
        &old_image,
        &path.join("manifest.json"),
        &path.join("images").join(format!("{id}.png")),
    ] {
        assert!(
            attr(p, "system.posix_acl_access").is_none(),
            "inherited access ACL not removed: {}",
            p.display()
        );
        if p.is_dir() {
            assert!(attr(p, "system.posix_acl_default").is_none());
        }
    }
    assert_eq!(fs::metadata(&old_image).unwrap().mode() & 0o777, 0o600);
    assert_eq!(document::open(&path).unwrap().layers.len(), 2);
}
#[test]
fn independent_xattr_change_during_publish_callback_preserves_concurrent_policy() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("race.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    set_attr(&path.join("manifest.json"), "user.policy", b"old");
    let before = fs::read(path.join("manifest.json")).unwrap();
    doc.layers[0].name = "must not publish".into();
    let error = document::save_checked_report(&doc, &path, || {
        set_attr(
            &path.join("manifest.json"),
            "user.policy",
            b"concurrent policy",
        );
        Ok(())
    })
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("changed while saving"),
        "{error:#}"
    );
    assert_eq!(
        attr(&path.join("manifest.json"), "user.policy").unwrap(),
        b"concurrent policy"
    );
    assert_eq!(fs::read(path.join("manifest.json")).unwrap(), before);
    assert!(wrappers(tmp.path()).is_empty());
}
#[test]
fn independent_create_keeps_heterogeneous_nested_asset_exceptions_and_new_page_acl() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("collection.omuse");
    let mut p = Project::new("before", Document::new(6, 4));
    let inactive = p.add_page("inactive", Document::new(7, 5)).unwrap();
    p.save(&path).unwrap();
    let old = p.page_document(&inactive).unwrap().layers[0].id.clone();
    let image = path
        .join("pages")
        .join(format!("{inactive}.omuse"))
        .join("images")
        .join(format!("{old}.png"));
    chmod(&image, 0o600);
    set_attr(&image, "user.private-exception", b"retained");
    let root = path.join("pages");
    chmod(&root, 0o2770);
    set_attr(&root, "system.posix_acl_default", &acl(tmp.path()));
    let expected = attr(&image, "system.posix_acl_access");
    let mut p = Project::open(&path).unwrap();
    let new_page = p.add_page("new page", Document::new(8, 6)).unwrap();
    p.title = "after".into();
    p.save(&path).unwrap();
    assert_eq!(fs::metadata(&image).unwrap().mode() & 0o777, 0o600);
    assert_eq!(attr(&image, "system.posix_acl_access"), expected);
    assert_eq!(attr(&image, "user.private-exception").unwrap(), b"retained");
    let new_root = path.join("pages").join(format!("{new_page}.omuse"));
    assert_eq!(
        attr(&new_root, "system.posix_acl_default").unwrap(),
        acl(tmp.path())
    );
    assert!(attr(&new_root, "system.posix_acl_access").is_some());
    assert_eq!(Project::open(&path).unwrap().title, "after");
}
#[test]
fn independent_fifo_known_counterpart_refuses_without_blocking_or_publication() {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn mkfifo(p: *const std::ffi::c_char, m: u32) -> i32;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fifo.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    let asset = path
        .join("images")
        .join(format!("{}.png", doc.layers[0].id));
    fs::remove_file(&asset).unwrap();
    let c = CString::new(asset.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { mkfifo(c.as_ptr(), 0o600) }, 0);
    let before = fs::read(path.join("manifest.json")).unwrap();
    doc.layers[0].name = "not published".into();
    let error = document::save(&doc, &path).unwrap_err();
    assert!(format!("{error:#}").contains("metadata"), "{error:#}");
    assert_eq!(fs::read(path.join("manifest.json")).unwrap(), before);
    assert!(!fs::symlink_metadata(&asset).unwrap().is_file());
    assert!(wrappers(tmp.path()).is_empty());
}
#[test]
#[cfg(feature = "ui-test")]
fn independent_destination_parent_flush_failure_retains_private_old_package() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("flush.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    chmod(&path, 0o700);
    let before = fs::read(path.join("manifest.json")).unwrap();
    let fault = std::cell::RefCell::new(None);
    doc.layers[0].name = "published".into();
    let outcome = document::save_checked_report(&doc, &path, || {
        *fault.borrow_mut() = Some(durable_fs::inject_directory_sync_failure(
            tmp.path(),
            std::io::Error::from_raw_os_error(5),
        ));
        Ok(())
    })
    .unwrap();
    assert!(outcome.warning().unwrap().contains("was written"));
    assert_eq!(document::open(&path).unwrap().layers[0].name, "published");
    let retained = wrappers(tmp.path());
    assert_eq!(retained.len(), 1);
    assert_eq!(fs::metadata(&retained[0]).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::read(retained[0].join("package/manifest.json")).unwrap(),
        before
    );
    drop(fault);
}
#[test]
#[cfg(feature = "ui-test")]
fn independent_unconfirmed_save_retains_read_only_previous_policy_without_cleanup_chmod() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("retain-readonly.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    let relative = PathBuf::from("images").join(format!("{}.png", doc.layers[0].id));
    chmod(&path.join(&relative), 0o440);
    chmod(&path.join("images"), 0o550);
    let before = fs::read(path.join("manifest.json")).unwrap();
    let fault = std::cell::RefCell::new(None);
    doc.layers[0].name = "published with retained recovery".into();
    let outcome = document::save_checked_report(&doc, &path, || {
        *fault.borrow_mut() = Some(durable_fs::inject_directory_sync_failure(
            tmp.path(),
            std::io::Error::from_raw_os_error(5),
        ));
        Ok(())
    })
    .unwrap();
    assert!(outcome.warning().is_some());
    let retained = wrappers(tmp.path());
    assert_eq!(retained.len(), 1);
    let previous = retained[0].join("package");
    assert_eq!(
        fs::metadata(previous.join("images")).unwrap().mode() & 0o777,
        0o550
    );
    assert_eq!(
        fs::metadata(previous.join(&relative)).unwrap().mode() & 0o777,
        0o440
    );
    assert_eq!(fs::read(previous.join("manifest.json")).unwrap(), before);
    assert_eq!(
        fs::metadata(path.join("images")).unwrap().mode() & 0o777,
        0o550
    );
    assert_eq!(
        fs::metadata(path.join(&relative)).unwrap().mode() & 0o777,
        0o440
    );
    assert_eq!(
        document::open(&path).unwrap().layers[0].name,
        "published with retained recovery"
    );
    drop(fault);
    assert!(previous.is_dir());
    // Release only this owned fixture after checking actual retention policy.
    all_writable(tmp.path());
}
