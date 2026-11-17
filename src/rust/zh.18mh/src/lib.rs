#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider,
	Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext,
	Result, Source, Viewer,
};

const WWW_URL: &str = "https://18mh.org";

const GENRE_PATHS: [&str; 4] = ["hanman", "zhenrenxiezhen", "riman", "aixiezhen"];

struct Mh18;

impl Mh18 {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".pb-2>a") else {
			return Ok(entries);
		};

		for item in items {
			let href = item.attr("href").unwrap_or_default();
			let key = href
				.trim_end_matches('/')
				.rsplit('/')
				.next()
				.unwrap_or_default()
				.to_string();
			if key.is_empty() {
				continue;
			}
			let cover = item
				.select_first("div>img")
				.and_then(|img| img.attr("src"))
				.filter(|src| !src.is_empty())
				.map(|src| abs_url(&src));
			let title = item
				.select_first("div>h3")
				.and_then(|title| title.text())
				.unwrap_or_default();
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

impl Source for Mh18 {
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

		for filter in filters {
			if let FilterValue::Select { id, value } = filter {
				if id == "category" {
					category = value;
				}
			}
		}

		let query = query.unwrap_or_default();
		let url = if query.is_empty() {
			let path = if category.is_empty() {
				String::from("manga")
			} else if GENRE_PATHS.contains(&category.as_str()) {
				format!("manga-genre/{category}")
			} else {
				format!("manga-tag/{category}")
			};
			format!("{WWW_URL}/{path}/page/{page}")
		} else {
			format!("{WWW_URL}/s/{}?page={page}", encode_uri(query))
		};

		Ok(MangaPageResult {
			entries: Mh18::parse_manga_list(url)?,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let mut parts = manga.key.split('/');
		let slug = parts.next().unwrap_or_default().to_string();
		let mut mid = parts.next().map(|mid| mid.to_string());

		if needs_details || (needs_chapters && mid.is_none()) {
			let url = format!("{WWW_URL}/manga/{slug}");
			let html = Request::get(&url)?.html()?;
			mid = html
				.select_first("#mangachapters")
				.and_then(|element| element.attr("data-mid"));

			if needs_details {
				if let Some(mid) = mid.as_ref() {
					manga.key = format!("{slug}/{mid}");
				}
				manga.title = html
					.select_first("title")
					.and_then(|title| title.text())
					.unwrap_or_default()
					.replace("-18漫畫", "");
				manga.cover = html
					.select_first("meta[property='og:image']")
					.and_then(|meta| meta.attr("content"))
					.map(|src| abs_url(&src));
				manga.description = html
					.select_first(".text-medium.my-unit-md")
					.and_then(|desc| desc.text());
				manga.url = Some(url);

				let authors: Vec<String> = html
					.select("a[href*=author]>span")
					.map(|list| {
						list.map(|author| {
							author
								.text()
								.unwrap_or_default()
								.replace(',', "")
								.trim()
								.to_string()
						})
						.filter(|author| !author.is_empty())
						.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};

				let tags: Vec<String> = html
					.select(".py-1>a:not([href*=author])>span")
					.map(|list| {
						list.map(|tag| {
							tag.text()
								.unwrap_or_default()
								.replace(',', "")
								.replace("熱門漫畫", "")
								.replace('#', "")
								.replace("推荐", "")
								.trim()
								.to_string()
						})
						.filter(|tag| !tag.is_empty())
						.collect::<Vec<String>>()
					})
					.unwrap_or_default();
				manga.tags = if tags.is_empty() { None } else { Some(tags) };

				manga.status = match html
					.select_first("h1.mb-2>span")
					.and_then(|status| status.text())
					.unwrap_or_default()
					.trim()
				{
					"連載中" => MangaStatus::Ongoing,
					"完結" => MangaStatus::Completed,
					_ => MangaStatus::Unknown,
				};
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::Webtoon;
			}
		}

		if needs_chapters {
			if let Some(mid) = mid {
				let url = format!("{WWW_URL}/manga/get?mid={mid}&mode=all");
				manga.chapters = Some(self.parse_chapter_list(url)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let mid = manga.key.split('/').nth(1).unwrap_or_default().to_string();
		let url = format!("{WWW_URL}/chapter/getcontent?m={mid}&c={}", chapter.key);
		let html = Request::get(url)?.header("Referer", WWW_URL).html()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(items) = html.select("#chapcontent>div>img") else {
			return Ok(pages);
		};

		for item in items {
			let url = item
				.attr("data-src")
				.filter(|src| !src.is_empty())
				.or_else(|| item.attr("src"))
				.unwrap_or_default();
			if url.is_empty() {
				continue;
			}
			pages.push(Page {
				content: PageContent::url(abs_url(&url)),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl Mh18 {
	fn parse_chapter_list(&self, url: String) -> Result<Vec<Chapter>> {
		let html = Request::get(url)?.html()?;
		let mut chapters: Vec<Chapter> = Vec::new();

		if let Some(items) = html.select("#allchapterlist>.chapteritem>a") {
			for (index, item) in items.enumerate() {
				let Some(key) = item.attr("data-cs") else {
					continue;
				};
				let title = item
					.select_first("div>span:nth-child(1)")
					.and_then(|title| title.text())
					.unwrap_or_default()
					.trim()
					.to_string();
				let slug = item.attr("href").unwrap_or_default();
				chapters.push(Chapter {
					key,
					title: Some(title),
					chapter_number: Some((index + 1) as f32),
					url: Some(abs_url(&slug)),
					..Default::default()
				});
			}
		}

		chapters.reverse();
		Ok(chapters)
	}
}

impl ListingProvider for Mh18 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let path = match listing.id.as_str() {
			"hots" => "hots",
			"dayup" => "dayup",
			"newss" => "newss",
			_ => "manga",
		};
		let url = format!("{WWW_URL}/{path}/page/{page}");
		Ok(MangaPageResult {
			entries: Mh18::parse_manga_list(url)?,
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for Mh18 {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

impl DeepLinkHandler for Mh18 {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Some(path) = url.strip_prefix(WWW_URL) else {
			return Ok(None);
		};
		if let Some(slug) = path.strip_prefix("/manga/") {
			let key = slug.trim_end_matches('/').to_string();
			if !key.is_empty() {
				return Ok(Some(DeepLinkResult::Manga { key }));
			}
		}
		Ok(None)
	}
}

fn abs_url(url: &str) -> String {
	if url.starts_with("http") {
		url.to_string()
	} else if let Some(rest) = url.strip_prefix("//") {
		format!("https://{rest}")
	} else if url.starts_with('/') {
		format!("{WWW_URL}{url}")
	} else {
		format!("{WWW_URL}/{url}")
	}
}

register_source!(Mh18, ListingProvider, ImageRequestProvider, DeepLinkHandler);
