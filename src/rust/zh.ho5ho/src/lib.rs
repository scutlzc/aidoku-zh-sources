#![no_std]

use aidoku::{
	alloc::{string::ToString, String, Vec},
	helpers::uri::encode_uri,
	imports::net::Request,
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};
use serde_json::Value;

const WWW_URL: &str = "https://www.ho5ho.com";
const MANGA_URL: &str = "https://www.ho5ho.com/%E4%B8%AD%E5%AD%97h%E6%BC%AB";
const GENRE_URL: &str = "https://www.ho5ho.com/h%E6%BC%AB%E5%88%86%E9%A1%9E";

const FILTER_SORT: [&str; 3] = ["latest", "rating", "views"];

struct Ho5Ho;

impl Ho5Ho {
	fn parse_manga_list(url: String) -> Result<Vec<Manga>> {
		let html = Request::get(url)?.html()?;
		let mut entries: Vec<Manga> = Vec::new();

		let Some(items) = html.select("article.ho5ho-v2-card") else {
			return Ok(entries);
		};

		for item in items {
			let Some(link) = item.select_first("a.ho5ho-v2-card-cover") else {
				continue;
			};
			let href = link.attr("href").unwrap_or_default();
			let Some(key) = href.split('/').filter(|part| !part.is_empty()).next_back() else {
				continue;
			};
			let cover = item
				.select_first("img")
				.and_then(|img| img.attr("src"))
				.unwrap_or_default();
			let title = item
				.select_first("h3.ho5ho-v2-card-title>a")
				.and_then(|title| title.text())
				.or_else(|| link.attr("aria-label"))
				.or_else(|| item.select_first("img").and_then(|img| img.attr("alt")))
				.unwrap_or_default();
			entries.push(Manga {
				key: key.to_string(),
				title,
				cover: Some(encode_uri(cover)),
				..Default::default()
			});
		}

		Ok(entries)
	}

	fn parse_chapter_list(html: &aidoku::imports::html::Document) -> Result<Vec<Chapter>> {
		let mut chapters: Vec<Chapter> = Vec::new();

		if let Some(items) = html.select(".wp-manga-chapter>a") {
			for item in items {
				let url = item.attr("href").unwrap_or_default();
				let Some(key) = url
					.split('/')
					.filter(|part| !part.is_empty())
					.next_back()
					.map(|part| part.to_string())
				else {
					continue;
				};
				let title = item.text().unwrap_or_default();
				chapters.push(Chapter {
					key,
					title: Some(title),
					url: Some(url),
					..Default::default()
				});
			}
		}
		chapters.reverse();
		for (index, chapter) in chapters.iter_mut().enumerate() {
			chapter.chapter_number = Some((index + 1) as f32);
		}

		Ok(chapters)
	}
}

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

// 取出文本开头处的完整 JSON 数组字面量，跳过字符串内部的括号。
// 图片数组后面紧跟的是 `, chapter_images_per_page = 3;`，不能按 `];` 截断。
fn json_array(value: &str) -> Option<&str> {
	let bytes = value.as_bytes();
	let mut index = 0;
	while index < bytes.len() && bytes[index].is_ascii_whitespace() {
		index += 1;
	}
	if bytes.get(index) != Some(&b'[') {
		return None;
	}
	let start = index;
	let mut depth = 0usize;
	let mut in_string = false;
	let mut escaped = false;
	while index < bytes.len() {
		let byte = bytes[index];
		if in_string {
			if escaped {
				escaped = false;
			} else if byte == b'\\' {
				escaped = true;
			} else if byte == b'"' {
				in_string = false;
			}
		} else if byte == b'"' {
			in_string = true;
		} else if byte == b'[' {
			depth += 1;
		} else if byte == b']' {
			depth -= 1;
			if depth == 0 {
				return Some(&value[start..index + 1]);
			}
		}
		index += 1;
	}
	None
}

