use std::{collections::HashMap, time::Duration};

use aoi::{
    playing::{cover_art_by_release_group, now_playing, previous_listen},
    template::playing_template,
};
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
    Router,
};
use listenbrainz::raw::Client;
use moka::future::Cache;
use reqwest::StatusCode;
use resvg::{render, tiny_skia::Pixmap};
use tera::Tera;
use usvg::{Options, Transform, Tree};

#[derive(Clone)]
pub struct AppState {
    pub tera: Tera,
    pub response_cache: Cache<String, Vec<u8>>,
    pub cover_art_cache: Cache<String, String>,
}

#[tokio::main]
async fn main() {
    // cache response data for 1 minute
    let response_cache = Cache::builder()
        .time_to_live(Duration::from_secs(60))
        .build();
    // cache cover art data for 5 days
    let cover_art_cache = Cache::builder()
        .time_to_live(Duration::from_secs(5 * 24 * 60 * 60))
        .build();

    let tera = match Tera::new("templates/**/*.html") {
        Ok(t) => t,
        Err(e) => {
            println!("Parsing error(s): {}", e);
            ::std::process::exit(1);
        }
    };

    // build our application with a single route
    let app = Router::new()
        .route("/{id}", get(get_playing_now))
        .route("/{id}/previous", get(get_playing_now))
        .with_state(AppState {
            tera,
            response_cache,
            cover_art_cache,
        });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_playing_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    const WIDTH: i32 = 1000;
    const HEIGHT: i32 = 200;

    let color_mode = params.get("color_mode");
    let fill = params.get("fill");
    let transparent = params.get("transparent");

    match state
        .response_cache
        .get(&format!(
            "{}-{}-{}-{}",
            id,
            color_mode.unwrap_or(&String::new()),
            fill.unwrap_or(&String::new()),
            transparent.unwrap_or(&String::new())
        ))
        .await
    {
        Some(val) => {
            println!("Cache HIT, user {}", id);
            return Ok((
                [
                    (header::CONTENT_TYPE, "image/png"),
                    (
                        header::CONTENT_DISPOSITION,
                        "inline; filename=\"now-playing.png\"",
                    ),
                ],
                val,
            ));
        }
        None => println!("Cache MISS, user {}", id),
    };

    let client = Client::new();

    let (listen, listening) = match now_playing(&client, &id).await {
        Ok(val) => (val, true),
        Err(_) => match previous_listen(&client, &id).await {
            Ok(val) => (val, false),
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("User has no listen history"),
                ))
            }
        },
    };

    let image = match state.cover_art_cache.get(&listen.release_group).await {
        Some(val) => {
            println!(
                "Cache HIT, getting cover art of release group #{}",
                &listen.release_group
            );
            Some(val)
        }
        None => match cover_art_by_release_group(&listen.release_group).await {
            Ok(val) => {
                println!(
                    "Cache MISS, inserting cover art of release group #{}",
                    &listen.release_group
                );
                state
                    .cover_art_cache
                    .insert(listen.release_group.clone(), val.clone())
                    .await;
                Some(val)
            }
            Err(_err) => None,
        },
    };

    let template = playing_template(
        &state.tera,
        WIDTH,
        HEIGHT,
        &listen.title,
        &listen.artist,
        &image.unwrap_or_default(),
        color_mode,
        fill,
        transparent.is_some(),
        listening,
    )
    .unwrap();

    let mut opt = Options::default();

    opt.fontdb_mut().load_system_fonts();

    let tree = Tree::from_data(&template.as_bytes(), &opt).unwrap();

    let tree_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(tree_size.width(), tree_size.height()).unwrap();
    render(&tree, Transform::default(), &mut pixmap.as_mut());

    let result = pixmap.encode_png().unwrap();

    state
        .response_cache
        .insert(
            format!(
                "{}-{}-{}-{}",
                id,
                color_mode.unwrap_or(&String::new()),
                fill.unwrap_or(&String::new()),
                transparent.unwrap_or(&String::new())
            ),
            result.clone(),
        )
        .await;

    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"now-playing.png\"",
            ),
        ],
        result,
    ))
}
