#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	imports::{
		html::Document,
		net::{HttpMethod, Request},
	},
	prelude::*,
	Chapter, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult,
	Page, PageContext, Result, Source,
};

mod parser;

// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_TAGID: [&str; 66] = [
	"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
	"17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31", "32",
	"33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46", "47", "48",
	"49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "60", "61", "62", "63", "64",
	"65",
];
#[allow(dead_code)]
const FILTER_SORTID: [&str; 13] = [
	"0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
];
#[allow(dead_code)]
const FILTER_RGROUPID: [&str; 6] = ["0", "1", "2", "3", "4", "5"];
const FILTER_ORDER: [&str; 10] = [
	"weekvisit",
	"monthvisit",
	"weekvote",
	"monthvote",
	"weekflower",
	"monthflower",
	"words",
	"goodnum",
	"lastupdate",
	"postdate",
];
#[allow(dead_code)]
const FILTER_ANIME: [&str; 3] = ["0", "1", "2"];
#[allow(dead_code)]
const FILTER_QUALITY: [&str; 3] = ["0", "1", "2"];
#[allow(dead_code)]
const FILTER_ISFULL: [&str; 3] = ["0", "1", "2"];
#[allow(dead_code)]
const FILTER_UPDATE: [&str; 5] = ["0", "1", "2", "3", "4"];

struct BiliComic;

impl BiliComic {
	fn gen_request(url: String, method: HttpMethod) -> Result<Request> {
		Ok(Request::new(url.as_str(), method)?
			.header("Origin", parser::BASE_URL)
			.header("User-Agent", parser::UA)
			.header("Accept-Language", "zh-CN,zh;q=0.9")
			.header("Cookie", "night=0"))
	}

	fn fetch_html(url: String, referer: Option<String>) -> Result<Document> {
		let request = BiliComic::gen_request(url, HttpMethod::Get)?;
		let request = match referer {
			Some(referer) => request.header("Referer", referer.as_str()),
			None => request,
		};

		Ok(request.html()?)
	}

	fn parse_chapter_list(&self, document: Document) -> Result<Vec<Chapter>> {
		let volumes = document
			.select(".catalog-volume")
			.map(|list| list.collect::<Vec<_>>())
			.unwrap_or_default();
		let mut chapters: Vec<Chapter> = Vec::new();

		for volume in volumes {
			let volume_title = volume
				.select("h3")
				.and_then(|list| list.text())
				.unwrap_or_default();
			let volume_number = parser::extract_chapter_number(&volume_title);
			let chapter_links = volume
				.select(".chapter-li-a")
				.map(|list| list.collect::<Vec<_>>())
				.unwrap_or_default();

			if chapter_links.is_empty() {
				continue;
			}

			let mut has_javascript_link = false;
			for link in &chapter_links {
				let href = link.attr("href").unwrap_or_default();
				if href.starts_with("javascript:") {
					has_javascript_link = true;
					break;
				}
			}

			if has_javascript_link {
				let vol_href = parser::element_first(&volume, ".volume-cover-img")
					.and_then(|node| node.attr("href"))
					.unwrap_or_default();
				let vol_url = format!("{}{}", parser::BASE_URL, vol_href);
				let vol_document = BiliComic::fetch_html(vol_url, None)?;
				let items: Vec<_> = vol_document
					.select(".catalog-volume .chapter-li-a")
					.map(|list| list.collect::<Vec<_>>())
					.unwrap_or_default();

				for chapter_item in items {
					let fallback = chapters.len() as f32 + 1.0;
					chapters.push(parser::chapter_from_link(
						&chapter_item,
						volume_number,
						fallback,
					));
				}
			} else {
				for item in chapter_links {
					let fallback = chapters.len() as f32 + 1.0;
					chapters.push(parser::chapter_from_link(&item, volume_number, fallback));
				}
			}
		}
		chapters.reverse();

		Ok(chapters)
	}
}

impl Source for BiliComic {
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
		let mut tagid = String::from("0");
		let mut sortid = String::from("0");
		let mut rgroupid = String::from("0");
		let mut order = String::from("lastupdate");
		let mut anime = String::from("0");
		let mut quality = String::from("0");
		let mut isfull = String::from("0");
		let mut update = String::from("0");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"tagid" => tagid = value,
					"sortid" => sortid = value,
					"rgroupid" => rgroupid = value,
					"anime" => anime = value,
					"quality" => quality = value,
					"isfull" => isfull = value,
					"update" => update = value,
					_ => continue,
				},
				FilterValue::Sort { index, .. } => {
					if let Some(value) = FILTER_ORDER.get(index.max(0) as usize) {
						order = value.to_string();
					}
				}
				_ => continue,
			}
		}

		if !query.is_empty() {
			bail!("站點搜尋已改為圖形驗證碼校驗，暫不可用；請清空關鍵字改用篩選瀏覽");
		}

		// The filter path now carries twelve slots:
		// order, tagid, isfull, anime, rgroupid, sortid, update, quality, page,
		// unused(0), pubtime, inmatch.
		let url = format!(
			"{}/filter/{}_{}_{}_{}_{}_{}_{}_{}_{}_0_0_0.html",
			parser::BASE_URL,
			order,
			tagid,
			isfull,
			anime,
			rgroupid,
			sortid,
			update,
			quality,
			page
		);
		let referer = format!("{}/filter/", parser::BASE_URL);
		let document = BiliComic::fetch_html(url, Some(referer))?;
		let (entries, has_next_page) = parser::parse_search_list(&document, true);

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
		if needs_details {
			let url = format!("{}/detail/{}.html", parser::BASE_URL, manga.key);
			let document = BiliComic::fetch_html(url, None)?;
			let key = manga.key.clone();
			manga.copy_from(parser::parse_manga(key, &document));
		}

		if needs_chapters {
			let url = format!("{}/read/{}/catalog", parser::BASE_URL, manga.key);
			let document = BiliComic::fetch_html(url, None)?;
			manga.chapters = Some(self.parse_chapter_list(document)?);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!(
			"{}/read/{}/{}.html",
			parser::BASE_URL,
			manga.key,
			chapter.key
		);
		let document = BiliComic::fetch_html(url, None)?;

		Ok(parser::parse_page_list(&document))
	}
}

impl ListingProvider for BiliComic {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let name = match listing.id.as_str() {
			"monthvisit" | "weekvisit" | "monthvote" | "weekvote" | "monthflower"
			| "weekflower" | "monthegg" | "weekegg" | "lastupdate" | "postdate" | "goodnum"
			| "newhot" => listing.id.clone(),
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		};

		let url = format!("{}/top/{}/1.html", parser::BASE_URL, name);
		let document = BiliComic::fetch_html(url, None)?;

		Ok(MangaPageResult {
			entries: parser::parse_books(&document),
			has_next_page: false,
		})
	}
}

impl ImageRequestProvider for BiliComic {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		// The image host (w.motiezw.com) is behind Cloudflare and rejects requests
		// without an Accept-Language header.
		Ok(Request::get(url)?
			.header("User-Agent", parser::UA)
			.header("Accept-Language", "zh-CN,zh;q=0.9")
			.header("Referer", parser::BASE_URL))
	}
}

register_source!(BiliComic, ListingProvider, ImageRequestProvider);
