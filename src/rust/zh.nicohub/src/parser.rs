use aidoku::prelude::*;
use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	Chapter, ContentRating, Manga, MangaStatus, Page, PageContent, Viewer,
};
use serde_json::Value;

pub const WWW_URL: &str = "https://nicohub.cc";

fn jstr(value: &Value, key: &str) -> String {
	value
		.get(key)
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string()
}

fn filter_keys(value: &Value) -> Vec<String> {
	value
		.as_object()
		.map(|object| object.keys().cloned().collect::<Vec<String>>())
		.unwrap_or_default()
}

pub fn parse_manga_list(manga_list: &[Value]) -> Vec<Manga> {
	manga_list.iter().map(parse_manga).collect::<Vec<Manga>>()
}

pub fn parse_manga(manga: &Value) -> Manga {
	let key = jstr(manga, "id");
	let cover = jstr(manga, "cover");
	let title = jstr(manga, "name");
	let authors = manga
		.get("author")
		.and_then(Value::as_array)
		.map(|authors| {
			authors
				.iter()
				.filter_map(|author| author.as_str().map(ToString::to_string))
				.collect::<Vec<String>>()
		})
		.unwrap_or_default()
		.join(", ");
	let description = jstr(manga, "introduction");
	let url = format!("{}/comic/info/{}", WWW_URL, key);
	let filters = manga.get("type").cloned().unwrap_or(Value::Null);
	let categories = filter_keys(filters.get("题材").unwrap_or(&Value::Null));
	let status = match filter_keys(filters.get("连载").unwrap_or(&Value::Null))
		.pop()
		.unwrap_or_default()
		.as_str()
	{
		"连载中" => MangaStatus::Ongoing,
		"已完结" => MangaStatus::Completed,
		_ => MangaStatus::Unknown,
	};
	let content_rating = ContentRating::Safe;
	let viewer = Viewer::RightToLeft;

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
		tags: if categories.is_empty() {
			None
		} else {
			Some(categories)
		},
		status,
		content_rating,
		viewer,
		..Default::default()
	}
}

pub fn parse_chapter_list(chapter_list: &[Value]) -> Vec<Chapter> {
	let mut chapters: Vec<Chapter> = Vec::new();

	for item in chapter_list {
		let key = jstr(item, "id");
		let title = jstr(item, "name");
		let chapter = item.get("sort").and_then(Value::as_f64).unwrap_or(0.0) as f32;
		chapters.push(Chapter {
			key,
			title: Some(title),
			chapter_number: Some(chapter),
			..Default::default()
		});
	}

	chapters
}

pub fn parse_page_list(page_list: &[Value]) -> Vec<Page> {
	let mut pages: Vec<Page> = Vec::new();

	for item in page_list {
		let url = jstr(item, "path");
		pages.push(Page {
			content: PageContent::url(url),
			..Default::default()
		});
	}

	pages
}
