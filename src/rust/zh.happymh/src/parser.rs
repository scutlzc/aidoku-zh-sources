use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	prelude::*,
	Chapter, ContentRating, Manga, MangaStatus, Page, PageContent, Viewer,
};
use serde_json::Value;

pub const WWW_URL: &str = "https://m.happymh.com";
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

fn jstr(item: &Value, key: &str) -> String {
	item.get(key)
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string()
}

pub fn parse_manga_list(items: &[Value]) -> Vec<Manga> {
	let mut mangas: Vec<Manga> = Vec::new();

	for item in items {
		let key = jstr(item, "manga_code");
		let cover = jstr(item, "cover");
		let title = jstr(item, "name");
		mangas.push(Manga {
			key,
			cover: Some(cover),
			title,
			..Default::default()
		});
	}

	mangas
}

pub fn parse_rank_list(document: aidoku::imports::html::Document) -> Vec<Manga> {
	let mut mangas: Vec<Manga> = Vec::new();
	let list = document
		.select(".manga-rank")
		.map(|list| list.collect::<Vec<_>>())
		.unwrap_or_default();

	for item in list {
		let key = item
			.select(".manga-rank-cover>a")
			.and_then(|list| list.first())
			.and_then(|node| node.attr("href"))
			.unwrap_or_default()
			.split('/')
			.filter(|part| !part.is_empty())
			.map(ToString::to_string)
			.collect::<Vec<String>>()
			.pop()
			.unwrap_or_default();
		let cover = item
			.select(".manga-rank-cover>a>mip-img")
			.and_then(|list| list.first())
			.and_then(|node| node.attr("src"))
			.unwrap_or_default();
		let title = item
			.select(".manga-title")
			.and_then(|node| node.text())
			.unwrap_or_default()
			.trim()
			.to_string();
		mangas.push(Manga {
			key,
			cover: Some(cover),
			title,
			..Default::default()
		});
	}

	mangas
}

pub fn parse_manga(key: String, document: aidoku::imports::html::Document) -> Manga {
	let cover = document
		.select(".mg-cover>mip-img")
		.and_then(|list| list.first())
		.and_then(|node| node.attr("src"))
		.unwrap_or_default();
	let title = document
		.select("h2.mg-title")
		.and_then(|node| node.text())
		.unwrap_or_default();
	let authors = document
		.select(".mg-sub-title>a")
		.map(|list| {
			list.map(|node| node.text().unwrap_or_default())
				.collect::<Vec<String>>()
		})
		.unwrap_or_default()
		.join(", ");
	let description = document
		.select("#showmore")
		.and_then(|node| node.text())
		.unwrap_or_default()
		.trim()
		.to_string();
	let categories = document
		.select(".mg-cate>a")
		.map(|list| {
			list.map(|node| node.text().unwrap_or_default())
				.collect::<Vec<String>>()
		})
		.unwrap_or_default();
	let status = MangaStatus::Unknown;
	let content_rating = ContentRating::Safe;
	let viewer = Viewer::Webtoon;
	let url = format!("{}/manga/{}", WWW_URL, key);

	Manga {
		key,
		title,
		cover: Some(cover),
		authors: Some(vec![authors]),
		description: Some(description),
		url: Some(url),
		tags: Some(categories),
		status,
		content_rating,
		viewer,
		..Default::default()
	}
}

pub fn parse_chapter_list(list: &[Value]) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	for (index, item) in list.iter().enumerate() {
		let key = jstr(item, "codes");
		let title = jstr(item, "chapterName");
		let chapter_number = (index + 1) as f32;
		let url = format!("{}/mangaread/{}", WWW_URL, key);
		chapters.push(Chapter {
			key,
			title: Some(title),
			chapter_number: Some(chapter_number),
			url: Some(url),
			..Default::default()
		});
	}
	chapters.reverse();

	chapters
}

pub fn parse_page_list(scans: &[Value]) -> Vec<Page> {
	let mut pages: Vec<Page> = Vec::new();

	for item in scans {
		let url = jstr(item, "url");
		pages.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	pages
}
