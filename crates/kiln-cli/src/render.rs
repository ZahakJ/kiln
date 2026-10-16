// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli render --scratch <file.txt> --png <out.png> [options]` (WP-54): lays a text file out
//! as a scratch document and writes the frame the canvas painter would show, painted by
//! `kiln-raster`. The rest of `render` (docx/xlsx/pptx pages, PDF) arrives with WP-60.

use std::path::PathBuf;

use kiln_protocol::geom::Point;
use kiln_protocol::{OpenHint, ViewMode, Viewport};
use kiln_raster::{Compositor, PaintOptions, Rgba, View};
use kiln_scene::Palette;
use kiln_session::{DeferredHost, Session};

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
pub const USAGE: &str = "kiln-cli render --scratch <file.txt> --png <out.png>
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

/// Runs `render` with the arguments after the subcommand name.
///
/// # Errors
///
/// A message for the user.
pub fn run(args: &[String]) -> Result<String, String> {
    let o = parse(args)?;
    let img = render(&o)?;
    let png = img.encode_png()?;
    std::fs::write(&o.output, png).map_err(|e| format!("{}: {e}", o.output.display()))?;
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
}
