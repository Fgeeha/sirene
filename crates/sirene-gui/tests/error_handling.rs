//! Consumer regressions for retained errors and recovery (ERRORH-1..5).

mod common;

use std::error::Error as _;

use gpui::{ClipboardEntry, ClipboardItem, TestAppContext};
use sirene_gui::CopyImage;
use sirene_gui::document::Document;
use sirene_gui::export::Format;

#[test]
fn failed_save_as_preserves_document_and_retry_clears_error() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.mmd");
    let before = "flowchart\nBefore\n";
    let edited = "flowchart\nAfter\n";
    std::fs::write(&original, before).unwrap();
    let mut doc = Document::open(&original);
    assert!(doc.set_source(edited.to_owned()));

    let target = dir.path().join("missing").join("other.mmd");
    let error = doc.save_as(target.clone()).unwrap_err();
    let io = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
    assert!(error.report().contains(&target.display().to_string()));
    assert_eq!(doc.path.as_ref(), Some(&original));
    assert_eq!(doc.source, edited);
    assert_eq!(doc.saved, before);
    assert!(doc.is_edited());
    assert_eq!(std::fs::read_to_string(&original).unwrap(), before);
    assert!(!target.exists());

    // The retained error survives the same cloning used by Save As and the viewer.
    let copy = doc.clone();
    let retained = copy.write_error.as_ref().unwrap();
    assert_eq!(retained.report(), error.report());
    assert_eq!(
        retained
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::NotFound,
    );

    doc.save().unwrap();
    assert!(doc.write_error.is_none());
    assert!(!doc.is_edited());
    assert_eq!(std::fs::read_to_string(&original).unwrap(), edited);
}

#[gpui::test]
fn correcting_source_replaces_cached_error_scene(cx: &mut TestAppContext) {
    let mut doc = Document::from_source(None, "flowchart\nA --> B\n".into());
    doc.chart.nodes.clear();
    let (viewer, vcx) = common::open(cx, doc);
    assert!(
        viewer
            .read_with(vcx, |v, _| v.status())
            .contains("endpoint")
    );

    vcx.update(|window, cx| {
        let editor = viewer.read(cx).source.clone();
        editor.update(cx, |state, cx| {
            state.replace_all("flowchart\nRecovered\n", window, cx);
        });
    });
    vcx.run_until_parked();

    assert!(
        common::texts(&viewer, vcx)
            .iter()
            .any(|text| text == "Recovered")
    );
    let status = viewer.read_with(vcx, |v, _| v.status());
    assert!(!status.contains("endpoint"), "{status}");
    assert!(!status.contains("cannot draw"), "{status}");
}

#[gpui::test]
fn failed_render_keeps_existing_export_and_clipboard(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("existing.svg");
    std::fs::write(&target, b"keep existing export").unwrap();
    let mut doc = Document::from_source(None, "flowchart\nA --> B\n".into());
    doc.chart.nodes.clear();
    let (viewer, vcx) = common::open(cx, doc);

    assert!(!viewer.update(vcx, |v, cx| v.export_to(Format::Svg, &target, cx)));
    assert_eq!(std::fs::read(&target).unwrap(), b"keep existing export");
    assert!(
        viewer
            .read_with(vcx, |v, _| v.status())
            .contains("endpoint")
    );

    vcx.write_to_clipboard(ClipboardItem::new_string("keep clipboard".to_owned()));
    vcx.dispatch_action(CopyImage);
    let item = vcx.read_from_clipboard().unwrap();
    assert!(matches!(item.entries(), [ClipboardEntry::String(_)]));
    let status = viewer.read_with(vcx, |v, _| v.status());
    assert!(status.contains("cannot copy the drawing"), "{status}");
    assert!(status.contains("endpoint"), "{status}");
}

#[test]
fn unreadable_original_requires_explicit_overwrite_and_allows_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("invalid.mmd");
    std::fs::write(&original, [0xff, 0xfe]).unwrap();
    let mut doc = Document::open(&original);
    doc.set_source("flowchart\nRecovered\n".into());
    assert!(doc.save().is_err());
    assert_eq!(std::fs::read(&original).unwrap(), [0xff, 0xfe]);
    assert!(doc.is_edited());
    let recovery = dir.path().join("recovered.mmd");
    doc.save_as(recovery.clone()).unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), [0xff, 0xfe]);
    assert_eq!(
        std::fs::read_to_string(recovery).unwrap(),
        "flowchart\nRecovered\n"
    );

    let missing = dir.path().join("new.mmd");
    let mut doc = Document::open(&missing);
    doc.set_source("flowchart\nNew\n".into());
    doc.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(missing).unwrap(),
        "flowchart\nNew\n"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_original_aliases_require_confirmation() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("invalid.mmd");
    let hard = dir.path().join("hard.mmd");
    let link = dir.path().join("link.mmd");
    std::fs::write(&original, [0xff]).unwrap();
    std::fs::hard_link(&original, &hard).unwrap();
    symlink(&original, &link).unwrap();
    let mut doc = Document::open(&original);
    doc.set_source("flowchart\nRecovered\n".into());
    for alias in [&hard, &link] {
        assert!(doc.save_as(alias.to_path_buf()).is_err());
        assert_eq!(std::fs::read(&original).unwrap(), [0xff]);
    }
    let conflict = doc.save_conflict(&link).unwrap();
    doc.save_as_confirmed(link, Some(&conflict)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&original).unwrap(),
        "flowchart\nRecovered\n"
    );
}

#[test]
fn save_detects_unpolled_and_newer_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("diagram.mmd");
    std::fs::write(&path, "flowchart\nOriginal\n").unwrap();
    let mut doc = Document::open(&path);
    doc.set_source("flowchart\nMine\n".into());
    std::fs::write(&path, "flowchart\nTheirs\n").unwrap();
    assert!(doc.save().is_err());
    let conflict = doc.save_conflict(&path).unwrap();
    std::fs::write(&path, "flowchart\nNewer\n").unwrap();
    assert!(doc.save_confirmed(Some(&conflict)).is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "flowchart\nNewer\n"
    );
    assert!(doc.is_edited());
    let conflict = doc.save_conflict(&path).unwrap();
    doc.save_confirmed(Some(&conflict)).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "flowchart\nMine\n");
}

#[cfg(unix)]
#[test]
fn confirmation_does_not_follow_a_retargeted_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.mmd");
    let second = dir.path().join("second.mmd");
    let link = dir.path().join("link.mmd");
    std::fs::write(&first, [0xff]).unwrap();
    std::fs::write(&second, [0xff]).unwrap();
    symlink(&first, &link).unwrap();
    let mut doc = Document::open(&link);
    doc.set_source("flowchart\nMine\n".into());
    let conflict = doc.save_conflict(&link).unwrap();
    std::fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert!(doc.save_confirmed(Some(&conflict)).is_err());
    assert_eq!(std::fs::read(&second).unwrap(), [0xff]);
    assert!(doc.is_edited());
}
