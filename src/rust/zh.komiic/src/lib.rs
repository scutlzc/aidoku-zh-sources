#![no_std]

use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	imports::{defaults::defaults_get, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Listing, ListingProvider, Manga,
	MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://komiic.com";
const API_URL: &str = "https://komiic.com/api/query";

const PAGE_SIZE: i32 = 20;

const FILTER_ORDER_BY: [&str; 3] = ["DATE_UPDATED", "VIEWS", "FAVORITE_COUNT"];

struct Komiic;

impl Komiic {
	fn api(body: String) -> Result<Value> {
		let mut request = Request::post(API_URL)?
			.header("Content-Type", "application/json")
			.body(body.as_bytes());

		if let Some(cookie) = defaults_get::<String>("cookie").filter(|cookie| !cookie.is_empty()) {
			request = request.header("Cookie", cookie.as_str());
		}

		request.json_owned::<Value>()
	}

	fn parse_manga_list(list: Option<&Vec<Value>>) -> Vec<Manga> {
		list.map(|list| list.iter().map(Komiic::parse_manga).collect())
			.unwrap_or_default()
	}

	fn parse_manga(manga: &Value) -> Manga {
		let id = jstr(manga, "id");
		let cover = Some(jstr(manga, "imageUrl")).filter(|url| !url.is_empty());
		let title = jstr(manga, "title");
		let authors: Vec<String> = manga
			.get("authors")
			.and_then(|authors| authors.as_array())
			.map(|authors| authors.iter().map(|author| jstr(author, "name")).collect())
			.unwrap_or_default();
		let tags: Vec<String> = manga
			.get("categories")
			.and_then(|categories| categories.as_array())
			.map(|categories| {
				categories
					.iter()
					.map(|category| jstr(category, "name"))
					.collect()
			})
			.unwrap_or_default();
		let status = match jstr(manga, "status").as_str() {
			"ONGOING" => MangaStatus::Ongoing,
			"END" => MangaStatus::Completed,
			_ => MangaStatus::Unknown,
		};

		Manga {
			key: id.clone(),
			title,
			cover,
			url: Some(gen_manga_url(&id)),
			authors: if authors.is_empty() {
				None
			} else {
				Some(authors)
			},
			tags: if tags.is_empty() { None } else { Some(tags) },
			status,
			content_rating: ContentRating::Safe,
			viewer: Viewer::RightToLeft,
			..Default::default()
		}
	}

	fn parse_chapter_list(manga_id: &str, chapter_list: Option<&Vec<Value>>) -> Vec<Chapter> {
		let mut volumes: Vec<Chapter> = Vec::new();
		let mut chapters: Vec<Chapter> = Vec::new();

		for item in chapter_list.map(|list| list.as_slice()).unwrap_or_default() {
			let id = jstr(item, "id");
			let serial = jstr(item, "serial");
			let chapter_type = jstr(item, "type");
			let url = Some(gen_chapter_url(manga_id, &id));

			if chapter_type == "book" {
				volumes.push(Chapter {
					key: id,
					title: Some(serial.clone()),
					volume_number: serial.parse::<f32>().ok(),
					scanlators: Some(vec![chapter_type]),
					url,
					..Default::default()
				});
			} else {
				chapters.push(Chapter {
					key: id,
					title: Some(serial.clone()),
					chapter_number: serial.parse::<f32>().ok(),
					scanlators: Some(vec![chapter_type]),
					url,
					..Default::default()
				});
			}
		}

		let mut all_chapters = volumes;
		all_chapters.extend(chapters);
		all_chapters.reverse();
		all_chapters
	}
}

fn jstr(value: &Value, key: &str) -> String {
	value
		.get(key)
		.and_then(|value| value.as_str())
		.unwrap_or_default()
		.to_string()
}

fn gen_manga_url(id: &str) -> String {
	format!("{WWW_URL}/comic/{id}")
}

fn gen_chapter_url(manga_id: &str, chapter_id: &str) -> String {
	format!("{WWW_URL}/comic/{manga_id}/chapter/{chapter_id}/images/all")
}

fn gen_page_url(manga_id: &str, chapter_id: &str, page_id: &str) -> String {
	format!("{WWW_URL}/api/image/{page_id}?mangaId={manga_id}&chapterId={chapter_id}")
}

impl Source for Komiic {
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
		let mut status = String::new();
		let mut order_by = String::new();

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "category" {
						category = value;
					} else if id == "status" {
						status = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					order_by = FILTER_ORDER_BY
						.get(index as usize)
						.copied()
						.unwrap_or_default()
						.to_string();
				}
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let is_search = !query.is_empty();
		let body = if is_search {
			gen_search_body_string(query)
		} else {
			gen_category_body_string(category, status, order_by, page)
		};

		let json = Komiic::api(body)?;
		let data = json.get("data");

		let entries = if !is_search {
			data.and_then(|data| data.get("comicByCategories").and_then(|v| v.as_array()))
				.map(|list| list.to_vec())
		} else {
			data.and_then(|data| {
				data.get("searchComicsAndAuthors")
					.and_then(|v| v.get("comics"))
					.and_then(|v| v.as_array())
			})
			.map(|list| list.to_vec())
		};

		Ok(MangaPageResult {
			entries: Komiic::parse_manga_list(entries.as_ref()),
			has_next_page: !is_search,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let json = Komiic::api(gen_id_body_string(&manga.key))?;
			if let Some(comic) = json.get("data").and_then(|data| data.get("comicById")) {
				let updated = Komiic::parse_manga(comic);
				manga.title = updated.title;
				manga.cover = updated.cover;
				manga.authors = updated.authors;
				manga.tags = updated.tags;
				manga.status = updated.status;
				manga.url = updated.url;
			}
		}

		if needs_chapters {
			let json = Komiic::api(gen_chapter_body_string(&manga.key))?;
			let list = json
				.get("data")
				.and_then(|data| data.get("chaptersByComicId"))
				.and_then(|v| v.as_array())
				.map(|list| list.to_vec());
			manga.chapters = Some(Komiic::parse_chapter_list(&manga.key, list.as_ref()));
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let json = Komiic::api(gen_images_body_string(&chapter.key))?;
		let list = json
			.get("data")
			.and_then(|data| data.get("imagesByChapterId"))
			.and_then(|v| v.as_array())
			.map(|list| list.to_vec())
			.unwrap_or_default();

		Ok(list
			.iter()
			.map(|page| Page {
				content: PageContent::url(gen_page_url(
					&manga.key,
					&chapter.key,
					&jstr(page, "kid"),
				)),
				..Default::default()
			})
			.collect())
	}
}

impl ListingProvider for Komiic {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let mut is_recent_update = false;
		let mut order_by = String::new();

		match listing.id.as_str() {
			"recent" => is_recent_update = true,
			"month" => order_by = String::from("MONTH_VIEWS"),
			"history" => order_by = String::from("VIEWS"),
			_ => {
				return self.get_search_manga_list(None, page, Vec::new());
			}
		}

		let body = if is_recent_update {
			gen_recent_update_body_string(page)
		} else {
			gen_hot_body_string(order_by, page)
		};

		let json = Komiic::api(body)?;
		let key = if is_recent_update {
			"recentUpdate"
		} else {
			"hotComics"
		};
		let entries = json
			.get("data")
			.and_then(|data| data.get(key))
			.and_then(|v| v.as_array())
			.map(|list| list.to_vec());

		Ok(MangaPageResult {
			entries: Komiic::parse_manga_list(entries.as_ref()),
			has_next_page: true,
		})
	}
}

impl ImageRequestProvider for Komiic {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		let mut request = Request::get(&url)?.header("Referer", gen_referer(&url).as_str());

		if let Some(cookie) = defaults_get::<String>("cookie").filter(|cookie| !cookie.is_empty()) {
			request = request.header("Cookie", cookie.as_str());
		}

		Ok(request)
	}
}

