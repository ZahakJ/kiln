// SPDX-License-Identifier: AGPL-3.0-or-later
//! Video and audio: Insert ▸ Video and Audio embed a clip with its poster frame (the
//! chrome reads the first frame of a video, or draws the audio icon) as a media shape the
//! renderer draws as its poster and the pptx writer saves as PowerPoint's `p:pic` with
//! `a:videoFile`/`a:audioFile` and `p14:media`. During the show the chrome plays the clips of
//! the slide over it ([`DeckQuery::SlideMedia`](kiln_protocol::commands::DeckQuery::SlideMedia),
//! `MediaBytes`): a click on a clip plays or pauses it, as PowerPoint's When Clicked On.
//!
//! Insert ▸ Equation is here too: Office Math from the linear form Kiln Write takes,
//! drawn on the slide as its linear text.

use kiln_common::{KilnError, Result};
use kiln_deck::ops::{Op, Transaction};
use kiln_deck::shape::{Media, MediaKind, MediaSource};
use kiln_deck::{Part, Shape, ShapeKind, Transform};
use kiln_protocol::commands::DeckRect;
use kiln_protocol::commands::deck_features::{DeckMediaItem, DeckMediaKind, DeckSlideMedia};

use super::super::DeckDoc;
use crate::model::Change;

/// The largest clip embedded (PowerPoint itself takes far more; the web core holds the file in
/// memory).
const MAX_MEDIA: usize = 512 * 1024 * 1024;
/// The side of the audio icon on the slide: 0.5 in, as PowerPoint inserts it.
const AUDIO_SIDE: i64 = 457_200;

/// The file extension of a clip by its media type, else by its name.
fn extension(mime: &str, name: &str) -> String {
    let known = match mime {
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/quicktime" => Some("mov"),
        "video/x-ms-wmv" => Some("wmv"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/ogg" => Some("ogg"),
        _ => None,
    };
    known.map_or_else(
        || {
            name.rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase())
                .filter(|e| {
                    !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric())
                })
                .unwrap_or_else(|| "bin".into())
        },
        str::to_owned,
    )
}

/// The media type a clip is saved and played as.
fn media_type(kind: DeckMediaKind, mime: &str, ext: &str) -> String {
    if mime.contains('/') {
        return mime.to_owned();
    }
    match (kind, ext) {
        (_, "mp4") => "video/mp4".into(),
        (_, "webm") => "video/webm".into(),
        (_, "mp3") => "audio/mpeg".into(),
        (_, "m4a") => "audio/mp4".into(),
        (_, "wav") => "audio/wav".into(),
        (DeckMediaKind::Video, _) => "video/unknown".into(),
        (DeckMediaKind::Audio, _) => "audio/unknown".into(),
    }
}

impl DeckDoc {
    /// A free part name `/ppt/media/<stem>N.<ext>`.
    fn free_part(&self, stem: &str, ext: &str) -> String {
        (1..)
            .map(|n| format!("/ppt/media/{stem}{n}.{ext}"))
            .find(|n| !self.pres.parts.contains_key(n))
            .unwrap_or_else(|| format!("/ppt/media/{stem}.{ext}"))
    }

