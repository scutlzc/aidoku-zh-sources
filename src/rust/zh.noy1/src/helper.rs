use aidoku::{
	alloc::{string::ToString, String},
	imports::{
		defaults::{defaults_get, defaults_set, DefaultValue},
		net::{HttpMethod, Request},
	},
	prelude::*,
	Result,
};
use serde_json::Value;

pub const WWW_URL: &str = "https://noy1.top";
const PIC_URL: &str = "https://img.noy.asia";

pub const FILTER_SORT: [&str; 3] = ["bid", "views", "favorites"];

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

fn gen_request(url: String, method: HttpMethod) -> Result<(Request, String)> {
	let session = defaults_get::<String>("session").unwrap_or_default();
	let session = if !url.contains("login") && session.is_empty() {
		login()?
	} else {
		session
	};
	let cookie = format!("NOY_SESSION={}", session);
	let request = Request::new(url.as_str(), method)?
		.header("Content-Type", "application/x-www-form-urlencoded")
		.header("Cookie", cookie.as_str());

	Ok((request, session))
}

pub fn login() -> Result<String> {
	let url = format!("{}/api/login", WWW_URL);
	let (request, _) = gen_request(url, HttpMethod::Post)?;
	let username = defaults_get::<String>("username").unwrap_or_default();
	let password = defaults_get::<String>("password").unwrap_or_default();

	if username.is_empty() || password.is_empty() {
		bail!("missing username or password");
	}

	let body = format!("user={}&pass={}", username, password);
	let response = request.body(body.as_bytes()).send()?;

	if response.status_code() != 200 {
		bail!("login failed");
	}

	let cookie_header = response.get_header("set-cookie").unwrap_or_default();
	let session = substring_after(cookie_header.as_str(), "NOY_SESSION=")
		.and_then(|value| substring_before(value, ";"))
		.unwrap_or_default()
		.to_string();

	defaults_set("session", DefaultValue::String(session.clone()));

	Ok(session)
}

pub fn get_json(url: String, body: String) -> Result<Value> {
	let (request, _) = gen_request(url.clone(), HttpMethod::Post)?;
	let response = request.body(body.as_bytes()).send()?;
	let status = response.status_code();

	if status == 401 {
		let session = login()?;
		let cookie = format!("NOY_SESSION={}", session);

		return Request::new(url.as_str(), HttpMethod::Post)?
			.header("Content-Type", "application/x-www-form-urlencoded")
			.header("Cookie", cookie.as_str())
			.body(body.as_bytes())
			.json_owned::<Value>();
	}

	let json = response.get_json_owned::<Value>()?;
	let list = json.as_array().cloned().unwrap_or_default();
	let is_login = list
		.first()
		.and_then(Value::as_str)
		.map(|value| value == "login")
		.unwrap_or(false);

	if is_login {
		let session = login()?;
		let cookie = format!("NOY_SESSION={}", session);

		return Request::new(url.as_str(), HttpMethod::Post)?
			.header("Content-Type", "application/x-www-form-urlencoded")
			.header("Cookie", cookie.as_str())
			.body(body.as_bytes())
			.json_owned::<Value>();
	}

	Ok(json)
}

pub fn explore(tag: &str, sort: &str, page: i32) -> Result<Value> {
	let url = if tag.is_empty() {
		format!("{}/api/booklist_v2", WWW_URL)
	} else {
		format!("{}/api/search_v2", WWW_URL)
	};
	let body = if tag.is_empty() {
		format!("page={}", page)
	} else {
		format!("info={}&type=tag&sort={}&page={}", tag, sort, page)
	};

	get_json(url, body)
}

pub fn search(keyword: &str, page: i32) -> Result<Value> {
	let url = format!("{}/api/search_v2", WWW_URL);
	let body = format!("info={}&type=de&sort=bid&page={}", keyword, page);

	get_json(url, body)
}

pub fn rank(name: &str, level: &str, page: i32) -> Result<Value> {
	let url = format!("{}/api/{}", WWW_URL, name);
	let body = if !level.is_empty() {
		format!("page={}&type={}", page, level)
	} else {
		format!("page={}", page)
	};

	get_json(url, body)
}

pub fn details(manga_id: &str) -> Result<Value> {
	let url = format!("{}/api/getbookinfo", WWW_URL);
	let body = format!("bid={}", manga_id);

	get_json(url, body)
}

pub fn gen_cover_url(manga_id: &str) -> String {
	format!("{}/{}/m1.webp", PIC_URL, manga_id)
}

pub fn gen_manga_url(manga_id: &str) -> String {
	format!("{}/#/book/{}", WWW_URL, manga_id)
}

pub fn gen_chapter_url(manga_id: &str) -> String {
	format!("{}/#/read/{}", WWW_URL, manga_id)
}

pub fn gen_page_url(manga_id: &str, page: i32) -> String {
	format!("{}/{}/{}.webp", PIC_URL, manga_id, page)
}
