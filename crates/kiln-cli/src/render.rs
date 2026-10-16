// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli render`, in two forms:
//!
//! - `render <file> --png out/page-%03d.png [--page N] [--dpi 96] [--dark]` (WP-60): opens the
//!   file through `kiln-session`'s app registry and writes its pages, painted by `kiln-raster`
//!   (see [`crate::pages`]). File kinds work as soon as their app registers in
//!   `kiln-session/src/apps/mod.rs`; today that is the scratch document (`.txt`, `.md`).
//! - `render --scratch <file.txt> --png <out.png> [options]` (WP-54): the frame the canvas surface
//!   would show for a viewport (window size, zoom, scroll), for golden images and painter parity.

use std::path::PathBuf;

use kiln_protocol::geom::Point;
use kiln_protocol::{OpenHint, ViewMode, Viewport};
use kiln_raster::{Compositor, PaintOptions, Rgba, View};
use kiln_scene::Palette;
use kiln_session::{DeferredHost, Session};

use crate::CliError;
use crate::pages::{PageOptions, RenderedPage};

/// Options of `render --scratch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// The text file.
    pub input: PathBuf,
    /// The PNG to write.
    pub output: PathBuf,
    /// Surface width in CSS pixels.
    pub width: f32,
    /// Surface height in CSS pixels.
    pub height: f32,
    /// Zoom factor.
    pub zoom: f32,
    /// Device pixel ratio.
    pub dpr: f32,
    /// Horizontal scroll in document pixels (`None`: centre the page like the surface does).
    pub scroll_x: Option<f32>,
    /// Vertical scroll in document pixels.
    pub scroll_y: f32,
    /// `light` or `dark`.
    pub theme: String,
}

/// Usage text of the subcommand.
pub const USAGE: &str = "kiln-cli render <file> --png <out.png | out/page-%03d.png | dir/>
    [--page N[,M...]] [--dpi 96] [--dark]
kiln-cli render --scratch <file.txt> --png <out.png>
    [--width 1000] [--height 900] [--zoom 1] [--dpr 1]
    [--scroll-x <px>] [--scroll-y 0] [--theme light|dark]";

fn number(flag: &str, value: Option<&String>) -> Result<f32, String> {
    let v = value.ok_or_else(|| format!("{flag} needs a value"))?;
    let n: f32 = v
        .parse()
        .map_err(|_| format!("{flag}: '{v}' is not a number"))?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(format!("{flag}: '{v}' is not a finite number"))
    }
}

/// Parses the arguments after `render`.
///
/// # Errors
///
/// A message for unknown flags, missing values and out-of-range numbers.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut input = None;
    let mut output = None;
    let mut o = Options {
        input: PathBuf::new(),
        output: PathBuf::new(),
        width: 1000.0,
        height: 900.0,
        zoom: 1.0,
        dpr: 1.0,
        scroll_x: None,
        scroll_y: 0.0,
        theme: "light".into(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scratch" => input = it.next().map(PathBuf::from),
            "--png" => output = it.next().map(PathBuf::from),
            "--width" => o.width = number(a, it.next())?,
            "--height" => o.height = number(a, it.next())?,
            "--zoom" => o.zoom = number(a, it.next())?,
            "--dpr" => o.dpr = number(a, it.next())?,
            "--scroll-x" => o.scroll_x = Some(number(a, it.next())?),
            "--scroll-y" => o.scroll_y = number(a, it.next())?,
            "--theme" => {
                o.theme = it
                    .next()
                    .cloned()
                    .ok_or_else(|| "--theme needs a value".to_owned())?;
            }
            other => return Err(format!("unknown option '{other}'\n\n{USAGE}")),
        }
    }
    o.input = input.ok_or_else(|| format!("--scratch <file.txt> is required\n\n{USAGE}"))?;
    o.output = output.ok_or_else(|| format!("--png <out.png> is required\n\n{USAGE}"))?;
    if !(1.0..=8192.0).contains(&o.width) || !(1.0..=8192.0).contains(&o.height) {
        return Err("--width and --height must be between 1 and 8192".into());
    }
    if !(0.1..=5.0).contains(&o.zoom) {
        return Err("--zoom must be between 0.1 and 5".into());
    }
    if !(0.5..=4.0).contains(&o.dpr) {
        return Err("--dpr must be between 0.5 and 4".into());
    }
    if Palette::named(&o.theme).is_none() {
        return Err(format!("--theme must be light or dark, not '{}'", o.theme));
    }
    Ok(o)
}

