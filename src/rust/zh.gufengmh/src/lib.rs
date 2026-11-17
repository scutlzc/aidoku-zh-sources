#![no_std]

use aidoku::{
	alloc::vec,
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{html::Document, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://www.gufengmh.com";
const IMG_URL: &str = "https://res1.xiaoqinre.com";

const FILTER_SORT: [&str; 3] = ["post", "update", "click"];

struct GuFengMh;

impl GuFengMh {
	fn parse_manga_list(url: String, selector: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(selector) else {
			return Ok(entries);
		};

		for item in items {
			let href = item
				.select_first(".cover")
				.and_then(|a| a.attr("href"))
				.unwrap_or_default();
			let key = href
				.split('/')
				.filter(|part| !part.is_empty())
				.map(|part| part.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default();
			if key.is_empty() {
				continue;
			}
			let cover = item
				.select_first(".cover>img")
				.and_then(|img| img.attr("src"))
				.unwrap_or_default();
			let title = item
				.select_first(".ell>a")
				.and_then(|title| title.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			entries.push(Manga {
				key,
				title,
				cover: Some(cover),
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_chapter_list(html: &Document, manga_id: &str) -> Result<Vec<Chapter>> {
		let mut chapters: Vec<Chapter> = Vec::new();

		if let Some(items) = html.select("#chapter-list-1>li>a") {
			for (index, item) in items.enumerate() {
				let href = item.attr("href").unwrap_or_default();
				let key = href
					.split('/')
					.map(|part| part.to_string())
					.collect::<Vec<String>>()
					.pop()
					.unwrap_or_default()
					.replace(".html", "");
				if key.is_empty() {
					continue;
				}
				let title = item
					.select_first("span")
					.and_then(|title| title.text())
					.unwrap_or_default();
				chapters.push(Chapter {
					key: key.clone(),
					title: Some(title),
					chapter_number: Some((index + 1) as f32),
					url: Some(format!("{WWW_URL}/manhua/{manga_id}/{key}.html")),
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

impl Source for GuFengMh {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut genre = String::new();
		let mut region = String::new();
		let mut status = String::new();
		let mut sort = String::from("click");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "genre" {
						genre = value;
					} else if id == "region" {
						region = value;
					} else if id == "status" {
						status = value;
					}
				}
				FilterValue::Sort {
					index, ascending, ..
				} => {
					sort = FILTER_SORT
						.get(index as usize)
						.copied()
						.unwrap_or_default()
						.to_string();
					if ascending {
						sort = format!("-{sort}");
					}
				}
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			format!("{WWW_URL}/list/{genre}-{region}-{status}/{sort}/{page}/")
		} else {
			format!(
				"{WWW_URL}/search/?keywords={}&page={page}",
				encode_uri(query)
			)
		};

		Ok(MangaPageResult {
			entries: GuFengMh::parse_manga_list(url, ".book-list>li")?,
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
			let url = format!("{WWW_URL}/manhua/{}/", manga.key);
			let html = Request::get(&url)?.html()?;

			if needs_details {
				manga.cover = Some(
					html.select_first(".book-cover>.cover>img")
						.and_then(|img| img.attr("src"))
						.unwrap_or_default(),
				);
				manga.title = html
					.select_first(".book-title>h1>span")
					.and_then(|title| title.text())
					.unwrap_or_default();
				let author = html
					.select_first("a[href*='author']")
					.and_then(|author| author.text())
					.unwrap_or_default();
				manga.authors = if author.is_empty() {
					None
				} else {
					Some(vec![author])
				};
				manga.description = html
					.select_first("#intro-cut>p")
					.and_then(|p| p.text())
					.map(|text| {
						text.replace("漫画简介：", "")
							.replace("介绍:", "")
							.trim()
							.to_string()
					});
				let tags: Vec<String> = html
					.select(".detail-list>li:nth-child(2)>span:nth-child(1)>a")
					.map(|list| {
						list.map(|tag| tag.text().unwrap_or_default().trim().to_string())
							.filter(|tag| !tag.is_empty())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.status = match html
					.select_first(".detail-list>li:nth-child(1)>span:nth-child(1)>a")
					.and_then(|status| status.text())
					.unwrap_or_default()
					.as_str()
				{
					"已完结" => MangaStatus::Completed,
					"连载中" => MangaStatus::Ongoing,
					_ => MangaStatus::Unknown,
				};
				manga.url = Some(url);
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::Webtoon;
			}

			if needs_chapters {
				manga.chapters = Some(GuFengMh::parse_chapter_list(&html, &manga.key)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/manhua/{}/{}.html", manga.key, chapter.key);
		let text = Request::get(url)?.string()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(list) = substring_after(&text, "var chapterImages = ")
			.and_then(|text| substring_before(text, ";"))
		else {
			return Ok(pages);
		};
		let Some(path) = substring_after(&text, "var chapterPath = ")
			.and_then(|text| substring_before(text, ";"))
			.map(|path| path.replace('"', ""))
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
				content: PageContent::url(format!("{IMG_URL}/{path}{image}")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for GuFengMh {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let name = match listing.id.as_str() {
			"popularity" => "popularity",
			"popularity-daily" => "popularity-daily",
			"popularity-weekly" => "popularity-weekly",
			"popularity-monthly" => "popularity-monthly",
			"click" => "click",
			"click-daily" => "click-daily",
			"click-weekly" => "click-weekly",
			"click-monthly" => "click-monthly",
			"subscribe" => "subscribe",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};
		let url = format!("{WWW_URL}/rank/{name}/");
		Ok(MangaPageResult {
			entries: GuFengMh::parse_manga_list(url, ".rank-list>li")?,
			has_next_page: false,
		})
	}
}

register_source!(GuFengMh, ListingProvider);
