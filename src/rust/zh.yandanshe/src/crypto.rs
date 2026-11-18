use aidoku::alloc::{
	format,
	string::{String, ToString},
	vec::Vec,
};

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const SBOX: [u8; 256] = build_sbox();
const RCON: [u8; 7] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];

const K: [u32; 64] = [
	0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
	0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
	0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
	0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
	0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
	0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
	0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
	0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const fn gf_mul(mut a: u8, mut b: u8) -> u8 {
	let mut p: u8 = 0;
	let mut i = 0;
	while i < 8 {
		if b & 1 != 0 {
			p ^= a;
		}
		let hi = a & 0x80;
		a <<= 1;
		if hi != 0 {
			a ^= 0x1b;
		}
		b >>= 1;
		i += 1;
	}
	p
}

const fn build_sbox() -> [u8; 256] {
	let mut sbox = [0u8; 256];
	let mut x = 0;
	while x < 256 {
		let mut inv: u8 = 0;
		if x != 0 {
			let mut y = 1;
			while y < 256 {
				if gf_mul(x as u8, y as u8) == 1 {
					inv = y as u8;
					break;
				}
				y += 1;
			}
		}
		let s = inv;
		sbox[x] =
			s ^ s.rotate_left(1) ^ s.rotate_left(2) ^ s.rotate_left(3) ^ s.rotate_left(4) ^ 0x63;
		x += 1;
	}
	sbox
}

fn b64_val(c: u8) -> Option<u8> {
	let mut i = 0;
	while i < 64 {
		if B64[i] == c {
			return Some(i as u8);
		}
		i += 1;
	}
	None
}

pub fn base64_decode(input: &str) -> Vec<u8> {
	let mut out: Vec<u8> = Vec::with_capacity(input.len() * 3 / 4 + 3);
	let mut acc: u32 = 0;
	let mut bits: u32 = 0;

	for &c in input.as_bytes() {
		if c == b'=' {
			break;
		}
		let Some(value) = b64_val(c) else {
			continue;
		};
		acc = (acc << 6) | value as u32;
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			out.push((acc >> bits) as u8);
		}
	}

	out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
	let mut state: [u32; 8] = [
		0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
		0x5be0cd19,
	];

	let mut chunks = data.chunks_exact(64);
	for block in chunks.by_ref() {
		sha256_compress(&mut state, block);
	}

	let rest = chunks.remainder();
	let mut tail = [0u8; 128];
	tail[..rest.len()].copy_from_slice(rest);
	tail[rest.len()] = 0x80;
	let total = if rest.len() < 56 { 64 } else { 128 };
	tail[total - 8..total].copy_from_slice(&((data.len() as u64) * 8).to_be_bytes());
	for block in tail[..total].chunks_exact(64) {
		sha256_compress(&mut state, block);
	}

	let mut out = [0u8; 32];
	for (index, word) in state.iter().enumerate() {
		out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
	}
	out
}

fn sha256_compress(state: &mut [u32; 8], block: &[u8]) {
	let mut w = [0u32; 64];
	for i in 0..16 {
		w[i] = u32::from_be_bytes([
			block[i * 4],
			block[i * 4 + 1],
			block[i * 4 + 2],
			block[i * 4 + 3],
		]);
	}
	for i in 16..64 {
		let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
		let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
		w[i] = w[i - 16]
			.wrapping_add(s0)
			.wrapping_add(w[i - 7])
			.wrapping_add(s1);
	}

	let mut a = state[0];
	let mut b = state[1];
	let mut c = state[2];
	let mut d = state[3];
	let mut e = state[4];
	let mut f = state[5];
	let mut g = state[6];
	let mut h = state[7];

	for i in 0..64 {
		let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
		let ch = (e & f) ^ (!e & g);
		let temp1 = h
			.wrapping_add(s1)
			.wrapping_add(ch)
			.wrapping_add(K[i])
			.wrapping_add(w[i]);
		let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
		let maj = (a & b) ^ (a & c) ^ (b & c);
		let temp2 = s0.wrapping_add(maj);

		h = g;
		g = f;
		f = e;
		e = d.wrapping_add(temp1);
		d = c;
		c = b;
		b = a;
		a = temp1.wrapping_add(temp2);
	}

	state[0] = state[0].wrapping_add(a);
	state[1] = state[1].wrapping_add(b);
	state[2] = state[2].wrapping_add(c);
	state[3] = state[3].wrapping_add(d);
	state[4] = state[4].wrapping_add(e);
	state[5] = state[5].wrapping_add(f);
	state[6] = state[6].wrapping_add(g);
	state[7] = state[7].wrapping_add(h);
}