/// Renders and returns the pixels (without writing).
///
/// # Errors
///
/// When the file cannot be read or opened, or the frame cannot be painted.
pub fn render(o: &Options) -> Result<Rgba, String> {
    let bytes = std::fs::read(&o.input).map_err(|e| format!("{}: {e}", o.input.display()))?;
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let name = o
        .input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let doc = session
        .open(
            &bytes,
            OpenHint {
                name,
                format: Some(kiln_protocol::FileFormat::Txt),
                ..OpenHint::default()
            },
        )
        .map_err(|e| e.to_string())?
        .doc;
    let mut viewport = Viewport {
        width_px: o.width,
        height_px: o.height,
        scroll: Point::new(o.scroll_x.unwrap_or(0.0), o.scroll_y),
        zoom: o.zoom,
        dpr: o.dpr,
        mode: ViewMode::PrintLayout,
    };
    let mut compositor = Compositor::new();
    session
        .set_viewport(doc, viewport)
        .map_err(|e| e.to_string())?;
    let delta = session.frame(doc).map_err(|e| e.to_string())?;
    compositor.apply(&delta).map_err(|e| e.to_string())?;
    if o.scroll_x.is_none() {
        // Centre the page when the surface is wider than the document (as the surface does).
        let visible_w = o.width / o.zoom;
        let extent = compositor.extent();
        if extent.w < visible_w {
            viewport.scroll.x = -(visible_w - extent.w) / 2.0;
            session
                .set_viewport(doc, viewport)
                .map_err(|e| e.to_string())?;
            let delta = session.frame(doc).map_err(|e| e.to_string())?;
            compositor.apply(&delta).map_err(|e| e.to_string())?;
        }
    }
    let palette = Palette::named(&o.theme).unwrap_or_else(Palette::light);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let view = View {
        width: (o.width * o.dpr).round() as u32,
        height: (o.height * o.dpr).round() as u32,
        scroll_x: viewport.scroll.x,
        scroll_y: viewport.scroll.y,
        options: PaintOptions::default(),
    };
    let pixmap = compositor
        .render(&view, &palette)
        .map_err(|e| e.to_string())?;
    Ok(Rgba::from_pixmap(&pixmap))
}

/// Options of `render <file>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOptions {
    /// The document.
    pub input: PathBuf,
    /// Output file, pattern or directory.
    pub output: String,
    /// Pages, resolution and theme.
    pub pages: PageOptions,
}

/// Parses the arguments of `render <file>`.
///
/// # Errors
///
/// A message for unknown flags and bad values.
pub fn parse_file(args: &[String]) -> Result<FileOptions, String> {
    let mut input = None;
    let mut output = None;
    let mut pages = PageOptions::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--png" => output = it.next().cloned(),
            "--dark" => pages.dark = true,
            "--light" => pages.dark = false,
            "--dpi" => {
                let v = it.next().ok_or("--dpi needs a value")?;
                pages.dpi = v
                    .parse()
                    .map_err(|_| format!("--dpi: '{v}' is not a whole number"))?;
                if !crate::pages::DPI_RANGE.contains(&pages.dpi) {
                    return Err(format!(
                        "--dpi must be between {} and {}",
                        crate::pages::DPI_RANGE.start(),
                        crate::pages::DPI_RANGE.end()
                    ));
                }
            }
            "--page" | "--pages" => {
                let v = it.next().ok_or("--page needs a page number")?;
                let mut list = Vec::new();
                for part in v.split(',') {
                    let n: usize = part
                        .trim()
                        .parse()
                        .map_err(|_| format!("--page: '{part}' is not a page number"))?;
                    if n == 0 {
                        return Err("--page: pages are numbered from 1".into());
                    }
                    list.push(n);
                }
                pages.pages = Some(list);
            }
            s if s.starts_with('-') => return Err(format!("unknown option '{s}'\n\n{USAGE}")),
            s if input.is_none() => input = Some(PathBuf::from(s)),
            s => return Err(format!("unexpected argument '{s}'\n\n{USAGE}")),
        }
    }
    Ok(FileOptions {
        input: input.ok_or_else(|| format!("a file is required\n\n{USAGE}"))?,
        output: output.ok_or_else(|| format!("--png <out> is required\n\n{USAGE}"))?,
        pages,
    })
}

