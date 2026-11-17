#![no_std]

use aidoku::{
	alloc::{
		string::{String, ToString},
		vec::Vec,
	},
	helpers::uri::encode_uri,
	imports::{
		html::{Document, Element},
		net::Request,
	},
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://baozimh.org";
const API_URL: &str = "https://api-get-v2.mgsearcher.com";
const IMG_URL: &str = "https://f40-1-4.g-mh.online";

const TITLE_SUFFIX: &str = "-🌈️包子漫畫";

struct Baozimh2;

fn handle_cover_url(url: String) -> String {
	if url.contains("url=") {
		url.split("url=")
			.last()
			.map(|part| part.to_string())
			.unwrap_or_default()
			.replace("%3A", ":")
			.replace("%2F", "/")
			.replace("&w=250&q=60", "")
	} else {
		url
	}
}

fn split_key(key: &str) -> (String, Option<String>) {
	let mut parts = key.split('/');
	let base = parts.next().unwrap_or_default().to_string();
	let mid = parts.next().map(|part| part.to_string());
	(base, mid)
}

fn last_segment(href: &str) -> Option<String> {
	href.split('/')
		.next_back()
		.filter(|part| !part.is_empty())
		.map(|part| part.to_string())
}

fn parse_item(item: &Element) -> Option<Manga> {
	let href = item.attr("href")?;
	let key = last_segment(&href)?;
	let cover = item
		.select_first("div>img")
		.and_then(|element| element.attr("src"))
		.map(handle_cover_url);
	let title = item
		.select_first("div>h3")
		.and_then(|element| element.text())
		.unwrap_or_default();
	Some(Manga {
		key,
		title,
		cover,
		..Default::default()
	})
}

fn parse_items(html: &Document) -> Vec<Manga> {
	let mut entries: Vec<Manga> = Vec::new();
	let Some(items) = html.select(".pb-2>a") else {
		return entries;
	};
	for item in items {
		if let Some(manga) = parse_item(&item) {
			entries.push(manga);
		}
	}
	entries
}

fn parse_chapters(json: &Value, base: &str) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	let Some(list) = json
		.get("data")
		.and_then(|data| data.get("chapters"))
		.and_then(|chapters| chapters.as_array())
	else {
		return chapters;
	};

	for (index, item) in list.iter().enumerate() {
		let Some(id) = item.get("id").and_then(|id| {
			id.as_i64()
				.map(|id| id.to_string())
				.or_else(|| id.as_str().map(|id| id.to_string()))
		}) else {
			continue;
		};
		let attributes = item.get("attributes");
		let title = attributes
			.and_then(|attributes| attributes.get("title"))
			.and_then(|title| title.as_str())
			.unwrap_or_default()
			.to_string();
		let Some(slug) = attributes
			.and_then(|attributes| attributes.get("slug"))
			.and_then(|slug| slug.as_str())
		else {
			continue;
		};
		chapters.push(Chapter {
			key: id,
			title: Some(title),
			chapter_number: Some((index + 1) as f32),
			url: Some(format!("{WWW_URL}/manga/{base}/{slug}")),
			..Default::default()
		});
	}

	chapters.reverse();
	chapters
}

impl Source for Baozimh2 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut category = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => category = value,
					_ => continue,
				},
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			let category_str = if category.is_empty() {
				String::from("manga")
			} else if category.len() <= 2 {
				format!("manga-genre/{category}")
			} else {
				format!("manga-tag/{category}")
			};
			format!("{WWW_URL}/{category_str}/page/{page}")
		} else {
			format!("{WWW_URL}/s/{}?page={page}", encode_uri(query))
		};

		let html = Request::get(url)?.html()?;
		Ok(MangaPageResult {
			entries: parse_items(&html),
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
			let (base, key_mid) = split_key(&manga.key);
			let manga_url = format!("{WWW_URL}/manga/{base}");
			let html = Request::get(&manga_url[..])?.html()?;
			let mid = html
				.select_first("#mangachapters")
				.and_then(|element| element.attr("data-mid"));

			if needs_details {
				manga.cover = html
					.select_first("meta[property='og:image']")
					.and_then(|element| element.attr("content"))
					.map(handle_cover_url);

				manga.title = html
					.select_first("title")
					.and_then(|element| element.text())
					.map(|title| title.replace(TITLE_SUFFIX, ""))
					.unwrap_or_default();

				let mut authors: Vec<String> = Vec::new();
				if let Some(list) = html.select("a[href*=author]>span") {
					for item in list {
						if let Some(author) = item.text() {
							let author = author.replace(",", "");
							if !author.is_empty() {
								authors.push(author);
							}
						}
					}
				}
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};

				manga.description = html
					.select_first(".text-medium.my-unit-md")
					.and_then(|element| element.text());

				let mut tags: Vec<String> = Vec::new();
				if let Some(list) = html.select(".py-1>a:not([href*=author])>span") {
					for item in list {
						if let Some(tag) = item.text() {
							let tag = tag
								.replace(",", "")
								.replace("热门漫画", "")
								.replace("#", "")
								.replace("热门推荐", "");
							let tag = tag.trim().to_string();
							if !tag.is_empty() {
								tags.push(tag);
							}
						}
					}
				}
				manga.tags = if tags.is_empty() { None } else { Some(tags) };

				manga.status = MangaStatus::Ongoing;
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::Webtoon;
				manga.url = Some(manga_url);
			}

			if let Some(mid) = mid.clone().or_else(|| key_mid.clone()) {
				manga.key = format!("{base}/{mid}");
			}

			if needs_chapters {
				if let Some(mid) = mid.or(key_mid) {
					let url = format!("{API_URL}/api/manga/get?mid={mid}&mode=all");
					let json: Value = Request::get(url)?
						.header("Origin", WWW_URL)
						.header("Referer", WWW_URL)
						.json_owned()?;
					manga.chapters = Some(parse_chapters(&json, &base));
				}
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let (_, Some(mid)) = split_key(&manga.key) else {
			return Ok(Vec::new());
		};

		let url = format!("{API_URL}/api/chapter/getinfo?m={mid}&c={}", chapter.key);
		let json: Value = Request::get(url)?
			.header("Origin", WWW_URL)
			.header("Referer", WWW_URL)
			.json_owned()?;

		let mut pages: Vec<Page> = Vec::new();
		let Some(list) = json
			.get("data")
			.and_then(|data| data.get("info"))
			.and_then(|info| info.get("images"))
			.and_then(|images| images.get("images"))
			.and_then(|images| images.as_array())
		else {
			return Ok(pages);
		};

		for item in list {
			let Some(url) = item.get("url").and_then(|url| url.as_str()) else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(format!("{IMG_URL}{url}")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Baozimh2 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let list = match listing.id.as_str() {
			"hots" => "hots",
			"dayup" => "dayup",
			"newss" => "newss",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};

		let url = format!("{WWW_URL}/{list}/page/{page}");
		let html = Request::get(url)?.html()?;

		Ok(MangaPageResult {
			entries: parse_items(&html),
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for Baozimh2 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(Baozimh2, ListingProvider, ImageRequestProvider);
