#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{html::Element, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://se8.us/index.php";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

const FILTER_ORDER: [&str; 2] = ["hits", "addtime"];

struct Se8;

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

fn jstr(value: &Value, key: &str) -> String {
	match value.get(key) {
		Some(Value::String(string)) => string.clone(),
		Some(Value::Number(number)) => number.to_string(),
		_ => String::new(),
	}
}

impl Source for Se8 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut tag = String::new();
		let mut finish = String::new();
		let mut order = String::from("hits");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "tag" {
						tag = value;
					} else if id == "finish" {
						finish = value;
					}
				}
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
		let url = if query.is_empty() {
			let mut url = format!("{WWW_URL}/category");
			if !tag.is_empty() {
				url.push_str(format!("/tags/{tag}").as_str());
			}
			if !finish.is_empty() {
				url.push_str(format!("/finish/{finish}").as_str());
			}
			format!("{url}/order/{order}/page/{page}")
		} else {
			format!("{WWW_URL}/search/{}/{page}", encode_uri(&query))
		};
		let html = Request::get(url)?.header("User-Agent", UA).html()?;
		let mut mangas: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".comic-item,.comic-list-item") else {
			return Ok(MangaPageResult {
				entries: mangas,
				has_next_page: true,
			});
		};

		for item in items {
			let Some(href) = item.select_first("a").and_then(|link| link.attr("href")) else {
				continue;
			};
			let key = href.split('/').last().unwrap_or_default().to_string();
			let attribute = if query.is_empty() { "data-src" } else { "src" };
			let cover = item
				.select_first("a>img")
				.and_then(|img| img.attr(attribute))
				.filter(|cover| !cover.is_empty());
			let title = item
				.select_first("div>.comic-name")
				.and_then(|title| title.text())
				.unwrap_or_default();
			mangas.push(Manga {
				key,
				title,
				cover,
				..Default::default()
			});
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
			let url = format!("{WWW_URL}/comic/{}", manga.key);
			let html = Request::get(&url)?.header("User-Agent", UA).html()?;
			manga.cover = html
				.select_first(".comic-info-box>.box-back")
				.and_then(|cover| cover.attr("style"))
				.map(|style| {
					style
						.replace("background-image: url('", "")
						.replace("')", "")
				})
				.filter(|cover| !cover.is_empty());
			manga.title = html
				.select_first(".comic-info-box>.comic-info>h1")
				.and_then(|title| title.text())
				.unwrap_or_default();
			let authors: Vec<String> = html
				.select_first(".comic-info-box>.comic-info>.au-name")
				.and_then(|author| author.text())
				.unwrap_or_default()
				.replace("作者：", "")
				.replace("&amp", "&")
				.split('&')
				.filter(|author| !author.trim().is_empty())
				.map(|author| author.to_string())
				.collect();
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = html
				.select_first(".comic-intro")
				.and_then(|description| description.text())
				.map(|description| description.trim().replace("&hellip", "…"));
			let tags: Vec<String> = html
				.select_first(".comic-info-box>.comic-info>.comic-tags")
				.and_then(|tags| tags.html())
				.and_then(|tags| {
					substring_after(tags.trim(), "</span>").map(|tags| {
						tags.trim()
							.split(' ')
							.filter(|tag| !tag.trim().is_empty())
							.map(|tag| tag.to_string())
							.collect::<Vec<String>>()
					})
				})
				.unwrap_or_default();
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.status = MangaStatus::Ongoing;
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(url);

			// the site exposes the real manga id in an inline `comic.show(...)` call
			let page_html = Element::from(html).html().unwrap_or_default();
			let key = substring_after(&page_html, "comic.show(")
				.and_then(|key| substring_before(key, ");"))
				.unwrap_or_default()
				.to_string();
			if !key.is_empty() {
				manga.key = key;
			}
		}

		if needs_chapters {
			let json = Request::get(format!("{WWW_URL}/api/comic/chapter?mid={}", manga.key))?
				.header("User-Agent", UA)
				.json_owned::<Value>()?;
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(list) = json.get("data").and_then(|data| data.as_array()) {
				for (index, item) in list.iter().enumerate() {
					let title = jstr(item, "name")
						.trim()
						.replace("&lt;", "<")
						.replace("&gt;", ">")
						.replace("&#40;", "(")
						.replace("&#41;", ")")
						.replace("&ldquo;", "“")
						.replace("&rdquo;", "”")
						.replace("&hellip;", "…")
						.replace("&hearts;", "♥");
					chapters.push(Chapter {
						key: jstr(item, "id"),
						title: Some(title),
						chapter_number: Some((index + 1) as f32),
						url: Some(jstr(item, "link")),
						..Default::default()
					});
				}
			}

			chapters.reverse();
			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/chapter/{}", chapter.key);
		let html = Request::get(url)?.header("User-Agent", UA).html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select(".comic-page>img") else {
			return Ok(pages);
		};

		for item in items {
			let Some(url) = item.attr("src") else {
				continue;
			};
			let url = url.trim().to_string();
			if url.is_empty() {
				continue;
			}
			pages.push(Page {
				content: PageContent::url(url),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

register_source!(Se8);
