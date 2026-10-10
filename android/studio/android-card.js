// KONKR-only source adapter. The official Studio editor/renderer are unchanged.
// Every method acts on ONE selected, read-only SAF ROM, never a filesystem path.
import { labelKey } from './card.js';

function decode(encoded) {
  const raw = atob(encoded || '');
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

function encode(bytes) {
  const chunks = [];
  for (let i = 0; i < bytes.length; i += 8192) {
    chunks.push(String.fromCharCode(...bytes.subarray(i, i + 8192)));
  }
  return btoa(chunks.join(''));
}

export function fromAndroid(bridge) {
  const data = JSON.parse(bridge.session());
  if (!['GB', 'GBC', 'GBA'].includes(data.platform) || !data.stem ||
      !Number.isSafeInteger(data.crc) || data.crc < 0 || data.crc > 0xffffffff) {
    throw new Error('Invalid KONKR cartridge selection');
  }
  const head = decode(data.head);
  if (head.length !== 0x150) throw new Error('ROM header incomplete');
  const file = {
    slotCrc: data.crc >>> 0,
    slice(start = 0, end = head.length) {
      const part = head.slice(start, end);
      return { arrayBuffer: async () => part.buffer };
    },
  };
  const labels = new Map();
  if (data.label) {
    labels.set(labelKey(data.platform, data.stem),
      async () => new Blob([decode(data.label)], { type: 'image/png' }));
  }
  return {
    carts: [{ platform: data.platform, stem: data.stem, file: async () => file }],
    labels,
    direct: true,
    systemShells: async () => '',
    labelShells: async () => data.shellText || '',
    async write(platform, stem, bytes, replace = false) {
      if (platform !== data.platform || stem !== data.stem) {
        throw new Error('Cannot write to a different cartridge');
      }
      const result = bridge.saveLabel(platform, stem, encode(bytes), replace);
      if (!['written', 'skipped'].includes(result)) throw new Error('Label save failed');
      return result;
    },
    async writeShells(text) {
      if (bridge.saveShells(text) !== true) throw new Error('Shell save failed');
    },
  };
}
