#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://yemancomic.com";

struct Yemancomic;

impl Yemancomic {
	fn parse_list(url: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".acgn-item") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item
				.select_first(".acgn-thumbnail")
				.and_then(|a| a.attr("href"))
			else {
				continue;
			};
			let mut parts: Vec<String> = href
				.split("/")
				.filter(|a| !a.is_empty())
				.map(|a| a.to_string())
				.collect();
			let key = parts.pop().unwrap_or_default();
			let cover = item
				.select_first(".acgn-thumbnail>img")
				.and_then(|img| img.attr("src"));
			let title = item
				.select_first(".acgn-info>.acgn-title>a")
				.and_then(|title| title.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			entries.push(Manga {
				key,
				cover,
				title,
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_top(url: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".acgn-item") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item
				.select_first(".acgn-thumbnail")
				.and_then(|a| a.attr("href"))
			else {
				continue;
			};
			let mut parts: Vec<String> = href
				.split("/")
				.filter(|a| !a.is_empty())
				.map(|a| a.to_string())
				.collect();
			let key = parts.pop().unwrap_or_default();
			let cover = item
				.select_first(".acgn-thumbnail>img")
				.and_then(|img| img.attr("src"));
			let title = item
				.select_first(".acgn-info>.acgn-title>a")
				.and_then(|title| title.attr("title"))
				.unwrap_or_default()
				.split(",")
				.map(|a| a.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default();
			entries.push(Manga {
				key,
				cover,
				title,
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_search(query: &str) -> Result<Vec<Manga>> {
		let url = format!("{WWW_URL}/api/front/index/search");
		let body = format!("key={query}");
		let json: Value = Request::post(url)?
			.header("Content-Type", "application/x-www-form-urlencoded")
			.body(body.as_bytes())
			.json_owned::<Value>()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(list) = json.get("data").and_then(|data| data.as_array()) else {
			return Ok(entries);
		};

		for item in list {
			let Some(key) = item.get("id").and_then(|id| {
				id.as_i64()
					.map(|id| id.to_string())
					.or_else(|| id.as_str().map(|id| id.to_string()))
			}) else {
				continue;
			};
			let cover = item
				.get("cover")
				.and_then(|cover| cover.as_str())
				.map(|cover| cover.to_string());
			let title = item
				.get("name")
				.and_then(|name| name.as_str())
				.unwrap_or_default()
				.to_string();
			entries.push(Manga {
				key,
				cover,
				title,
				..Default::default()
			});
		}

		Ok(entries)
	}
}

impl Source for Yemancomic {
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
		let mut category = String::from("全部");
		let mut region = String::from("9");
		let mut status = String::from("3");

		for filter in filters {
			if let FilterValue::Select { id, value } = filter {
				match id.as_str() {
					"category" => category = value,
					"region" => region = value,
					"status" => status = value,
					_ => continue,
				}
			}
		}

		if query.is_empty() {
			let url = format!(
				"{WWW_URL}/comiclists/{region}/{}/{status}/{page}.html",
				encode_uri(category)
			);
			Ok(MangaPageResult {
				entries: Yemancomic::parse_list(&url)?,
				has_next_page: true,
			})
		} else {
			Ok(MangaPageResult {
				entries: Yemancomic::parse_search(&query)?,
				has_next_page: false,
			})
		}
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{WWW_URL}/book/{}/", manga.key);

		if needs_details || needs_chapters {
			let html = Request::get(&url)?.html()?;

			if needs_details {
				manga.cover = html
					.select_first("meta[property='og:image']")
					.and_then(|meta| meta.attr("content"));
				if let Some(title) = html
					.select_first("meta[property='og:title']")
					.and_then(|meta| meta.attr("content"))
				{
					manga.title = title;
				}
				let authors: Vec<String> = html
					.select_first("meta[property='og:novel:author']")
					.and_then(|meta| meta.attr("content"))
					.unwrap_or_default()
					.split("|")
					.map(|a| a.to_string())
					.collect();
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.description = html
					.select_first("meta[property='og:description']")
					.and_then(|meta| meta.attr("content"));
				let tags: Vec<String> = html
					.select_first("meta[property='og:novel:category']")
					.and_then(|meta| meta.attr("content"))
					.unwrap_or_default()
					.split(",")
					.map(|a| a.to_string())
					.filter(|a| !a.is_empty())
					.collect();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.status = match html
					.select_first("meta[property='og:novel:status']")
					.and_then(|meta| meta.attr("content"))
					.unwrap_or_default()
					.as_str()
				{
					"连载" => MangaStatus::Ongoing,
					"完结" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::Webtoon;
				manga.url = Some(url.clone());
			}

			if needs_chapters {
				let mut chapters: Vec<Chapter> = Vec::new();
				if let Some(list) = html.select(".chapter-list>li") {
					for (index, item) in list.enumerate() {
						let Some(chapter_id) = item.attr("data-chapter") else {
							continue;
						};
						let title = item.select_first("a").and_then(|a| a.attr("title"));
						chapters.push(Chapter {
							key: chapter_id.clone(),
							title,
							chapter_number: Some((index + 1) as f32),
							url: Some(format!("{WWW_URL}/chapter/{}/{chapter_id}.html", manga.key)),
							..Default::default()
						});
					}
				}
				chapters.reverse();
				manga.chapters = Some(chapters);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/chapter/{}/{}.html", manga.key, chapter.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		if let Some(items) = html.select(".acgn-reader-chapter__item>div>img") {
			for item in items {
				let Some(src) = item.attr("src") else {
					continue;
				};
				pages.push(Page {
					content: PageContent::url(encode_uri(src)),
					..Default::default()
				});
			}
		}

		Ok(pages)
	}
}

impl ListingProvider for Yemancomic {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let name = match listing.id.as_str() {
			"alldj" => "alldj",
			"ydj" => "ydj",
			"zdj" => "zdj",
			"rdj" => "rdj",
			"allfav" => "allfav",
			_ => {
				return self.get_search_manga_list(None, page, Vec::new());
			}
		};

		let url = format!("{WWW_URL}/top/{name}.html");

		Ok(MangaPageResult {
			entries: Yemancomic::parse_top(&url)?,
			has_next_page: false,
		})
	}
}

register_source!(Yemancomic, ListingProvider);
