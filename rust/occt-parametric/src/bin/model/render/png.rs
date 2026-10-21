//! Minimal PNG writer: 8-bit RGB, per-row adaptive filters, and a zlib stream
//! of one fixed-Huffman deflate block with hash-chain LZ77. Renders are mostly
//! flat background and shaded faces, so this stays small without a dependency.
//! O(pixels * MAX_CHAIN).

/// Candidates tried per position; bounds the worst case on repetitive images.
const MAX_CHAIN: usize = 16;
const WINDOW: usize = 32_768;
const HASH_BITS: u32 = 15;

/// Encodes `rgb` (row-major, 3 bytes per pixel) as a PNG file.
pub fn encode(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = width as usize * 3;
    assert_eq!(rgb.len(), stride * height as usize, "pixel buffer size");
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    let zero = vec![0; stride];
    for y in 0..height as usize {
        let row = &rgb[y * stride..(y + 1) * stride];
        let up = if y == 0 {
            &zero[..]
        } else {
            &rgb[(y - 1) * stride..y * stride]
        };
        // Minimum sum of absolute residuals among None, Sub and Up.
        let sub = |i: usize| row[i].wrapping_sub(if i >= 3 { row[i - 3] } else { 0 });
        let upf = |i: usize| row[i].wrapping_sub(up[i]);
        let cost = |f: &dyn Fn(usize) -> u8| -> u64 {
            (0..stride)
                .map(|i| (f(i) as i8).unsigned_abs() as u64)
                .sum()
        };
        let (none, s, u) = (cost(&|i| row[i]), cost(&sub), cost(&upf));
        if s <= none && s <= u {
            raw.push(1);
            raw.extend((0..stride).map(sub));
        } else if u <= none {
            raw.push(2);
            raw.extend((0..stride).map(upf));
        } else {
            raw.push(0);
            raw.extend_from_slice(row);
        }
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib(&raw));
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

pub fn crc32(data: &[u8]) -> u32 {
    let table: [u32; 256] = std::array::from_fn(|n| {
        (0..8).fold(n as u32, |c, _| {
            if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            }
        })
    });
    !data.iter().fold(!0u32, |c, &b| {
        table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8)
    })
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for block in data.chunks(5552) {
        for &x in block {
            a += x as u32;
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    count: u32,
}

impl Bits {
    /// Appends `n` bits of `value`, least significant first.
    fn put(&mut self, value: u32, n: u32) {
        self.acc |= (value as u64) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.count -= 8;
        }
    }
    /// Appends a Huffman code, which deflate stores most significant first.
    fn code(&mut self, code: u32, n: u32) {
        self.put(code.reverse_bits() >> (32 - n), n);
    }
    fn literal(&mut self, symbol: u32) {
        match symbol {
            0..=143 => self.code(0x30 + symbol, 8),
            144..=255 => self.code(0x190 + symbol - 144, 9),
            256..=279 => self.code(symbol - 256, 7),
            _ => self.code(0xC0 + symbol - 280, 8),
        }
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Index of the last table entry whose base is at most `value`.
fn bucket(table: &[u16], value: usize) -> usize {
    table.partition_point(|&base| base as usize <= value) - 1
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits {
        out: vec![0x78, 0x01],
        acc: 0,
        count: 0,
    };
    // One final block with the fixed Huffman tables.
    bits.put(1, 1);
    bits.put(1, 2);
    let hash = |i: usize| -> usize {
        let v = (data[i] as u32) << 16 | (data[i + 1] as u32) << 8 | data[i + 2] as u32;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    };
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; WINDOW];
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 2 < data.len() {
            let h = hash(i);
            prev[i % WINDOW] = head[h];
            head[h] = i;
        }
    };
    let mut i = 0;
    while i < data.len() {
        let (mut best, mut distance) = (0, 0);
        if i + 2 < data.len() {
            let mut candidate = head[hash(i)];
            let limit = (data.len() - i).min(258);
            for _ in 0..MAX_CHAIN {
                if candidate == usize::MAX || i - candidate > WINDOW - 1 {
                    break;
                }
                let length = (0..limit)
                    .take_while(|&k| data[candidate + k] == data[i + k])
                    .count();
                if length > best {
                    (best, distance) = (length, i - candidate);
                    if length == limit {
                        break;
                    }
                }
                let next = prev[candidate % WINDOW];
                if next == usize::MAX || next >= candidate {
                    break;
                }
                candidate = next;
            }
        }
        if best >= 3 {
            let l = bucket(&LENGTH_BASE, best);
            bits.literal(257 + l as u32);
            bits.put(
                (best - LENGTH_BASE[l] as usize) as u32,
                LENGTH_EXTRA[l] as u32,
            );
            let d = bucket(&DISTANCE_BASE, distance);
            bits.code(d as u32, 5);
            bits.put(
                (distance - DISTANCE_BASE[d] as usize) as u32,
                DISTANCE_EXTRA[d] as u32,
            );
            for k in i..i + best {
                insert(k, &mut head, &mut prev);
            }
            i += best;
        } else {
            bits.literal(data[i] as u32);
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    bits.literal(256);
    bits.put(0, 7);
    let mut out = bits.out;
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Inflates the fixed-Huffman blocks `zlib` writes, to check round trips.
    pub fn inflate(stream: &[u8]) -> Vec<u8> {
        assert_eq!(&stream[..2], &[0x78, 0x01]);
        let body = &stream[2..stream.len() - 4];
        let mut position = 0usize;
        let mut bit = |n: u32| -> u32 {
            let mut v = 0;
            for k in 0..n {
                let b = (body[position / 8] >> (position % 8)) & 1;
                v |= (b as u32) << k;
                position += 1;
            }
            v
        };
        assert_eq!((bit(1), bit(2)), (1, 1));
        let mut out = Vec::new();
        loop {
            // Read the fixed literal/length code most significant bit first.
            let mut code = 0;
            let mut length = 0;
            let symbol = loop {
                code = (code << 1) | bit(1);
                length += 1;
                match (length, code) {
                    (7, 0..=23) => break 256 + code,
                    (8, 0x30..=0xBF) => break code - 0x30,
                    (8, 0xC0..=0xC7) => break 280 + code - 0xC0,
                    (9, 0x190..=0x1FF) => break 144 + code - 0x190,
                    _ => assert!(length < 9),
                }
            };
            match symbol {
                0..=255 => out.push(symbol as u8),
                256 => break,
                _ => {
                    let l = symbol as usize - 257;
                    let len = LENGTH_BASE[l] as usize + bit(LENGTH_EXTRA[l] as u32) as usize;
                    let d = (0..5).fold(0, |c, _| (c << 1) | bit(1)) as usize;
                    let dist = DISTANCE_BASE[d] as usize + bit(DISTANCE_EXTRA[d] as u32) as usize;
                    for _ in 0..len {
                        out.push(out[out.len() - dist]);
                    }
                }
            }
        }
        assert_eq!(&stream[stream.len() - 4..], &adler32(&out).to_be_bytes());
        out
    }

    #[test]
    fn zlib_round_trips_varied_data() {
        let mut data: Vec<u8> = (0..200_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        data.extend(std::iter::repeat_n(7, 100_000));
        data.extend((0..50_000u32).map(|i| (i % 251) as u8));
        for sample in [
            &data[..0],
            &data[..1],
            &data[..2],
            &data[..],
            &data[200_000..],
        ] {
            let packed = zlib(sample);
            assert_eq!(inflate(&packed), sample);
        }
        // Runs of one byte compress far below one bit per byte.
        assert!(zlib(&data[200_000..300_000]).len() < 1_000);
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    /// Width, height and RGB pixels of a PNG `encode` wrote; checks chunk CRCs.
    pub fn decode(png: &[u8]) -> (usize, usize, Vec<u8>) {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let mut at = 8;
        let (mut w, mut h, mut idat) = (0, 0, Vec::new());
        while at < png.len() {
            let n = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let kind = &png[at + 4..at + 8];
            let crc = u32::from_be_bytes(png[at + 8 + n..at + 12 + n].try_into().unwrap());
            assert_eq!(crc, crc32(&png[at + 4..at + 8 + n]));
            let data = &png[at + 8..at + 8 + n];
            match kind {
                b"IHDR" => {
                    w = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
                    h = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
                    assert_eq!(&data[8..], &[8, 2, 0, 0, 0]);
                }
                b"IDAT" => idat.extend_from_slice(data),
                _ => {}
            }
            at += 12 + n;
        }
        // Undo the row filters.
        let raw = inflate(&idat);
        let stride = w * 3;
        assert_eq!(raw.len(), (stride + 1) * h);
        let mut pixels = vec![0u8; stride * h];
        for y in 0..h {
            let filter = raw[y * (stride + 1)];
            for x in 0..stride {
                let v = raw[y * (stride + 1) + 1 + x];
                let left = if x >= 3 {
                    pixels[y * stride + x - 3]
                } else {
                    0
                };
                let up = if y > 0 {
                    pixels[(y - 1) * stride + x]
                } else {
                    0
                };
                pixels[y * stride + x] = match filter {
                    0 => v,
                    1 => v.wrapping_add(left),
                    2 => v.wrapping_add(up),
                    f => panic!("filter {f}"),
                };
            }
        }
        (w, h, pixels)
    }

    #[test]
    fn png_has_valid_chunks() {
        let (w, h) = (5u32, 3u32);
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i * 17) as u8).collect();
        assert_eq!(decode(&encode(w, h, &rgb)), (5, 3, rgb));
    }
}
