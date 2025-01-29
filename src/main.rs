use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::header,
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine};
use listenbrainz::raw::{response::UserPlayingNowListen, Client};
use moka::future::Cache;
use musicbrainz_rs::{entity::release_group::ReleaseGroup, Browse};
use reqwest::StatusCode;
use resvg::{render, tiny_skia::Pixmap};
use tera::{Context, Tera};
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
    Path(path): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    match state.response_cache.get(&path).await {
        Some(val) => {
            println!("Cache HIT, user {}", path);
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
        None => println!("Cache MISS, user {}", path),
    };

    let client = Client::new();

    let mut context = Context::new();

    let (track, release_group) = match now_playing(client, path.clone()).await {
        Ok(val) => val,
        Err(err) => panic!("Error while getting now playing: {}", err),
    };

    let image = format!(
        "https://coverartarchive.org/release-group/{}/front-250.jpg",
        release_group.id,
    );

    let image_data = reqwest::get(&image).await.unwrap().bytes().await.unwrap();
    let image_encoded = general_purpose::STANDARD.encode(&image_data);

    context.insert("title", &track.track_metadata.track_name);
    context.insert("artist", &track.track_metadata.artist_name);
    context.insert(
        "image",
        &format!("data:image/jpeg;base64,{}", &image_encoded),
    );

    let template = state
        .tera
        .render("widget.html", &context)
        .unwrap()
        .to_string();

    let mut opt = Options::default();

    opt.fontdb_mut().load_system_fonts();

    let tree = Tree::from_data(&template.as_bytes(), &opt).unwrap();

    let tree_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(tree_size.width(), tree_size.height()).unwrap();
    render(&tree, Transform::default(), &mut pixmap.as_mut());

    let result = pixmap.encode_png().unwrap();

    state
        .response_cache
        .insert(path.clone(), result.clone())
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

async fn now_playing(
    client: Client,
    user: String,
) -> Result<(UserPlayingNowListen, ReleaseGroup), String> {
    let now_playing = match client.user_playing_now(&user) {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting user now playing: {}", err)),
    };

    println!("{:#?}", now_playing);

    let listen = match now_playing.payload.listens.first() {
        Some(val) => val,
        None => return Err(format!("User [{}] does not have any listen", user)),
    };

    let release_id = match listen.track_metadata.additional_info.get("release_mbid") {
        Some(val) => match val.as_str() {
            Some(val) => val,
            None => return Err(format!("Cannot parse track release ID")),
        },
        None => {
            return Err(format!(
                "Track [{}] does not have a release ID",
                listen.track_metadata.track_name
            ))
        }
    };

    let results = match ReleaseGroup::browse()
        .by_release(release_id)
        .execute()
        .await
    {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while browsing release group: {}", err)),
    };

    let release_group = match results.entities.first() {
        Some(val) => val,
        None => return Err(format!("Error while getting release group")),
    };

    Ok((listen.clone(), release_group.clone()))
}
