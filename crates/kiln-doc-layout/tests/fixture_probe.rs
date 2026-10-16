mod common;
use common::*;

#[test]
#[ignore = "manual: renders every feature fixture"]
fn probe_fixtures() {
    let dir = std::env::var("PROBE_DIR").unwrap_or("/tmp".into());
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/features");
    let only = std::env::var("ONLY").ok();
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    for path in names {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let doc = kiln_io_docx::read_docx(&bytes).unwrap().document;
        let t = std::time::Instant::now();
        let mut l = layout(&doc);
        println!(
            "== {name} ({:?})\n{}",
            t.elapsed(),
            kiln_doc_layout::snapshot::summary(&l)
        );
        for i in 0..l.pages().len().min(6) {
            let img = render_page(&mut l, &doc, i, 72.0, false);
            std::fs::write(format!("{dir}/{name}-{i}.png"), img.encode_png().unwrap()).unwrap();
        }
    }
}
