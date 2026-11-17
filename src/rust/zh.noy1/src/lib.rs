#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	imports::net::Request,
	prelude::*,
	Chapter, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult,
	Page, PageContent, PageContext, Result, Source,
};
use serde_json::Value;

mod helper;
mod parser;

struct Noy1;

impl ListingProvider for Noy1 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let mut name = String::new();
		let mut level = String::new();

		match listing.id.as_str() {
			"day-read" => {
				name.push_str("readLeaderboard");
				level.push_str("day");
			}
			"week-read" => {
				name.push_str("readLeaderboard");
				level.push_str("week");
			}
			"month-read" => {
				name.push_str("readLeaderboard");
				level.push_str("moon");
			}
			"day-fav" => {
				name.push_str("favLeaderboard");
				level.push_str("day");
			}
			"week-fav" => {
				name.push_str("favLeaderboard");
				level.push_str("week");
			}
			"month-fav" => {
				name.push_str("favLeaderboard");
				level.push_str("moon");
			}
			"proportion" => {
				name.push_str("proportion");
			}
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		}

		let json = helper::rank(&name, &level, page)?;
		let list: Vec<Value> = json
			.get("info")
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();
		let total = json.get("len").and_then(Value::as_i64).unwrap_or(0) as i32;
		let has_next_page = page * 20 < total;

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(&list),
			has_next_page,
		})
	}
}

impl Source for Noy1 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let query = query.unwrap_or_default();
		let mut tag = String::new();
		let mut sort = String::from("bid");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } if id == "tag" => {
					tag = value;
				}
				FilterValue::Sort { index, .. } => {
					if let Some(value) = helper::FILTER_SORT.get(index.max(0) as usize) {
						sort = value.to_string();
					}
				}
				_ => continue,
			}
		}

		let json = if query.is_empty() {
			helper::explore(&tag, &sort, page)?
		} else {
			helper::search(&query, page)?
		};

		let key = if query.is_empty() && tag.is_empty() {
			"info"
		} else {
			"Info"
		};
		let list: Vec<Value> = json
			.get(key)
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();
		let total = json.get("len").and_then(Value::as_i64).unwrap_or(0) as i32;
		let has_next_page = page * 20 < total;

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(&list),
			has_next_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let json = helper::details(&manga.key)?;
			manga.copy_from(parser::parse_manga(&json));
		}

		if needs_chapters {
			let url = helper::gen_chapter_url(&manga.key);
			let mut chapters: Vec<Chapter> = Vec::new();
			chapters.push(Chapter {
				key: manga.key.clone(),
				title: Some(String::from("第 1 话")),
				chapter_number: Some(1 as f32),
				url: Some(url),
				..Default::default()
			});

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, _chapter: Chapter) -> Result<Vec<Page>> {
		let json = helper::details(&manga.key)?;
		let len = json.get("Len").and_then(Value::as_i64).unwrap_or(0) as i32;
		let mut pages: Vec<Page> = Vec::new();
		let mut index = 0;

		while index < len {
			let url = helper::gen_page_url(&manga.key, index + 1);
			pages.push(Page {
				content: PageContent::url(url),
				..Default::default()
			});
			index += 1;
		}

		Ok(pages)
	}
}

impl ImageRequestProvider for Noy1 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", helper::WWW_URL))
	}
}

register_source!(Noy1, ListingProvider, ImageRequestProvider);
