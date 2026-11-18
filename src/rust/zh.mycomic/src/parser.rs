use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	imports::html::{Document, Element},
	prelude::*,
	Chapter, ContentRating, Manga, MangaStatus, Page, PageContent, Viewer,
};
use serde_json::Value;

pub const WWW_URL: &str = "https://mycomic.com/cn";

pub const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

fn doc_text(document: &Document, selector: &str) -> String {
	document
		.select(selector)
		.and_then(|list| list.text())
		.unwrap_or_default()
}

fn doc_attr(document: &Document, selector: &str, key: &str) -> String {
	document
		.select(selector)
		.and_then(|list| list.first())
		.and_then(|node| node.attr(key))
		.unwrap_or_default()
}

fn element_first(element: &Element, selector: &str) -> Option<Element> {
	element.select(selector).and_then(|list| list.first())
}

fn substring_after<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value
		.find(pattern)
		.map(|index| &value[index + pattern.len()..])
}

fn substring_before<'a>(value: &'a str, pattern: &str) -> Option<&'a str> {
	value.find(pattern).map(|index| &value[..index])
}

fn char_len(byte: u8) -> usize {
	if byte < 0x80 {
		1
	} else if byte >> 5 == 0b110 {
		2
	} else if byte >> 4 == 0b1110 {
		3
	} else if byte >> 3 == 0b11110 {
		4
	} else {
		1
	}
}

