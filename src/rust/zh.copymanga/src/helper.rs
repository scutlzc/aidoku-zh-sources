use aidoku::{
	alloc::{string::String, string::ToString, vec::Vec},
	helpers::uri::encode_uri,
	imports::{
		defaults::{defaults_get, defaults_set, DefaultValue},
		html::Document,
		net::Request,
		std::current_date,
	},
	prelude::*,
	Result,
};
use serde_json::Value;

use crate::crypto;

const WWW_URL: &str = "https://www.mangacopy.com";
const API_VERSION: &str = "3.0.0";
const TOKEN_KEY: &str = "token";
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
	let request = Request::get(url.as_str())?.header("User-Agent", UA);

	with_auth(request).string()
}

pub fn get_html(url: String) -> Result<Document> {
	let request = Request::get(url.as_str())?.header("User-Agent", UA);

	Ok(with_auth(request).html()?)
}

fn token() -> Option<String> {
	defaults_get::<String>(TOKEN_KEY).filter(|token| !token.is_empty())
}

fn with_auth(request: Request) -> Request {
	match token() {
		Some(token) => {
			let value = format!("Token {token}");
			request.header("Authorization", value.as_str())
		}
		None => request,
	}
}

pub fn get_json(url: String) -> Result<Value> {
	let request = if url.starts_with(WWW_URL) {
		Request::get(url.as_str())?.header("User-Agent", UA)
	} else {
		Request::get(url.as_str())?
			.header("User-Agent", "COPY/3.0.0")
			.header("version", API_VERSION)
			.header("platform", "3")
			.header("region", "1")
			.header("webp", "1")
	};

	with_auth(request).json_owned::<Value>()
}

/// 章节列表接口要求 JSON 内容协商。
pub fn get_chapter_list_json(url: String) -> Result<Value> {
	let dnts = String::from("9");
	let request = Request::get(url.as_str())?
		.header("User-Agent", UA)
		.header("Accept", "application/json")
		.header("dnts", dnts.as_str());

	with_auth(request).json_owned::<Value>()
}

/// 提取页面内联脚本中第一个 `var x = '...'` 的单引号值。
///
/// 站点会轮换变量名（ccz/cct/...），因此不能按变量名匹配。
pub fn script_var(text: &str) -> Option<String> {
	let mut rest = text;

	while let Some(start) = rest.find("<script") {
		let block_start = start + rest[start..].find('>')? + 1;
		let block = &rest[block_start..];
		let end = block.find("</script>")?;
		let block = &block[..end];

		if block.contains("var ") && block.contains('\'') {
			let mut parts = block.split('\'');
			parts.next();
			if let Some(value) = parts.next() {
				if !value.is_empty() {
					return Some(value.to_string());
				}
			}
		}

		rest = &rest[block_start + end..];
	}

	None
}

/// 取标记之后第一个单引号字符串的值。
pub fn quoted_after(text: &str, marker: &str) -> Option<String> {
	let start = text.find(marker)? + marker.len();
	let rest = &text[start..];
	let open = rest.find('\'')?;
	let after = &rest[open + 1..];
	let end = after.find('\'')?;

	Some(after[..end].to_string())
}

fn base64_encode(input: &[u8]) -> String {
	const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

	let mut output = String::new();

	for chunk in input.chunks(3) {
		let first = chunk[0] as u32;
		let second = chunk.get(1).copied().unwrap_or_default() as u32;
		let third = chunk.get(2).copied().unwrap_or_default() as u32;
		let bits = (first << 16) | (second << 8) | third;

		output.push(TABLE[(bits >> 18) as usize & 0x3f] as char);
		output.push(TABLE[(bits >> 12) as usize & 0x3f] as char);
		output.push(if chunk.len() > 1 {
			TABLE[(bits >> 6) as usize & 0x3f] as char
		} else {
			'='
		});
		output.push(if chunk.len() > 2 {
			TABLE[bits as usize & 0x3f] as char
		} else {
			'='
		});
	}

	output
}

/// 用账号密码登录站点并保存 token。
pub fn login(username: &str, password: &str) -> Result<()> {
	let salt = current_date().rem_euclid(900_000) + 100_000;
	let encoded = base64_encode(format!("{password}-{salt}").as_bytes());
	let body = format!(
		"username={}&password={}&salt={salt}&platform=2&version=1.0.0&source=copyweb",
		encode_uri(username),
		encode_uri(&encoded),
	);
	let url = format!("{WWW_URL}/api/v1/login");

	let json = Request::post(url.as_str())?
		.header(
			"Content-Type",
			"application/x-www-form-urlencoded;charset=UTF-8",
		)
		.header("X-Requested-With", "XMLHttpRequest")
		.header("Accept", "application/json")
		.body(body)
		.json_owned::<Value>()?;

	let code = json.get("code").and_then(Value::as_i64).unwrap_or_default();
	if code != 200 {
		let message = json
			.get("message")
			.and_then(Value::as_str)
			.unwrap_or("未知錯誤");
		bail!("登入失敗：{message}");
	}

	let token = json
		.get("results")
		.and_then(|results| results.get("token"))
		.and_then(Value::as_str)
		.ok_or(error!("登入回應缺少 token"))?;

	defaults_set(TOKEN_KEY, DefaultValue::String(token.to_string()));

	Ok(())
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
		"{}/ranks?date_type={}&audience_type={}&limit={}&offset={}",
		API_URL,
		date_type,
		"male",
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
