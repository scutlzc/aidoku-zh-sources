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
use serde_json::Value;

mod parser;

// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_GENRE: [&str; 132] = [
	"",
	"rexue",
	"gedou",
	"wuxia",
	"mohuan",
	"mofa",
	"maoxian",
	"aiqing",
	"gaoxiao",
	"xiaoyuan",
	"kehuan",
	"hougong",
	"lizhi",
	"zhichang",
	"meishi",
	"shehui",
	"heidao",
	"zhanzheng",
	"lishi",
	"xuanyi",
	"jingji",
	"tiyu",
	"kongbu",
	"tuili",
	"shenghuo",
	"weiniang",
	"zhiyu",
	"shengui",
	"sige",
	"baihe",
	"danmei",
	"wudao",
	"zhentan",
	"zhainan",
	"yinyue",
	"mengxi",
	"gufeng",
	"lianai",
	"dushi",
	"xingzhuan",
	"chuanyue",
	"youxi",
	"qita",
	"aiqi",
	"richang",
	"fuhei",
	"guzhuang",
	"xianxia",
	"shenghua",
	"xiuxian",
	"qinggan",
	"gaibian",
	"chunai",
	"weimei",
	"qiangwei",
	"mingxing",
	"lieqi",
	"qingchun",
	"huanxiang",
	"jingqi",
	"caihong",
	"qiwen",
	"quanmou",
	"zhaidou",
	"xianzhiji",
	"zhuangbi",
	"langman",
	"ouxiang",
	"danvzhu",
	"fuchou",
	"nuexin",
	"egao",
	"lingyi",
	"jingxian",
	"chongai",
	"nixi",
	"yaoguai",
	"aimei",
	"tongren",
	"jiakong",
	"zhenren",
	"dongzuo",
	"juwei",
	"gongdou",
	"naodong",
	"mangai",
	"zhandou",
	"sangshi",
	"meishaonv",
	"guaiwu",
	"xitong",
	"zhidou",
	"jijia",
	"gaotian",
	"jiangshi",
	"zhiyu",
	"dianjing",
	"shenmo",
	"yineng",
	"mori",
	"yinv",
	"haokuai",
	"qihuan",
	"shenshi",
	"zhengnengliang",
	"gongting",
	"qinqing",
	"yangcheng",
	"juqing",
	"qingxiaoshuo",
	"anhei",
	"changtiao",
	"xuanhuan",
	"bazong",
	"ouhuang",
	"shengcun",
	"yishijie",
	"qita",
	"C99",
	"jiecao",
	"AA",
	"yingshihua",
	"oufeng",
	"nvshen",
	"shuanggan",
	"zhuansheng",
	"yixing",
	"fantaolu",
	"shuangnanzhu",
	"wudiliu",
	"xingzhuanhuan",
	"zhongsheng",
];
// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_AREA: [&str; 7] = ["", "china", "japan", "hongkong", "europe", "korea", "other"];
// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_AUDIENCE: [&str; 6] = ["", "shaonian", "shaonv", "qingnian", "BL", "GL"];
// Site values are carried by res/filters.json (ids), kept here for reference.
#[allow(dead_code)]
const FILTER_STATUS: [&str; 3] = ["-1", "0", "1"];
const FILTER_ORDER: [&str; 2] = ["last_date", "views"];

struct HappyMH;

impl HappyMH {
	fn get_chapter_list_by_page(&self, id: String, page: i32) -> Result<Vec<Value>> {
		let url = format!(
			"{}/v2.0/apis/manga/chapterByPage?code={}&page={}&lang=cn&order=asc",
			parser::WWW_URL,
			id,
			page
		);
		let referer = format!("{}/manga/{}", parser::WWW_URL, id);
		let json = Request::new(url.as_str(), HttpMethod::Get)?
			.header("Referer", referer.as_str())
			.header("User-Agent", parser::UA)
			.header("Origin", parser::WWW_URL)
			.json_owned::<Value>()?;
		let data = json.get("data").cloned().unwrap_or(Value::Null);
		let is_end = data.get("isEnd").and_then(Value::as_i64).unwrap_or(1);
		let items: Vec<Value> = data
			.get("items")
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();

		if is_end == 1 {
			return Ok(items);
		}

		let mut list = items;
		let mut next_list = self.get_chapter_list_by_page(id, page + 1)?;
		list.append(&mut next_list);

		Ok(list)
	}
}

