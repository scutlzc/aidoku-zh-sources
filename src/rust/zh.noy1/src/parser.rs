use aidoku::{
	alloc::{string::ToString, vec, String, Vec},
	ContentRating, Manga, MangaStatus, Viewer,
};
use serde_json::Value;

pub fn parse_manga_list(manga_list: &[Value]) -> Vec<Manga> {
	manga_list.iter().map(parse_manga).collect::<Vec<Manga>>()
}

pub fn parse_manga(manga: &Value) -> Manga {
	let id = manga
		.get("Bid")
		.and_then(Value::as_i64)
		.map(|value| value.to_string())
		.or_else(|| {
			manga
				.get("Bid")
				.and_then(Value::as_str)
				.map(ToString::to_string)
		})
		.unwrap_or_default();
	let cover = crate::helper::gen_cover_url(id.as_str());
	let title = manga
		.get("Bookname")
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string();
	let author = manga
		.get("Author")
		.and_then(Value::as_str)
		.unwrap_or_default()
		.to_string();
	let description = String::new();
	let url = crate::helper::gen_manga_url(id.as_str());
	let categories = manga
		.get("Ptag")
		.and_then(Value::as_str)
		.unwrap_or_default()
		.split(' ')
		.map(ToString::to_string)
		.collect::<Vec<String>>();
	let status = MangaStatus::Completed;
	let content_rating = ContentRating::NSFW;
	let viewer = Viewer::RightToLeft;

	Manga {
		key: id,
		title,
		cover: Some(cover),
		authors: Some(vec![author]),
		description: Some(description),
		url: Some(url),
		tags: Some(categories),
		status,
		content_rating,
		viewer,
		..Default::default()
	}
}
