#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://godamh.com";
const API_URL: &str = "https://api-get-v2.mgsearcher.com";
const IMG_URL: &str = "https://f40-1-4.g-mh.online";

struct GoDaManga;

impl GoDaManga {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".pb-2>a") else {
			return Ok(entries);
		};

		for item in items {
			let href = item.attr("href").unwrap_or_default();
			let key = href
				.split('/')
				.map(|part| part.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default();
			if key.is_empty() {
				continue;
			}
			let cover = handle_cover_url(
				item.select_first("div>img")
					.and_then(|img| img.attr("src"))
					.unwrap_or_default(),
			);
			let title = item
				.select_first("div>h3")
				.and_then(|title| title.text())
				.unwrap_or_default();
			entries.push(Manga {
				key,
				title,
				cover: Some(cover),
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_chapter_list(&self, manga_slug: &str, mid: &str) -> Result<Vec<Chapter>> {
		let url = format!("{API_URL}/api/manga/get?mid={mid}&mode=all");
		let json = Request::get(url)?
			.header("Origin", WWW_URL)
			.header("Referer", WWW_URL)
			.json_owned::<Value>()?;
		let list = json
			.get("data")
			.and_then(|data| data.get("chapters"))
			.and_then(|chapters| chapters.as_array());
		let mut chapters: Vec<Chapter> = Vec::new();

		let Some(list) = list else {
			return Ok(chapters);
		};

		for (index, item) in list.iter().enumerate() {
			let key = item
				.get("id")
				.and_then(|id| id.as_i64())
				.map(|id| id.to_string())
				.unwrap_or_default();
			if key.is_empty() {
				continue;
			}
			let attributes = item.get("attributes");
			let title = attributes
				.and_then(|attributes| attributes.get("title"))
				.and_then(|title| title.as_str())
				.unwrap_or_default()
				.to_string();
			let slug = attributes
				.and_then(|attributes| attributes.get("slug"))
				.and_then(|slug| slug.as_str())
				.unwrap_or_default();
			chapters.push(Chapter {
				key,
				title: Some(title),
				chapter_number: Some((index + 1) as f32),
				url: Some(format!("{WWW_URL}/manga/{manga_slug}/{slug}")),
				..Default::default()
			});
		}
		chapters.reverse();

		Ok(chapters)
	}
}

fn handle_cover_url(url: String) -> String {
	if url.contains("url=") {
		url.split("url=")
			.map(|part| part.to_string())
			.collect::<Vec<String>>()
			.pop()
			.unwrap_or_default()
			.replace("%3A", ":")
			.replace("%2F", "/")
			.replace("&w=250&q=60", "")
	} else {
		url
	}
}

impl Source for GoDaManga {
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
			if let FilterValue::Select { id, value } = filter {
				if id == "category" {
					category = value;
				}
			}
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			let category_path = if category.is_empty() {
				String::from("manga")
			} else if category.len() <= 2 {
				format!("manga-genre/{category}")
			} else {
				format!("manga-tag/{category}")
			};
			format!("{WWW_URL}/{category_path}/page/{page}")
		} else {
			format!("{WWW_URL}/s/{}?page={page}", encode_uri(query))
		};

		Ok(MangaPageResult {
			entries: GoDaManga::parse_manga_list(url)?,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let mut parts = manga.key.split('/');
		let slug = parts.next().unwrap_or_default().to_string();
		let mut mid = parts.next().map(|mid| mid.to_string());

		if needs_details || (needs_chapters && mid.is_none()) {
			let url = format!("{WWW_URL}/manga/{slug}");
			let html = Request::get(&url)?.html()?;
			mid = html
				.select_first("#mangachapters")
				.and_then(|element| element.attr("data-mid"));

			if needs_details {
				if let Some(mid) = mid.as_ref() {
					manga.key = format!("{slug}/{mid}");
				}
				manga.cover = Some(handle_cover_url(
					html.select_first("meta[property='og:image']")
						.and_then(|meta| meta.attr("content"))
						.unwrap_or_default(),
				));
				manga.title = html
					.select_first("title")
					.and_then(|title| title.text())
					.unwrap_or_default()
					.replace("-G站漫畫", "");
				let authors: Vec<String> = html
					.select("a[href*=author]>span")
					.map(|list| {
						list.map(|author| author.text().unwrap_or_default().replace(',', ""))
							.filter(|author| !author.is_empty())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.description = html
					.select_first(".text-medium.my-unit-md")
					.and_then(|description| description.text());
				let tags: Vec<String> = html
					.select(".py-1>a:not([href*=author])>span")
					.map(|list| {
						list.map(|tag| {
							tag.text()
								.unwrap_or_default()
								.replace(',', "")
								.replace("热门漫画", "")
								.replace('#', "")
								.replace("热门推荐", "")
								.trim()
								.to_string()
						})
						.filter(|tag| !tag.is_empty())
						.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.url = Some(url);
				manga.status = MangaStatus::Ongoing;
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::Webtoon;
			}
		}

		if needs_chapters {
			if let Some(mid) = mid {
				manga.chapters = Some(self.parse_chapter_list(&slug, &mid)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let mid = manga.key.split('/').nth(1).unwrap_or_default().to_string();
		let url = format!("{API_URL}/api/chapter/getinfo?m={mid}&c={}", chapter.key);
		let json = Request::get(url)?
			.header("Origin", WWW_URL)
			.header("Referer", WWW_URL)
			.json_owned::<Value>()?;
		let list = json
			.get("data")
			.and_then(|data| data.get("info"))
			.and_then(|info| info.get("images"))
			.and_then(|images| images.get("images"))
			.and_then(|images| images.as_array());
		let mut pages: Vec<Page> = Vec::new();

		let Some(list) = list else {
			return Ok(pages);
		};

		for item in list.iter() {
			let Some(path) = item.get("url").and_then(|url| url.as_str()) else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(format!("{IMG_URL}{path}")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for GoDaManga {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let path = match listing.id.as_str() {
			"hots" => "hots",
			"dayup" => "dayup",
			"newss" => "newss",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};
		let url = format!("{WWW_URL}/{path}/page/{page}");
		Ok(MangaPageResult {
			entries: GoDaManga::parse_manga_list(url)?,
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for GoDaManga {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(GoDaManga, ListingProvider, ImageRequestProvider);
