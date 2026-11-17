#![no_std]

use aidoku::{
	alloc::{string::String, vec::Vec},
	imports::net::Request,
	prelude::*,
	Chapter, FilterValue, ImageRequestProvider, Manga, MangaPageResult, Page, PageContext, Result,
	Source,
};
use serde_json::Value;

mod helper;
mod parser;

const FILTER_SORT: [&str; 3] = ["0", "1", "2"];

struct Manhuaren;

impl Source for Manhuaren {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut category = String::from("0");
		let mut status = String::from("0");
		let mut sort = String::from("0");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "category" {
						category = value;
					} else if id == "status" {
						status = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					sort = String::from(FILTER_SORT.get(index as usize).copied().unwrap_or("0"));
				}
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let is_search = !query.is_empty();
		let url = if is_search {
			helper::gen_search_url(query, page)
		} else {
			helper::gen_explore_url(category, status, sort, page)
		};

		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("response").unwrap_or(&empty);

		if is_search {
			let list = parser::parse_manga_list(
				data.get("result")
					.and_then(|list| list.as_array())
					.map(|list| list.as_slice())
					.unwrap_or_default(),
			);
			let total = data
				.get("total")
				.and_then(|total| total.as_i64())
				.unwrap_or_default() as i32;

			Ok(MangaPageResult {
				entries: list,
				has_next_page: page * 20 < total,
			})
		} else {
			let list = parser::parse_manga_list(
				data.get("mangas")
					.and_then(|list| list.as_array())
					.map(|list| list.as_slice())
					.unwrap_or_default(),
			);

			Ok(MangaPageResult {
				entries: list,
				has_next_page: true,
			})
		}
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}

		let url = helper::gen_manga_details_url(manga.key.clone());
		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("response").unwrap_or(&empty);

		if needs_details {
			manga.copy_from(parser::parse_manga(data));
		}

		if needs_chapters {
			manga.chapters = Some(parser::parse_chapter_list(data));
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = helper::gen_page_list_url(manga.key, chapter.key);
		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("response").unwrap_or(&empty);

		Ok(parser::parse_page_list(data))
	}
}

impl ImageRequestProvider for Manhuaren {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url.as_str())?
			.header("X-Yq-Yqci", r#"{"le": "zh"}"#)
			.header("User-Agent", "okhttp/3.11.0")
			.header("Referer", "http://www.dm5.com/dm5api/")
			.header("ClubReferer", "http://mangaapi.manhuaren.com/"))
	}
}

register_source!(Manhuaren, ImageRequestProvider);
