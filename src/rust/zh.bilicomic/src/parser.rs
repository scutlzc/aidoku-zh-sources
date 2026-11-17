use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	imports::html::{Document, Element, ElementList},
	prelude::*,
	Chapter, ContentRating, Manga, MangaStatus, Page, PageContent, Viewer,
};

pub const BASE_URL: &str = "https://www.bilimanga.net";
pub const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";

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

fn byte_at(bytes: &[u8], index: usize) -> Option<u8> {
	bytes.get(index).copied()
}

/// Reads one digit (ASCII or fullwidth) at `index`, returning the ASCII value and the
/// byte length consumed.
fn digit_at(bytes: &[u8], index: usize) -> Option<(char, usize)> {
	let byte = byte_at(bytes, index)?;

	if byte.is_ascii_digit() {
		return Some((byte as char, 1));
	}

	if byte == 0xEF {
		let second = byte_at(bytes, index + 1)?;
		let third = byte_at(bytes, index + 2)?;
		if second == 0xBC {
			// fullwidth digits ０-９ (EF BC 90 - EF BC 99)
			if (0x90..=0x99).contains(&third) {
				return Some(((b'0' + (third - 0x90)) as char, 3));
			}
		}
	}

	None
}

/// Reads a fullwidth full stop U+FF0E (EF BD A1).
fn is_fullwidth_dot(bytes: &[u8], index: usize) -> bool {
	byte_at(bytes, index) == Some(0xEF)
		&& byte_at(bytes, index + 1) == Some(0xBD)
		&& byte_at(bytes, index + 2) == Some(0xA1)
}

fn parse_number_at(bytes: &[u8], start: usize) -> Option<(f32, usize)> {
	let mut index = start;
	let mut text = String::new();
	let mut seen_dot = false;

	loop {
		if let Some((digit, len)) = digit_at(bytes, index) {
			text.push(digit);
			index += len;
			continue;
		}

		if !seen_dot && text.chars().count() > 0 {
			if byte_at(bytes, index) == Some(b'.') {
				if let Some((_, _)) = digit_at(bytes, index + 1) {
					text.push('.');
					seen_dot = true;
					index += 1;
					continue;
				}
			} else if is_fullwidth_dot(bytes, index) {
				let mut next = index + 3;
				while next < bytes.len() && is_space(bytes[next]) {
					next += 1;
				}
				if digit_at(bytes, index + 3).is_some() {
					text.push('.');
					seen_dot = true;
					index += 3;
					continue;
				}
			}
		}

		break;
	}

	if text.is_empty() {
		return None;
	}

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

pub fn element_first(element: &Element, selector: &str) -> Option<Element> {
	element
		.select(selector)
		.and_then(|list: ElementList| list.first())
}

pub fn doc_text(document: &Document, selector: &str) -> String {
	document
		.select(selector)
		.and_then(|list| list.text())
		.unwrap_or_default()
}

pub fn doc_attr(document: &Document, selector: &str, key: &str) -> String {
	document
		.select(selector)
		.and_then(|list| list.first())
		.and_then(|node| node.attr(key))
		.unwrap_or_default()
}

pub fn manga_id_from_href(href: &str) -> String {
	href.split('/')
		.map(ToString::to_string)
		.filter(|part| !part.is_empty())
		.collect::<Vec<String>>()
		.pop()
		.unwrap_or_default()
		.replace(".html", "")
}

pub fn item_manga(item: &Element) -> Manga {
	let key = manga_id_from_href(&item.attr("href").unwrap_or_default());
	let cover = element_first(item, ".book-cover>img")
		.and_then(|node| node.attr("data-src"))
		.unwrap_or_default();
	let title = item
		.select(".book-title")
		.and_then(|list| list.text())
		.unwrap_or_default();

	Manga {
		key,
		cover: Some(cover),
		title,
		..Default::default()
	}
}

pub fn parse_books(document: &Document) -> Vec<Manga> {
	let mut mangas: Vec<Manga> = Vec::new();
	let items = document
		.select(".book-li>a")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for item in items {
		mangas.push(item_manga(&item));
	}

	mangas
}

pub fn parse_search_list(document: &Document, by_filter: bool) -> (Vec<Manga>, bool) {
	let has_more = if by_filter {
		let current = doc_text(document, "#pagelink strong");
		let last = doc_text(document, "#pagelink .last");
		current != last
	} else {
		let next = doc_attr(document, "#pagelink .next", "href");
		let next = if next.is_empty() {
			String::from("#")
		} else {
			next
		};
		next != "#"
	};

	let alternate_url = doc_attr(document, "link[rel='alternate']", "href");
	let mangas = if alternate_url.contains("detail") {
		let key = manga_id_from_href(&alternate_url);
		let cover = doc_attr(document, ".book-cover", "src");
		let title = doc_text(document, "h1.book-title");
		vec![Manga {
			key,
			cover: Some(cover),
			title,
			..Default::default()
		}]
	} else {
		parse_books(document)
	};

	(mangas, has_more)
}

pub fn parse_manga(key: String, document: &Document) -> Manga {
	let cover = doc_attr(document, ".book-cover", "src");
	let title = doc_text(document, "h1.book-title");
	let authors = document
		.select(".authorname,.illname")
		.map(|list| {
			list.map(|node| node.text().unwrap_or_default())
				.collect::<Vec<String>>()
		})
		.unwrap_or_default()
		.join(", ");
	let description = doc_text(document, ".book-summary>content");
	let categories = document
		.select(".tag-small-group>.tag-small>a")
		.map(|list| {
			list.map(|node| node.text().unwrap_or_default())
				.collect::<Vec<String>>()
		})
		.unwrap_or_default();
	let status_text = doc_text(document, ".book-layout-inline");
	let first_status = status_text
		.trim()
		.split('|')
		.map(|part| part.trim().to_string())
		.collect::<Vec<String>>()
		.first()
		.cloned()
		.unwrap_or_default();
	let status = match first_status.as_str() {
		"連載" => MangaStatus::Ongoing,
		"完結" => MangaStatus::Completed,
		_ => MangaStatus::Unknown,
	};
	let content_rating = ContentRating::Safe;
	let viewer = Viewer::RightToLeft;
	let url = format!("{}/detail/{}.html", BASE_URL, key);

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

pub fn chapter_from_link(
	item: &Element,
	volume_number: Option<f32>,
	fallback_number: f32,
) -> Chapter {
	let chapter_href = item.attr("href").unwrap_or_default();
	let key = manga_id_from_href(&chapter_href);
	let title = item
		.select("span")
		.and_then(|list| list.text())
		.unwrap_or_default();
	let chapter_number = extract_chapter_number(&title).unwrap_or(fallback_number);
	let url = format!("{}{}", BASE_URL, chapter_href);

	Chapter {
		key,
		title: Some(title),
		chapter_number: Some(chapter_number),
		volume_number,
		url: Some(url),
		..Default::default()
	}
}

pub fn parse_page_list(document: &Document) -> Vec<Page> {
	let mut pages: Vec<Page> = Vec::new();
	let images = document
		.select("#acontentz>img")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for item in images {
		let url = item.attr("data-src").unwrap_or_default().trim().to_string();
		pages.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	pages
}