    /// Insert ▸ Video or Audio.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_media(
        &mut self,
        kind: DeckMediaKind,
        name: &str,
        mime: &str,
        bytes: &[u8],
        poster: &[u8],
        (width, height): (u32, u32),
    ) -> Result<Change> {
        if bytes.is_empty() {
            return Err(KilnError::InvalidInput("the file is empty".into()));
        }
        if bytes.len() > MAX_MEDIA {
            return Err(KilnError::InvalidInput(format!(
                "{name} is larger than {} MB",
                MAX_MEDIA / (1024 * 1024)
            )));
        }
        if kiln_deck_layout::picture::decode(poster, kiln_scene::ImageId(0)).is_none() {
            return Err(KilnError::InvalidInput(
                "the poster frame is not a picture".into(),
            ));
        }
        let slide = self.slide_id()?;
        let ext = extension(mime, name);
        let content_type = media_type(kind, mime, &ext);
        let clip = self.free_part("media", &ext);
        let still = self.free_part("image", "png");
        let (sw, sh) = (self.pres.slide_size.cx, self.pres.slide_size.cy);
        let xfrm = match kind {
            DeckMediaKind::Audio => Transform::new(
                (sw - AUDIO_SIDE) / 2,
                (sh - AUDIO_SIDE) / 2,
                AUDIO_SIDE,
                AUDIO_SIDE,
            ),
            DeckMediaKind::Video => {
                // As large as fits two thirds of the slide, centred, in the video's shape.
                let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
                #[allow(clippy::cast_precision_loss)]
                let (bw, bh) = (sw as f64 * 2.0 / 3.0, sh as f64 * 2.0 / 3.0);
                let (w, h) = if bw / bh > aspect {
                    (bh * aspect, bh)
                } else {
                    (bw, bw / aspect)
                };
                #[allow(clippy::cast_possible_truncation)]
                let (w, h) = (w.round() as i64, h.round() as i64);
                Transform::new((sw - w) / 2, (sh - h) / 2, w.max(1), h.max(1))
            }
        };
        let id = self.pres.alloc_id();
        let base = name.rsplit_once('.').map_or(name, |(b, _)| b);
        let mut shape = Shape::new(
            id,
            if base.trim().is_empty() {
                match kind {
                    DeckMediaKind::Video => format!("Video {}", id.0),
                    DeckMediaKind::Audio => format!("Audio {}", id.0),
                }
            } else {
                base.to_owned()
            },
            Some(xfrm),
            ShapeKind::Media(Media {
                kind: match kind {
                    DeckMediaKind::Video => MediaKind::Video,
                    DeckMediaKind::Audio => MediaKind::Audio,
                },
                source: MediaSource::Embedded(clip.clone()),
                poster: Some(still.clone()),
                ..Media::default()
            }),
        );
        shape.alt_text = Some(name.to_owned()).filter(|n| !n.is_empty());
        let index = self.pres.slide(slide).map_or(0, |s| s.shapes.len());
        let tx = Transaction::new(vec![
            Op::SetPart {
                key: clip,
                part: Some(Part {
                    content_type,
                    bytes: bytes.to_vec(),
                }),
            },
            Op::SetPart {
                key: still,
                part: Some(Part {
                    content_type: "image/png".into(),
                    bytes: poster.to_vec(),
                }),
            },
            Op::InsertShapes {
                parent: slide,
                index,
                shapes: vec![shape],
            },
        ])
        .with_selection(Vec::new(), vec![id]);
        self.end_text();
        self.apply(
            match kind {
                DeckMediaKind::Video => "deck.insert.video",
                DeckMediaKind::Audio => "deck.insert.audio",
            },
            tx,
        )?;
        self.view.shapes = vec![id];
        self.view.rail = false;
        Ok(Change::Content)
    }

    /// Insert ▸ Equation: Office Math from its linear form, at the caret or in a new text box.
    pub(crate) fn insert_equation(&mut self, linear: &str) -> Result<Change> {
        let omml = crate::apps::write::office::equation::linear_to_omml(linear)?;
        self.slide_id()?;
        self.begin_group();
        let result = (|| {
            if self.view.text.is_none() {
                self.insert_text_box(None)?;
            }
            self.insert_object(
                "deck.insert.equation",
                kiln_doc::InlineObject::Equation(kiln_doc::Equation {
                    omml,
                    display: false,
                }),
            )
        })();
        self.end_group();
        result
    }

    /// The clips of the slide the show shows, or the stage's.
    pub(crate) fn slide_media(&self) -> Result<DeckSlideMedia> {
        let index = self
            .show
            .as_ref()
            .map_or(self.view.current, super::super::show::Show::slide);
        let slide = self
            .pres
            .slides
            .get(index)
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no slides".into()))?;
        let mut items = Vec::new();
        for s in &slide.shapes {
            s.walk(&mut |x| {
                let ShapeKind::Media(m) = &x.kind else { return };
                if x.hidden {
                    return;
                }
                let Some(t) = kiln_deck_layout::render::absolute_xfrm(&self.pres, x.id) else {
                    return;
                };
                let px = kiln_deck_layout::geom::px;
                let part = match &m.source {
                    MediaSource::Embedded(k) => Some(k.clone()),
                    MediaSource::Linked(_) => None,
                };
                let mime = part
                    .as_ref()
                    .and_then(|k| self.pres.parts.get(k))
                    .map(|p| p.content_type.clone())
                    .unwrap_or_default();
                items.push(DeckMediaItem {
                    shape: x.id,
                    kind: match m.kind {
                        MediaKind::Video => DeckMediaKind::Video,
                        MediaKind::Audio => DeckMediaKind::Audio,
                    },
                    part,
                    mime,
                    rect: DeckRect {
                        x: px(t.x),
                        y: px(t.y),
                        width: px(t.cx),
                        height: px(t.cy),
                    },
                    trim_start_ms: m.trim_start_ms,
                    trim_end_ms: m.trim_end_ms,
                });
            });
        }
        Ok(DeckSlideMedia {
            slide: u32::try_from(index).unwrap_or(u32::MAX),
            width: kiln_deck_layout::geom::px(self.pres.slide_size.cx),
            height: kiln_deck_layout::geom::px(self.pres.slide_size.cy),
            items,
        })
    }

    /// A clip's bytes.
    pub(crate) fn media_bytes(&self, part: &str) -> Result<Vec<u8>> {
        let used = self.pres.slides.iter().any(|s| {
            let mut found = false;
            for sh in &s.shapes {
                sh.walk(&mut |x| {
                    if let ShapeKind::Media(m) = &x.kind
                        && m.source == MediaSource::Embedded(part.to_owned())
                    {
                        found = true;
                    }
                });
            }
            found
        });
        match self.pres.parts.get(part) {
            Some(p) if used => Ok(p.bytes.clone()),
            _ => Err(KilnError::InvalidInput(format!("no clip {part}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_get_an_extension_and_a_type() {
        assert_eq!(extension("video/mp4", "x"), "mp4");
        assert_eq!(extension("", "Talk.WAV"), "wav");
        assert_eq!(extension("", "noext"), "bin");
        assert_eq!(media_type(DeckMediaKind::Audio, "", "mp3"), "audio/mpeg");
        assert_eq!(
            media_type(DeckMediaKind::Video, "video/webm", "webm"),
            "video/webm"
        );
    }
}
