#![no_std]

use aidoku::{
	alloc::{string::String, string::ToString, vec::Vec},
	helpers::uri::encode_uri,
	imports::{html::Element, net::Request},
	prelude::*,
	Chapter, ContentRating, FilterValue, ImageRequestProvider, Manga, MangaPageResult, MangaStatus,
	Page, PageContent, PageContext, Result, Source, Viewer,
};

mod helper;

const FILTER_ODFIE: [&str; 2] = ["addtime", "edittime"];

struct ZeroByWns;

fn last_segment(value: &str) -> String {
	value
		.split('=')
		.last()
		.unwrap_or_default()
		.trim()
		.to_string()
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

		if query.is_empty() {
			let mut url = format!(
				"{}/plugin.php?id=jameson_manhua&c=index&a=ku",
				helper::get_url()
			);

			if !category_id.is_empty() {
				url.push_str(&format!("&category_id={}", category_id));
			}

			if !jindu.is_empty() {
				url.push_str(&format!("&jindu={}", jindu));
			}

			if !shuxing.is_empty() {
				url.push_str(&format!("&shuxing={}", encode_uri(shuxing)));
			}

			if !area.is_empty() {
				url.push_str(&format!("&area={}", encode_uri(area)));
			}

			url.push_str(&format!("&odfie={}&order={}&page={}", odfie, order, page));

			let html = helper::get_html(url)?;

			if let Some(items) = html.select(".uk-card") {
				for item in items {
					let id = last_segment(
						&item
							.select_first("div:nth-child(1)>a")
							.and_then(|a| a.attr("href"))
							.unwrap_or_default(),
					);
					let cover = item
						.select_first("div:nth-child(1)>a>img")
						.and_then(|img| img.attr("src"))
						.unwrap_or_default();
					let title = item
						.select_first("div:nth-child(2)>p>a")
						.and_then(|p| p.text())
						.unwrap_or_default()
						.trim()
						.to_string();
					mangas.push(Manga {
						key: id,
						cover: if cover.is_empty() { None } else { Some(cover) },
						title,
						..Default::default()
					});
				}
			}
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
					let id = last_segment(&item.attr("href").unwrap_or_default());
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
					mangas.push(Manga {
						key: id,
						cover: if cover.is_empty() { None } else { Some(cover) },
						title,
						..Default::default()
					});
				}
			}
		}

		Ok(MangaPageResult {
			entries: mangas,
			has_next_page: true,
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

		let manga_url = format!(
			"{}/plugin.php?id=jameson_manhua&c=index&a=bofang&kuid={}",
			helper::get_url(),
			manga.key
		);
		let html = helper::get_html(manga_url.clone())?;

		if needs_details {
			let cover = html
				.select_first(".uk-width-medium>img")
				.and_then(|img| img.attr("src"))
				.unwrap_or_default();
			let title = html
				.select_first(".uk-margin-left>ul>li>h3")
				.and_then(|h3| h3.text())
				.unwrap_or_default();
			let authors: Vec<String> = html
				.select_first(".uk-margin-left>ul>li>.cl>a[href*='zuozhe']")
				.and_then(|a| a.text())
				.unwrap_or_default()
				.replace("作者:", "")
				.split('×')
				.map(|a| a.trim().to_string())
				.filter(|a| !a.is_empty())
				.collect();
			let description = html
				.select_first(".uk-margin-left>ul>li>.uk-alert")
				.and_then(|div| div.text())
				.unwrap_or_default()
				.trim()
				.to_string();
			let tags: Vec<String> = html
				.select(".uk-margin-left>ul>li>.cl>a[href*='category']")
				.map(|links| {
					links
						.into_iter()
						.filter_map(|a| a.text())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();
			let status = match html
				.select_first(".uk-margin-left>ul>li>.cl>span:nth-child(6)")
				.and_then(|span| span.text())
				.unwrap_or_default()
				.as_str()
			{
				"连载中" => MangaStatus::Ongoing,
				"已完结" => MangaStatus::Completed,
				_ => MangaStatus::Unknown,
			};

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

			if let Some(items) = html.select(".muludiv>a") {
				for (index, item) in items.into_iter().enumerate() {
					let key = last_segment(&item.attr("href").unwrap_or_default());
					let title = item.text().unwrap_or_default();
					let url = format!(
						"{}/plugin.php?id=jameson_manhua&a=read&zjid={}",
						helper::get_url(),
						key
					);
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
		let url = format!(
			"{}/plugin.php?id=jameson_manhua&a=read&zjid={}",
			helper::get_url(),
			chapter.key
		);
		let html = helper::get_html(url)?;
		let text = Element::from(html).html().unwrap_or_default();
		let list = substring_after(&text, "let listimg=")
			.and_then(|text| substring_before(text, ";"))
			.unwrap_or_default();
		let list = match serde_json::from_str::<serde_json::Value>(list) {
			Ok(list) => list,
			Err(_) => bail!("failed to parse page list"),
		};

		let mut pages: Vec<Page> = Vec::new();

		if let Some(items) = list.as_array() {
			for item in items {
				let url = item
					.get("file")
					.and_then(|file| file.as_str())
					.unwrap_or_default();

				if url.is_empty() {
					continue;
				}

				pages.push(Page {
					content: PageContent::url(url),
					..Default::default()
				});
			}
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

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

register_source!(ZeroByWns, ImageRequestProvider);
