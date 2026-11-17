#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://www.ho5ho.com";
const MANGA_URL: &str = "https://www.ho5ho.com/%E4%B8%AD%E5%AD%97h%E6%BC%AB";

const FILTER_SORT: [&str; 3] = ["latest", "rating", "views"];

struct Ho5Ho;

impl Ho5Ho {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select("div[class*='c-image-hover']>a") else {
			return Ok(entries);
		};

		for item in items {
			let list: Vec<String> = item
				.attr("href")
				.unwrap_or_default()
				.split('/')
				.filter(|part| !part.is_empty())
				.map(|part| part.to_string())
				.collect();
			let Some(key) = list.get(3) else {
				continue;
			};
			let cover = item
				.select_first("img")
				.and_then(|img| img.attr("data-src"))
				.unwrap_or_default();
			entries.push(Manga {
				key: key.clone(),
				title: item.attr("title").unwrap_or_default(),
				cover: Some(encode_uri(cover)),
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_chapter_list(html: &aidoku::imports::html::Document) -> Result<Vec<Chapter>> {
		let mut chapters: Vec<Chapter> = Vec::new();

		if let Some(items) = html.select(".wp-manga-chapter>a") {
			for (index, item) in items.enumerate() {
				let url = item.attr("href").unwrap_or_default();
				let key = url
					.split('/')
					.filter(|part| !part.is_empty())
					.map(|part| part.to_string())
					.collect::<Vec<String>>()
					.pop()
					.unwrap_or_default();
				if key.is_empty() {
					continue;
				}
				let title = item.text().unwrap_or_default();
				chapters.push(Chapter {
					key,
					title: Some(title),
					chapter_number: Some((index + 1) as f32),
					url: Some(url),
					..Default::default()
				});
			}
		}
		chapters.reverse();

		Ok(chapters)
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

impl Source for Ho5Ho {
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
		let mut sort = String::from("latest");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "category" {
						category = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					sort = FILTER_SORT
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
			if category.is_empty() {
				format!("{WWW_URL}/page/{page}/?m_orderby={sort}")
			} else {
				format!(
					"{WWW_URL}/manga-genre/{}/page/{page}/?m_orderby={sort}",
					encode_uri(category)
				)
			}
		} else {
			format!(
				"{WWW_URL}/page/{page}/?s={}&post_type=wp-manga",
				encode_uri(query)
			)
		};

		Ok(MangaPageResult {
			entries: Ho5Ho::parse_manga_list(url)?,
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
			let url = format!("{MANGA_URL}/{}/", manga.key);
			let html = Request::get(&url)?.html()?;

			if needs_details {
				let cover = html
					.select_first(".summary_image>a>img")
					.and_then(|img| img.attr("data-src"))
					.unwrap_or_default()
					.replace("193x278", "175x238");
				manga.cover = Some(encode_uri(cover));
				manga.title = html
					.select_first(".post-title>h1")
					.and_then(|title| title.text())
					.unwrap_or_default();
				let authors = html
					.select(".author-content>a")
					.map(|authors| {
						authors
							.map(|author| author.text().unwrap_or_default())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.description = html
					.select(".description-summary>div>p")
					.map(|paragraphs| {
						paragraphs
							.map(|paragraph| paragraph.text().unwrap_or_default())
							.collect::<Vec<String>>()
					})
					.map(|paragraphs| paragraphs.join("\n"));
				let tags = html
					.select(".genres-content>a")
					.map(|tags| {
						tags.map(|tag| tag.text().unwrap_or_default())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.status = match html
					.select_first(".post-status>div:nth-child(2)>.summary-content")
					.and_then(|status| status.text())
					.unwrap_or_default()
					.trim()
				{
					"OnGoing" => MangaStatus::Ongoing,
					"Completed" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
				manga.url = Some(url);
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::RightToLeft;
			}

			if needs_chapters {
				manga.chapters = Some(Ho5Ho::parse_chapter_list(&html)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{MANGA_URL}/{}/{}/", manga.key, chapter.key);
		let text = Request::get(url)?.string()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(list) = substring_after(&text, "var chapter_preloaded_images = ")
			.and_then(|text| substring_before(text, ", chapter_images_per_page ="))
		else {
			return Ok(pages);
		};

		let Ok(list) = serde_json::from_str::<Value>(list) else {
			return Ok(pages);
		};
		let Some(list) = list.as_array() else {
			return Ok(pages);
		};

		for item in list.iter() {
			let Some(image) = item.as_str() else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(image.to_string()),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ImageRequestProvider for Ho5Ho {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(Ho5Ho, ImageRequestProvider);