fn is_space(byte: u8) -> bool {
	matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn parse_number_at(bytes: &[u8], start: usize) -> Option<(f32, usize)> {
	let mut index = start;
	let mut seen_dot = false;

	while index < bytes.len() {
		let byte = bytes[index];
		if byte.is_ascii_digit() {
			index += 1;
		} else if byte == b'.'
			&& !seen_dot
			&& matches!(bytes.get(index + 1), Some(next) if next.is_ascii_digit())
		{
			seen_dot = true;
			index += 1;
		} else {
			break;
		}
	}

	if index == start {
		return None;
	}

	let text = core::str::from_utf8(&bytes[start..index]).ok()?;
	let value = text.parse::<f32>().ok()?;

	Some((value, index))
}

pub fn extract_chapter_number(title: &str) -> Option<f32> {
	const UNITS: [&str; 7] = ["话", "話", "章", "回", "卷", "册", "冊"];
	let bytes = title.as_bytes();
	let mut index = 0;

	while index < bytes.len() {
		if bytes[index..].starts_with("第".as_bytes()) {
			let mut next = index + "第".len();
			while next < bytes.len() && is_space(bytes[next]) {
				next += 1;
			}
			if let Some((value, _)) = parse_number_at(bytes, next) {
				return Some(value);
			}
		}

		if let Some((value, end)) = parse_number_at(bytes, index) {
			let mut next = end;
			while next < bytes.len() && is_space(bytes[next]) {
				next += 1;
			}
			for unit in UNITS {
				if bytes[next..].starts_with(unit.as_bytes()) {
					return Some(value);
				}
			}
		}

		index += char_len(bytes[index]);
	}

	None
}

fn manga_key(item: &Element) -> String {
	element_first(item, "a")
		.and_then(|node| node.attr("href"))
		.unwrap_or_default()
		.split('/')
		.map(ToString::to_string)
		.filter(|part| !part.is_empty())
		.collect::<Vec<String>>()
		.pop()
		.unwrap_or_default()
}

pub fn parse_manga_list(document: Document) -> Vec<Manga> {
	let mut mangas: Vec<Manga> = Vec::new();
	let groups = document
		.select(".group")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for item in groups {
		let key = manga_key(&item);
		let img = element_first(&item, "a>img");
		let mut cover = img
			.as_ref()
			.and_then(|node| node.attr("data-src"))
			.unwrap_or_default();
		if cover.is_empty() {
			cover = img
				.as_ref()
				.and_then(|node| node.attr("src"))
				.unwrap_or_default();
		}
		let title = img.and_then(|node| node.attr("alt")).unwrap_or_default();
		mangas.push(Manga {
			key,
			cover: Some(cover),
			title,
			..Default::default()
		});
	}

	mangas
}

pub fn parse_manga(key: String, document: Document) -> Manga {
	let cdn_url = doc_attr(&document, "body[x-data]", "x-data");
	let cdn_base = if let Some(start) = cdn_url.find("cdnUrl: '") {
		if let Some(end) = cdn_url[start + 9..].find('\'') {
			format!("https://{}", &cdn_url[start + 9..start + 9 + end])
		} else {
			String::from("https://biccam.com")
		}
	} else {
		String::from("https://biccam.com")
	};

	let mut cover = doc_attr(&document, "meta[name='og:image']", "content");
	if cover.is_empty() {
		cover = doc_attr(&document, "img.object-cover", "src");
	}
	if !cover.is_empty() && !cover.starts_with("http") {
		if cover.starts_with("//") {
			cover = format!("https:{}", cover);
		} else if cover.starts_with('/') {
			cover = format!("{}{}", cdn_base, cover);
		}
	}
	let title = doc_text(&document, "title").replace(" - MYCOMIC - 我的漫画", "");
	let authors = doc_attr(&document, "meta[name='author']", "content");
	let mut description = doc_text(&document, "div[x-show='show']").trim().to_string();
	if description.is_empty() {
		description = doc_attr(&document, "meta[name='description']", "content")
			.trim()
			.to_string();
	}
	let categories = document
		.select("a[href*='tag']")
		.map(|list| {
			list.map(|node| node.text().unwrap_or_default())
				.collect::<Vec<String>>()
		})
		.unwrap_or_default();
	let status = match doc_text(&document, "div[data-flux-badge]").trim() {
		"连载中" => MangaStatus::Ongoing,
		"已完结" => MangaStatus::Completed,
		_ => MangaStatus::Unknown,
	};
	let content_rating = ContentRating::Safe;
	let viewer = Viewer::RightToLeft;
	let url = format!("{}/comics/{}", WWW_URL, key);

	Manga {
		key,
		title,
		cover: Some(cover),
		authors: if authors.is_empty() {
			None
		} else {
			Some(vec![authors])
		},
		description: Some(description),
		url: Some(url),
		tags: Some(categories),
		status,
		content_rating,
		viewer,
		..Default::default()
	}
}

pub fn parse_chapter_list(document: Document) -> Vec<Chapter> {
	let mut all_chapters: Vec<Chapter> = Vec::new();
	let elements = document
		.select("div[x-data*='chapters']")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for element in elements {
		let scanlator = element_first(&element, "div[data-flux-subheading] div")
			.and_then(|node| node.text())
			.unwrap_or_default()
			.trim()
			.to_string();

		let data = element.attr("x-data").unwrap_or_default();
		let text = match substring_after(&data, "chapters:") {
			Some(text) => text,
			None => continue,
		};
		let text = match substring_before(text, "],") {
			Some(text) => text,
			None => continue,
		};
		let text = text.trim().to_string() + "]";
		let data: Value = match serde_json::from_str(&text) {
			Ok(data) => data,
			Err(_) => continue,
		};
		let list = match data.as_array() {
			Some(list) => list.clone(),
			None => continue,
		};
		let len = list.len();

		for (index, item) in list.iter().enumerate() {
			let chapter_id = match item.get("id") {
				Some(Value::String(value)) => value.clone(),
				Some(value) => value.to_string(),
				None => continue,
			};
			let title = item
				.get("title")
				.and_then(Value::as_str)
				.unwrap_or_default()
				.to_string();

			let chapter_num = (len - index) as f32;
			let chapter_or_volume = extract_chapter_number(&title).unwrap_or(chapter_num);
			let (chapter, volume) = if title.trim().ends_with('卷') {
				(None, Some(chapter_or_volume))
			} else {
				(Some(chapter_or_volume), None)
			};

			let chapter_url = format!("{}/chapters/{}", WWW_URL, chapter_id);
			all_chapters.push(Chapter {
				key: chapter_id,
				title: Some(title),
				volume_number: volume,
				chapter_number: chapter,
				url: Some(chapter_url),
				scanlators: if scanlator.is_empty() {
					None
				} else {
					Some(vec![scanlator.clone()])
				},
				..Default::default()
			});
		}
	}

	all_chapters
}

pub fn parse_page_list(document: Document) -> Vec<Page> {
	let mut pages: Vec<Page> = Vec::new();
	let images = document
		.select("img.page")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for item in images {
		let url = if item.has_attr("data-src") {
			item.attr("data-src").unwrap_or_default()
		} else {
			item.attr("src").unwrap_or_default()
		};
		pages.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	pages
}
