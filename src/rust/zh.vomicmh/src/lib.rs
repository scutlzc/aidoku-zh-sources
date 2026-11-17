#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "http://www.vomicmh.com";
const API_URL: &str = "http://api.vomicmh.com";

struct Vomicmh;

fn encode_img_url(url: String) -> String {
	if url.contains('%') {
		url
	} else {
		encode_uri(url)
	}
}

fn jstr(value: &Value, key: &str) -> String {
	match value.get(key) {
		Some(Value::String(string)) => string.clone(),
		Some(Value::Number(number)) => number.to_string(),
		_ => String::new(),
	}
}

fn jstr_list(value: Option<&Value>) -> Vec<String> {
	value
		.and_then(|list| list.as_array())
		.map(|list| {
			list.iter()
				.filter_map(|item| match item {
					Value::String(string) => Some(string.clone()),
					Value::Number(number) => Some(number.to_string()),
					_ => None,
				})
				.collect::<Vec<String>>()
		})
		.unwrap_or_default()
}

impl Source for Vomicmh {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			format!("{API_URL}/api/v1/rank/rank-data?rank_id=1&page={page}")
		} else {
			format!(
				"{API_URL}/api/v1/search/search-comic-data?title={}&page={page}",
				encode_uri(&query)
			)
		};
		let json = Request::get(url)?.json_owned::<Value>()?;
		let mut mangas: Vec<Manga> = Vec::new();

		if let Some(list) = json
			.get("data")
			.and_then(|data| data.get("result"))
			.and_then(|result| result.as_array())
		{
			for item in list {
				let cover = Some(encode_img_url(jstr(item, "cover_img_url")))
					.filter(|cover| !cover.is_empty());
				mangas.push(Manga {
					key: jstr(item, "mid"),
					title: jstr(item, "title"),
					cover,
					..Default::default()
				});
			}
		}

		Ok(MangaPageResult {
			entries: mangas,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let json = Request::get(format!(
				"{API_URL}/api/v1/detail/get-comic-detail-data?mid={}",
				manga.key
			))?
			.json_owned::<Value>()?;

			if let Some(data) = json.get("data") {
				manga.cover = Some(encode_img_url(jstr(data, "cover_img_url")))
					.filter(|cover| !cover.is_empty());
				manga.title = jstr(data, "title");
				let authors = jstr_list(data.get("authors_name"));
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.description =
					Some(jstr(data, "description")).filter(|description| !description.is_empty());
				let tags = jstr_list(data.get("categories"));
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.status = match jstr(data, "status").as_str() {
					"连载中" => MangaStatus::Ongoing,
					"已完结" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
			}

			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(format!("{WWW_URL}/#/detail?id={}", manga.key));
		}

		if needs_chapters {
			let json = Request::get(format!(
				"{API_URL}/api/v1/detail/get-comic-detail-chapter-data?mid={}",
				manga.key
			))?
			.json_owned::<Value>()?;
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(list) = json.get("data").and_then(|data| data.as_array()) {
				let len = list.len();
				for (index, item) in list.iter().enumerate() {
					let key = jstr(item, "cid");
					chapters.push(Chapter {
						url: Some(format!("{WWW_URL}/#/page/{}/{key}", manga.key)),
						key,
						title: Some(jstr(item, "title")),
						chapter_number: Some((len - index) as f32),
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
			"{WWW_URL}/m_{}/chapterimage.ashx?mid={}",
			chapter.key, manga.key
		);
		let json = Request::get(url)?.json_owned::<Value>()?;
		let mut pages: Vec<Page> = Vec::new();

		for url in jstr_list(json.get("img_url_list")) {
			if url.is_empty() {
				continue;
			}
			pages.push(Page {
				content: PageContent::url(encode_img_url(url)),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

register_source!(Vomicmh);
