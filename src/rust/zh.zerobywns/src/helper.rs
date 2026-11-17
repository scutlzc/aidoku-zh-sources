use aidoku::{
	alloc::{string::String, vec::Vec},
	imports::{
		defaults::{defaults_get, defaults_set, DefaultValue},
		html::Document,
		net::Request,
	},
	prelude::*,
	Result,
};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

fn handle_cookie_header(cookie_header: String) -> String {
	cookie_header
		.replace(",", ";")
		.split(';')
		.filter(|a| a.contains("Ckng"))
		.map(|a| a.trim())
		.collect::<Vec<&str>>()
		.join(";")
}

fn get_default(key: &str) -> Option<String> {
	defaults_get::<String>(key)
}

pub fn get_url() -> String {
	get_default("url").unwrap_or_default()
}

pub fn get_html(url: String) -> Result<Document> {
	let default_cookie = get_default("cookie").unwrap_or_default();
	let response = Request::get(url.as_str())?
		.header("User-Agent", UA)
		.header("Cookie", default_cookie.as_str())
		.send()?;

	let cookie_header = response.get_header("set-cookie").unwrap_or_default();
	let html = response.get_html()?;

	let need_login = html
		.select_first("#main_message #messagetext>p")
		.and_then(|element| element.text())
		.unwrap_or_default()
		.contains("仅限用户观看，请先登录");

	if !need_login {
		return Ok(html);
	}

	let username = get_default("username").unwrap_or_default();
	let password = get_default("password").unwrap_or_default();

	if username.is_empty() || password.is_empty() {
		bail!("missing username or password");
	}

	let formhash = html
		.select_first("input[name=formhash]")
		.and_then(|element| element.attr("value"))
		.unwrap_or_default();
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

	get_html(url)
}
