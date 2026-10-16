import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import type { SceneDelta } from './generated/SceneDelta';
import { decode, decodeAs, type Format, PostcardError, type Registry, SCHEMA, shortestF32 } from './postcard';

const VECTORS = fileURLToPath(new URL('../../../crates/kiln-protocol/tests/vectors', import.meta.url));

interface VectorFile {
  type: string;
  format: Format;
  values: unknown[];
}

const files = readdirSync(VECTORS)
  .filter((f) => f.endsWith('.json'))
  .sort();

describe('postcard vectors produced by Rust', () => {
  it('has a vector for every protocol type', () => {
    expect(files.length).toBeGreaterThanOrEqual(50);
    for (const name of ['Command', 'Event', 'SceneDelta', 'Viewport', 'HostReply', 'QueryResult']) {
      expect(files).toContain(`${name}.json`);
    }
  });

  it.each(files)('decodes %s', (file) => {
    const vector = JSON.parse(readFileSync(`${VECTORS}/${file}`, 'utf8')) as VectorFile;
    const bytes = new Uint8Array(readFileSync(`${VECTORS}/${file.replace(/\.json$/, '.bin')}`));
    const decoded = decode({ SEQ: vector.format }, bytes, SCHEMA, { canonicalF32: true });
    expect(decoded).toEqual(vector.values);
  });
});

describe('postcard decoder', () => {
  it('decodes a frame', () => {
    // SceneDelta { revision: 300 } = varint 300 = [0xac, 0x02]
    expect(decodeAs<SceneDelta>('SceneDelta', Uint8Array.of(0xac, 0x02))).toEqual({ revision: 300 });
  });

  it('decodes primitives', () => {
    expect(decode('I32', Uint8Array.of(0x03))).toBe(-2);
    expect(decode('I64', Uint8Array.of(0x04))).toBe(2);
    expect(decode('I8', Uint8Array.of(0xff))).toBe(-1);
    expect(decode('U16', Uint8Array.of(0xff, 0xff, 0x03))).toBe(65535);
    expect(decode('BOOL', Uint8Array.of(1))).toBe(true);
    expect(decode('CHAR', Uint8Array.of(2, 0xc3, 0xa9))).toBe('é');
    expect(decode('BYTES', Uint8Array.of(2, 7, 8))).toEqual([7, 8]);
    expect(decode('UNIT', new Uint8Array())).toBeNull();
    expect(decode({ OPTION: 'U8' }, Uint8Array.of(0))).toBeNull();
    expect(decode({ TUPLE: ['U8', 'BOOL'] }, Uint8Array.of(5, 0))).toEqual([5, false]);
    expect(decode({ TUPLEARRAY: { CONTENT: 'U8', SIZE: 2 } }, Uint8Array.of(1, 2))).toEqual([1, 2]);
    expect(decode({ MAP: { KEY: 'STR', VALUE: 'U8' } }, Uint8Array.of(1, 1, 0x61, 9))).toEqual({ a: 9 });
    const f64 = new Uint8Array(8);
    new DataView(f64.buffer).setFloat64(0, -2.5, true);
    expect(decode('F64', f64)).toBe(-2.5);
  });

  it('decodes named containers of every kind', () => {
    const registry: Registry = {
      Unit: 'UNITSTRUCT',
      Wrap: { NEWTYPESTRUCT: 'U8' },
      Pair: { TUPLESTRUCT: ['U8', 'U8'] },
      Shape: {
        ENUM: {
          '0': { Dot: 'UNIT' },
          '1': { Size: { NEWTYPE: 'U8' } },
          '2': { Line: { TUPLE: ['U8', 'U8'] } },
          '3': { Box: { STRUCT: [{ w: 'U8' }, { h: 'U8' }] } },
        },
      },
    };
    expect(decode({ TYPENAME: 'Unit' }, new Uint8Array(), registry)).toBeNull();
    expect(decode({ TYPENAME: 'Wrap' }, Uint8Array.of(4), registry)).toBe(4);
    expect(decode({ TYPENAME: 'Pair' }, Uint8Array.of(1, 2), registry)).toEqual([1, 2]);
    expect(
      decode({ SEQ: { TYPENAME: 'Shape' } }, Uint8Array.of(4, 0, 1, 7, 2, 1, 2, 3, 3, 4), registry),
    ).toEqual(['Dot', { Size: 7 }, { Line: [1, 2] }, { Box: { w: 3, h: 4 } }]);
  });

  it('rejects malformed input', () => {
    const bad: [Format, number[], RegExp][] = [
      ['U32', [0x80], /unexpected end/],
      ['U32', [0x80, 0x80, 0x80, 0x80, 0x80, 0x01], /longer than/],
      ['U32', [0xff, 0xff, 0xff, 0xff, 0x7f], /overflows/],
      ['U64', [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f], /does not fit/],
      ['BOOL', [2], /invalid bool/],
      ['STR', [1, 0xff], /UTF-8/],
      ['STR', [5, 0x61], /exceeds/],
      [{ OPTION: 'U8' }, [2], /option tag/],
      ['U8', [1, 2], /trailing/],
      [{ TYPENAME: 'Nope' }, [0], /not in the schema/],
      [{ TYPENAME: 'App' }, [9], /no variant 9/],
    ];
    for (const [format, bytes, message] of bad) {
      expect(() => decode(format, Uint8Array.from(bytes))).toThrow(message);
    }
    expect(() => decode('U8', new Uint8Array())).toThrow(PostcardError);
  });

  it('prints f32 like Rust when asked', () => {
    expect(shortestF32(Math.fround(0.1))).toBe(0.1);
    expect(shortestF32(Math.fround(16384.25))).toBe(16384.25);
    expect(shortestF32(Math.fround(1e-3))).toBe(0.001);
    expect(shortestF32(0)).toBe(0);
    expect(shortestF32(Number.POSITIVE_INFINITY)).toBe(Number.POSITIVE_INFINITY);
    const f32 = new Uint8Array(4);
    new DataView(f32.buffer).setFloat32(0, 0.1, true);
    expect(decode('F32', f32)).toBe(Math.fround(0.1));
    expect(decode('F32', f32, SCHEMA, { canonicalF32: true })).toBe(0.1);
  });
});