fn gen_referer(image_url: &str) -> String {
	if image_url.starts_with(WWW_URL) {
		let query = substring_after(image_url, "?").unwrap_or_default();
		let manga_id = substring_after(query, "mangaId=")
			.and_then(|query| substring_before(query, "&"))
			.unwrap_or_default();
		let chapter_id = substring_after(query, "chapterId=").unwrap_or_default();
		gen_chapter_url(manga_id, chapter_id)
	} else {
		WWW_URL.to_string()
	}
}

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

fn gen_category_body_string(
	category: String,
	status: String,
	order_by: String,
	page: i32,
) -> String {
	let category_id = if category.is_empty() {
		"[]".to_string()
	} else {
		format!(r#"["{}"]"#, category)
	};
	format!(
		r#"{{
      "operationName": "comicByCategories",
      "query": "query comicByCategories($categoryId: [ID!]!, $pagination: Pagination!) {{\n  comicByCategories(categoryId: $categoryId, pagination: $pagination) {{\n    id\n    title\n    status\n    year\n    imageUrl\n    authors {{\n      id\n      name\n      __typename\n    }}\n    categories {{\n      id\n      name\n      __typename\n    }}\n    dateUpdated\n    monthViews\n    views\n    favoriteCount\n    lastBookUpdate\n    lastChapterUpdate\n    __typename\n  }}\n}}\n",
      "variables": {{
        "categoryId": {},
        "pagination": {{
            "asc": false,
            "limit": {},
            "offset": {},
            "orderBy": "{}",
						"status": "{}"
        }}
      }}
    }}"#,
		category_id,
		PAGE_SIZE,
		(page - 1) * PAGE_SIZE,
		order_by,
		status,
	)
}

