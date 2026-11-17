#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::{
		html::Document,
		net::{HttpMethod, Request},
	},
	prelude::*,
	Chapter, FilterValue, ImageRequestProvider, Manga, MangaPageResult, Page, PageContext, Result,
	Source,
};

mod parser;

// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_TAG: [&str; 39] = [
	"",
	"mohuan",
	"mofa",
	"rexue",
	"maoxian",
	"xuanyi",
	"zhentan",
	"aiqing",
	"xiaoyuan",
	"gaoxiao",
	"sige",
	"kehuan",
	"shengui",
	"wudao",
	"yinyue",
	"baihe",
	"hougong",
	"jizhan",
	"gedou",
	"kongbu",
	"mengxi",
	"wuxia",
	"shehui",
	"lishi",
	"danmei",
	"lizhi",
	"zhichang",
	"shenghuo",
	"zhiyu",
	"weiniang",
	"heidao",
	"zhanzheng",
	"jingji",
	"tiyu",
	"meishi",
	"funv",
	"zhainan",
	"tuili",
	"zazhi",
];
#[allow(dead_code)]
const FILTER_COUNTRY: [&str; 7] = ["", "japan", "hongkong", "europe", "china", "korea", "other"];
#[allow(dead_code)]
const FILTER_AUDIENCE: [&str; 6] = ["", "shaonv", "shaonian", "qingnian", "ertong", "tongyong"];
#[allow(dead_code)]
const FILTER_YEAR: [&str; 21] = [
	"", "2025", "2024", "2023", "2022", "2021", "2020", "2019", "2018", "2017", "2016", "2015",
	"2014", "2013", "2012", "2011", "2010", "200x", "199x", "198x", "197x",
];
#[allow(dead_code)]
const FILTER_END: [&str; 3] = ["", "0", "1"];
const FILTER_SORT: [&str; 3] = ["", "update", "views"];

struct MyComic;

impl Source for MyComic {
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
		let mut tag = String::new();
		let mut country = String::new();
		let mut audience = String::new();
		let mut year = String::new();
		let mut end = String::new();
		let mut sort = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"tag" => tag = value,
					"country" => country = value,
					"audience" => audience = value,
					"year" => year = value,
					"end" => end = value,
					_ => continue,
				},
				FilterValue::Sort {
					index, ascending, ..
				} => {
					if let Some(value) = FILTER_SORT.get(index.max(0) as usize) {
						sort = value.to_string();
					}

					if sort.is_empty() && ascending {
						sort.push_str("time");
					} else if !sort.is_empty() && !ascending {
						sort = format!("-{}", sort);
					}
				}
				_ => continue,
			}
		}

		let url = if query.is_empty() {
			format!(
				"{}/comics?filter[tag]={}&filter[country]={}&filter[audience]={}&filter[year]={}&filter[end]={}&sort={}&page={}",
				parser::WWW_URL,
				tag,
				country,
				audience,
				year,
				end,
				sort,
				page
			)
		} else {
			format!(
				"{}/comics?q={}&page={}",
				parser::WWW_URL,
				encode_uri(query),
				page
			)
		};
		let document: Document = Request::new(url.as_str(), HttpMethod::Get)?.html()?;

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(document),
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
			let url = format!("{}/comics/{}", parser::WWW_URL, manga.key);
			let document: Document = Request::new(url.as_str(), HttpMethod::Get)?.html()?;
			let key = manga.key.clone();
			manga.copy_from(parser::parse_manga(key, document));
		}

		if needs_chapters {
			let url = format!("{}/comics/{}", parser::WWW_URL, manga.key);
			let document: Document = Request::new(url.as_str(), HttpMethod::Get)?.html()?;
			manga.chapters = Some(parser::parse_chapter_list(document));
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{}/chapters/{}", parser::WWW_URL, chapter.key);
		let document: Document = Request::new(url.as_str(), HttpMethod::Get)?.html()?;

		Ok(parser::parse_page_list(document))
	}
}

impl ImageRequestProvider for MyComic {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", parser::WWW_URL))
	}
}

register_source!(MyComic, ImageRequestProvider);
