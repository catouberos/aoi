use std::usize;

use regex::Regex;
use tera::{Context, Tera};
use unicode_segmentation::UnicodeSegmentation;

use crate::render::text_width;

pub fn font_template(
    tera: &Tera,
    content: &String,
    font_family: Option<&str>,
    font_size: Option<i32>,
    font_weight: Option<&str>,
) -> Result<String, String> {
    let mut context = Context::new();

    context.insert("content", content);

    if let Some(font_family) = font_family {
        context.insert("font_family", font_family);
    }

    if let Some(font_size) = font_size {
        context.insert("font_size", &font_size);
    }

    if let Some(font_weight) = font_weight {
        context.insert("font_weight", font_weight);
    }

    let template = tera.render("font.html", &context).unwrap().to_string();

    Ok(template)
}

pub fn playing_template(
    tera: &Tera,
    width: i32,
    height: i32,
    title: &String,
    artist: &String,
    image_encoded: &String,
    color_mode: Option<&String>,
    fill: Option<&String>,
    transparent: bool,
    listening: bool,
) -> Result<String, String> {
    let mut context = Context::new();
    let mut title: String = title.clone();
    let mut artist: String = artist.clone();

    if let Ok(ellipsised) = text_ellipsis(
        &tera,
        width - height - 24,
        Some("Inter"),
        Some(36),
        Some("700"),
        &title,
    ) {
        title = ellipsised;
    }

    if let Ok(ellipsised) = text_ellipsis(
        &tera,
        width - height - 24,
        Some("Inter"),
        Some(28),
        Some("400"),
        &artist,
    ) {
        artist = ellipsised;
    }

    context.insert("width", &width);
    context.insert("height", &height);
    context.insert("title", &title);
    context.insert("artist", &artist);
    context.insert(
        "image",
        &format!("data:image/jpeg;base64,{}", image_encoded),
    );
    context.insert("transparent", &transparent);
    context.insert("listening", &listening);

    match color_mode {
        Some(val) => match val.as_str() {
            "dark" => context.insert("dark", "dark"),
            "light" => context.insert("light", "light"),
            _ => (),
        },
        _ => (),
    }

    let re = Regex::new(r"\#[abcdefABCDEF\d]{3,6}").unwrap();

    match fill {
        Some(val) => {
            if re.is_match(val) {
                context.insert("fill", val);
            }
        }
        _ => (),
    }

    let template = tera.render("widget.html", &context).unwrap().to_string();

    Ok(template)
}

pub fn text_ellipsis(
    tera: &Tera,
    width: i32,
    font_family: Option<&str>,
    font_size: Option<i32>,
    font_weight: Option<&str>,
    content: &String,
) -> Result<String, String> {
    let graphemes = content.graphemes(true);
    let size: usize = graphemes.clone().count();
    let mut end = size;

    loop {
        let mut text: Vec<&str> = graphemes.clone().collect::<Vec<&str>>()[0..end].to_vec();

        if text.len() > 0 && end < size {
            text.push("…");
        }

        let text_width = text_width(&tera, &text.concat(), font_family, font_size, font_weight)
            .unwrap_or_default();

        if text_width > width as f32 {
            end -= 1;
            continue;
        }

        return Ok(text.concat());
    }
}
