#![cfg(target_os = "linux")]
#[cfg(feature = "ui-test")]
use omuse::durable_fs;
use omuse::{create_project::Project, document, model::Document};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
fn tree(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut all = vec![];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(fs::read_dir(&path).unwrap().map(|p| p.unwrap().path()));
        }
        all.push(path);
    }
    all
}
fn policy(root: &Path, dir: u32, file: u32) {
    let mut paths = tree(root);
    paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for p in paths {
        fs::set_permissions(
            &p,
            fs::Permissions::from_mode(if p.is_dir() { dir } else { file }),
        )
        .unwrap();
    }
}
fn state(root: &Path) -> BTreeMap<PathBuf, (u32, u32, u32)> {
    tree(root)
        .into_iter()
        .map(|p| {
            let m = fs::symlink_metadata(&p).unwrap();
            (
                p.strip_prefix(root).unwrap().to_owned(),
                (m.mode() & 0o7777, m.uid(), m.gid()),
            )
        })
        .collect()
}
fn stages(parent: &Path, prefix: &str) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(prefix)
                .then_some(p)
        })
        .collect()
}
fn set_attr(path: &Path, name: &str, value: &[u8]) {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn setxattr(
            path: *const std::ffi::c_char,
            name: *const std::ffi::c_char,
            value: *const std::ffi::c_void,
            size: usize,
            flags: i32,
        ) -> i32;
    }
    let p = CString::new(path.as_os_str().as_bytes()).unwrap();
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
        "set_attr {}: {}",
        name,
        std::io::Error::last_os_error()
    );
}
fn attr(path: &Path, name: &str) -> Vec<u8> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn getxattr(
            path: *const std::ffi::c_char,
            name: *const std::ffi::c_char,
            value: *mut std::ffi::c_void,
            size: usize,
        ) -> isize;
    }
    let p = CString::new(path.as_os_str().as_bytes()).unwrap();
    let n = CString::new(name).unwrap();
    let len = unsafe { getxattr(p.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0) };
    assert!(
        len >= 0,
        "get_attr {}: {}",
        name,
        std::io::Error::last_os_error()
    );
    let mut v = vec![0; len as usize];
    assert_eq!(
        unsafe { getxattr(p.as_ptr(), n.as_ptr(), v.as_mut_ptr().cast(), v.len()) },
        len
    );
    v
}
fn default_acl() -> Vec<u8> {
    let mut b = 2u32.to_le_bytes().to_vec();
    for (tag, perm, id) in [
        (1u16, 7u16, u32::MAX),
        (2, 5, {
            unsafe extern "C" {
                fn getuid() -> u32;
            }
            unsafe { getuid() }
        }),
        (4, 7, u32::MAX),
        (16, 7, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        b.extend(tag.to_le_bytes());
        b.extend(perm.to_le_bytes());
        b.extend(id.to_le_bytes());
    }
    b
}

#[test]
fn private_and_shared_document_replacement_preserves_policy_and_pixels() {
    for (dir, file) in [(0o700, 0o600), (0o2770, 0o660)] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("art.omuse");
        let mut doc = Document::new(6, 4);
        document::save(&doc, &path).unwrap();
        policy(&path, dir, file);
        let before = state(&path);
        doc.layers[0].name = "after".into();
        document::save(&doc, &path).unwrap();
        assert_eq!(state(&path), before);
        assert_eq!(document::open(&path).unwrap().layers[0].name, "after");
        assert!(stages(temp.path(), ".omuse-stage-").is_empty());
    }
}

#[test]
fn private_and_shared_create_replacement_preserves_nested_policy() {
    for (dir, file) in [(0o700, 0o600), (0o2770, 0o660)] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("campaign.omuse");
        let mut project = Project::new("before", Document::new(6, 4));
        project.add_page("inactive", Document::new(7, 5)).unwrap();
        let layer = project.active_document().unwrap().layers[0].clone();
        project.define_component("component", vec![layer]).unwrap();
        project
            .add_resource("bytes", "application/octet-stream", vec![3, 5, 7])
            .unwrap();
        project.save(&path).unwrap();
        policy(&path, dir, file);
        let before = state(&path);
        project = Project::open(&path).unwrap();
        project.title = "after".into();
        project.save(&path).unwrap();
        assert_eq!(state(&path), before);
        assert_eq!(Project::open(&path).unwrap().title, "after");
        assert!(stages(temp.path(), ".omuse-create-stage-").is_empty());
    }
}

