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

const WWW_URL: &str = "https://www.manhuadb.com";
const STATIC_URL: &str = "https://i2.manhuadb.com/static";

struct Manhuadb;

fn handle_cover(cover: Option<String>) -> Option<String> {
	let cover = cover.filter(|cover| !cover.is_empty())?;
	if cover.starts_with("https") {
		Some(cover)
	} else {
		Some(format!("{WWW_URL}{cover}"))
	}
}

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

// standard base64 decoding (the site's `var img_data = '...'` payload)
fn base64_decode(input: &str) -> Vec<u8> {
	let mut output: Vec<u8> = Vec::new();
	let mut buffer: u32 = 0;
	let mut bits: u32 = 0;
	for byte in input.bytes() {
		let value = match byte {
			b'A'..=b'Z' => byte - b'A',
			b'a'..=b'z' => byte - b'a' + 26,
			b'0'..=b'9' => byte - b'0' + 52,
			b'+' => 62,
			b'/' => 63,
			_ => continue,
		} as u32;
		buffer = (buffer << 6) | value;
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			output.push((buffer >> bits) as u8);
			if bits == 0 {
				buffer = 0;
			} else {
				buffer &= (1 << bits) - 1;
			}
		}
	}
	output
}

impl Manhuadb {
	fn parse_manga_list(url: String, class: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut mangas: Vec<Manga> = Vec::new();

		let Some(items) = html.select(format!("div[class*='{class}']")) else {
			return Ok(mangas);
		};

		for item in items {
			let Some(href) = item.select_first("a").and_then(|a| a.attr("href")) else {
				continue;
			};
			let key = href.split('/').last().unwrap_or_default().to_string();
			let cover = handle_cover(
				item.select_first("a>img")
					.and_then(|img| img.attr("data-original")),
			);
			let title = item
				.select_first("div>h2>a")
				.and_then(|title| title.text())
				.unwrap_or_default();
			mangas.push(Manga {
				key,
				title,
				cover,
				..Default::default()
			});
		}

		Ok(mangas)
	}
}

impl Source for Manhuadb {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut region = String::new();
		let mut audience = String::new();
		let mut status = String::new();
		let mut category = String::new();

		for filter in filters {
			if let FilterValue::Select { id, value } = filter {
				if id == "region" {
					region = value;
				} else if id == "audience" {
					audience = value;
				} else if id == "status" {
					status = value;
				} else if id == "category" {
					category = value;
				}
			}
		}

		let query = query.unwrap_or_default();
		let (url, class) = if query.is_empty() {
			(
				format!("{WWW_URL}/manhua/list-r-{region}-a-{audience}-s-{status}-c-{category}-page-{page}.html"),
				"comic-book",
			)
		} else {
			(
				format!("{WWW_URL}/search?q={}&p={page}", encode_uri(query)),
				"comicbook",
			)
		};

		Ok(MangaPageResult {
			entries: Manhuadb::parse_manga_list(url, class)?,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{WWW_URL}/manhua/{}", manga.key);

		if needs_details {
			let html = Request::get(&url)?.html()?;
			manga.cover = handle_cover(
				html.select_first(".comic-cover>img")
					.and_then(|img| img.attr("src")),
			);
			manga.title = html
				.select_first(".comic-title")
				.and_then(|title| title.text())
				.unwrap_or_default();
			let authors: Vec<String> = html
				.select_first("meta[property='og:novel:author']")
				.and_then(|author| author.attr("content"))
				.unwrap_or_default()
				.split(' ')
				.map(|author| author.trim().to_string())
				.filter(|author| !author.is_empty())
				.collect();
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = html
				.select_first(".comic_story")
				.and_then(|description| description.text());
			let tags: Vec<String> = html
				.select_first("meta[property='og:novel:category']")
				.and_then(|category| category.attr("content"))
				.unwrap_or_default()
				.split(' ')
				.map(|tag| tag.to_string())
				.collect();
			manga.tags = Some(tags);
			manga.status = match html
				.select_first("meta[property='og:novel:status']")
				.and_then(|status| status.attr("content"))
				.unwrap_or_default()
				.as_str()
			{
				"连载中" => MangaStatus::Ongoing,
				"已完结" => MangaStatus::Completed,
				_ => MangaStatus::Unknown,
			};
			manga.content_rating = match html
				.select_first(".comic_age")
				.and_then(|age| age.text())
				.unwrap_or_default()
				.as_str()
			{
				"青年" => ContentRating::Suggestive,
				"女青" => ContentRating::Suggestive,
				_ => ContentRating::Safe,
			};
			manga.viewer = Viewer::RightToLeft;
			manga.url = Some(url);
		}

		if needs_chapters {
			let html = Request::get(format!("{WWW_URL}/manhua/{}", manga.key))?.html()?;
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(items) = html.select(".links-of-books>li>a") {
				for (index, item) in items.enumerate() {
					let Some(href) = item.attr("href") else {
						continue;
					};
					let key = href
						.split('/')
						.last()
						.unwrap_or_default()
						.replace(".html", "");
					let title = item.text().unwrap_or_default();
					chapters.push(Chapter {
						key: key.clone(),
						title: Some(title),
						chapter_number: Some((index + 1) as f32),
						url: Some(format!("{WWW_URL}/manhua/{}/{key}.html", manga.key)),
						..Default::default()
					});
				}
			}

			chapters.reverse();
			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/manhua/{}/{}.html", manga.key, chapter.key);
		let string = Request::get(url)?.string()?;
		let Some(data) = substring_after(&string, "var img_data = ")
			.and_then(|data| substring_before(data, ";"))
		else {
			bail!("img_data not found");
		};
		let list = serde_json::from_slice::<Value>(&base64_decode(&data.replace('\'', "")))?;
		let Some(list) = list.as_array() else {
			bail!("invalid img_data");
		};
		let sub_path = chapter.key.replace('_', "/");
		let mut pages: Vec<Page> = Vec::new();

		for item in list {
			let Some(img) = item.get("img").and_then(|img| img.as_str()) else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(format!("{STATIC_URL}/{sub_path}/{img}")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

register_source!(Manhuadb);