impl Source for Ho5Ho {
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
		let mut sort = String::from("latest");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "category" {
						category = value;
					}
				}
				FilterValue::Sort { index, .. } => {
					sort = FILTER_SORT
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
			let listing = if category.is_empty() {
				WWW_URL.to_string()
			} else {
				format!("{GENRE_URL}/{}", encode_uri(category))
			};
			if page > 1 {
				format!("{listing}/page/{page}/?m_orderby={sort}")
			} else {
				format!("{listing}/?m_orderby={sort}")
			}
		} else if page > 1 {
			format!(
				"{WWW_URL}/page/{page}/?s={}&post_type=wp-manga",
				encode_uri(query)
			)
		} else {
			format!("{WWW_URL}/?s={}&post_type=wp-manga", encode_uri(query))
		};

		Ok(MangaPageResult {
			entries: Ho5Ho::parse_manga_list(url)?,
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
			let url = format!("{MANGA_URL}/{}/", manga.key);
			let html = Request::get(&url)?.html()?;

			if needs_details {
				if let Some(title) = html
					.select_first("h1#ho5ho-detail-title")
					.and_then(|title| title.text())
				{
					manga.title = title;
				}
				let cover = html
					.select_first(".ho5ho-v3-cover>img")
					.and_then(|img| img.attr("src"))
					.unwrap_or_default();
				if !cover.is_empty() {
					manga.cover = Some(encode_uri(cover));
				}
				let mut authors: Vec<String> = Vec::new();
				let mut tags: Vec<String> = Vec::new();
				if let Some(groups) = html.select("section.ho5ho-v3-taxonomy-group") {
					for group in groups {
						let label = group
							.select_first("h3")
							.and_then(|label| label.text())
							.unwrap_or_default();
						let terms = if label == "作者" {
							&mut authors
						} else {
							&mut tags
						};
						if let Some(items) = group.select("div.ho5ho-v3-term-chips>a") {
							for item in items {
								let term = item.text().unwrap_or_default();
								if !term.is_empty() {
									terms.push(term);
								}
							}
						}
					}
				}
				manga.authors = if authors.is_empty() {
					None
				} else {
					Some(authors)
				};
				manga.tags = if tags.is_empty() { None } else { Some(tags) };
				// 新版详情页已移除连载状态字段，统一置为未知
				manga.status = MangaStatus::Unknown;
				// 新版详情页已移除简介，改用「別名」等作品资料兜底
				let mut details: Vec<String> = Vec::new();
				if let Some(items) = html.select(".ho5ho-v3-data-list>div") {
					for item in items {
						let label = item
							.select_first("dt")
							.and_then(|label| label.text())
							.unwrap_or_default();
						let value = item
							.select_first("dd")
							.and_then(|value| value.text())
							.unwrap_or_default();
						if !value.is_empty() {
							details.push(format!("{label}：{value}"));
						}
					}
				}
				manga.description = if details.is_empty() {
					None
				} else {
					Some(details.join("\n"))
				};
				manga.url = Some(url);
				manga.content_rating = ContentRating::NSFW;
				manga.viewer = Viewer::RightToLeft;
			}

			if needs_chapters {
				manga.chapters = Some(Ho5Ho::parse_chapter_list(&html)?);
			}
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let url = format!("{MANGA_URL}/{}/{}/", manga.key, chapter.key);
		let text = Request::get(url)?.string()?;
		let mut pages: Vec<Page> = Vec::new();

		let Some(list) = substring_after(&text, "var chapter_preloaded_images = ") else {
			return Ok(pages);
		};
		let Some(list) = json_array(list) else {
			return Ok(pages);
		};

		let Ok(list) = serde_json::from_str::<Value>(list) else {
			return Ok(pages);
		};
		let Some(list) = list.as_array() else {
			return Ok(pages);
		};

		for item in list.iter() {
			let Some(image) = item.as_str() else {
				continue;
			};
			pages.push(Page {
				content: PageContent::url(image.to_string()),
				..Default::default()
			});
		}

		Ok(pages)
	}
}

impl ImageRequestProvider for Ho5Ho {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", WWW_URL))
	}
}

register_source!(Ho5Ho, ImageRequestProvider);
