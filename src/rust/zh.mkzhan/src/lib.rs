#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{defaults::defaults_get, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Listing, ListingProvider, Manga, MangaPageResult,
	MangaStatus, Page, PageContent, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://www.mkzhan.com";
const API_URL: &str = "https://comic.mkzcdn.com";

const FILTER_ORDER: [&str; 3] = ["3", "1", "2"];

struct Mkzhan;

fn jstr(value: &Value, key: &str) -> String {
	match value.get(key) {
		Some(Value::String(string)) => string.clone(),
		Some(Value::Number(number)) => number.to_string(),
		_ => String::new(),
	}
}

impl Mkzhan {
	fn parse_manga_list(json: &Value) -> Vec<Manga> {
		let mut mangas: Vec<Manga> = Vec::new();

		let Some(list) = json
			.get("data")
			.and_then(|data| data.get("list"))
			.and_then(|list| list.as_array())
		else {
			return mangas;
		};

		for item in list {
			let cover = Some(jstr(item, "cover")).filter(|cover| !cover.is_empty());
			mangas.push(Manga {
				key: jstr(item, "comic_id"),
				title: jstr(item, "title"),
				cover,
				..Default::default()
			});
		}

		mangas
	}
}

impl Source for Mkzhan {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut theme_id = String::from("0");
		let mut finish = String::new();
		let mut audience = String::new();
		let mut copyright = String::new();
		let mut free = String::new();
		let mut order = String::from("3");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "theme" {
						theme_id = value;
					} else if id == "finish" {
						finish = value;
					} else if id == "audience" {
						audience = value;
					} else if id == "copyright" {
						copyright = value;
					} else if id == "free" {
						free = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					order = FILTER_ORDER
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
			let mut url = format!(
				"{API_URL}/search/filter/?theme_id={theme_id}&order={order}&page_num={page}&page_size=15"
			);
			if !finish.is_empty() {
				url.push_str(format!("&finish={finish}").as_str());
			}
			if !audience.is_empty() {
				url.push_str(format!("&audience={audience}").as_str());
			}
			if !copyright.is_empty() {
				url.push_str(format!("&copyright={copyright}").as_str());
			}
			if !free.is_empty() {
				url.push_str(format!("&{free}").as_str());
			}
			url
		} else {
			format!(
				"{API_URL}/search/keyword/?keyword={}&page_num={page}&page_size=20",
				encode_uri(query)
			)
		};

		let json = Request::get(url)?.json_owned::<Value>()?;

		Ok(MangaPageResult {
			entries: Mkzhan::parse_manga_list(&json),
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
			let url = format!("{WWW_URL}/{}/", manga.key);
			let html = Request::get(&url)?.html()?;
			manga.cover = html
				.select_first(".de-info__cover>img")
				.and_then(|cover| cover.attr("data-src"))
				.map(|cover| cover.replace("!cover-400", ""))
				.filter(|cover| !cover.is_empty());
			manga.title = html
				.select_first(".j-comic-title")
				.and_then(|title| title.text())
				.unwrap_or_default();
			let authors: Vec<String> = html
				.select(".comic-author>.name>a")
				.map(|list| {
					list.map(|author| author.text().unwrap_or_default())
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
				.select_first(".intro-total")
				.and_then(|description| description.text());
			let tags: Vec<String> = html
				.select_first(".comic-status>span:nth-child(1)>b")
				.and_then(|tags| tags.text())
				.unwrap_or_default()
				.split(' ')
				.map(|tag| tag.to_string())
				.collect();
			manga.tags = Some(tags);
			manga.status = match html
				.select_first(".de-chapter__title>span:nth-child(1)")
				.and_then(|status| status.text())
				.unwrap_or_default()
				.as_str()
			{
				"连载" => MangaStatus::Ongoing,
				"完结" => MangaStatus::Completed,
				_ => MangaStatus::Unknown,
			};
			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(url);
		}

		if needs_chapters {
			let json = Request::get(format!("{API_URL}/chapter/v1/?comic_id={}", manga.key))?
				.json_owned::<Value>()?;
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(list) = json.get("data").and_then(|data| data.as_array()) {
				for (index, item) in list.iter().enumerate() {
					let key = jstr(item, "chapter_id");
					chapters.push(Chapter {
						title: Some(jstr(item, "title")),
						chapter_number: Some((index + 1) as f32),
						url: Some(format!("{WWW_URL}/{}/{key}.html", manga.key)),
						key,
						..Default::default()
					});
				}
			}

			chapters.reverse();
			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let uid = defaults_get::<String>("uid").unwrap_or_default();
		let sign = defaults_get::<String>("sign").unwrap_or_default();
		let url = format!(
			"{API_URL}/chapter/content/v1/?comic_id={}&chapter_id={}&format=1&quality=1&type=1&uid={uid}&sign={sign}",
			manga.key, chapter.key
		);
		let json = Request::get(url)?.json_owned::<Value>()?;
		let mut pages: Vec<Page> = Vec::new();

		if let Some(list) = json
			.get("data")
			.and_then(|data| data.get("page"))
			.and_then(|page| page.as_array())
		{
			for item in list {
				let url = jstr(item, "image");
				if url.is_empty() {
					continue;
				}
				pages.push(Page {
					content: PageContent::url(url),
					..Default::default()
				});
			}
		}

		Ok(pages)
	}
}

impl ListingProvider for Mkzhan {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let name = match listing.id.as_str() {
			"popular" => "popular",
			"collection" => "collection",
			"latest" => "latest",
			"ascension" => "ascension",
			"ticket" => "ticket",
			"gratuity" => "gratuity",
			"score" => "score",
			"pay" => "popular/pay",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};

		let url = format!("{API_URL}/top/{name}/?type=1&page_num={page}&page_size=10");
		let json = Request::get(url)?.json_owned::<Value>()?;

		Ok(MangaPageResult {
			entries: Mkzhan::parse_manga_list(&json),
			has_next_page: true,
		})
	}
}

register_source!(Mkzhan, ListingProvider);
