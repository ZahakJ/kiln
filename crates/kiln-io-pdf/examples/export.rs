//! Times the PDF export of a document: `cargo run --release -p kiln-io-pdf --example export -- in.docx out.pdf`.
fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: export <in.docx> <out.pdf>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).expect("input");
    let t = std::time::Instant::now();
    let doc = kiln_io_docx::read_docx(&bytes).expect("docx").document;
    let layout =
        kiln_io_pdf::write::print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
    let t1 = t.elapsed();
    let pdf = kiln_io_pdf::export(
        &kiln_io_pdf::write::WritePrint::new(&doc, &layout, None),
        &kiln_io_pdf::PdfOptions::default(),
    )
    .expect("pdf");
    let t2 = t.elapsed();
    std::fs::write(&output, &pdf).expect("output");
    println!(
        "{} pages: read+layout {:?}, total {:?}, {} bytes",
        layout.pages().len(),
        t1,
        t2,
        pdf.len()
    );
}
