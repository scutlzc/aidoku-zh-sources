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
	Chapter, ContentRating, FilterValue, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://www.baozimh.com";
const IMG_URL: &str = "https://static-tw.baozimh.com";

struct Baozimh1;

fn last_segment(url: &str) -> Option<String> {
	url.split('/')
		.filter(|part| !part.is_empty())
		.next_back()
		.map(|part| part.to_string())
}

fn meta_content(html: &Document, selector: &str) -> Option<String> {
	html.select_first(selector)
		.and_then(|element| element.attr("content"))
}

fn parse_card(item: &Element) -> Option<Manga> {
	let href = item.select_first(".comics-card__info")?.attr("href")?;
	let key = last_segment(&href)?;
	let cover = item
		.select_first(".comics-card__poster>amp-img")
		.and_then(|element| element.attr("src"));
	let title = item
		.select_first(".comics-card__title")
		.and_then(|element| element.text())
		.map(|title| title.trim().to_string())
		.unwrap_or_default();
	Some(Manga {
		key,
		title,
		cover,
		..Default::default()
	})
}

fn chapter_prefix(chapter_id: &str) -> String {
	chapter_id
		.split('_')
		.take(2)
		.collect::<Vec<&str>>()
		.join("_")
}

impl Source for Baozimh1 {
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
		let mut region = String::new();
		let mut status = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => category = value,
					"region" => region = value,
					"status" => status = value,
					_ => continue,
				},
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let has_next_page = query.is_empty();
		let mut entries: Vec<Manga> = Vec::new();

		if query.is_empty() {
			let url = format!(
				"{WWW_URL}/api/bzmhq/amp_comic_list?type={category}&region={region}&state={status}&page={page}&language=tw"
			);
			let json: Value = Request::get(url)?.json_owned()?;
			if let Some(list) = json.get("items").and_then(|items| items.as_array()) {
				for item in list {
					let Some(id) = item.get("comic_id").and_then(|id| id.as_str()) else {
						continue;
					};
					let topic_img = item
						.get("topic_img")
						.and_then(|img| img.as_str())
						.unwrap_or_default();
					let cover = format!("{IMG_URL}/cover/{topic_img}?w=285&h=375&q=100");
					let title = item
						.get("name")
						.and_then(|name| name.as_str())
						.unwrap_or_default()
						.to_string();
					entries.push(Manga {
						key: id.to_string(),
						title,
						cover: Some(cover),
						..Default::default()
					});
				}
			}
		} else {
			let url = format!("{WWW_URL}/search/?q={}", encode_uri(query));
			let html = Request::get(url)?.html()?;
			if let Some(list) = html.select(".pure-g>.comics-card") {
				for item in list {
					if let Some(manga) = parse_card(&item) {
						entries.push(manga);
					}
				}
			}
		}

		Ok(MangaPageResult {
			entries,
			has_next_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details || needs_chapters {
			let manga_url = format!("{WWW_URL}/comic/{}", manga.key);
			let html = Request::get(&manga_url[..])?.html()?;

			if needs_details {
				if let Some(read_url) = meta_content(&html, "meta[name='og:novel:read_url']") {
					if let Some(key) = last_segment(&read_url) {
						manga.key = key;
					}
				}

				manga.cover = meta_content(&html, "meta[name='og:image'");
				manga.title =
					meta_content(&html, "meta[name='og:novel:book_name']").unwrap_or_default();
				manga.authors = meta_content(&html, "meta[name='og:novel:author']")
					.filter(|author| !author.is_empty())
					.map(|author| {
						let mut authors: Vec<String> = Vec::new();
						authors.push(author);
						authors
					});

				manga.description =
					meta_content(&html, "meta[name='og:description']").map(|description| {
						description
							.split(',')
							.skip(2)
							.collect::<Vec<&str>>()
							.join(", ")
					});

				let mut tags: Vec<String> = Vec::new();
				if let Some(categories) = meta_content(&html, "meta[name='og:novel:category']") {
					for category in categories.split(',') {
						if !category.starts_with("types") {
							tags.push(category.to_string());
						}
					}
				}
				manga.tags = if tags.is_empty() { None } else { Some(tags) };

				manga.status = match meta_content(&html, "meta[name='og:novel:status']")
					.unwrap_or_default()
					.as_str()
				{
					"連載中" => MangaStatus::Ongoing,
					"已完結" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};

				manga.url = Some(manga_url.clone());
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::RightToLeft;
			}

			if needs_chapters {
				let key = manga.key.clone();
				let mut chapters: Vec<Chapter> = Vec::new();
				if let Some(list) = html.select("div[id^='chapter']>div>a") {
					for (index, item) in list.enumerate() {
						let Some(href) = item.attr("href") else {
							continue;
						};
						let chapter_id: String = href
							.split('&')
							.skip(1)
							.filter_map(|part| part.split('=').nth(1))
							.map(|part| part.to_string())
							.collect::<Vec<String>>()
							.join("_");
						if chapter_id.is_empty() {
							continue;
						}
						let title = item
							.select_first("div>span")
							.and_then(|element| element.text())
							.unwrap_or_default();
						let url = format!("{WWW_URL}/comic/chapter/{key}/{chapter_id}.html");
						chapters.push(Chapter {
							key: chapter_id,
							title: Some(title),
							chapter_number: Some((index + 1) as f32),
							url: Some(url),
							..Default::default()
						});
					}
				}
				chapters.reverse();
				manga.chapters = Some(chapters);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let mut pages: Vec<Page> = Vec::new();
		let mut current_chapter_id = chapter.key;

		loop {
			let url = format!(
				"{WWW_URL}/comic/chapter/{}/{}.html",
				manga.key, current_chapter_id
			);
			let html = Request::get(url)?.html()?;

			let next_chapter_id = html
				.select_first("#next-chapter")
				.and_then(|element| element.attr("href"))
				.and_then(|href| last_segment(&href))
				.map(|id| id.replace(".html", ""));

			if let Some(list) = html.select("amp-img[id^='chapter-img']") {
				for item in list {
					let Some(src) = item.attr("src") else {
						continue;
					};
					pages.push(Page {
						content: PageContent::url(src.replace("fcomic", "scomic")),
						..Default::default()
					});
				}
			}

			let Some(next_chapter_id) = next_chapter_id else {
				break;
			};
			if !next_chapter_id.starts_with(&chapter_prefix(&current_chapter_id)) {
				break;
			}

			current_chapter_id = next_chapter_id;
		}

		Ok(pages)
	}
}

register_source!(Baozimh1);
