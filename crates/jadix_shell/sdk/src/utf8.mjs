// UTF-8 length without a browser encoder or a second string scan.
export function bytes(value, invalid = 'unpaired surrogate') {
  let size = value.length;
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code < 0x80) continue;
    if (code < 0x800) { size++; continue; }
    if (code >= 0xd800 && code <= 0xdfff) {
      const low = value.charCodeAt(++i);
      if (code > 0xdbff || !(low >= 0xdc00 && low <= 0xdfff)) throw new TypeError(invalid);
    }
    size += 2;
  }
  return size;
}
