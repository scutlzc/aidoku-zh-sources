#![no_std]

use aidoku::{
	alloc::{
		string::{String, ToString},
		vec::Vec,
	},
	helpers::uri::encode_uri,
	imports::{html::Document, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};

const WWW_URL: &str = "https://www.55dmh.com";

const FILTER_SORT: [&str; 3] = ["click", "update", "post"];

struct Dmh;

fn last_segment(url: &str) -> Option<String> {
	url.split('/')
		.filter(|part| !part.is_empty())
		.next_back()
		.map(|part| part.to_string())
}

fn parse_list(html: &Document) -> Vec<Manga> {
	let mut entries: Vec<Manga> = Vec::new();

	let Some(items) = html.select("#dmList>ul>li") else {
		return entries;
	};

	for item in items {
		let Some(href) = item
			.select_first(".cover")
			.and_then(|element| element.attr("href"))
		else {
			continue;
		};
		let Some(id) = last_segment(&href) else {
			continue;
		};
		let cover = item
			.select_first(".cover>img")
			.and_then(|element| element.attr("src"));
		let title = item
			.select_first("dl>dt>a")
			.and_then(|element| element.text())
			.unwrap_or_default();
		entries.push(Manga {
			key: id,
			title,
			cover,
			..Default::default()
		});
	}

	entries
}

impl Source for Dmh {
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
		let mut audience = String::new();
		let mut status = String::new();
		let mut sort = FILTER_SORT[0].to_string();
		let mut ascending = false;

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => category = value,
					"audience" => audience = value,
					"status" => status = value,
					_ => continue,
				},
				FilterValue::Sort {
					index,
					ascending: asc,
					..
				} => {
					sort = FILTER_SORT
						.get(index as usize)
						.copied()
						.unwrap_or_default()
						.to_string();
					ascending = asc;
				}
				_ => continue,
			}
		}

		if ascending {
			sort = format!("-{sort}");
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			format!("{WWW_URL}/list/{category}-{audience}-{status}/{sort}/?page={page}")
		} else {
			format!(
				"{WWW_URL}/search/?keywords={}&page={page}",
				encode_uri(query)
			)
		};

		let html = Request::get(url)?.html()?;
		Ok(MangaPageResult {
			entries: parse_list(&html),
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
			let url = format!("{WWW_URL}/manhua/{key}/");
			let html = Request::get(url)?.html()?;

			if needs_details {
				manga.cover = html
					.select_first(".cover>img")
					.and_then(|element| element.attr("src"));

				manga.title = html
					.select_first("h3[class]")
					.and_then(|element| element.text())
					.map(|title| title.replace("简介：", ""))
					.unwrap_or_default();

				let mut authors: Vec<String> = Vec::new();
				if let Some(text) = html
					.select_first(".info>p:nth-child(2)>a")
					.and_then(|element| element.text())
				{
					for author in text.split(',') {
						let author = author.trim();
						if !author.is_empty() {
							authors.push(author.to_string());
						}
					}
				}
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};

				manga.description = html
					.select_first(".introduction>p:nth-child(1)")
					.and_then(|element| element.text())
					.map(|description| {
						description
							.replace("漫画简介：", "")
							.replace("介绍:", "")
							.trim()
							.to_string()
					})
					.filter(|description| !description.is_empty());

				let mut tags: Vec<String> = Vec::new();
				if let Some(list) = html.select(".info>p:nth-child(5)>a") {
					for item in list {
						if let Some(tag) = item.text() {
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
				manga.viewer = Viewer::RightToLeft;
			}

			if needs_chapters {
				let mut chapters: Vec<Chapter> = Vec::new();
				if let Some(list) = html.select("#chapter-list-1>li>a") {
					let size = list.size();
					for (index, item) in list.enumerate() {
						let Some(href) = item.attr("href") else {
							continue;
						};
						let Some(id) = last_segment(&href).map(|id| id.replace(".html", "")) else {
							continue;
						};
						let title = item.text().unwrap_or_default();
						chapters.push(Chapter {
							key: id.clone(),
							title: Some(title),
							chapter_number: Some((size - index) as f32),
							url: Some(format!("{WWW_URL}/manhua/{key}/{id}.html")),
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
		let url = format!("{WWW_URL}/manhua/{}/{}.html", manga.key, chapter.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select("#imagesOld>img") else {
			return Ok(pages);
		};

		for item in items {
			let Some(src) = item.attr("data-original") else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(src),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Dmh {
	fn get_manga_list(&self, listing: Listing, _page: i32) -> Result<MangaPageResult> {
		let url = match listing.id.as_str() {
			"recent" => format!("{WWW_URL}/main/recent/"),
			_ => {
				return Ok(MangaPageResult {
					entries: Vec::new(),
					has_next_page: false,
				})
			}
		};

		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		if let Some(items) = html.select(".updateList>ul>li>a[i]") {
			for item in items {
				let Some(href) = item.attr("href") else {
					continue;
				};
				let Some(id) = last_segment(&href) else {
					continue;
				};
				let cover = item.attr("i");
				let title = item.text().unwrap_or_default();
				entries.push(Manga {
					key: id,
					title,
					cover,
					..Default::default()
				});
			}
		}

		Ok(MangaPageResult {
			entries,
			has_next_page: false,
		})
	}
}

register_source!(Dmh, ListingProvider);