#[test]
fn xattrs_default_acl_and_new_assets_inherit_existing_directory_policy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("acl.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    policy(&path, 0o2770, 0o660);
    let acl = default_acl();
    set_attr(&path, "system.posix_acl_default", &acl);
    set_attr(&path.join("images"), "system.posix_acl_default", &acl);
    set_attr(&path, "user.omuse-root", b"custom root");
    set_attr(
        &path.join("manifest.json"),
        "user.omuse-private",
        b"policy token",
    );
    let layer = omuse::model::Layer::paint("new", 6, 4);
    let id = layer.id.clone();
    doc.layers.push(layer);
    document::save(&doc, &path).unwrap();
    assert_eq!(attr(&path, "system.posix_acl_default"), acl);
    assert_eq!(attr(&path, "user.omuse-root"), b"custom root");
    assert_eq!(
        attr(&path.join("manifest.json"), "user.omuse-private"),
        b"policy token"
    );
    let actual = attr(
        &path.join("images").join(format!("{id}.png")),
        "system.posix_acl_access",
    );
    let mut expected = acl.clone();
    expected[6..8].copy_from_slice(&6u16.to_le_bytes());
    expected[30..32].copy_from_slice(&6u16.to_le_bytes());
    assert_eq!(actual, expected);
    assert_eq!(
        fs::metadata(path.join("images").join(format!("{id}.png")))
            .unwrap()
            .gid(),
        fs::metadata(path.join("images")).unwrap().gid()
    );
}

#[test]
fn metadata_budget_failure_preserves_prior_package() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("art.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    for i in 0..65 {
        set_attr(
            &path.join("manifest.json"),
            &format!("user.omuse{i:02}"),
            b"x",
        );
    }
    let before = fs::read(path.join("manifest.json")).unwrap();
    let policy = state(&path);
    doc.layers[0].name = "must not publish".into();
    let error = document::save(&doc, &path).unwrap_err();
    assert!(
        format!("{error:#}").contains("Too many extended attributes"),
        "{error:#}"
    );
    assert_eq!(fs::read(path.join("manifest.json")).unwrap(), before);
    assert_eq!(state(&path), policy);
    assert!(stages(temp.path(), ".omuse-stage-").is_empty());
}

#[cfg(feature = "ui-test")]
#[test]
fn wrapper_is_private_until_publish_and_unconfirmed_retains_previous_payload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("art.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    policy(&path, 0o2770, 0o660);
    doc.layers[0].name = "published".into();
    let fault = std::cell::RefCell::new(None);
    let wrapper = std::cell::RefCell::new(None);
    let outcome = document::save_checked_report(&doc, &path, || {
        let roots = stages(temp.path(), ".omuse-stage-");
        assert_eq!(roots.len(), 1);
        assert_eq!(fs::metadata(&roots[0]).unwrap().mode() & 0o777, 0o700);
        assert_eq!(
            fs::metadata(roots[0].join("package")).unwrap().mode() & 0o7777,
            0o2770
        );
        *fault.borrow_mut() = Some(durable_fs::inject_directory_sync_failure(
            &roots[0],
            std::io::Error::from_raw_os_error(5),
        ));
        *wrapper.borrow_mut() = Some(roots[0].clone());
        Ok(())
    })
    .unwrap();
    assert!(outcome.warning().unwrap().contains("was written"));
    assert_eq!(document::open(&path).unwrap().layers[0].name, "published");
    let wrapper = wrapper.into_inner().unwrap();
    assert_eq!(
        document::open(&wrapper.join("package")).unwrap().layers[0].name,
        "Layer 1"
    );
    drop(fault);
    assert!(wrapper.join("package").is_dir());
}

