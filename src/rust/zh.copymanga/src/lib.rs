#![no_std]

use aidoku::{
	alloc::{string::String, vec::Vec},
	prelude::*,
	BasicLoginHandler, Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, Result, Source, Viewer,
};
use serde_json::Value;

mod crypto;
mod helper;
mod parser;

const FILTER_ORDERING: [&str; 2] = ["popular", "datetime_updated"];

struct Copymanga;

impl Source for Copymanga {
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
		let mut theme = String::new();
		let mut top = String::new();
		let mut ordering = String::new();

		for filter in filters {
			match filter {
				FilterValue::Text { value, .. } => {
					query = value;
				}
				FilterValue::Select { id, value } => {
					if id == "theme" {
						theme = value;
					} else if id == "region" {
						top = value;
					}
				}
				FilterValue::Sort {
					index, ascending, ..
				} => {
					ordering.push_str(if ascending { "" } else { "-" });
					ordering.push_str(
						FILTER_ORDERING
							.get(index as usize)
							.copied()
							.unwrap_or_default(),
					);
				}
				_ => continue,
			}
		}

		let url = if query.is_empty() {
			helper::gen_explore_url(theme, top, ordering, page)
		} else {
			helper::gen_search_url(query, page)
		};

		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("results").unwrap_or(&empty);

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(data.get("list")),
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
			let url = helper::gen_manga_url(manga.key.clone());
			let html = helper::get_html(url.clone())?;
			let cover = html
				.select_first(".comicParticulars-left-img>img")
				.and_then(|element| element.attr("data-src"));
			let title = html
				.select_first("h6")
				.and_then(|element| element.text())
				.unwrap_or_default();
			let mut authors: Vec<String> = Vec::new();
			if let Some(list) = html.select(".comicParticulars-right-txt>a") {
				for element in list {
					if let Some(text) = element.text() {
						authors.push(text);
					}
				}
			}
			let description = html
				.select_first(".intro")
				.and_then(|element| element.text())
				.unwrap_or_default();
			let mut tags: Vec<String> = Vec::new();
			if let Some(list) = html.select(".comicParticulars-tag>a") {
				for element in list {
					if let Some(text) = element.text() {
						tags.push(text.replace('#', ""));
					}
				}
			}
			let full_title = html
				.select_first("title")
				.and_then(|element| element.text())
				.unwrap_or_default();
			let status = if full_title.contains("連載中") {
				MangaStatus::Ongoing
			} else if full_title.contains("已完結") {
				MangaStatus::Completed
			} else {
				MangaStatus::Unknown
			};

			manga.cover = cover.filter(|cover| !cover.is_empty());
			manga.title = title;
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = if description.is_empty() {
				None
			} else {
				Some(description)
			};
			manga.url = Some(url);
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.status = status;
			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::RightToLeft;
		}

		if needs_chapters {
			let manga_url = helper::gen_manga_url(manga.key.clone());
			let text = helper::get_text(manga_url)?;
			let key = helper::script_var(&text)
				.ok_or(error!("找不到章節解密金鑰，站點頁面結構可能已變更"))?;
			let url = helper::gen_chapter_list_url(manga.key.clone());
			let json = helper::get_chapter_list_json(url)?;
			let data = json
				.get("results")
				.and_then(|data| data.as_str())
				.unwrap_or_default();
			let data = helper::decrypt(String::from(data), key)?;
			let data: Value =
				serde_json::from_str(&data).map_err(|_| error!("Failed to parse chapter list"))?;

			manga.chapters = Some(parser::parse_chapter_list(&data));
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = helper::gen_page_list_url(manga.key, chapter.key);
		let text = helper::get_text(url)?;
		let key = helper::script_var(&text)
			.ok_or(error!("找不到章節解密金鑰，站點頁面結構可能已變更"))?;
		let data = helper::quoted_after(&text, "var contentKey")
			.ok_or(error!("找不到圖片資料，站點頁面結構可能已變更"))?;
		if data.is_empty() {
			bail!("站點目前要求登入後才能閱讀，請在圖源設定中登入拷貝漫畫帳號");
		}
		let data = helper::decrypt(data, key)?;
		let data: Value =
			serde_json::from_str(&data).map_err(|_| error!("Failed to parse page list"))?;

		Ok(parser::parse_page_list(&data))
	}
}

impl ListingProvider for Copymanga {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let mut date_type = String::new();
		let mut is_recs = false;
		let mut is_newest = false;

		match listing.id.as_str() {
			"day" => date_type.push_str("day"),
			"week" => date_type.push_str("week"),
			"month" => date_type.push_str("month"),
			"total" => date_type.push_str("total"),
			"recs" => is_recs = true,
			"newest" => is_newest = true,
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		}

		let url = if !date_type.is_empty() {
			helper::gen_rank_url(date_type, page)
		} else if is_recs {
			helper::gen_recs_url(page)
		} else if is_newest {
			helper::gen_newest_url(page)
		} else {
			String::new()
		};

		let json = helper::get_json(url)?;
		let empty = Value::Null;
		let data = json.get("results").unwrap_or(&empty);

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(data.get("list")),
			has_next_page: parser::has_more(data),
		})
	}
}

impl BasicLoginHandler for Copymanga {
	fn handle_basic_login(
		&self,
		key: String,
		_username: String,
		_password: String,
	) -> Result<bool> {
		if key != "login" {
			return Ok(false);
		}

		helper::login(&_username, &_password)?;

		Ok(true)
	}
}

register_source!(Copymanga, ListingProvider, BasicLoginHandler);
