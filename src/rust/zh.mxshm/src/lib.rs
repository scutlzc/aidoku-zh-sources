#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{defaults::defaults_get, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, Manga, MangaPageResult, MangaStatus, Page, PageContent,
	Result, Source, Viewer,
};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";
const DEFAULT_URL: &str = "https://www.mxs13.cc";

struct Mxshm;

fn get_url() -> String {
	defaults_get::<String>("url")
		.filter(|url| !url.is_empty())
		.unwrap_or_else(|| DEFAULT_URL.to_string())
}

fn gen_request(url: String) -> Result<Request> {
	Ok(Request::get(url)?.header("User-Agent", UA))
}

impl Source for Mxshm {
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
		// the site only accepts -1 for "all" on area/end; empty values return an empty list
		let mut area = String::from("-1");
		let mut end = String::from("-1");

		for filter in filters {
			if let FilterValue::Select { id, value } = filter {
				if id == "tag" {
					tag = value;
				} else if id == "area" {
					area = value;
				} else if id == "end" {
					end = value;
				}
			}
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			format!(
				"{}/booklist?tag={}&area={area}&end={end}&page={page}",
				get_url(),
				encode_uri(tag)
			)
		} else {
			format!("{}/search?keyword={}", get_url(), encode_uri(&query))
		};
		let html = gen_request(url)?.html()?;
		let mut mangas: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".mh-item") else {
			return Ok(MangaPageResult {
				entries: mangas,
				has_next_page: query.is_empty(),
			});
		};

		for item in items {
			let Some(href) = item.select_first("a").and_then(|link| link.attr("href")) else {
				continue;
			};
			let key = href.split('/').last().unwrap_or_default().to_string();
			let cover = item
				.select_first("a>p")
				.and_then(|cover| cover.attr("style"))
				.map(|style| style.replace("background-image: url(", "").replace(")", ""))
				.filter(|cover| !cover.is_empty());
			let title = item
				.select_first(".mh-item-detali>h2>a")
				.and_then(|title| title.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			mangas.push(Manga {
				key,
				title,
				cover,
				..Default::default()
			});
		}

		Ok(MangaPageResult {
			entries: mangas,
			has_next_page: query.is_empty(),
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let url = format!("{}/book/{}", get_url(), manga.key);
			let html = gen_request(url.clone())?.html()?;
			manga.cover = html
				.select_first(".banner_detail_form>.cover>img")
				.and_then(|cover| cover.attr("src"))
				.filter(|cover| !cover.is_empty());
			manga.title = html
				.select_first(".banner_detail_form>.info>h1")
				.and_then(|title| title.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			let authors: Vec<String> = html
				.select_first(".banner_detail_form>.info>p:nth-child(3)")
				.and_then(|author| author.text())
				.unwrap_or_default()
				.trim()
				.replace("作者：", "")
				.split('&')
				.map(|author| author.to_string())
				.filter(|author| !author.is_empty())
				.collect();
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = html
				.select_first(".banner_detail_form>.info>.content")
				.and_then(|description| description.text())
				.map(|description| description.trim().to_string());
			let tags: Vec<String> = html
				.select(".banner_detail_form>.info>p:nth-child(5)>span>a")
				.map(|list| {
					list.map(|tag| tag.text().unwrap_or_default().trim().to_string())
						.filter(|tag| !tag.is_empty())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.status = match html
				.select_first(".banner_detail_form>.info>p:nth-child(4)>span:nth-child(1)>span")
				.and_then(|status| status.text())
				.unwrap_or_default()
				.trim()
				.to_string()
				.as_str()
			{
				"连载中" => MangaStatus::Ongoing,
				"已完结" => MangaStatus::Completed,
				_ => MangaStatus::Unknown,
			};
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::Webtoon;
			manga.url = Some(url);
		}

		if needs_chapters {
			let url = format!("{}/book/{}", get_url(), manga.key);
			let html = gen_request(url)?.html()?;
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(items) = html.select("#detail-list-select>li>a") {
				for (index, item) in items.enumerate() {
					let Some(href) = item.attr("href") else {
						continue;
					};
					let key = href.split('/').last().unwrap_or_default().to_string();
					let title = item.text().unwrap_or_default().trim().to_string();
					chapters.push(Chapter {
						url: Some(format!("{}/chapter/{}", get_url(), key)),
						key,
						title: Some(title),
						chapter_number: Some((index + 1) as f32),
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
		let url = format!("{}/chapter/{}", get_url(), chapter.key);
		let html = gen_request(url)?.html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select(".comicpage>div>img,#cp_img>img") else {
			return Ok(pages);
		};

		for item in items {
			let Some(url) = item.attr("data-original") else {
				continue;
			};
			let url = url.trim().to_string();
			if url.is_empty() {
				continue;
			}
			pages.push(Page {
				content: PageContent::url(url),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

register_source!(Mxshm);
