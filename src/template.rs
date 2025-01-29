use regex::Regex;
use tera::{Context, Tera};

pub fn playing_template(
    tera: Tera,
    title: &String,
    artist: &String,
    image_encoded: &String,
    color_mode: Option<&String>,
    fill: Option<&String>,
) -> Result<String, String> {
    let mut context = Context::new();

    context.insert("title", title);
    context.insert("artist", artist);
    context.insert(
        "image",
        &format!("data:image/jpeg;base64,{}", image_encoded),
    );

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