impl Source for HappyMH {
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
		let mut genre = String::new();
		let mut area = String::new();
		let mut audience = String::new();
		let mut status = String::from("-1");
		let mut order = String::from("last_date");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => match id.as_str() {
					"genre" => genre = value,
					"area" => area = value,
					"audience" => audience = value,
					"status" => status = value,
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

		let json = if query.is_empty() {
			let url = format!(
				"{}/apis/c/index?genre={}&area={}&audience={}&series_status={}&order={}&pn={}",
				parser::WWW_URL,
				genre,
				area,
				audience,
				status,
				order,
				page
			);
			let referer = format!("{}/latest", parser::WWW_URL);
			Request::new(url.as_str(), HttpMethod::Get)?
				.header("Referer", referer.as_str())
				.header("User-Agent", parser::UA)
				.header("Origin", parser::WWW_URL)
				.json_owned::<Value>()?
		} else {
			let url = format!("{}/v2.0/apis/manga/ssearch", parser::WWW_URL);
			let body = format!("searchkey={}&v=v2.13", query);
			let referer = format!("{}/sssearch", parser::WWW_URL);
			Request::new(url.as_str(), HttpMethod::Post)?
				.header("Content-Type", "application/x-www-form-urlencoded")
				.header("Referer", referer.as_str())
				.header("User-Agent", parser::UA)
				.header("Origin", parser::WWW_URL)
				.body(body.as_bytes())
				.json_owned::<Value>()?
		};

		let items: Vec<Value> = json
			.get("data")
			.and_then(|data| data.get("items"))
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();

		Ok(MangaPageResult {
			entries: parser::parse_manga_list(&items),
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
			let url = format!("{}/manga/{}", parser::WWW_URL, manga.key);
			let referer = format!("{}/latest", parser::WWW_URL);
			let document: Document = Request::new(url.as_str(), HttpMethod::Get)?
				.header("Referer", referer.as_str())
				.header("User-Agent", parser::UA)
				.header("Origin", parser::WWW_URL)
				.html()?;
			let key = manga.key.clone();
			manga.copy_from(parser::parse_manga(key, document));
		}

		if needs_chapters {
			let list = self.get_chapter_list_by_page(manga.key.clone(), 1)?;
			manga.chapters = Some(parser::parse_chapter_list(&list));
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!(
			"{}/v2.0/apis/manga/reading?code={}&v=v3.1818134",
			parser::WWW_URL,
			chapter.key
		);
		let referer = format!("{}/mangaread/{}", parser::WWW_URL, chapter.key);
		let json = Request::new(url.as_str(), HttpMethod::Get)?
			.header("Referer", referer.as_str())
			.header("User-Agent", parser::UA)
			.header("Origin", parser::WWW_URL)
			.header("X-Requested-With", "XMLHttpRequest")
			.json_owned::<Value>()?;
		let scans: Vec<Value> = json
			.get("data")
			.and_then(|data| data.get("scans"))
			.and_then(Value::as_array)
			.cloned()
			.unwrap_or_default();

		Ok(parser::parse_page_list(&scans))
	}
}

impl ListingProvider for HappyMH {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let mut name = String::new();

		match listing.id.as_str() {
			"day" => {
				name.push_str("day");
			}
			"dayBookcases" => {
				name.push_str("dayBookcases");
			}
			"week" => {
				name.push_str("week");
			}
			"weekBookcase" => {
				name.push_str("weekBookcase");
			}
			"month" => {
				name.push_str("month");
			}
			"monthBookcases" => {
				name.push_str("monthBookcases");
			}
			"voteRank" => {
				name.push_str("voteRank");
			}
			"voteNumMonthRank" => {
				name.push_str("voteNumMonthRank");
			}
			_ => return self.get_search_manga_list(None, page, Vec::new()),
		}

		let url = format!("{}/rank/{}", parser::WWW_URL, name);
		let document: Document = Request::new(url.as_str(), HttpMethod::Get)?
			.header("Referer", url.as_str())
			.header("User-Agent", parser::UA)
			.header("Origin", parser::WWW_URL)
			.html()?;

		Ok(MangaPageResult {
			entries: parser::parse_rank_list(document),
			has_next_page: false,
		})
	}
}

impl ImageRequestProvider for HappyMH {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?
			.header("Referer", parser::WWW_URL)
			.header("User-Agent", parser::UA)
			.header("Origin", parser::WWW_URL))
	}
}

register_source!(HappyMH, ListingProvider, ImageRequestProvider);
