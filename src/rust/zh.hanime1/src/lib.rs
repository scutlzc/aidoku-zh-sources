#![no_std]

use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult, Page,
	PageContent, Result, Source, Viewer,
};

const WWW_URL: &str = "https://hanime1.me";

struct Hanime1;

impl Hanime1 {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".comic-rows-videos-div>a") else {
			return Ok(entries);
		};

		for item in items {
			let key = item
				.attr("href")
				.unwrap_or_default()
				.split('/')
				.map(|part| part.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default();
			if key.is_empty() {
				continue;
			}
			let cover = item
				.select_first("img")
				.and_then(|img| img.attr("data-srcset"))
				.unwrap_or_default();
			let title = item
				.select_first("div>.comic-rows-videos-title")
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
}

impl Source for Hanime1 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let query = query.unwrap_or_default();
		let url = format!(
			"{WWW_URL}/comics/search?query={}&page={page}",
			encode_uri(query)
		);

		Ok(MangaPageResult {
			entries: Hanime1::parse_manga_list(url)?,
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
			let html = Request::get(&url)?.html()?;

			manga.cover = Some(
				html.select_first("meta[property='og:image']")
					.and_then(|meta| meta.attr("content"))
					.unwrap_or_default(),
			);
			manga.title = html
				.select("h3[class^=title]>span")
				.map(|spans| {
					spans
						.map(|span| span.text().unwrap_or_default().trim().to_string())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default()
				.join(" ");
			let author = html
				.select_first("a[href*=artists]>div[style]")
				.and_then(|author| author.text())
				.unwrap_or_default();
			manga.authors = if author.is_empty() {
				None
			} else {
				Some(vec![author])
			};
			manga.description = html
				.select_first("meta[property='og:description']")
				.and_then(|meta| meta.attr("content"))
				.map(|description| description.trim().to_string());
			let tags = html
				.select("a[href*=tags]>div[style]")
				.map(|tags| {
					tags.map(|tag| tag.text().unwrap_or_default().trim().to_string())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.url = Some(url);
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::RightToLeft;
		}

		if needs_chapters {
			manga.chapters = Some(vec![Chapter {
				key: manga.key.clone(),
				title: Some(String::from("第 1 话")),
				chapter_number: Some(1.0),
				url: Some(format!("{WWW_URL}/comic/{}/1", manga.key)),
				..Default::default()
			}]);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, _chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/comic/{}", manga.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select(".comics-panel-margin>a>img") else {
			return Ok(pages);
		};

		for item in items {
			let Some(image) = item.attr("data-srcset") else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(image.replace("//t", "//i").replace("t.", ".")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Hanime1 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular-today" => "popular-today",
			"popular-week" => "popular-week",
			"popular" => "popular",
			_ => {
				return Ok(MangaPageResult {
					entries: Hanime1::parse_manga_list(format!(
						"{WWW_URL}/comics/search?query=&page={page}"
					))?,
					has_next_page: true,
				});
			}
		};
		Ok(MangaPageResult {
			entries: Hanime1::parse_manga_list(format!(
				"{WWW_URL}/comics/search?sort={sort}&query=&page={page}"
			))?,
			has_next_page: true,
		})
	}
}

register_source!(Hanime1, ListingProvider);
