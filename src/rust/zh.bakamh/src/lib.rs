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

const WWW_URL: &str = "https://bakamh.com";

struct Bakamh;

fn last_segment(url: &str) -> Option<String> {
	let url = url.trim();
	url.split('/')
		.filter(|part| !part.is_empty())
		.next_back()
		.map(|part| part.to_string())
}

fn parse_page_item_detail(item: &Element) -> Option<Manga> {
	let href = item.select_first(".item-thumb>a")?.attr("href")?;
	let key = last_segment(&href)?;
	let cover = item
		.select_first(".item-thumb>a>img")
		.and_then(|element| element.attr("src"))
		.map(|src| src.replace("-175x238", ""));
	let title = item
		.select_first(".item-summary>.post-title>h3>a")
		.and_then(|element| element.text())
		.unwrap_or_default();
	Some(Manga {
		key,
		title,
		cover,
		..Default::default()
	})
}

fn parse_tab_item(item: &Element) -> Option<Manga> {
	let href = item.select_first(".col-4>.tab-thumb>a")?.attr("href")?;
	let key = last_segment(&href)?;
	let cover = item
		.select_first(".col-4>.tab-thumb>a>img")
		.and_then(|element| element.attr("src"))
		.map(|src| src.replace("-193x278", ""));
	let title = item
		.select_first(".col-8>.tab-summary>.post-title>h3>a")
		.and_then(|element| element.text())
		.unwrap_or_default();
	Some(Manga {
		key,
		title,
		cover,
		..Default::default()
	})
}

fn parse_item_list(html: &Document, selector: &str) -> Vec<Manga> {
	let mut entries: Vec<Manga> = Vec::new();
	let Some(items) = html.select(selector) else {
		return entries;
	};
	for item in items {
		let parsed = if selector == ".c-tabs-item__content" {
			parse_tab_item(&item)
		} else {
			parse_page_item_detail(&item)
		};
		if let Some(manga) = parsed {
			entries.push(manga);
		}
	}
	entries
}

fn meta_content(html: &Document, selector: &str) -> Option<String> {
	html.select_first(selector)
		.and_then(|element| element.attr("content"))
}

impl Source for Bakamh {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut finish = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"finish" => finish = value,
					_ => continue,
				},
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let is_search = !query.is_empty();
		let url = if !is_search {
			format!("{WWW_URL}/{finish}/page/{page}/")
		} else {
			format!(
				"{WWW_URL}/page/{page}/?s={}&post_type=wp-manga",
				encode_uri(query)
			)
		};

		let html = Request::get(url)?.html()?;
		let selector = if !is_search {
			".page-item-detail"
		} else {
			".c-tabs-item__content"
		};

		Ok(MangaPageResult {
			entries: parse_item_list(&html, selector),
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
			let url = format!("{WWW_URL}/manga/{key}/");
			let html = Request::get(url.clone())?.html()?;

			if needs_details {
				manga.cover = meta_content(&html, "meta[property='og:image']");
				manga.title = html
					.select_first("meta[property='og:title']")
					.and_then(|element| element.text())
					.unwrap_or_default();
				if manga.title.is_empty() {
					manga.title =
						meta_content(&html, "meta[property='og:title']").unwrap_or_default();
				}

				let mut authors: Vec<String> = Vec::new();
				if let Some(list) = html.select(".author-content>a") {
					for item in list {
						if let Some(author) = item.text() {
							authors.push(author);
						}
					}
				}
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};

				let len = html
					.select(".post-content>div")
					.map_or(0, |list| list.size());

				manga.description = html
					.select_first(&format!(".post-content>div:nth-child({len})>div>p"))
					.and_then(|element| element.text())
					.filter(|description| !description.is_empty());

				let mut tags: Vec<String> = Vec::new();
				if let Some(list) = html.select(".tags-content>a") {
					for item in list {
						if let Some(tag) = item.text() {
							tags.push(tag);
						}
					}
				}
				manga.tags = if tags.is_empty() { None } else { Some(tags) };

				let status = html
					.select_first(&format!(
						".post-content>div:nth-child({})>.summary-content",
						len.saturating_sub(2)
					))
					.and_then(|element| element.text())
					.map(|status| status.trim().to_string())
					.unwrap_or_default();
				manga.status = match status.as_str() {
					"OnGoing" => MangaStatus::Ongoing,
					"Completed" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};

				manga.url = Some(url);
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::Webtoon;
			}

			if needs_chapters {
				let mut chapters: Vec<Chapter> = Vec::new();
				if let Some(list) = html.select(".wp-manga-chapter>a") {
					let len = list.size();
					for (index, item) in list.enumerate() {
						let Some(url) = item.attr("href") else {
							continue;
						};
						let Some(id) = last_segment(&url) else {
							continue;
						};
						let title = item.text().unwrap_or_default().trim().to_string();
						chapters.push(Chapter {
							key: id,
							title: Some(title),
							chapter_number: Some((len - index) as f32),
							url: Some(url),
							..Default::default()
						});
					}
				}
				manga.chapters = Some(chapters);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/manga/{}/{}", manga.key, chapter.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select("img[id]") else {
			return Ok(pages);
		};

		for item in items {
			let Some(src) = item.attr("src") else {
				continue;
			};
			let src = src.trim().to_string();
			if src.is_empty() {
				continue;
			}
			pages.push(Page {
				content: PageContent::url(src),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Bakamh {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let path = match listing.id.as_str() {
			"new" => "newmanga",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};

		let url = format!("{WWW_URL}/{path}/page/{page}/");
		let html = Request::get(url)?.html()?;

		Ok(MangaPageResult {
			entries: parse_item_list(&html, ".page-item-detail"),
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for Bakamh {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(Bakamh, ListingProvider, ImageRequestProvider);