fn key_expansion(key: &[u8; 32]) -> [u8; 240] {
	const NK: usize = 8;
	const NR: usize = 14;

	let mut w = [0u8; 240];
	w[..32].copy_from_slice(key);

	for i in NK..(4 * (NR + 1)) {
		let mut temp = [
			w[(i - 1) * 4],
			w[(i - 1) * 4 + 1],
			w[(i - 1) * 4 + 2],
			w[(i - 1) * 4 + 3],
		];
		if i % NK == 0 {
			temp = [temp[1], temp[2], temp[3], temp[0]];
			for byte in temp.iter_mut() {
				*byte = SBOX[*byte as usize];
			}
			temp[0] ^= RCON[i / NK - 1];
		} else if i % NK == 4 {
			for byte in temp.iter_mut() {
				*byte = SBOX[*byte as usize];
			}
		}
		for j in 0..4 {
			w[i * 4 + j] = w[(i - NK) * 4 + j] ^ temp[j];
		}
	}

	w
}

fn add_round_key(block: &mut [u8; 16], round_keys: &[u8; 240], round: usize) {
	for i in 0..16 {
		block[i] ^= round_keys[round * 16 + i];
	}
}

fn sub_bytes(block: &mut [u8; 16]) {
	for byte in block.iter_mut() {
		*byte = SBOX[*byte as usize];
	}
}

fn shift_rows(block: &mut [u8; 16]) {
	let src = *block;
	for row in 0..4 {
		for column in 0..4 {
			block[column * 4 + row] = src[((column + row) % 4) * 4 + row];
		}
	}
}

fn mix_columns(block: &mut [u8; 16]) {
	for column in 0..4 {
		let i = column * 4;
		let a = [block[i], block[i + 1], block[i + 2], block[i + 3]];
		block[i] = gf_mul(a[0], 2) ^ gf_mul(a[1], 3) ^ a[2] ^ a[3];
		block[i + 1] = a[0] ^ gf_mul(a[1], 2) ^ gf_mul(a[2], 3) ^ a[3];
		block[i + 2] = a[0] ^ a[1] ^ gf_mul(a[2], 2) ^ gf_mul(a[3], 3);
		block[i + 3] = gf_mul(a[0], 3) ^ a[1] ^ a[2] ^ gf_mul(a[3], 2);
	}
}

fn encrypt_block(round_keys: &[u8; 240], block: &mut [u8; 16]) {
	add_round_key(block, round_keys, 0);
	for round in 1..14 {
		sub_bytes(block);
		shift_rows(block);
		mix_columns(block);
		add_round_key(block, round_keys, round);
	}
	sub_bytes(block);
	shift_rows(block);
	add_round_key(block, round_keys, 14);
}

fn ghash_mul(value: &mut [u8; 16], hash_key: &[u8; 16]) {
	let mut z = [0u8; 16];
	let mut v = *hash_key;

	for i in 0..128 {
		if (value[i / 8] >> (7 - (i % 8))) & 1 == 1 {
			for j in 0..16 {
				z[j] ^= v[j];
			}
		}
		let lsb = v[15] & 1;
		for j in (1..16).rev() {
			v[j] = (v[j] >> 1) | ((v[j - 1] & 1) << 7);
		}
		v[0] >>= 1;
		if lsb == 1 {
			v[0] ^= 0xe1;
		}
	}

	*value = z;
}

