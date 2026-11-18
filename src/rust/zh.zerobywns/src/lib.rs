#![no_std]

use aidoku::{
	alloc::{string::String, string::ToString, vec::Vec},
	helpers::uri::encode_uri,
	imports::{
		html::{Document, Element},
		net::Request,
	},
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};

mod helper;

const FILTER_ODFIE: [&str; 2] = ["addtime", "edittime"];
const LIST_PATH: &str = "/Android2/Android/";
const DETAIL_PATH: &str = "/Android2/details/";
const VIEW_PATH: &str = "/Android2/view/";
const IMAGE_MARKER: &str = "window.imagelist.push(\"";

struct ZeroByWns;

fn last_segment(value: &str) -> String {
	value
		.split('=')
		.last()
		.unwrap_or_default()
		.trim()
		.to_string()
}

fn has_next_page(html: Document) -> bool {
	Element::from(html)
		.html()
		.unwrap_or_default()
		.contains("下一页")
}

fn push_manga(mangas: &mut Vec<Manga>, key: String, cover: String, title: String) {
	if key.is_empty() || title.is_empty() {
		return;
	}

	mangas.push(Manga {
		key,
		cover: if cover.is_empty() { None } else { Some(cover) },
		title,
		..Default::default()
	});
}

impl Source for ZeroByWns {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let mut category_id = String::new();
		let mut jindu = String::new();
		let mut shuxing = String::new();
		let mut area = String::new();
		let mut odfie = String::from("addtime");
		let mut order = String::from("desc");

		for filter in filters {
			match filter {
				FilterValue::Select { id, value } => {
					if id == "category_id" {
						category_id = value;
					} else if id == "jindu" {
						jindu = value;
					} else if id == "shuxing" {
						shuxing = value;
					} else if id == "area" {
						area = value;
					}
				}
				FilterValue::Sort {
					index, ascending, ..
				} => {
					odfie = FILTER_ODFIE
						.get(index as usize)
						.copied()
						.unwrap_or_default()
						.to_string();

					if ascending {
						order = String::from("asc");
					}
				}
				_ => continue,
			}
		}

		let query = query.unwrap_or_default();
		let mut mangas: Vec<Manga> = Vec::new();
		let has_more;

		if query.is_empty() {
			let mut url = format!("{}{}?", helper::get_url(), LIST_PATH);
			let mut params: Vec<String> = Vec::new();

			if !category_id.is_empty() {
				params.push(format!("category_id={}", category_id));
			}

			if !jindu.is_empty() {
				params.push(format!("jindu={}", jindu));
			}

			if !shuxing.is_empty() {
				params.push(format!("shuxing={}", encode_uri(shuxing)));
			}

			if !area.is_empty() {
				params.push(format!("area={}", encode_uri(area)));
			}

			params.push(format!("odfie={}&order={}&page={}", odfie, order, page));
			url.push_str(&params.join("&"));

			let html = helper::get_html(url)?;

			if let Some(items) = html.select(".indexrow .col-50") {
				for item in items {
					let key = last_segment(
						&item
							.select_first("a")
							.and_then(|a| a.attr("href"))
							.unwrap_or_default(),
					);
					let cover = item
						.select_first("img")
						.and_then(|img| img.attr("src"))
						.unwrap_or_default();
					let title = item
						.select_first(".image-title")
						.and_then(|div| div.text())
						.unwrap_or_default()
						.trim()
						.to_string();
					push_manga(&mut mangas, key, cover, title);
				}
			}

			has_more = has_next_page(html);
		} else {
			let url = format!(
				"{}/plugin.php?id=jameson_manhua&c=index&a=search&keyword={}&page={}",
				helper::get_url(),
				encode_uri(query),
				page
			);
			let html = helper::get_html(url)?;

			if let Some(items) = html.select(".uk-card") {
				for item in items {
					let key = last_segment(&item.attr("href").unwrap_or_default());
					let cover = item
						.select_first("div:nth-child(1)>img")
						.and_then(|img| img.attr("src"))
						.unwrap_or_default();
					let title = item
						.select_first("div:nth-child(2)>p")
						.and_then(|p| p.text())
						.unwrap_or_default()
						.trim()
						.to_string();
					push_manga(&mut mangas, key, cover, title);
				}
			}

			has_more = has_next_page(html);
		}

