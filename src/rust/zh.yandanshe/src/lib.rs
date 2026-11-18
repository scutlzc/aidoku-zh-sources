#![no_std]

mod crypto;

use aidoku::{
	alloc::{
		format,
		string::{String, ToString},
		vec,
		vec::Vec,
	},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};

const WWW_URL: &str = "https://yandanshe.org";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

// 阅读页的 CSRF 令牌与 reader_grant 都绑定在 PHPSESSID 上，插件自选一个固定
// session id 即可在一次图片请求里复用两段式流程，无需读取 Set-Cookie。
const SESSION_ID: &str = "aidoku0a1b2c3d4e5f6a7b";

const FILTER_ORDER: [&str; 5] = ["1", "2", "3", "4", "5"];
const LIST_PAGE_SIZE: usize = 20;

struct Yandanshe;

fn between<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
	let start = text.find(prefix)? + prefix.len();
	let rest = &text[start..];
	let end = rest.find(suffix)?;
	Some(&rest[..end])
}

fn list_url(class: &str, status: &str, order: &str, page: i32) -> String {
	let mut url = String::from(WWW_URL);
	if !status.is_empty() {
		url.push('/');
		url.push_str(status);
	}
	url.push_str("/sort/");
	if class.is_empty() {
		url.push('0');
	} else {
		url.push_str(class);
		url.push_str("/0");
	}
	url.push('/');
	url.push_str(&page.max(1).to_string());
	url.push('/');
	url.push_str(order);
	url.push('/');
	url
}

impl Yandanshe {
	fn parse_manga_list(url: &str) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.header("User-Agent", UA).html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select("ul.flex>li") else {
			return Ok(entries);
		};

		for item in items {
			let Some(image) = item.select_first("div.img_span>a>img") else {
				continue;
			};
			let Some(href) = item
				.select_first("div.img_span>a")
				.and_then(|link| link.attr("href"))
			else {
				continue;
			};
			let key = href
				.split('/')
				.filter(|part| !part.is_empty())
				.last()
				.unwrap_or_default()
				.to_string();
			let title = item
				.select_first("div.w100>a>h2")
				.and_then(|title| title.text())
				.or_else(|| image.attr("title"))
				.map(|title| title.trim().to_string())
				.unwrap_or_default();
			let cover = image
				.attr("data-original")
				.or_else(|| image.attr("src"))
				.map(|cover| cover.trim().to_string());

			entries.push(Manga {
				key,
				title,
				cover,
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
		let mut class = String::new();
		let mut status = String::new();
		let mut order = String::from(FILTER_ORDER[0]);

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => class = value,
					"status" => status = value,
					_ => continue,
				},
				FilterValue::Sort { index, .. } => {
					if let Some(sort_order) = FILTER_ORDER.get(index as usize) {
						order = sort_order.to_string();
					}
				}
				_ => continue,
			}
		}

		// 站点搜索需要登录，未登录会跳转到 /login/，这里保持真实搜索入口。
		let url = if query.is_empty() {
			list_url(&class, &status, &order, page)
		} else {
			format!(
				"{WWW_URL}/search/?searchkey={}&searchtype=1",
				encode_uri(query)
			)
		};

		let entries = Yandanshe::parse_manga_list(&url)?;
		let has_next_page = entries.len() >= LIST_PAGE_SIZE;

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
		let url = format!("{WWW_URL}/book/{}/", manga.key);

		if needs_details || needs_chapters {
			let html = Request::get(&url)?.header("User-Agent", UA).html()?;

			if needs_details {
				if let Some(title) = html.select_first("h1").and_then(|title| title.text()) {
					manga.title = title.trim().to_string();
				}
				manga.cover = html
					.select_first(".novel_info_main>img")
					.and_then(|cover| cover.attr("src"))
					.map(|cover| cover.trim().to_string());
				manga.authors = html
					.select_first(".novel_info_title>i>a")
					.and_then(|author| author.text())
					.map(|author| vec![author.trim().to_string()])
					.filter(|authors: &Vec<String>| !authors.is_empty());
				manga.description = html
					.select_first(".intro>p")
					.and_then(|description| description.text())
					.map(|description| description.trim().to_string())
					.filter(|description| !description.is_empty());

				let status = html
					.select(".novel_info_title>p>span")
					.and_then(|spans| spans.last())
					.and_then(|span| span.text())
					.unwrap_or_default();
				manga.status = match status.trim() {
					"连载" => MangaStatus::Ongoing,
					"完本" | "完结" | "全本" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};

				let mut tags: Vec<String> = Vec::new();
				if let Some(links) = html.select(".novel_info_title a") {
					for link in links {
						let Some(href) = link.attr("href") else {
							continue;
						};
						if !href.starts_with("/tags/") {
							continue;
						}
						if let Some(tag) = link.text() {
							tags.push(tag.trim().to_string());
						}
					}
				}
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::RightToLeft;
				manga.url = Some(url.clone());
			}

			if needs_chapters {
				let mut chapters: Vec<Chapter> = Vec::new();

				if let Some(list) = html.select("ul#ul_all_chapters>li>a") {
					for (index, item) in list.enumerate() {
						let Some(href) = item.attr("href") else {
							continue;
						};
						let Some(file) = href.rsplit('/').next() else {
							continue;
						};
						let key = file.trim_end_matches(".html").to_string();
						if key.is_empty() {
							continue;
						}
						let title = item
							.text()
							.map(|title| title.trim().trim_end_matches("收费").trim().to_string())
							.or_else(|| item.attr("title"))
							.map(|title| title.trim().to_string())
							.unwrap_or_default();
						chapters.push(Chapter {
							key,
							title: Some(title),
							chapter_number: Some((index + 1) as f32),
							url: Some(format!("{WWW_URL}{href}")),
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
		let mut pages: Vec<Page> = Vec::new();
		let read_url = format!("{WWW_URL}/read/{}/{}.html", manga.key, chapter.key);
		let cookie = format!("PHPSESSID={SESSION_ID}");

		let html = Request::get(&read_url)?
			.header("User-Agent", UA)
			.header("Cookie", cookie.as_str())
			.string()?;

		let Some(csrf) = between(&html, "FRONT_CSRF_TOKEN = '", "'") else {
			return Ok(pages);
		};
		let Some(grant) = between(&html, "reader_grant: '", "'") else {
			return Ok(pages);
		};
		let Some(sort_id) = between(&html, "sortid: '", "'") else {
			return Ok(pages);
		};
		let Some(pid) = between(&html, "pid: '", "'") else {
			return Ok(pages);
		};

		let body = format!(
			"action=show&sortid={sort_id}&chaptertype=0&articleid={}&chapterid={}&line=0&pid={pid}&csrf_token={csrf}&reader_grant={grant}",
			manga.key, chapter.key
		);
		let response = Request::post(format!("{WWW_URL}/api/reader_js.php"))?
			.header("User-Agent", UA)
			.header("Cookie", cookie.as_str())
			.header("Referer", read_url.as_str())
			.header("Content-Type", "application/x-www-form-urlencoded")
			.body(body.as_bytes())
			.string()?;

		for url in crypto::decode_images(&response, grant, &manga.key, &chapter.key) {
			pages.push(Page {
				content: PageContent::url(url),
				..Default::default()
			});
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
