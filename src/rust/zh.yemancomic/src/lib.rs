#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::{encode_uri, encode_uri_component},
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://yemancomic.com";
/// 桌面 UA 会被 307 到 baidu.com，只有移动端 UA 才返回图源页面
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";
/// 阅读接口每次最多返回 10 张图
const PICS_PER_REQUEST: usize = 10;

fn get(url: &str) -> Result<Request> {
	Ok(Request::get(url)?.header("User-Agent", UA))
}

fn post(url: &str) -> Result<Request> {
	Ok(Request::post(url)?
		.header("User-Agent", UA)
		.header("Content-Type", "application/x-www-form-urlencoded"))
}

/// 从 `/book/72343/`、`/chapter/72343/2362660.html` 这类链接里取出 id
fn key_from_href(href: &str) -> String {
	let mut last = "";
	for part in href.split("/") {
		let part = part.split("?").next().unwrap_or_default();
		if !part.is_empty() {
			last = part;
		}
	}
	last.split(".").next().unwrap_or_default().to_string()
}

struct Yemancomic;

impl Yemancomic {
	fn parse_list(url: &str) -> Result<Vec<Manga>> {
		let html = get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".comic-item") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item.select_first("a").and_then(|a| a.attr("href")) else {
				continue;
			};
			let key = key_from_href(&href);
			if key.is_empty() {
				continue;
			}
			let cover = item.select_first("img.img").and_then(|img| img.attr("src"));
			let title = item
				.select_first("p.title")
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
		let html = get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		// 榜单前三名有封面图，其余条目只有书名和链接
		let mut covers: Vec<(String, String)> = Vec::new();
		if let Some(items) = html.select(".comic-item") {
			for item in items {
				let Some(href) = item.select_first("a").and_then(|a| a.attr("href")) else {
					continue;
				};
				let Some(cover) = item.select_first("img").and_then(|img| img.attr("src")) else {
					continue;
				};
				covers.push((key_from_href(&href), cover));
			}
		}

		let Some(items) = html.select(".comic-name>a") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item.attr("href") else {
				continue;
			};
			let key = key_from_href(&href);
			if key.is_empty() {
				continue;
			}
			let title = item.text().unwrap_or_default().trim().to_string();
			let cover = covers
				.iter()
				.find(|(cover_key, _)| cover_key == &key)
				.map(|(_, cover)| cover.to_string());
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
		let body = format!("key={}", encode_uri_component(query));
		let json: Value = post(&url)?.body(body.as_bytes()).json_owned::<Value>()?;
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
			let html = get(&url)?.html()?;

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
				if let Some(list) = html.select(".comic-chapter-link") {
					for (index, item) in list.enumerate() {
						let Some(href) = item.attr("href") else {
							continue;
						};
						let chapter_id = key_from_href(&href);
						if chapter_id.is_empty() {
							continue;
						}
						let title = item.attr("title").or_else(|| item.text());
						let chapter_url = if href.starts_with("http") {
							href
						} else {
							format!("{WWW_URL}{href}")
						};
						chapters.push(Chapter {
							key: chapter_id,
							title,
							chapter_number: Some((index + 1) as f32),
							url: Some(chapter_url),
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
		// 阅读页 HTML 里只有占位图，真实图片地址要分批向接口取
		let url = format!("{WWW_URL}/api/comic/read/pics");
		let mut pages: Vec<Page> = Vec::new();
		let mut offset = 0usize;

		while offset < 2000 {
			let body = format!(
				"id={}&aid={}&offset={}&limit={PICS_PER_REQUEST}",
				encode_uri_component(&chapter.key),
				encode_uri_component(&manga.key),
				offset
			);
			let json: Value = post(&url)?.body(body.as_bytes()).json_owned::<Value>()?;
			let Some(data) = json.get("data") else {
				break;
			};
			let Some(list) = data.get("pic").and_then(|pic| pic.as_array()) else {
				break;
			};
			if list.is_empty() {
				break;
			}
			for item in list {
				if let Some(src) = item.get("pic").and_then(|pic| pic.as_str()) {
					pages.push(Page {
						content: PageContent::url(encode_uri(src)),
						..Default::default()
					});
				}
			}
			offset += list.len();
			let total = data
				.get("total")
				.and_then(|total| total.as_i64())
				.unwrap_or(0);
			if total > 0 && offset as i64 >= total {
				break;
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
