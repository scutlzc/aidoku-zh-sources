#![no_std]

use aidoku::{
	alloc::{
		string::{String, ToString},
		vec::Vec,
	},
	helpers::uri::encode_uri,
	imports::{html::Html, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use encoding_rs::BIG5;

const WWW_URL: &str = "https://www.cartoonmad.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

fn gen_request(url: String) -> Result<Request> {
	Ok(Request::get(url)?.header("User-Agent", UA))
}

fn handle_img_url(url: String) -> String {
	if url.starts_with("http") {
		return url;
	}
	format!("https:{}", url)
}

fn last_segment(url: &str) -> Option<String> {
	url.split("/")
		.map(|part| part.to_string())
		.filter(|part| !part.is_empty())
		.collect::<Vec<String>>()
		.pop()
}

fn parse_big5(data: &[u8]) -> Result<aidoku::imports::html::Document> {
	let decoded = BIG5.decode(data).0;
	Ok(Html::parse(decoded.as_bytes())?)
}

struct Cartoonmad;

impl Cartoonmad {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = gen_request(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".comic_prev") else {
			return Ok(entries);
		};

		for item in items {
			let Some(key) = item
				.select_first(".a1")
				.and_then(|link| link.attr("href"))
				.and_then(|href| last_segment(&href))
				.map(|segment| segment.replace(".html", ""))
			else {
				continue;
			};
			let cover = format!(
				"{}{}",
				WWW_URL,
				item.select_first("img")
					.and_then(|img| img.attr("src"))
					.unwrap_or_default()
			);
			let title = item
				.select_first(".covertxt+a")
				.and_then(|link| link.attr("title"))
				.unwrap_or_default();

			entries.push(Manga {
				key,
				cover: Some(cover),
				title,
				..Default::default()
			});
		}

		Ok(entries)
	}
}

impl Source for Cartoonmad {
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

		let url = if query.is_empty() {
			format!("{WWW_URL}/m/?page={page}")
		} else {
			format!("{WWW_URL}/m/?keyword={}", encode_uri(query.clone()))
		};
		let has_more = query.is_empty();

		Ok(MangaPageResult {
			entries: Cartoonmad::parse_manga_list(url)?,
			has_next_page: has_more,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}

		let url = format!("{WWW_URL}/m/comic/{}.html", manga.key);
		let data = gen_request(url.clone())?.data()?;
		let html = parse_big5(&data)?;

		if needs_details {
			manga.cover = Some(format!(
				"{}{}",
				WWW_URL,
				html.select_first("link[rel='image_src']")
					.and_then(|link| link.attr("href"))
					.unwrap_or_default()
			));

			let title = html
				.select_first("meta[name='keywords']")
				.and_then(|meta| meta.attr("content"))
				.unwrap_or_default()
				.split(",")
				.map(|part| part.trim().to_string())
				.filter(|part| !part.is_empty())
				.collect::<Vec<String>>()
				.first()
				.map(|title| title.to_string());
			if let Some(title) = title {
				manga.title = title;
			}

			let author = html
				.select("td[height='24'")
				.and_then(|list| list.get(1))
				.and_then(|cell| cell.untrimmed_text())
				.map(|text| text.trim().to_string())
				.map(|text| text.replace("作者：", ""));
			manga.authors = match author {
				Some(author) if !author.is_empty() => {
					let mut authors: Vec<String> = Vec::new();
					authors.push(author);
					Some(authors)
				}
				_ => None,
			};

			manga.description = html
				.select("td[style='font-size:11pt;']")
				.and_then(|list| list.get(2))
				.and_then(|cell| cell.untrimmed_text());

			let mut categories: Vec<String> = Vec::new();
			if let Some(links) = html.select("a[href*='tkey']") {
				for link in links {
					if let Some(text) = link.untrimmed_text() {
						categories.push(text);
					}
				}
			}
			manga.tags = if categories.is_empty() {
				None
			} else {
				Some(categories)
			};

			manga.url = Some(url);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::RightToLeft;
		}

		if needs_chapters {
			let mut chapters: Vec<Chapter> = Vec::new();
			if let Some(list) = html
				.select("td[style='font-size:11pt;']")
				.and_then(|list| list.get(3))
				.and_then(|cell| cell.select("td a"))
			{
				for (index, item) in list.enumerate() {
					let Some(key) = item
						.attr("href")
						.and_then(|href| last_segment(&href))
						.map(|segment| segment.replace(".html", ""))
					else {
						continue;
					};
					let title = item
						.untrimmed_text()
						.map(|text| text.trim().to_string())
						.unwrap_or_default();
					let chapter_url = format!("{WWW_URL}/m/comic/{key}.html");

					chapters.push(Chapter {
						key,
						title: Some(title),
						chapter_number: Some((index + 1) as f32),
						url: Some(chapter_url),
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
		let url = format!("{WWW_URL}/m/comic/{}.html", chapter.key);
		let data = gen_request(url)?.data()?;
		let html = parse_big5(&data)?;

		let img_url = handle_img_url(
			html.select_first("img[onload]")
				.and_then(|img| img.attr("src"))
				.unwrap_or_default(),
		);
		let length = html
			.select(".pages:not(:has(img))")
			.and_then(|list| list.last())
			.and_then(|element| element.untrimmed_text())
			.map(|text| text.trim().parse::<i32>().unwrap_or_default())
			.unwrap_or_default();

		let mut pages: Vec<Page> = Vec::new();

		for index in 0..length {
			let url = img_url.replace("001.", &format!("{:03}.", index + 1));
			pages.push(Page {
				content: PageContent::url(url),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Cartoonmad {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let act = match listing.id.as_str() {
			"latest" => "1",
			"popular" => "2",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};

		let url = format!("{WWW_URL}/m/?act={act}&page={page}");

		Ok(MangaPageResult {
			entries: Cartoonmad::parse_manga_list(url)?,
			has_next_page: true,
		})
	}
}

register_source!(Cartoonmad, ListingProvider);
