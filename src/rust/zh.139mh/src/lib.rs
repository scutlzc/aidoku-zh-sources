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

const WWW_URL: &str = "https://139mh.com";
const API_URL: &str = "https://api.139mh.com";
const IMG_URL: &str = "https://i.139img.com";

const FILTER_ORDER: [&str; 2] = ["hot", "last"];

struct M139;

fn handle_cover_url(url: &str) -> String {
	format!("{IMG_URL}/cover/{url}")
}

fn handle_page_url(url: &str) -> String {
	format!("{IMG_URL}/img/{url}")
}

fn jstr(value: &Value, key: &str) -> Option<String> {
	value
		.get(key)
		.and_then(|value| value.as_str())
		.map(|value| value.to_string())
}

fn jint(value: &Value, key: &str) -> Option<i64> {
	value.get(key).and_then(|value| {
		value.as_i64().or_else(|| {
			value
				.as_str()
				.and_then(|value| value.trim().parse::<i64>().ok())
		})
	})
}

fn jid(value: &Value) -> Option<String> {
	value.get("id").and_then(|value| {
		value
			.as_i64()
			.map(|id| id.to_string())
			.or_else(|| value.as_str().map(|id| id.to_string()))
	})
}

fn parse_manga_list(list: Option<&Vec<Value>>) -> Vec<Manga> {
	let mut mangas: Vec<Manga> = Vec::new();

	for item in list.map(|list| list.as_slice()).unwrap_or_default() {
		let Some(key) = jid(item) else { continue };
		let cover = jstr(item, "cover")
			.filter(|cover| !cover.is_empty())
			.map(|cover| handle_cover_url(&cover));
		let title = jstr(item, "title").unwrap_or_default();
		mangas.push(Manga {
			key,
			title,
			cover,
			..Default::default()
		});
	}

	mangas
}

fn parse_chapter_list(list: &[Value], manga_key: &str) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	for (index, item) in list.iter().enumerate() {
		let Some(id) = jid(item) else { continue };
		let title = jstr(item, "title").unwrap_or_default();
		let url = format!("{WWW_URL}/comic/vol-{id}");
		chapters.push(Chapter {
			key: id,
			title: Some(title),
			chapter_number: Some((index + 1) as f32),
			url: Some(url),
			..Default::default()
		});
	}
	chapters.reverse();

	let _ = manga_key;
	chapters
}

impl M139 {
	fn parse_details(manga: &mut Manga, info: &Value, key: &str) {
		manga.cover = jstr(info, "cover")
			.filter(|cover| !cover.is_empty())
			.map(|cover| handle_cover_url(&cover));
		manga.title = jstr(info, "title").unwrap_or_default();

		let authors: Vec<String> = info
			.get("auth")
			.and_then(|auth| auth.as_array())
			.map(|auth| {
				auth.iter()
					.filter_map(|author| jstr(author, "auth_name"))
					.collect()
			})
			.unwrap_or_default();
		manga.authors = if authors.is_empty() {
			None
		} else {
			Some(authors)
		};

		manga.description = jstr(info, "summary");
		manga.url = Some(format!("{WWW_URL}/comic-{key}"));

		let tags: Vec<String> = info
			.get("cata_list")
			.and_then(|list| list.as_array())
			.map(|list| list.iter().filter_map(|tag| jstr(tag, "title")).collect())
			.unwrap_or_default();
		manga.tags = if tags.is_empty() { None } else { Some(tags) };

		manga.status = match jint(info, "progress") {
			Some(1) => MangaStatus::Completed,
			Some(2) => MangaStatus::Hiatus,
			Some(3) => MangaStatus::Ongoing,
			_ => MangaStatus::Unknown,
		};
		manga.content_rating = ContentRating::Safe;
		manga.viewer = Viewer::RightToLeft;
	}
}

impl Source for M139 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut subject = String::from("all");
		let mut area = String::from("all");
		let mut progress = String::from("all");
		let mut order = String::from("hot");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"subject" => subject = value,
					"area" => area = value,
					"progress" => progress = value,
					_ => continue,
				},
				FilterValue::Sort { index, .. } => {
					order = FILTER_ORDER
						.get(index as usize)
						.copied()
						.unwrap_or_default()
						.to_string();
				}
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let is_search = !query.is_empty();
		let url = if !is_search {
			format!(
				"{API_URL}/api/comic/index?page={page}&area={area}&subject={subject}&progress={progress}&order={order}"
			)
		} else {
			format!(
				"{API_URL}/api/comic/search?page={page}&q={}",
				encode_uri(query)
			)
		};

		let json: Value = Request::get(url)?.json_owned()?;
		let list = if !is_search {
			json.get("result").and_then(|result| result.as_array())
		} else {
			json.get("result")
				.and_then(|result| result.get("list"))
				.and_then(|list| list.as_array())
		};

		Ok(MangaPageResult {
			entries: parse_manga_list(list),
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details || needs_chapters {
			let key = manga.key.clone();
			let url = format!("{API_URL}/api/comic/detail?id={key}");
			let json: Value = Request::get(url)?.json_owned()?;
			let result = json.get("result");

			if needs_details {
				if let Some(info) = result.and_then(|result| result.get("info")) {
					M139::parse_details(&mut manga, info, &key);
				}
			}

			if needs_chapters {
				let list = result
					.and_then(|result| result.get("vol_list"))
					.and_then(|list| list.as_array());
				manga.chapters = Some(
					list.map(|list| parse_chapter_list(list, &key))
						.unwrap_or_default(),
				);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{API_URL}/api/vol/detail?id={}", chapter.key);
		let json: Value = Request::get(url)?.json_owned()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(list) = json
			.get("result")
			.and_then(|result| result.get("img_list"))
			.and_then(|list| list.as_array())
		else {
			return Ok(pages);
		};

		for item in list {
			let Some(image) = item.as_str() else { continue };
			pages.push(Page {
				content: PageContent::url(handle_page_url(image)),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

register_source!(M139);