pub fn aes256_gcm_decrypt(key: &[u8; 32], iv: &[u8], cipher: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
	if iv.len() != 12 || tag.len() != 16 {
		return None;
	}

	let round_keys = key_expansion(key);

	let mut hash_key = [0u8; 16];
	encrypt_block(&round_keys, &mut hash_key);

	let mut counter_block = [0u8; 16];
	counter_block[..12].copy_from_slice(iv);
	counter_block[15] = 1;

	let mut plain: Vec<u8> = Vec::with_capacity(cipher.len());
	for (index, chunk) in cipher.chunks(16).enumerate() {
		let mut keystream = counter_block;
		keystream[12..16].copy_from_slice(&((index as u32) + 2).to_be_bytes());
		encrypt_block(&round_keys, &mut keystream);
		for (offset, byte) in chunk.iter().enumerate() {
			plain.push(byte ^ keystream[offset]);
		}
	}

	let mut state = [0u8; 16];
	for chunk in cipher.chunks(16) {
		let mut block = [0u8; 16];
		block[..chunk.len()].copy_from_slice(chunk);
		for j in 0..16 {
			state[j] ^= block[j];
		}
		ghash_mul(&mut state, &hash_key);
	}
	let mut lengths = [0u8; 16];
	lengths[8..16].copy_from_slice(&((cipher.len() as u64) * 8).to_be_bytes());
	for j in 0..16 {
		state[j] ^= lengths[j];
	}
	ghash_mul(&mut state, &hash_key);

	let mut expected = counter_block;
	encrypt_block(&round_keys, &mut expected);
	for j in 0..16 {
		expected[j] ^= state[j];
	}
	if expected != tag {
		return None;
	}

	Some(plain)
}

fn between<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
	let start = text.find(prefix)? + prefix.len();
	let rest = &text[start..];
	let end = rest.find(suffix)?;
	Some(&rest[..end])
}

fn unescape_slash(value: &str) -> String {
	if value.contains("\\/") {
		value.replace("\\/", "/")
	} else {
		value.to_string()
	}
}

fn string_field(body: &str, key: &str) -> Option<String> {
	let pattern = format!("\"{key}\":\"");
	let raw = between(body, &pattern, "\"")?;
	Some(unescape_slash(raw))
}

fn int_field(body: &str, key: &str) -> Option<i64> {
	let pattern = format!("\"{key}\":");
	let raw = between(body, &pattern, ",").or_else(|| between(body, &pattern, "}"))?;
	raw.trim().parse::<i64>().ok()
}

fn parse_images(plain: &str) -> Vec<String> {
	let mut images: Vec<String> = Vec::new();
	let mut rest = plain;
	while let Some(index) = rest.find("\"pic\":\"") {
		let tail = &rest[index + 7..];
		let Some(end) = tail.find('"') else {
			break;
		};
		let url = unescape_slash(&tail[..end]);
		if !url.is_empty() {
			images.push(url);
		}
		rest = &tail[end..];
	}
	images
}

pub fn decode_images(body: &str, grant: &str, article_id: &str, chapter_id: &str) -> Vec<String> {
	let images: Vec<String> = Vec::new();

	if int_field(body, "status") != Some(1) {
		return images;
	}

	let Some(content_type) = string_field(body, "content_type") else {
		return images;
	};
	let Some(timestamp) = int_field(body, "timestamp") else {
		return images;
	};
	let Some(nonce) = string_field(body, "nonce") else {
		return images;
	};
	let Some(grant_id) = string_field(body, "grant_id") else {
		return images;
	};
	let Some(iv) = string_field(body, "iv") else {
		return images;
	};
	let Some(tag) = string_field(body, "tag") else {
		return images;
	};
	let Some(chunks) = between(body, "\"chunks\":[", "]") else {
		return images;
	};

	let mut cipher_b64 = String::new();
	for part in chunks.split(',') {
		cipher_b64.push_str(part.trim().trim_matches('"'));
	}

	let secret = grant.split('.').next().unwrap_or_default();
	let material =
		format!("{secret}|{article_id}|{chapter_id}|{content_type}|{timestamp}|{nonce}|{grant_id}");
	let key = sha256(material.as_bytes());
	let Some(plain) = aes256_gcm_decrypt(
		&key,
		&base64_decode(&iv),
		&base64_decode(&cipher_b64),
		&base64_decode(&tag),
	) else {
		return images;
	};
	let Ok(plain) = String::from_utf8(plain) else {
		return images;
	};

	parse_images(&plain)
}
