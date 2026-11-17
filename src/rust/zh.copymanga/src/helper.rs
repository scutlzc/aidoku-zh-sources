use aidoku::{
	alloc::{string::String, string::ToString, vec::Vec},
	helpers::uri::encode_uri,
	imports::{html::Document, net::Request},
	prelude::*,
	Result,
};
use serde_json::Value;

use crate::crypto;

const WWW_URL: &str = "https://www.mangacopy.com";
const API_URL: &str = "https://api.mangacopy.com/api/v3";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

fn hex_value(byte: u8) -> Option<u8> {
	match byte {
		b'0'..=b'9' => Some(byte - b'0'),
		b'a'..=b'f' => Some(byte - b'a' + 10),
		b'A'..=b'F' => Some(byte - b'A' + 10),
		_ => None,
	}
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
	let bytes = text.as_bytes();

	if bytes.len() % 2 != 0 {
		return None;
	}

	let mut result: Vec<u8> = Vec::new();

	for chunk in bytes.chunks(2) {
		let high = hex_value(chunk[0])?;
		let low = hex_value(chunk[1])?;
		result.push((high << 4) | low);
	}

	Some(result)
}

pub fn substring_after(text: &str, pattern: &str) -> Option<String> {
	text.find(pattern)
		.map(|index| text[index + pattern.len()..].to_string())
}

pub fn substring_before(text: &str, pattern: &str) -> Option<String> {
	text.find(pattern).map(|index| text[..index].to_string())
}

pub fn between(text: &str, start: &str, end: &str) -> Option<String> {
	substring_after(text, start).and_then(|text| substring_before(&text, end))
}

pub fn decrypt(text: String, key: String) -> Result<String> {
	let text_bytes = text.as_bytes();

	if text_bytes.len() < 16 {
		bail!("Invalid encrypted text");
	}

	let iv = &text_bytes[..16];
	let cipher = hex_decode(&text[16..]).ok_or(error!("Invalid hex string"))?;
	let key_bytes = key.as_bytes();
	let key: &[u8; 16] = key_bytes
		.try_into()
		.map_err(|_| error!("Invalid decryption key"))?;
	let plain = crypto::decrypt(&cipher, key, iv).map_err(|_| error!("Failed to decrypt"))?;
	let text = String::from_utf8_lossy(&plain);

	Ok(text.replace('\u{feff}', ""))
}

pub fn get_text(url: String) -> Result<String> {
	Request::get(url.as_str())?
		.header("User-Agent", UA)
		.string()
}

pub fn get_html(url: String) -> Result<Document> {
	Ok(Request::get(url.as_str())?
		.header("User-Agent", UA)
		.html()?)
}

pub fn get_json(url: String) -> Result<Value> {
	let request = Request::get(url.as_str())?;

	let request = if url.starts_with(WWW_URL) {
		request.header("User-Agent", UA)
	} else {
		request
			.header("User-Agent", "COPY/2.3.1")
			.header("version", "2.3.1")
			.header("platform", "3")
			.header("region", "1")
			.header("webp", "1")
	};

	request.json_owned::<Value>()
}

pub fn gen_explore_url(theme: String, top: String, ordering: String, page: i32) -> String {
	format!(
		"{}/comics?theme={}&top={}&ordering={}&limit={}&offset={}",
		API_URL,
		theme,
		top,
		ordering,
		50,
		(page - 1) * 50,
	)
}

pub fn gen_search_url(query: String, page: i32) -> String {
	format!(
		"{}/search/comic?q={}&q_type={}&limit={}&offset={}",
		API_URL,
		encode_uri(query),
		"",
		20,
		(page - 1) * 20
	)
}

pub fn gen_rank_url(date_type: String, page: i32) -> String {
	format!(
		"{}/ranks?date_type={}&limit={}&offset={}",
		API_URL,
		date_type,
		30,
		(page - 1) * 30,
	)
}

pub fn gen_recs_url(page: i32) -> String {
	format!(
		"{}/recs?pos={}&limit={}&offset={}",
		API_URL,
		"3200102",
		30,
		(page - 1) * 30,
	)
}

pub fn gen_newest_url(page: i32) -> String {
	format!(
		"{}/update/newest?limit={}&offset={}",
		API_URL,
		30,
		(page - 1) * 30,
	)
}

pub fn gen_manga_url(id: String) -> String {
	format!("{}/comic/{}", WWW_URL, id)
}

pub fn gen_chapter_list_url(id: String) -> String {
	format!("{}/comicdetail/{}/chapters", WWW_URL, id)
}

pub fn gen_chapter_url(manga_id: String, chapter_id: String) -> String {
	format!("{}/comic/{}/chapter/{}", WWW_URL, manga_id, chapter_id)
}

pub fn gen_page_list_url(manga_id: String, chapter_id: String) -> String {
	format!("{}/comic/{}/chapter/{}", WWW_URL, manga_id, chapter_id)
}
