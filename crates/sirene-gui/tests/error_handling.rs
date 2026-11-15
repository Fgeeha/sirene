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
