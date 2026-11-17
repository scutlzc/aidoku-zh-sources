#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};

const WWW_URL: &str = "https://www.wnacg01.cc";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

const FILTER_CATEGORY_5: [&str; 4] = ["5", "1", "12", "16"];
const FILTER_CATEGORY_6: [&str; 4] = ["6", "9", "13", "17"];
const FILTER_CATEGORY_7: [&str; 4] = ["7", "10", "14", "18"];

struct Wnacg;

impl Wnacg {
	fn gen_request(url: &str) -> Result<Request> {
		Ok(Request::get(url)?.header("User-Agent", UA))
	}

	fn parse_manga_list(url: &str) -> Result<Vec<Manga>> {
		let html = Wnacg::gen_request(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select(".gallary_item") else {
			return Ok(entries);
		};

		for item in items {
			let Some(href) = item.select_first(".pic_box>a").and_then(|a| a.attr("href")) else {
				continue;
			};
			let mut parts: Vec<String> = href.split('-').map(|a| a.replace(".html", "")).collect();
			let key = parts.pop().unwrap_or_default();
			let cover = item
				.select_first(".pic_box>a>img")
				.and_then(|img| img.attr("src"))
				.map(|src| format!("https:{src}"));
			let title = item
				.select_first(".info>.title>a")
				.and_then(|title| title.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			entries.push(Manga {
				key,
				cover,
				title,
				..Default::default()
			});
		}

		Ok(entries)
	}
}

impl Source for Wnacg {
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
		let mut category = String::new();
		let mut language = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"category" => category = value,
					"language" => language = value,
					_ => continue,
				},
				_ => continue,
			}
		}

		// 语言选项的站点取值取决于所选类别，分别对应 FILTER_CATEGORY_5/6/7 的下标。
		if !language.is_empty() {
			let index = match language.as_str() {
				"zh" => 1,
				"ja" => 2,
				"en" => 3,
				_ => 0,
			};
			let mapped = match category.as_str() {
				"5" => FILTER_CATEGORY_5.get(index),
				"6" => FILTER_CATEGORY_6.get(index),
				"7" => FILTER_CATEGORY_7.get(index),
				_ => None,
			};
			if let Some(mapped) = mapped {
				category = mapped.to_string();
			}
		}

		let url = if query.is_empty() {
			format!("{WWW_URL}/albums-index-page-{page}-cate-{category}.html")
		} else {
			format!(
				"{WWW_URL}/search/index.php?q={}&s=create_time_DESC&syn=yes&p={page}",
				encode_uri(query)
			)
		};

		Ok(MangaPageResult {
			entries: Wnacg::parse_manga_list(&url)?,
			has_next_page: true,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let url = format!("{WWW_URL}/photos-index-aid-{}.html", manga.key);

		if needs_details {
			let html = Wnacg::gen_request(&url)?.html()?;
			if let Some(cover) = html
				.select_first("#bodywrap>div>.uwthumb>img")
				.and_then(|img| img.attr("src"))
			{
				manga.cover = Some(format!("https://{}", cover.replace("//", "")));
			}
			if let Some(title) = html.select_first("#bodywrap>h2").and_then(|t| t.text()) {
				manga.title = title;
			}
			let mut categories: Vec<String> = html
				.select("#bodywrap>div>.uwconn>label:nth-child(1)")
				.and_then(|list| list.text())
				.unwrap_or_default()
				.replace("分類：", "")
				.split("／")
				.flat_map(|a| a.split("&"))
				.map(|a| a.trim().to_string())
				.collect();
			if let Some(tags) = html.select("#bodywrap>div>.uwconn>.addtags>.tagshow") {
				for tag in tags {
					if let Some(text) = tag.text() {
						categories.push(text.trim().to_string());
					}
				}
			}
			manga.tags = Some(categories);
			manga.status = MangaStatus::Unknown;
			manga.content_rating = ContentRating::NSFW;
			manga.viewer = Viewer::RightToLeft;
			manga.url = Some(url.clone());
		}

		if needs_chapters {
			let mut chapters: Vec<Chapter> = Vec::new();
			chapters.push(Chapter {
				key: manga.key.clone(),
				title: Some(String::from("第 1 话")),
				chapter_number: Some(1.0),
				url: Some(url),
				..Default::default()
			});
			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, _chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{WWW_URL}/photos-gallery-aid-{}.html", manga.key);
		let text = Wnacg::gen_request(&url)?.string()?;
		let mut pages: Vec<Page> = Vec::new();

		for item in text.split("\\\"").filter(|a| a.starts_with("//")) {
			pages.push(Page {
				content: PageContent::url(format!("https:{item}")),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ListingProvider for Wnacg {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let category = match listing.id.as_str() {
			"cg" => "2",
			"3d" => "22",
			"cosplay" => "3",
			"hanman" => "19",
			_ => {
				return self.get_search_manga_list(None, page, Vec::new());
			}
		};

		let url = format!("{WWW_URL}/albums-index-page-{page}-cate-{category}.html");

		Ok(MangaPageResult {
			entries: Wnacg::parse_manga_list(&url)?,
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for Wnacg {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(Wnacg, ListingProvider, ImageRequestProvider);