#[test]
fn concurrent_policy_change_rejects_publication_and_preserves_other_edits() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("art.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    doc.layers[0].name = "do not publish".into();
    let error = document::save_checked_report(&doc, &path, || {
        fs::set_permissions(
            path.join("manifest.json"),
            fs::Permissions::from_mode(0o600),
        )?;
        Ok(())
    })
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("changed while saving"),
        "{error:#}"
    );
    assert_eq!(
        fs::metadata(path.join("manifest.json")).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(document::open(&path).unwrap().layers[0].name, "Layer 1");
    assert!(stages(temp.path(), ".omuse-stage-").is_empty());
}

#[test]
fn legacy_create_upgrade_maps_nested_policy_and_keeps_new_entries_inheritance() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("legacy.omuse");
    let mut project = Project::new("before", Document::new(6, 4));
    let page = project.active_page_id().to_owned();
    let prototype = project.active_document().unwrap().layers[0].clone();
    let component = project
        .define_component("component", vec![prototype])
        .unwrap();
    project.save(&path).unwrap();
    for dir in ["pages", "components"] {
        for entry in fs::read_dir(path.join(dir)).unwrap() {
            let old = entry.unwrap().path();
            fs::rename(&old, old.with_extension("comp")).unwrap();
        }
    }
    let manifest_path = path.join("project.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["version"] = serde_json::json!(1);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    policy(&path, 0o700, 0o600);
    let page_root = path.join("pages").join(format!("{page}.comp"));
    let component_root = path.join("components").join(format!("{component}.comp"));
    set_attr(&page_root, "user.omuse-page", b"page policy");
    set_attr(&component_root, "user.omuse-component", b"component policy");
    set_attr(
        &page_root.join("images"),
        "system.posix_acl_default",
        &default_acl(),
    );
    let before = state(&path);
    let mut project = Project::open(&path).unwrap();
    let mut active = project.active_document().unwrap().clone();
    let layer = omuse::model::Layer::paint("new photo", 6, 4);
    let new_id = layer.id.clone();
    active.layers.push(layer);
    project.replace_active_document(active).unwrap();
    project.save(&path).unwrap();
    for (old, policy) in before {
        let mut new = PathBuf::new();
        for component in old.components() {
            let component = component.as_os_str();
            new.push(component);
            if new.extension().is_some_and(|extension| extension == "comp") {
                new.set_extension("omuse");
            }
        }
        let meta = fs::metadata(path.join(&new)).unwrap();
        assert_eq!(
            (meta.mode() & 0o7777, meta.uid(), meta.gid()),
            policy,
            "{}",
            new.display()
        );
    }
    let page_root = path.join("pages").join(format!("{page}.omuse"));
    assert_eq!(attr(&page_root, "user.omuse-page"), b"page policy");
    assert_eq!(
        attr(
            &path.join("components").join(format!("{component}.omuse")),
            "user.omuse-component"
        ),
        b"component policy"
    );
    assert!(
        !attr(
            &page_root.join("images").join(format!("{new_id}.png")),
            "system.posix_acl_access"
        )
        .is_empty()
    );
    assert_eq!(
        Project::open(&path)
            .unwrap()
            .active_document()
            .unwrap()
            .layers
            .len(),
        2
    );
}

#[test]
fn unknown_old_files_are_not_copied_and_symlink_counterparts_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("art.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    fs::write(path.join("not-owned-by-document"), b"unknown").unwrap();
    doc.layers[0].name = "safe".into();
    document::save(&doc, &path).unwrap();
    assert!(!path.join("not-owned-by-document").exists());
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::rename(path.join("images"), outside.join("images")).unwrap();
    std::os::unix::fs::symlink(outside.join("images"), path.join("images")).unwrap();
    let manifest = fs::read(path.join("manifest.json")).unwrap();
    let error = document::save(&doc, &path).unwrap_err();
    assert!(format!("{error:#}").contains("metadata"), "{error:#}");
    assert_eq!(fs::read(path.join("manifest.json")).unwrap(), manifest);
    assert!(outside.join("images").is_dir());
}

