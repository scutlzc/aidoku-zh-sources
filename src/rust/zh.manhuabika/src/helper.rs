use aidoku::{
	alloc::{format, string::String, string::ToString},
	helpers::uri::encode_uri,
	imports::{
		defaults::{defaults_get, defaults_set, DefaultValue},
		net::{Request, Response},
	},
	prelude::*,
	Result,
};
use md5::compute;
use serde_json::Value;

use crate::crypto;

const KEY: &[u8; 63] = br"~d}$Q7$eIni=V)9\RK/P.RM4;9[7|@/CA}b~OW!3?EV`:<>M7pddUBL5n|0/*Cn";
const API_KEY: &str = "C69BAF41DA5ABD1FFEDC6D2FEA56B";

const WWW_URL: &str = "https://manhuabika.com";
const API_URL: &str = "https://picaapi.picacomic.com";

#[derive(Clone, Copy)]
pub enum Method {
	Get,
	Post,
}

impl Method {
	fn as_str(&self) -> &'static str {
		match self {
			Method::Get => "Get",
			Method::Post => "Post",
		}
	}
}

pub fn substring_after(text: &str, pattern: &str) -> Option<String> {
	text.find(pattern)
		.map(|index| text[index + pattern.len()..].to_string())
}

pub fn gen_time() -> String {
	(aidoku::imports::std::current_date() as i64).to_string()
}

pub fn gen_nonce() -> String {
	format!("{:x}", compute(gen_time()))
}

pub fn gen_signature(url: &str, time: &str, nonce: &str, method: &str) -> String {
	let url = substring_after(url, &format!("{}/", API_URL)).unwrap_or_default();
	let text = format!("{}{}{}{}{}", url, time, nonce, method, API_KEY).to_ascii_lowercase();
	crypto::encrypt(text.as_bytes(), KEY)
}

fn base_request(url: String, method: Method, authorization: String) -> Result<Request> {
	let time = gen_time();
	let nonce = gen_nonce();
	let signature = gen_signature(&url, &time, &nonce, method.as_str());
	let request = match method {
		Method::Get => Request::get(url.as_str())?,
		Method::Post => Request::post(url.as_str())?,
	};

	Ok(request
		.header("api-key", API_KEY)
		.header("app-build-version", "45")
		.header("app-channel", "1")
		.header("app-platform", "android")
		.header("app-uuid", "defaultUuid")
		.header("app-version", "2.2.1.3.3.4")
		.header("image-quality", "original")
		.header("time", time.as_str())
		.header("nonce", nonce.as_str())
		.header("signature", signature.as_str())
		.header("Accept", "application/vnd.picacomic.com.v1+json")
		.header("Authorization", authorization.as_str())
		.header("Content-Type", "application/json; charset=UTF-8")
		.header("User-Agent", "okhttp/3.8.1"))
}

pub fn get_token() -> String {
	defaults_get::<String>("token").unwrap_or_default()
}

pub fn gen_request(url: String) -> Result<Request> {
	let token = get_token();
	let authorization = if !url.contains("sign-in") && token.is_empty() {
		login()?
	} else {
		token
	};

	base_request(url, Method::Get, authorization)
}

pub fn login() -> Result<String> {
	let username = defaults_get::<String>("username").unwrap_or_default();
	let password = defaults_get::<String>("password").unwrap_or_default();

	if username.is_empty() || password.is_empty() {
		bail!("Username or password is empty");
	}

	let body = format!(
		r#"{{
			"email": "{}",
			"password": "{}"
		}}"#,
		username, password
	);
	let request = base_request(gen_login_url(), Method::Post, String::new())?;
	let response = request.body(body.as_bytes()).send()?;

	if response.status_code() != 200 {
		bail!("Failed to login");
	}

	let json: Value = response.get_json_owned::<Value>()?;
	let token = json
		.get("data")
		.and_then(|data| data.get("token"))
		.and_then(|token| token.as_str())
		.unwrap_or_default()
		.to_string();

	defaults_set("token", DefaultValue::String(token.clone()));

	Ok(token)
}

fn json_from_response(response: Response) -> Result<Value> {
	response.get_json_owned::<Value>()
}

fn send_with_retry(
	request: Request,
	body: Option<String>,
	url: String,
	method: Method,
) -> Result<Value> {
	let request = match body {
		Some(ref body) => request.body(body.as_bytes()),
		None => request,
	};
	let response = request.send()?;

	if response.status_code() == 401 {
		let token = login()?;
		let request = base_request(url, method, token)?;
		let request = match body {
			Some(ref body) => request.body(body.as_bytes()),
			None => request,
		};

		return json_from_response(request.send()?);
	}

	json_from_response(response)
}

pub fn search(keyword: String, page: i32) -> Result<Value> {
	let url = gen_search_url(page);
	let body = format!(
		r#"{{
			"keyword": "{}",
			"sort": "dd"
		}}"#,
		keyword,
	);
	let request = gen_request(url.clone())?;

	send_with_retry(request, Some(body), url, Method::Post)
}

pub fn gen_login_url() -> String {
	format!("{}/{}", API_URL, "auth/sign-in")
}

pub fn gen_explore_url(category: String, sort: String, page: i32) -> String {
	if category.is_empty() {
		format!("{}/comics?page={}&s={}", API_URL, page, sort,)
	} else {
		format!(
			"{}/comics?page={}&c={}&s={}",
			API_URL,
			page,
			encode_uri(category),
			sort,
		)
	}
}

pub fn gen_rank_url(time: String) -> String {
	format!("{}/comics/leaderboard?tt={}&ct=VC", API_URL, time)
}

pub fn gen_random_url() -> String {
	format!("{}/comics/random", API_URL)
}

pub fn gen_search_url(page: i32) -> String {
	format!("{}/comics/advanced-search?page={}&s=dd", API_URL, page)
}

pub fn gen_manga_url(id: String) -> String {
	format!("{}/pcomicview/?cid={}", WWW_URL, id)
}

pub fn gen_manga_details_url(id: String) -> String {
	format!("{}/comics/{}", API_URL, id)
}

pub fn gen_chapter_list_url(id: String, page: i32) -> String {
	format!("{}/comics/{}/eps?page={}", API_URL, id, page)
}

pub fn gen_chapter_url(manga_id: String, chapter_id: String) -> String {
	format!(
		"{}/pchapter/?cid={}&chapter={}",
		WWW_URL, manga_id, chapter_id
	)
}

pub fn gen_page_list_url(manga_id: String, chapter_id: String, page: i32) -> String {
	format!(
		"{}/comics/{}/order/{}/pages?page={}",
		API_URL, manga_id, chapter_id, page
	)
}

pub fn get_json(url: String) -> Result<Value> {
	let request = gen_request(url.clone())?;

	send_with_retry(request, None, url, Method::Get)
}
