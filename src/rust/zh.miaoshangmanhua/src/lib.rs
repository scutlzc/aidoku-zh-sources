#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};

const WWW_URL: &str = "https://www.miaoshangmanhua.cc";

const FILTER_ORDER: [&str; 2] = ["hits", "addtime"];

struct MiaoShangManHua;

fn handle_cover_url(url: String) -> String {
	if url.contains("url=") {
		url.split("url=")
			.map(|part| part.to_string())
			.collect::<Vec<String>>()
			.pop()
			.unwrap_or_default()
			.replace("&w=320&q=60", "")
	} else {
		url
	}
}

impl MiaoShangManHua {
	fn parse_manga_list(url: String, selector: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(selector) else {
			return Ok(entries);
		};

		for item in items {
			let key = item
				.select_first("li:nth-child(1)>a")
				.and_then(|a| a.attr("href"))
				.unwrap_or_default()
				.split('/')
				.map(|part| part.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default()
				.replace(".html", "");
			if key.is_empty() {
				continue;
			}
			let cover = handle_cover_url(
				item.select_first("li:nth-child(1)>a>img")
					.and_then(|img| img.attr("src"))
					.unwrap_or_default(),
			);
			let title = item
				.select_first(".title>a")
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

	fn parse_ranking(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".cy_ph_list_mh>ul") else {
			return Ok(entries);
		};

		for item in items {
			let key = item
				.select_first(".pic>a")
				.and_then(|a| a.attr("href"))
				.unwrap_or_default()
				.split('/')
				.map(|part| part.to_string())
				.collect::<Vec<String>>()
				.pop()
				.unwrap_or_default()
				.replace(".html", "");
			if key.is_empty() {
				continue;
			}
			let cover = handle_cover_url(
				item.select_first(".pic>a>img")
					.and_then(|img| img.attr("src"))
					.unwrap_or_default(),
			);
			let title = item
				.select_first(".title>a")
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

	fn parse_chapter_list(
		html: &aidoku::imports::html::Document,
		manga_id: &str,
	) -> Result<Vec<Chapter>> {
		let mut chapters: Vec<Chapter> = Vec::new();
		let mut links: Vec<(String, String)> = Vec::new();

		if let Some(items) = html.select("#mh-chapter-list-ol-0>li>a") {
			for item in items {
				let key = item
					.attr("href")
					.unwrap_or_default()
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
					.select_first("p")
					.and_then(|p| p.text())
					.unwrap_or_default()
					.trim()
					.to_string();
				links.push((key, title));
			}
		}

		let len = links.len();
		for (index, (key, title)) in links.into_iter().enumerate() {
			chapters.push(Chapter {
				key: key.clone(),
				title: Some(title),
				chapter_number: Some((len - index) as f32),
				url: Some(format!("{WWW_URL}/comic/{manga_id}/{key}.html")),
				..Default::default()
			});
		}

		Ok(chapters)
	}
}

impl Source for MiaoShangManHua {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut tag = String::new();
		let mut order = String::from("hits");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "tag" {
						tag = value;
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
			if tag.is_empty() {
				format!("{WWW_URL}/category/list/1/order/{order}/page/{page}")
			} else {
				format!("{WWW_URL}/category/list/1/order/{order}/tags/{tag}/page/{page}")
			}
		} else {
			format!("{WWW_URL}/search/{}/{page}", encode_uri(query))
		};

		Ok(MangaPageResult {
			entries: MiaoShangManHua::parse_manga_list(url, ".cy_list_mh>ul")?,
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
			let url = format!("{WWW_URL}/comic/{}.html", manga.key);
			let html = Request::get(&url)?.html()?;

			if needs_details {
				manga.cover = Some(handle_cover_url(
					html.select_first(".cy_info_cover>a>img")
						.and_then(|img| img.attr("src"))
						.unwrap_or_default(),
				));
				manga.title = html
					.select_first(".cy_title>h1")
					.and_then(|title| title.text())
					.unwrap_or_default();
				let authors = html
					.select(".cy_intro_l>.cy_xinxi:nth-child(4)>span:nth-child(1)>a")
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
					.select_first("#comic-description")
					.and_then(|description| description.text());
				let tags = html
					.select(".cy_intro_l>.cy_xinxi:nth-child(5)>span:nth-child(1)>a")
					.map(|tags| {
						tags.map(|tag| tag.text().unwrap_or_default())
							.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.status = match html
					.select_first(".cy_intro_l>.cy_xinxi:nth-child(4)>span:nth-child(2)>font")
					.and_then(|status| status.text())
					.unwrap_or_default()
					.trim()
				{
					"连载中" => MangaStatus::Ongoing,
					"已完结" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
				manga.url = Some(url);
				manga.content_rating = ContentRating::Safe;
				manga.viewer = Viewer::Webtoon;
			}

			if needs_chapters {
				manga.chapters = Some(MiaoShangManHua::parse_chapter_list(&html, &manga.key)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/comic/{}/{}.html", manga.key, chapter.key);
		let html = Request::get(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(text) = html
			.select_first(".rd-article-wr")
			.and_then(|element| element.html())
		else {
			return Ok(pages);
		};
		let Some(text) = text.split("rd-article__end").next() else {
			return Ok(pages);
		};

		for item in text
			.split('"')
			.filter(|part| part.starts_with("http") && !part.ends_with("/add.png"))
		{
			pages.push(Page {
				content: PageContent::url(item.to_string()),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for MiaoShangManHua {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let list = match listing.id.as_str() {
			"hot" => "hot",
			"month" => "month",
			"week" => "week",
			"day" => "day",
			"fav" => "fav",
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};
		Ok(MangaPageResult {
			entries: MiaoShangManHua::parse_ranking(format!("{WWW_URL}/custom/{list}"))?,
			has_next_page: false,
		})
	}
}

impl ImageRequestProvider for MiaoShangManHua {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		let referer = url.clone();
		Ok(Request::get(url)?.header("Referer", referer.as_str()))
	}
}

register_source!(MiaoShangManHua, ListingProvider, ImageRequestProvider);
