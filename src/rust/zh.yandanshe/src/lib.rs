#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};

const WWW_URL: &str = "https://yandanshe.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const FILTER_SORT: [&str; 2] = ["time", "like"];

struct Yandanshe;

impl Yandanshe {
	fn parse_manga_list(url: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select("article") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item.select_first("h3>a").and_then(|a| a.attr("href")) else {
				continue;
			};
			let mut parts: Vec<String> = href
				.split("/")
				.map(|a| a.to_string())
				.filter(|a| !a.is_empty())
				.collect();
			let key = parts.pop().unwrap_or_default();
			let cover = item
				.select_first(".thumbnail>a>img")
				.and_then(|img| img.attr("src"));
			let title = item
				.select_first("h3>a")
				.and_then(|title| title.text())
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
}

impl Source for Yandanshe {
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
		let mut category = String::new();
		let mut status = String::new();
		let mut tag = String::new();
		let mut sort = String::from("time");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => category = value,
					"status" => status = value,
					"tag" => tag = value,
					_ => continue,
				},
				FilterValue::Sort { index, .. } => {
					if let Some(sort_type) = FILTER_SORT.get(index as usize) {
						sort = sort_type.to_string();
					}
				}
				_ => continue,
			}
		}

		if category.is_empty() || status.is_empty() {
			category = String::new();
			status = String::new();
		}

		let url = if query.is_empty() {
			format!(
				"{WWW_URL}/{category}{status}/page/{page}/?tag={}&sort={sort}",
				encode_uri(tag)
			)
		} else {
			format!("{WWW_URL}/page/{page}/?s={}", encode_uri(query))
		};

		Ok(MangaPageResult {
			entries: Yandanshe::parse_manga_list(&url)?,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{WWW_URL}/{}", manga.key);

		if needs_details || needs_chapters {
			let html = Request::get(&url)?.html()?;

			if needs_details {
				if let Some(title) = html
					.select_first("h1.article-title")
					.and_then(|title| title.text())
				{
					manga.title = title;
				}
				let authors: Vec<String> = html
					.select(".article-meta>.item-author")
					.map(|list| {
						list.filter_map(|author| author.text())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.description = html
					.select_first(".article-content>blockquote>p:nth-child(2)")
					.and_then(|description| description.text())
					.map(|description| description.replace("內容簡介：", ""));
				let mut categories: Vec<String> = html
					.select(".article-meta>.item-cat>a")
					.and_then(|list| list.text())
					.unwrap_or_default()
					.trim()
					.split("·")
					.map(|a| a.to_string())
					.collect();
				let mut tags: Vec<String> = html
					.select(".article-tags>.inner>a")
					.map(|list| list.filter_map(|tag| tag.text()).collect::<Vec<String>>())
					.unwrap_or_default();
				manga.status = match categories.pop().unwrap_or_default().as_str() {
					"連載" => MangaStatus::Ongoing,
					"完結" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
				categories.append(&mut tags);
				manga.tags = if categories.is_empty() {
					None
				} else {
					Some(categories)
				};
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::RightToLeft;
				manga.url = Some(url.clone());
			}

			if needs_chapters {
				let mut chapters: Vec<Chapter> = Vec::new();
				let list = html.select(".list>*");
				let size = list.as_ref().map(|list| list.size()).unwrap_or(0);

				if size == 0 {
					chapters.push(Chapter {
						key: String::from("1"),
						title: Some(String::from("第 1 话")),
						chapter_number: Some(1.0),
						url: Some(format!("{url}/1/")),
						..Default::default()
					});
				} else if let Some(list) = list {
					for (index, item) in list.enumerate() {
						let Some(text) = item.text() else {
							continue;
						};
						let key = text.trim().to_string();
						chapters.push(Chapter {
							key: key.clone(),
							title: Some(format!("第 {key} 话")),
							chapter_number: Some((index + 1) as f32),
							url: Some(format!("{url}/{key}/")),
							..Default::default()
						});
					}
					chapters.reverse();
				}

				manga.chapters = Some(chapters);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/{}/{}/", manga.key, chapter.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		if let Some(items) = html.select(".article-content>p>img") {
			for item in items {
				let Some(src) = item.attr("data-src") else {
					continue;
				};
				pages.push(Page {
					content: PageContent::url(src.trim().to_string()),
					..Default::default()
				});
			}
		}

		Ok(pages)
	}
}

impl ImageRequestProvider for Yandanshe {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("Referer", WWW_URL)
			.header("User-Agent", UA))
	}
}

register_source!(Yandanshe, ImageRequestProvider);