#[test]
fn read_only_cross_parent_exchange_refuses_without_relaxing_source_policy() {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    if unsafe { getuid() } == 0 {
        eprintln!(
            "Read-only DAC refusal requires an unprivileged fixture process; root bypasses it"
        );
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("read-only.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    policy(&path, 0o550, 0o440);
    let before = state(&path);
    doc.layers[0].name = "changed".into();
    let error = document::save(&doc, &path).unwrap_err();
    assert!(
        format!("{error:#}").contains("Permission denied"),
        "{error:#}"
    );
    assert_eq!(state(&path), before);
    assert_eq!(document::open(&path).unwrap().layers[0].name, "Layer 1");
    // Linux checks directory write permission for cross-parent exchange. The
    // original remains read-only; do not temporarily chmod the source to save.
    policy(&path, 0o700, 0o600);
    for wrapper in stages(temp.path(), ".omuse-stage-") {
        policy(&wrapper, 0o700, 0o600);
    }
}

#[cfg(feature = "ui-test")]
#[test]
fn create_unconfirmed_flush_preserves_old_wrapper_after_session_drop_and_copy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("campaign.omuse");
    let copy = temp.path().join("copy.omuse");
    let mut project = Project::new("before", Document::new(6, 4));
    let inactive = project.add_page("inactive", Document::new(7, 5)).unwrap();
    let resource = project
        .add_resource("bytes", "application/octet-stream", vec![3, 5, 7])
        .unwrap();
    project.save(&path).unwrap();
    policy(&path, 0o700, 0o600);
    let mut project = Project::open(&path).unwrap();
    project.title = "after".into();
    let fault = durable_fs::inject_publication_failure(&path, std::io::ErrorKind::Other);
    let result = project.save_checked_report(&path, || Ok(())).unwrap();
    assert!(result.warning().is_some());
    let wrappers = stages(temp.path(), ".omuse-create-stage-");
    assert_eq!(wrappers.len(), 1);
    let previous = wrappers[0].join("package");
    assert_eq!(Project::open(&previous).unwrap().title, "before");
    assert_eq!(Project::open(&path).unwrap().title, "after");
    assert_eq!(project.page_document(&inactive).unwrap().width, 7);
    assert_eq!(project.resource_bytes(&resource).unwrap(), [3, 5, 7]);
    drop(fault);
    project.save(&copy).unwrap();
    drop(project);
    assert_eq!(Project::open(&copy).unwrap().title, "after");
    assert!(previous.is_dir());
    assert_eq!(
        fs::metadata(wrappers[0].clone()).unwrap().mode() & 0o777,
        0o700
    );
}

#[test]
fn retired_and_rejected_read_only_nested_trees_are_disposed_inside_private_wrappers() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested.omuse");
    let mut doc = Document::new(6, 4);
    document::save(&doc, &path).unwrap();
    fs::set_permissions(path.join("images"), fs::Permissions::from_mode(0o550)).unwrap();
    for entry in fs::read_dir(path.join("images")).unwrap() {
        fs::set_permissions(entry.unwrap().path(), fs::Permissions::from_mode(0o440)).unwrap();
    }
    let before = state(&path);
    doc.layers[0].name = "saved".into();
    document::save(&doc, &path).unwrap();
    assert_eq!(state(&path), before);
    assert!(stages(temp.path(), ".omuse-stage-").is_empty());
    doc.layers[0].name = "rejected".into();
    let error = document::save_checked_report(&doc, &path, || {
        anyhow::bail!("test rejects only staged draft")
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("test rejects"));
    assert_eq!(document::open(&path).unwrap().layers[0].name, "saved");
    assert_eq!(state(&path), before);
    assert!(stages(temp.path(), ".omuse-stage-").is_empty());
    policy(&path, 0o700, 0o600);

    let path = temp.path().join("campaign.omuse");
    let mut project = Project::new("before", Document::new(6, 4));
    project.add_page("inactive", Document::new(7, 5)).unwrap();
    project.save(&path).unwrap();
    for entry in tree(&path) {
        if entry.file_name().is_some_and(|name| name == "images") {
            fs::set_permissions(&entry, fs::Permissions::from_mode(0o550)).unwrap();
        }
    }
    let before = state(&path);
    let mut project = Project::open(&path).unwrap();
    project.title = "after".into();
    project.save(&path).unwrap();
    assert_eq!(state(&path), before);
    assert!(stages(temp.path(), ".omuse-create-stage-").is_empty());
    assert_eq!(Project::open(&path).unwrap().title, "after");
    policy(&path, 0o700, 0o600);
}
