#![no_std]

use aidoku::{
	alloc::{string::String, vec::Vec},
	prelude::*,
	Chapter, FilterValue, Listing, ListingProvider, Manga, MangaPageResult, Page, Result, Source,
};
use serde_json::Value;

mod crypto;
mod helper;
mod parser;

const FILTER_SORT: [&str; 4] = ["dd", "da", "ld", "vd"];

struct Manhuabika;

impl Source for Manhuabika {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut query = query.unwrap_or_default();
		let mut category = String::new();
		let mut sort = String::from("dd");

		for filter in filters {
			match filter {
				FilterValue::Text { value, .. } => {
					query = value;
				}
				FilterValue::Select { id, value } => {
					if id == "category" {
						category = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					sort = String::from(FILTER_SORT.get(index as usize).copied().unwrap_or("dd"));
				}
				_ => continue,
			}
		}

		let json = if query.is_empty() {
			helper::get_json(helper::gen_explore_url(category, sort, page))?
		} else {
			helper::search(query, page)?
		};

		let empty = Value::Null;
		let data = json.get("data").unwrap_or(&empty);
		let data = data.get("comics").unwrap_or(&empty);

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(&jarray(data, "docs")),
			has_next_page: parser::has_more(data),
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let url = helper::gen_manga_details_url(manga.key.clone());
			let json = helper::get_json(url)?;
			let empty = Value::Null;
			let data = json.get("data").unwrap_or(&empty);

			manga.copy_from(parser::parse_manga(data));
		}

		if needs_chapters {
			let mut page = 1;
			let url = helper::gen_chapter_list_url(manga.key.clone(), page);
			let json = helper::get_json(url)?;
			let empty = Value::Null;
			let data = json.get("data").unwrap_or(&empty);
			let data = data.get("eps").unwrap_or(&empty);
			let mut chapters = parser::parse_chapter_list(manga.key.clone(), &jarray(data, "docs"));
			let pages = jint(data, "pages").unwrap_or_default() as i32;

			while page < pages {
				page += 1;
				let next = get_chapter_list_by_page(manga.key.clone(), page)?;
				chapters.extend(next);
			}

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let mut page = 1;
		let url = helper::gen_page_list_url(manga.key.clone(), chapter.key.clone(), page);
		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("data").unwrap_or(&empty);
		let data = data.get("pages").unwrap_or(&empty);
		let mut pages = parser::parse_page_list(&jarray(data, "docs"));
		let total_pages = jint(data, "pages").unwrap_or_default() as i32;

		while page < total_pages {
			page += 1;
			let next = get_page_list_by_page(manga.key.clone(), chapter.key.clone(), page)?;
			pages.extend(next);
		}

		Ok(pages)
	}
}

impl ListingProvider for Manhuabika {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let mut rank_time = String::new();
		let mut is_random = false;
		let mut category = String::new();
		let sort = String::from("dd");

		match listing.id.as_str() {
			"day" => rank_time = String::from("H24"),
			"week" => rank_time = String::from("D7"),
			"month" => rank_time = String::from("D30"),
			"random" => is_random = true,
			"dashi" => category = String::from("大濕推薦"),
			"today" => category = String::from("那年今天"),
			"everyone" => category = String::from("大家都在看"),
			"official" => category = String::from("官方都在看"),
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		}

		let url = if !rank_time.is_empty() {
			helper::gen_rank_url(rank_time.clone())
		} else if is_random {
			helper::gen_random_url()
		} else if !category.is_empty() {
			helper::gen_explore_url(category, sort, page)
		} else {
			String::new()
		};

		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("data").unwrap_or(&empty);

		if !rank_time.is_empty() || is_random {
			Ok(MangaPageResult {
				entries: parser::parse_manga_list(&jarray(data, "comics")),
				has_next_page: false,
			})
		} else {
			let data = data.get("comics").unwrap_or(&empty);

			Ok(MangaPageResult {
				entries: parser::parse_manga_list(&jarray(data, "docs")),
				has_next_page: parser::has_more(data),
			})
		}
	}
}

fn jarray(value: &Value, key: &str) -> Vec<Value> {
	value
		.get(key)
		.and_then(|value| value.as_array())
		.map(|list| list.to_vec())
		.unwrap_or_default()
}

fn jint(value: &Value, key: &str) -> Option<i64> {
	value.get(key).and_then(|value| {
		value
			.as_i64()
			.or_else(|| value.as_f64().map(|value| value as i64))
	})
}

fn get_chapter_list_by_page(id: String, page: i32) -> Result<Vec<Chapter>> {
	let url = helper::gen_chapter_list_url(id.clone(), page);
	let json = helper::get_json(url)?;
	let empty = Value::Null;
	let data = json.get("data").unwrap_or(&empty);
	let data = data.get("eps").unwrap_or(&empty);

	Ok(parser::parse_chapter_list(id, &jarray(data, "docs")))
}

fn get_page_list_by_page(manga_id: String, chapter_id: String, page: i32) -> Result<Vec<Page>> {
	let url = helper::gen_page_list_url(manga_id, chapter_id, page);
	let json = helper::get_json(url)?;
	let empty = Value::Null;
	let data = json.get("data").unwrap_or(&empty);
	let data = data.get("pages").unwrap_or(&empty);

	Ok(parser::parse_page_list(&jarray(data, "docs")))
}

register_source!(Manhuabika, ListingProvider);
