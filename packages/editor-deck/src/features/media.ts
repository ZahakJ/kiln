/**
 * Insert ▸ Video and Audio in the browser: the file picked, its poster frame made here (the
 * browser decodes the video; the core cannot) — the video's first frame, or PowerPoint's speaker
 * icon for audio — and the clip handed to the core (`deck.insert.media`).
 */
import type { DeckMediaKind } from '@kiln/protocol';
import { pickFile } from '../files.ts';
import type { DeckEditorModel } from '../model.ts';
import { messageOf } from '../model.ts';

/** The file types offered, as PowerPoint offers them on the web. */
export const MEDIA_ACCEPT: Record<DeckMediaKind, string> = {
  Video: 'video/mp4,video/webm,video/quicktime,.mp4,.m4v,.webm,.mov',
  Audio: 'audio/mpeg,audio/mp4,audio/wav,audio/ogg,.mp3,.m4a,.wav,.ogg',
};

/** A canvas as PNG bytes. */
async function png(canvas: HTMLCanvasElement): Promise<number[]> {
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
  if (!blob) throw new Error('The poster frame could not be made.');
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}

/** Resolves on an element's event, or rejects on `error` or after `ms`. */
function once(el: HTMLMediaElement, event: string, ms = 8000): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => done(new Error('timeout')), ms);
    const ok = () => done(null);
    const bad = () => done(new Error('error'));
    function done(e: Error | null) {
      clearTimeout(timer);
      el.removeEventListener(event, ok);
      el.removeEventListener('error', bad);
      if (e) reject(e);
      else resolve();
    }
    el.addEventListener(event, ok);
    el.addEventListener('error', bad);
  });
}

/** A film frame on dark grey, when the browser cannot show the video. */
function filmPoster(w: number, h: number): HTMLCanvasElement {
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  const g = c.getContext('2d');
  if (!g) return c;
  g.fillStyle = '#262626';
  g.fillRect(0, 0, w, h);
  g.fillStyle = '#d9d9d9';
  const s = Math.min(w, h) / 6;
  g.beginPath();
  g.moveTo(w / 2 - s / 2, h / 2 - s * 0.6);
  g.lineTo(w / 2 + s * 0.6, h / 2);
  g.lineTo(w / 2 - s / 2, h / 2 + s * 0.6);
  g.closePath();
  g.fill();
  return c;
}

/** The first frame of a video (just after its start, as PowerPoint picks it), and its size. */
export async function videoPoster(file: File): Promise<{ png: number[]; width: number; height: number }> {
  const url = URL.createObjectURL(file);
  const v = document.createElement('video');
  v.muted = true;
  v.preload = 'auto';
  v.playsInline = true;
  v.src = url;
  try {
    await once(v, 'loadeddata');
    const t = Number.isFinite(v.duration) && v.duration > 0 ? Math.min(0.1, v.duration / 2) : 0;
    if (t > 0) {
      v.currentTime = t;
      await once(v, 'seeked');
    }
    const w = v.videoWidth || 1280;
    const h = v.videoHeight || 720;
    const c = document.createElement('canvas');
    c.width = w;
    c.height = h;
    const g = c.getContext('2d', { willReadFrequently: true });
    g?.drawImage(v, 0, 0, w, h);
    // A frame the browser did not paint (it could not decode it) is no poster.
    const seen = g?.getImageData(0, 0, w, h).data.some((x, i) => i % 4 === 3 && x > 0) ?? false;
    if (!seen) throw new Error('no frame');
    return { png: await png(c), width: w, height: h };
  } catch {
    return { png: await png(filmPoster(1280, 720)), width: 1280, height: 720 };
  } finally {
    v.removeAttribute('src');
    v.load();
    URL.revokeObjectURL(url);
  }
}

/** PowerPoint's audio icon: a grey speaker with two sound waves, 192 px square. */
export async function audioPoster(): Promise<{ png: number[]; width: number; height: number }> {
  const n = 192;
  const c = document.createElement('canvas');
  c.width = n;
  c.height = n;
  const g = c.getContext('2d');
  if (g) {
    g.scale(n / 24, n / 24);
    g.fillStyle = '#7f7f7f';
    g.strokeStyle = '#7f7f7f';
    g.lineWidth = 1.8;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.beginPath();
    g.moveTo(3, 9);
    g.lineTo(7, 9);
    g.lineTo(12, 4.5);
    g.lineTo(12, 19.5);
    g.lineTo(7, 15);
    g.lineTo(3, 15);
    g.closePath();
    g.fill();
    g.beginPath();
    g.arc(12, 12, 4, -Math.PI / 4, Math.PI / 4);
    g.stroke();
    g.beginPath();
    g.arc(12, 12, 7.5, -Math.PI / 3.2, Math.PI / 3.2);
    g.stroke();
  }
  return { png: await png(c), width: n, height: n };
}

/** Picks a file of `kind`. */
function pick(kind: DeckMediaKind): Promise<File | null> {
  return pickFile(MEDIA_ACCEPT[kind], `deck-insert-${kind.toLowerCase()}-file`);
}

/** Inserts a clip from a file (picked when none is given). */
export async function insertMedia(
  model: DeckEditorModel,
  kind: DeckMediaKind,
  given?: File,
): Promise<boolean> {
  const file = given ?? (await pick(kind));
  if (!file) return false;
  try {
    const poster = kind === 'Video' ? await videoPoster(file) : await audioPoster();
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    return await model.core('deck.insert.media', {
      kind,
      name: file.name,
      mime: file.type,
      bytes,
      poster: poster.png,
      width: poster.width,
      height: poster.height,
    });
  } catch (error) {
    model.set({ notice: messageOf(error) });
    return false;
  }
}
