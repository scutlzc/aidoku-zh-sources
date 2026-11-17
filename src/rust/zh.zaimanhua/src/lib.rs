#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{error::AidokuError, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://m.zaimanhua.com";
const API_URL: &str = "https://manhua.zaimanhua.com/api/v1";
const APP_URL: &str = "https://manhua.zaimanhua.com/app/v1";
const V4_APP_URL: &str = "https://v4api.zaimanhua.com/app/v1";

const FILTER_SORT_TYPE: [&str; 1] = ["0"];

fn jstr(value: &Value, key: &str) -> Option<String> {
	value
		.get(key)
		.and_then(|v| v.as_str())
		.map(|v| v.to_string())
}

fn jnum(value: &Value, key: &str) -> Option<String> {
	value.get(key).and_then(|v| {
		v.as_i64()
			.map(|n| n.to_string())
			.or_else(|| v.as_str().map(|s| s.to_string()))
	})
}

fn jobj<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
	value.get(key).filter(|v| v.is_object())
}

fn jarr<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
	value.get(key).and_then(|v| v.as_array())
}

fn jtags(value: &Value, key: &str) -> Vec<String> {
	jarr(value, key)
		.map(|list| {
			list.iter()
				.filter_map(|item| jstr(item, "tag_name"))
				.collect::<Vec<String>>()
		})
		.unwrap_or_default()
}

fn parse_manga_list(list: &[Value], title_key: &str, key_name: &str) -> Vec<Manga> {
	let mut entries: Vec<Manga> = Vec::new();

	for item in list {
		let Some(key) = jnum(item, key_name) else {
			continue;
		};
		entries.push(Manga {
			key,
			cover: jstr(item, "cover"),
			title: jstr(item, title_key).unwrap_or_default(),
			..Default::default()
		});
	}

	entries
}

struct Zaimanhua;

impl Source for Zaimanhua {
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
		let mut status = String::from("0");
		let mut audience = String::from("0");
		let mut theme = String::from("0");
		let mut cate = String::from("0");
		let mut first_letter = String::new();
		let mut sort_type = String::from("0");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"status" => status = value,
					"audience" => audience = value,
					"theme" => theme = value,
					"cate" => cate = value,
					"firstLetter" => first_letter = value,
					_ => continue,
				},
				FilterValue::Sort { index, .. } => {
					if let Some(value) = FILTER_SORT_TYPE.get(index as usize) {
						sort_type = value.to_string();
					}
				}
				_ => continue,
			}
		}

		let url = if query.is_empty() {
			format!("{API_URL}/comic1/filter?sortType={sort_type}&page={page}&size=18&status={status}&audience={audience}&theme={theme}&cate={cate}&firstLetter={first_letter}")
		} else {
			format!(
				"{APP_URL}/search/index?keyword={}&source=0&page={page}&size=20",
				encode_uri(query.clone())
			)
		};

		let json: Value = Request::get(url)?.json_owned::<Value>()?;
		let data = json.get("data");
		let entries = if query.is_empty() {
			match data.and_then(|data| jarr(data, "comicList")) {
				Some(list) => parse_manga_list(list, "name", "id"),
				None => return Err(AidokuError::DeserializeError),
			}
		} else {
			match data.and_then(|data| jarr(data, "list")) {
				Some(list) => parse_manga_list(list, "title", "id"),
				None => return Err(AidokuError::DeserializeError),
			}
		};

		Ok(MangaPageResult {
			entries,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{V4_APP_URL}/comic/detail/{}", manga.key);
		let json: Value = Request::get(url)?.json_owned::<Value>()?;
		let data = json.get("data").and_then(|data| jobj(data, "data"));

		if needs_details {
			let Some(data) = data else {
				return Err(AidokuError::DeserializeError);
			};
			manga.cover = jstr(data, "cover");
			if let Some(title) = jstr(data, "title") {
				manga.title = title;
			}
			let authors = jarr(data, "authors")
				.map(|list| {
					list.iter()
						.filter_map(|author| jstr(author, "tag_name"))
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = jstr(data, "description");
			manga.url = Some(format!("{WWW_URL}/pages/comic/detail?id={}", manga.key));
			let tags = jtags(data, "types");
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.status = match jarr(data, "status")
				.and_then(|list| list.first())
				.and_then(|status| jstr(status, "tag_name"))
				.unwrap_or_default()
				.as_str()
			{
				"连载中" => MangaStatus::Ongoing,
				"已完结" => MangaStatus::Completed,
				_ => MangaStatus::Unknown,
			};
			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::RightToLeft;
		}

		if needs_chapters {
			let mut chapters: Vec<Chapter> = Vec::new();
			let list = data
				.and_then(|data| jarr(data, "chapters"))
				.and_then(|list| list.first())
				.and_then(|chapter| jarr(chapter, "data"));

			if let Some(list) = list {
				let len = list.len();
				for (index, item) in list.iter().enumerate() {
					let Some(chapter_id) = jnum(item, "chapter_id") else {
						continue;
					};
					chapters.push(Chapter {
						key: chapter_id.clone(),
						title: jstr(item, "chapter_title"),
						chapter_number: Some((len - index) as f32),
						url: Some(format!(
							"{WWW_URL}/pages/comic/page?comic_id={}&chapter_id={chapter_id}",
							manga.key
						)),
						..Default::default()
					});
				}
			}

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!(
			"{API_URL}/comic1/chapter/detail?channel=pc&app_name=zmh&version=1.0.0&comic_id={}&chapter_id={}",
			manga.key, chapter.key
		);
		let json: Value = Request::get(url)?.json_owned::<Value>()?;
		let mut pages: Vec<Page> = Vec::new();

		let list = json
			.get("data")
			.and_then(|data| jobj(data, "chapterInfo"))
			.and_then(|info| jarr(info, "page_url"));

		if let Some(list) = list {
			for item in list {
				let Some(url) = item.as_str() else {
					continue;
				};
				pages.push(Page {
					content: PageContent::url(url.to_string()),
					..Default::default()
				});
			}
		}

		Ok(pages)
	}
}

impl ListingProvider for Zaimanhua {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let (cate, duration) = match listing.id.as_str() {
			"pop_week" => ("1", "1"),
			"pop_month" => ("1", "2"),
			"pop_all" => ("1", "3"),
			"click_week" => ("2", "1"),
			"click_month" => ("2", "2"),
			"click_all" => ("2", "3"),
			_ => {
				return self.get_search_manga_list(None, page, Vec::new());
			}
		};

		let url = format!("{API_URL}/comic1/rank_list?channel=pc&app_name=zmh&version=1.0.0&page={page}&size=10&duration={duration}&cate={cate}&tag=0&theme=0");
		let json: Value = Request::get(url)?.json_owned::<Value>()?;
		let list = json
			.get("data")
			.and_then(|data| jarr(data, "list"))
			.ok_or(AidokuError::DeserializeError)?;

		Ok(MangaPageResult {
			entries: parse_manga_list(list, "title", "comic_id"),
			has_next_page: true,
		})
	}
}

register_source!(Zaimanhua, ListingProvider);