		Ok(MangaPageResult {
			entries: mangas,
			has_next_page: has_more,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}

		let manga_url = format!("{}{}?kuid={}", helper::get_url(), DETAIL_PATH, manga.key);
		let html = helper::get_html(manga_url.clone())?;

		if needs_details {
			let cover = html
				.select_first(".col-33>img")
				.and_then(|img| img.attr("src"))
				.unwrap_or_default();
			let title = html
				.select_first(".col-66 h3")
				.and_then(|h3| h3.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			let description = html
				.select_first(".summary.back-ff")
				.and_then(|div| div.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			let tags: Vec<String> = html
				.select(".shuxinglabel>a[href*='category_id']")
				.map(|links| {
					links
						.into_iter()
						.filter_map(|a| a.text())
						.map(|a| a.trim().to_string())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();

			let mut authors: Vec<String> = Vec::new();
			let mut status = MangaStatus::Unknown;

			if let Some(spans) = html.select(".shuxinglabel>span") {
				for span in spans {
					let text = span.text().unwrap_or_default().trim().to_string();

					if let Some(value) = text
						.strip_prefix("作者:")
						.or_else(|| text.strip_prefix("作者："))
					{
						authors = value
							.split('×')
							.map(|author| author.trim().to_string())
							.filter(|author| !author.is_empty())
							.collect();
					} else if text.contains("连载") {
						status = MangaStatus::Ongoing;
					} else if text.contains("完结") {
						status = MangaStatus::Completed;
					}
				}
			}

			manga.cover = if cover.is_empty() { None } else { Some(cover) };
			manga.title = title;
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			manga.description = if description.is_empty() {
				None
			} else {
				Some(description)
			};
			manga.url = Some(manga_url);
			manga.tags = if tags.is_empty() { None } else { Some(tags) };
			manga.status = status;
			manga.content_rating = ContentRating::Safe;
			manga.viewer = Viewer::RightToLeft;
		}

		if needs_chapters {
			let mut chapters: Vec<Chapter> = Vec::new();

			if let Some(items) = html.select(".zjlist a") {
				for (index, item) in items.into_iter().enumerate() {
					let key = last_segment(&item.attr("href").unwrap_or_default());

					if key.is_empty() {
						continue;
					}

					let title = item.text().unwrap_or_default().trim().to_string();
					let url = format!("{}{}?zjid={}", helper::get_url(), VIEW_PATH, key);
					chapters.push(Chapter {
						key,
						title: Some(title),
						chapter_number: Some((index + 1) as f32),
						url: Some(url),
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
		let url = format!("{}{}?zjid={}", helper::get_url(), VIEW_PATH, chapter.key);
		let html = helper::get_html(url)?;
		let text = Element::from(html).html().unwrap_or_default();
		let mut pages: Vec<Page> = Vec::new();
		let mut remaining = text.as_str();

		while let Some(index) = remaining.find(IMAGE_MARKER) {
			let value = &remaining[index + IMAGE_MARKER.len()..];
			let end = match value.find('"') {
				Some(end) => end,
				None => break,
			};
			let path = &value[..end];
			remaining = &value[end..];

			if path.is_empty() {
				continue;
			}

			pages.push(Page {
				content: PageContent::url(path),
				..Default::default()
			});
		}

		if pages.is_empty() {
			bail!("no pages found, login or vip is required");
		}

		Ok(pages)
	}
}

impl ImageRequestProvider for ZeroByWns {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		let referer = helper::get_url();
		Ok(Request::get(url.as_str())?.header("Referer", referer.as_str()))
	}
}

register_source!(ZeroByWns, ImageRequestProvider);