/// Opens `input` in a fresh session and renders its pages.
///
/// # Errors
///
/// Unreadable or unsupported files and render errors.
pub fn render_file(o: &FileOptions) -> Result<Vec<RenderedPage>, String> {
    let bytes = std::fs::read(&o.input).map_err(|e| format!("{}: {e}", o.input.display()))?;
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let hint = OpenHint {
        name: o
            .input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        ..OpenHint::default()
    };
    let doc = session
        .open(&bytes, hint)
        .map_err(|e| format!("{}: {e}", o.input.display()))?
        .doc;
    // Kinds whose app has no layout yet (Write before WP-63) open but have no pages.
    crate::pages::render(&mut session, doc, &o.pages)
        .map_err(|e| format!("{}: {e}", o.input.display()))
}

/// Runs `render` with the arguments after the subcommand name.
///
/// # Errors
///
/// Usage errors (exit status 2) and failures (exit status 1).
pub fn run(args: &[String]) -> Result<String, CliError> {
    if !args.iter().any(|a| a == "--scratch") {
        let o = parse_file(args).map_err(CliError::Usage)?;
        let pages = render_file(&o).map_err(CliError::Failed)?;
        let written = crate::pages::write_pages(&pages, &o.output).map_err(CliError::Failed)?;
        let lines: Vec<String> = pages
            .iter()
            .zip(&written)
            .map(|(p, path)| {
                format!(
                    "wrote {} (page {}, {}×{})",
                    path.display(),
                    p.number,
                    p.image.width,
                    p.image.height
                )
            })
            .collect();
        return Ok(lines.join("\n"));
    }
    let o = parse(args).map_err(CliError::Usage)?;
    let img = render(&o).map_err(CliError::Failed)?;
    let png = img.encode_png().map_err(CliError::Failed)?;
    std::fs::write(&o.output, png)
        .map_err(|e| CliError::Failed(format!("{}: {e}", o.output.display())))?;
    Ok(format!(
        "wrote {} ({}×{})",
        o.output.display(),
        img.width,
        img.height
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_options_and_rejects_bad_ones() {
        let o = parse(&args(
            "--scratch a.txt --png b.png --dpr 2 --theme dark --scroll-y 40",
        ))
        .unwrap();
        assert_eq!(o.input, PathBuf::from("a.txt"));
        assert_eq!((o.dpr, o.scroll_y, o.scroll_x), (2.0, 40.0, None));
        assert_eq!(o.theme, "dark");
        assert!(
            parse(&args("--png b.png"))
                .unwrap_err()
                .contains("--scratch")
        );
        assert!(parse(&args("--scratch a --png b --zoom 9")).is_err());
        assert!(parse(&args("--scratch a --png b --width nope")).is_err());
        assert!(parse(&args("--scratch a --png b --theme sepia")).is_err());
        assert!(parse(&args("--scratch a --png b --frobnicate")).is_err());
        assert!(parse(&args("--scratch a --png b --dpr NaN")).is_err());
    }

    #[test]
    fn renders_the_lorem_fixture_like_the_golden() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/scene");
        let o = parse(&[
            "--scratch".into(),
            root.join("lorem.txt").to_string_lossy().into_owned(),
            "--png".into(),
            "unused.png".into(),
        ])
        .unwrap();
        let img = render(&o).unwrap();
        assert_eq!((img.width, img.height), (1000, 900));
        kiln_raster::golden::check_in(
            &img,
            &root.join("lorem.png"),
            &kiln_raster::golden::diff_dir(),
        )
        .unwrap();
        let missing = Options {
            input: root.join("missing.txt"),
            ..o
        };
        assert!(render(&missing).is_err());
    }

    fn lorem() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/scene/lorem.txt")
    }

    #[test]
    fn parses_file_options() {
        let o = parse_file(&args(
            "doc.txt --png out/p-%03d.png --page 2,3 --dpi 150 --dark",
        ))
        .unwrap();
        assert_eq!(o.input, PathBuf::from("doc.txt"));
        assert_eq!(o.output, "out/p-%03d.png");
        assert_eq!(o.pages.pages, Some(vec![2, 3]));
        assert_eq!((o.pages.dpi, o.pages.dark), (150, true));
        assert!(parse_file(&args("doc.txt")).unwrap_err().contains("--png"));
        assert!(
            parse_file(&args("--png a.png"))
                .unwrap_err()
                .contains("file is required")
        );
        assert!(parse_file(&args("a --png b --dpi 9000")).is_err());
        assert!(parse_file(&args("a --png b --page 0")).is_err());
        assert!(parse_file(&args("a --png b --page x")).is_err());
        assert!(parse_file(&args("a b --png c")).is_err());
        assert!(matches!(run(&args("a.txt")), Err(CliError::Usage(_))));
    }

    #[test]
    fn renders_pages_of_a_text_file() {
        let o = FileOptions {
            input: lorem(),
            output: String::new(),
            pages: PageOptions::default(),
        };
        let pages = render_file(&o).unwrap();
        assert!(!pages.is_empty());
        let first = &pages[0];
        assert_eq!(first.number, 1);
        // US Letter at 96 dpi.
        assert_eq!((first.image.width, first.image.height), (816, 1056));
        let px = |img: &Rgba, x: u32, y: u32| {
            let i = ((y * img.width + x) * 4) as usize;
            [img.data[i], img.data[i + 1], img.data[i + 2]]
        };
        let corner = px(&first.image, 5, 5);
        assert!(corner.iter().all(|c| *c > 240), "light page: {corner:?}");
        let ink = first
            .image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] < 100)
            .count();
        assert!(ink > 1000, "text is painted: {ink}");
        // Dark theme, page 1 only, at 48 dpi.
        let dark = render_file(&FileOptions {
            pages: PageOptions {
                dpi: 48,
                dark: true,
                pages: Some(vec![1]),
            },
            ..o.clone()
        })
        .unwrap();
        assert_eq!(dark.len(), 1);
        assert_eq!((dark[0].image.width, dark[0].image.height), (408, 528));
        let c = px(&dark[0].image, 3, 3);
        assert!(c.iter().all(|v| *v < 90), "dark page: {c:?}");
        let missing = render_file(&FileOptions {
            pages: PageOptions {
                pages: Some(vec![999]),
                ..PageOptions::default()
            },
            ..o
        });
        assert!(missing.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn writes_numbered_pages_and_refuses_unknown_kinds() {
        let dir = std::env::temp_dir().join(format!("kiln-cli-render-{}", std::process::id()));
        let pattern = format!("{}/page-%03d.png", dir.display());
        let out = run(&[
            lorem().to_string_lossy().into_owned(),
            "--png".into(),
            pattern,
            "--dpi".into(),
            "24".into(),
        ])
        .unwrap();
        assert!(out.contains("page-001.png"), "{out}");
        assert!(dir.join("page-001.png").is_file());
        // Word documents render through the Write layout (WP-63).
        let docx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/docx/features/tables.docx");
        let out = run(&[
            docx.to_string_lossy().into_owned(),
            "--png".into(),
            dir.join("t.png").to_string_lossy().into_owned(),
            "--dpi".into(),
            "24".into(),
        ])
        .unwrap();
        assert!(
            out.contains("t.png") && dir.join("t.png").is_file(),
            "{out}"
        );
        // A kind no app opens is refused, naming the file.
        let unknown = dir.join("notes.xyz");
        std::fs::write(&unknown, b"?").unwrap();
        let err = run(&[
            unknown.to_string_lossy().into_owned(),
            "--png".into(),
            dir.join("u.png").to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(
            matches!(err, CliError::Failed(_) | CliError::Usage(_)),
            "{err}"
        );
        assert!(
            err.to_string().contains("notes.xyz") || err.to_string().contains("xyz"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
