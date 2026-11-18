use aidoku::{
	alloc::{string::String, string::ToString, vec::Vec},
	imports::{
		defaults::{defaults_get, defaults_set, DefaultValue},
		html::{Document, Element},
		net::Request,
	},
	prelude::*,
	Result,
};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

const DEFAULT_URL: &str = "https://www.zerobyw33.com";
const LEGACY_URLS: [&str; 1] = ["zerobywav.com"];

const COOKIE_ATTRIBUTES: [&str; 10] = [
	"path", "expires", "max-age", "domain", "secure", "httponly", "samesite", "version", "comment",
	"priority",
];

fn handle_cookie_header(cookie_header: String) -> String {
	cookie_header
		.replace(',', ";")
		.split(';')
		.map(|a| a.trim())
		.filter(|a| match a.split_once('=') {
			Some((name, _)) => {
				let name = name.trim();

				!name.is_empty()
					&& !COOKIE_ATTRIBUTES
						.iter()
						.any(|attribute| name.eq_ignore_ascii_case(attribute))
			}
			None => false,
		})
		.collect::<Vec<&str>>()
		.join("; ")
}

fn get_default(key: &str) -> Option<String> {
	defaults_get::<String>(key)
}

pub fn get_url() -> String {
	let url = get_default("url").unwrap_or_default();
	let url = url.trim().trim_end_matches('/').to_string();

	// the old default domain is gone, fall back to the current one
	if url.is_empty() || LEGACY_URLS.iter().any(|host| url.contains(host)) {
		return DEFAULT_URL.to_string();
	}

	url
}

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

fn requires_login(html: &Document) -> bool {
	// restricted chapters redirect to the discuz login page
	if html.select_first("form[name=login]").is_some() {
		return true;
	}

	let limited = html
		.select_first("#main_message #messagetext>p")
		.and_then(|element| element.text())
		.map(|text| text.contains("仅限") || text.contains("请先登录"))
		.unwrap_or(false);

	if limited {
		return true;
	}

	html.select_first(".uk-alert-danger>p")
		.and_then(|element| element.text())
		.map(|text| text.contains("仅限"))
		.unwrap_or(false)
}

fn get_formhash(html: Document) -> String {
	let formhash = html
		.select_first("input[name=formhash]")
		.and_then(|element| element.attr("value"))
		.unwrap_or_default();

	if !formhash.is_empty() {
		return formhash;
	}

	let text = Element::from(html).html().unwrap_or_default();

	substring_after(&text, "formhash:'")
		.and_then(|value| substring_before(value, "'"))
		.unwrap_or_default()
		.to_string()
}

pub fn get_html(url: String) -> Result<Document> {
	fetch(url, true)
}

fn fetch(url: String, allow_login: bool) -> Result<Document> {
	let default_cookie = get_default("cookie").unwrap_or_default();
	let response = Request::get(url.as_str())?
		.header("User-Agent", UA)
		.header("Cookie", default_cookie.as_str())
		.send()?;

	let cookie_header = response.get_header("set-cookie").unwrap_or_default();
	let html = response.get_html()?;

	// only try to log in once, restricted content stays restricted afterwards
	if !allow_login || !requires_login(&html) {
		return Ok(html);
	}

	let username = get_default("username").unwrap_or_default();
	let password = get_default("password").unwrap_or_default();

	if username.is_empty() || password.is_empty() {
		bail!("missing username or password");
	}

	let formhash = get_formhash(html);
	let login_cookie = handle_cookie_header(cookie_header);
	let body = format!(
		"username={}&cookietime=2592000&password={}&formhash={}&quickforward=yes&handlekey=ls",
		username, password, formhash
	);
	let login_url = format!(
		"{}/member.php?mod=logging&action=login&loginsubmit=yes&infloat=yes&lssubmit=yes&inajax=1",
		get_url()
	);

	let login_response = Request::post(login_url.as_str())?
		.header("User-Agent", UA)
		.header("Content-Type", "application/x-www-form-urlencoded")
		.header("Cookie", login_cookie.as_str())
		.body(body.as_bytes())
		.send()?;

	let new_cookie_header = login_response.get_header("set-cookie").unwrap_or_default();

	if !new_cookie_header.contains("auth") {
		bail!("login failed");
	}

	let new_cookie = handle_cookie_header(new_cookie_header);

	defaults_set("cookie", DefaultValue::String(new_cookie));

	fetch(url, false)
}
