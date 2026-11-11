/**
 * Reads one entry of a zip package (an .xlsx, .docx or .pptx) in Node, so the specs need no
 * `unzip` on the machine: the central directory gives the entry's offset and method, and stored
 * or deflated data is all an Office package uses.
 */
import { inflateRawSync } from 'node:zlib';

export function zipEntry(zip: Buffer, name: string): Buffer {
  // The end of central directory record: its signature, searched from the end (a comment may
  // follow it).
  let end = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 22 - 0xffff); i -= 1) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      end = i;
      break;
    }
  }
  if (end < 0) throw new Error('not a zip package');
  const count = zip.readUInt16LE(end + 10);
  let at = zip.readUInt32LE(end + 16);
  for (let n = 0; n < count; n += 1) {
    if (zip.readUInt32LE(at) !== 0x02014b50) throw new Error('a damaged central directory');
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLength = zip.readUInt16LE(at + 28);
    const extraLength = zip.readUInt16LE(at + 30);
    const commentLength = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const entry = zip.toString('utf8', at + 46, at + 46 + nameLength);
    if (entry === name) {
      const dataAt = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(dataAt, dataAt + size);
      if (method === 0) return Buffer.from(data);
      if (method === 8) return inflateRawSync(data);
      throw new Error(`${name}: compression method ${method}`);
    }
    at += 46 + nameLength + extraLength + commentLength;
  }
  throw new Error(`${name} is not in the package`);
}
