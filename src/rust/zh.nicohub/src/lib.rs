#![no_std]

use aidoku::{
	alloc::{String, Vec},
	helpers::uri::encode_uri_component,
	imports::net::Request,
	prelude::*,
	Chapter, FilterValue, Manga, MangaPageResult, Page, Result, Source,
};
use serde_json::Value;

mod parser;

struct NicoHub;

impl NicoHub {
	fn post_json(url: String, body: String) -> Result<Value> {
		let request = Request::post(url)?
			.body(body.as_bytes())
			.header("Content-Type", "application/x-www-form-urlencoded");
		request.json_owned::<Value>()
	}
}

impl Source for NicoHub {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut category = String::new();
		let mut region = String::new();
		let mut audience = String::new();
		let mut status = String::new();

		for filter in filters {
			if let FilterValue::Select { id, value } = filter {
				match id.as_str() {
					"category" => category = value,
					"region" => region = value,
					"audience" => audience = value,
					"status" => status = value,
					_ => continue,
				}
			}
		}

		let query = query.unwrap_or_default();
		let query = if query.is_empty() {
			format!("board:漫画 {} {} {} {}", category, region, audience, status)
		} else {
			format!("board:漫画 {}", query)
		};
		let url = format!("{}/api/search/", parser::WWW_URL);
		let body = format!("query={}&page={}", encode_uri_component(query), page);
		let json = NicoHub::post_json(url, body)?;
		let list: Vec<Value> = json
			.get("data")
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();
		let total_page = json
			.get("page")
			.and_then(|page| page.get("total_page"))
			.and_then(Value::as_i64)
			.unwrap_or(0) as i32;

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(&list),
			has_next_page: page < total_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let url = format!("{}/api/get/info/", parser::WWW_URL);
			let body = format!("category=comic&id={}", manga.key);
			let json = NicoHub::post_json(url, body)?;
			if let Some(data) = json.get("data") {
				manga.copy_from(parser::parse_manga(data));
			}
		}

		if needs_chapters {
			let url = format!("{}/api/get/episode/page/", parser::WWW_URL);
			let mut page = 1;
			let mut chapters: Vec<Chapter> = Vec::new();

			loop {
				let body = format!(
					"category=comic&id={}&page={}&limit=20",
					manga.key.clone(),
					page
				);
				let json = NicoHub::post_json(url.clone(), body)?;
				let list: Vec<Value> = json
					.get("data")
					.and_then(Value::as_array)
					.cloned()
					.unwrap_or_default();
				let total_page = json
					.get("page")
					.and_then(|page| page.get("total_page"))
					.and_then(Value::as_i64)
					.unwrap_or(0) as i32;
				chapters.extend(parser::parse_chapter_list(&list));

				if page >= total_page {
					break;
				}

				page += 1;
			}
			chapters.reverse();

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{}/api/get/episode/", parser::WWW_URL);
		let body = format!("category=comic&post_id={}&id={}", manga.key, chapter.key);
		let json = NicoHub::post_json(url, body)?;
		let list: Vec<Value> = json
			.get("data")
			.and_then(|data| data.get("param"))
			.and_then(|param| param.get("img"))
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();

		Ok(parser::parse_page_list(&list))
	}
}

register_source!(NicoHub);
