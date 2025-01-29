use std::{collections::HashMap, time::Duration};

use aoi::{
    playing::{now_playing, previous_listen},
    template::playing_template,
};
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine};
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
    pub cover_art_cache: Cache<String, Vec<u8>>,
}

#[tokio::main]
async fn main() {
    let response_cache = Cache::builder()
        .time_to_live(Duration::from_secs(30))
        .build();
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
    /*
    * TODO: hit cache
        match state.response_cache.get(&id).await {
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
    */

    let client = Client::new();

    let listen = match now_playing(&client, &id).await {
        Ok(val) => val,
        Err(_) => match previous_listen(&client, &id).await {
            Ok(val) => val,
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("User has no listen history"),
                ))
            }
        },
    };

    let image = format!(
        "https://coverartarchive.org/release-group/{}/front-250.jpg",
        listen.release_group,
    );

    let image_data = reqwest::get(&image).await.unwrap().bytes().await.unwrap();
    let image_encoded = general_purpose::STANDARD.encode(&image_data);

    let color_mode = params.get("color_mode");
    let fill = params.get("fill");

    let template = playing_template(
        state.tera,
        &listen.title,
        &listen.artist,
        &image_encoded,
        color_mode,
        fill,
    )
    .unwrap();

    let mut opt = Options::default();

    opt.fontdb_mut().load_system_fonts();

    let tree = Tree::from_data(&template.as_bytes(), &opt).unwrap();

    let tree_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(tree_size.width(), tree_size.height()).unwrap();
    render(&tree, Transform::default(), &mut pixmap.as_mut());

    let result = pixmap.encode_png().unwrap();

    /*
    * TODO: insert cache
        state
            .response_cache
            .insert(id.clone(), result.clone())
            .await;
    */

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
