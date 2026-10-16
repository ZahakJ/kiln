/**
 * Test doubles: a recording 2D context and canvas, and a `Path2D` stand-in for Node.
 * Not exported from the package index.
 */

export interface Call {
  op: string;
  args: unknown[];
  composite: string;
  fill: unknown;
  transform: [number, number, number, number, number, number];
  alpha: number;
}

/** A 2D context that records what is drawn. */
export class RecordingContext {
  calls: Call[] = [];
  fillStyle: unknown = '#000';
  strokeStyle: unknown = '#000';
  lineWidth = 1;
  lineJoin = 'miter';
  lineCap = 'butt';
  globalAlpha = 1;
  globalCompositeOperation = 'source-over';
  private m: [number, number, number, number, number, number] = [1, 0, 0, 1, 0, 0];
  private stack: Array<{
    m: [number, number, number, number, number, number];
    composite: string;
    alpha: number;
  }> = [];
  dash: number[] = [];

  constructor(readonly canvas: { width: number; height: number }) {}

  private rec(op: string, ...args: unknown[]): void {
    this.calls.push({
      op,
      args,
      composite: this.globalCompositeOperation,
      fill: this.fillStyle,
      transform: [...this.m],
      alpha: this.globalAlpha,
    });
  }
  save(): void {
    this.stack.push({ m: [...this.m], composite: this.globalCompositeOperation, alpha: this.globalAlpha });
  }
  restore(): void {
    const s = this.stack.pop();
    if (s) {
      this.m = s.m;
      this.globalCompositeOperation = s.composite;
      this.globalAlpha = s.alpha;
    }
  }
  setTransform(a: number, b: number, c: number, d: number, e: number, f: number): void {
    this.m = [a, b, c, d, e, f];
  }
  transform(a: number, b: number, c: number, d: number, e: number, f: number): void {
    const [A, B, C, D, E, F] = this.m;
    this.m = [
      A * a + C * b,
      B * a + D * b,
      A * c + C * d,
      B * c + D * d,
      A * e + C * f + E,
      B * e + D * f + F,
    ];
  }
  getTransform(): { a: number; b: number; c: number; d: number; e: number; f: number } {
    const [a, b, c, d, e, f] = this.m;
    return { a, b, c, d, e, f };
  }
  clearRect(...a: number[]): void {
    this.rec('clearRect', ...a);
  }
  fillRect(...a: number[]): void {
    this.rec('fillRect', ...a);
  }
  fill(...a: unknown[]): void {
    this.rec('fill', ...a);
  }
  stroke(...a: unknown[]): void {
    this.rec('stroke', ...a);
  }
  clip(...a: unknown[]): void {
    this.rec('clip', ...a);
  }
  beginPath(): void {}
  rect(...a: number[]): void {
    this.rec('rect', ...a);
  }
  moveTo(): void {}
  lineTo(): void {}
  setLineDash(d: number[]): void {
    this.dash = d;
  }
  drawImage(...a: unknown[]): void {
    this.rec('drawImage', ...a);
  }
  createImageData(w: number, h: number): { width: number; height: number; data: Uint8ClampedArray } {
    return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4) };
  }
  putImageData(img: { data: Uint8ClampedArray }): void {
    this.rec('putImageData', img);
  }
  createLinearGradient(...a: number[]): {
    stops: Array<[number, string]>;
    addColorStop(o: number, c: string): void;
  } {
    const g = {
      kind: 'linear',
      a,
      stops: [] as Array<[number, string]>,
      addColorStop(o: number, c: string) {
        g.stops.push([o, c]);
      },
    };
    return g;
  }
  createRadialGradient(...a: number[]): {
    stops: Array<[number, string]>;
    addColorStop(o: number, c: string): void;
  } {
    const g = {
      kind: 'radial',
      a,
      stops: [] as Array<[number, string]>,
      addColorStop(o: number, c: string) {
        g.stops.push([o, c]);
      },
    };
    return g;
  }
}

/** A canvas whose context records. */
export class RecordingCanvas {
  readonly ctx: RecordingContext;
  constructor(
    public width: number,
    public height: number,
  ) {
    this.ctx = new RecordingContext(this);
  }
  getContext(): RecordingContext {
    return this.ctx;
  }
}

/** Installs a `Path2D` stand-in that records its segments. */
export function installPath2D(): void {
  class FakePath2D {
    segs: unknown[][] = [];
    moveTo(...a: number[]) {
      this.segs.push(['M', ...a]);
    }
    lineTo(...a: number[]) {
      this.segs.push(['L', ...a]);
    }
    quadraticCurveTo(...a: number[]) {
      this.segs.push(['Q', ...a]);
    }
    bezierCurveTo(...a: number[]) {
      this.segs.push(['C', ...a]);
    }
    closePath() {
      this.segs.push(['Z']);
    }
  }
  (globalThis as unknown as { Path2D: unknown }).Path2D = FakePath2D;
}
