use tera::Tera;
use usvg::{Options, Tree};

use crate::template::font_template;

pub fn text_width(
    tera: &Tera,
    content: &String,
    font_family: Option<&str>,
    font_size: Option<i32>,
    font_weight: Option<&str>,
) -> Result<f32, String> {
    let template = font_template(&tera, &content, font_family, font_size, font_weight)?;

    let mut opt = Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = match Tree::from_str(template.as_str(), &opt) {
        Ok(val) => val,
        Err(err) => {
            return Err(format!(
                "An error occurred while rendering tree: {:#?}",
                err
            ))
        }
    };

    let bounding_box = match tree.root().children().first() {
        Some(val) => val.bounding_box(),
        _ => return Err("Tree does not contain any children".to_string()),
    };

    Ok(bounding_box.right() - bounding_box.left())
}