fn gen_recent_update_body_string(page: i32) -> String {
	format!(
		r#"{{
      "operationName": "recentUpdate",
    	"query": "query recentUpdate($pagination: Pagination!) {{\n  recentUpdate(pagination: $pagination) {{\n    id\n    title\n    status\n    year\n    imageUrl\n    authors {{\n      id\n      name\n      __typename\n    }}\n    categories {{\n      id\n      name\n      __typename\n    }}\n    dateUpdated\n    monthViews\n    views\n    favoriteCount\n    lastBookUpdate\n    lastChapterUpdate\n    __typename\n  }}\n}}\n",
    	"variables": {{
        "pagination": {{
            "asc": true,
            "limit": {},
            "offset": {},
            "orderBy": "DATE_UPDATED"
        }}
    	}}
    }}"#,
		PAGE_SIZE,
		(page - 1) * PAGE_SIZE
	)
}

fn gen_hot_body_string(order_by: String, page: i32) -> String {
	format!(
		r#"{{
      "operationName": "hotComics",
    	"query": "query hotComics($pagination: Pagination!) {{\n  hotComics(pagination: $pagination) {{\n    id\n    title\n    status\n    year\n    imageUrl\n    authors {{\n      id\n      name\n      __typename\n    }}\n    categories {{\n      id\n      name\n      __typename\n    }}\n    dateUpdated\n    monthViews\n    views\n    favoriteCount\n    lastBookUpdate\n    lastChapterUpdate\n    __typename\n  }}\n}}\n",
    	"variables": {{
        "pagination": {{
            "asc": true,
            "limit": {},
            "offset": {},
            "orderBy": "{}",
            "status": ""
        }}
    	}}
    }}"#,
		PAGE_SIZE,
		(page - 1) * PAGE_SIZE,
		order_by,
	)
}

fn gen_search_body_string(query: String) -> String {
	format!(
		r#"{{
      "operationName": "searchComicAndAuthorQuery",
      "query": "query searchComicAndAuthorQuery($keyword: String!) {{\n  searchComicsAndAuthors(keyword: $keyword) {{\n    comics {{\n      id\n      title\n      status\n      year\n      imageUrl\n      authors {{\n        id\n        name\n        __typename\n      }}\n      categories {{\n        id\n        name\n        __typename\n      }}\n      dateUpdated\n      monthViews\n      views\n      favoriteCount\n      lastBookUpdate\n      lastChapterUpdate\n      __typename\n    }}\n    authors {{\n      id\n      name\n      chName\n      enName\n      wikiLink\n      comicCount\n      views\n      __typename\n    }}\n    __typename\n  }}\n}}\n",
      "variables": {{
        "keyword": "{}"
      }}
    }}"#,
		query,
	)
}

fn gen_id_body_string(id: &str) -> String {
	format!(
		r#"{{
      "operationName": "comicById",
      "query": "query comicById($comicId: ID!) {{\n  comicById(comicId: $comicId) {{\n    id\n    title\n    status\n    year\n    imageUrl\n    authors {{\n      id\n      name\n      __typename\n    }}\n    categories {{\n      id\n      name\n      __typename\n    }}\n    dateCreated\n    dateUpdated\n    views\n    favoriteCount\n    lastBookUpdate\n    lastChapterUpdate\n    __typename\n  }}\n}}\n",
      "variables": {{
        "comicId": "{}"
      }}
    }}"#,
		id
	)
}

fn gen_chapter_body_string(id: &str) -> String {
	format!(
		r#"{{
      "operationName": "chapterByComicId",
      "query": "query chapterByComicId($comicId: ID!) {{\n  chaptersByComicId(comicId: $comicId) {{\n    id\n    serial\n    type\n    dateCreated\n    dateUpdated\n    size\n    __typename\n  }}\n}}\n",
      "variables": {{
        "comicId": "{}"
      }}
    }}"#,
		id
	)
}

fn gen_images_body_string(id: &str) -> String {
	format!(
		r#"{{
      "operationName": "imagesByChapterId",
      "query": "query imagesByChapterId($chapterId: ID!) {{\n  imagesByChapterId(chapterId: $chapterId) {{\n    id\n    kid\n    height\n    width\n    __typename\n  }}\n}}\n",
      "variables": {{
        "chapterId": "{}"
      }}
    }}"#,
		id
	)
}

register_source!(Komiic, ListingProvider, ImageRequestProvider);
