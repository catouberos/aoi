use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use aoi::{
    logger::SimpleLogger,
    playing::{
        cover_art_by_release_group, cover_art_by_spotify_path, now_playing, previous_listen,
        ListenMetadata,
    },
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
use log::{error, info, LevelFilter};
use moka::future::Cache;
use reqwest::StatusCode;
use resvg::{render, tiny_skia::Pixmap};
use tera::Tera;
use usvg::{Options, Transform, Tree};

#[derive(Clone)]
pub struct ResponseCache {
    pub timestamp: Instant,
    pub response: Vec<u8>,
}

#[derive(Clone)]
pub struct AppState {
    pub tera: Tera,
    pub response_cache: Cache<String, ResponseCache>,
    pub cover_art_cache: Cache<String, String>,
}

static LOGGER: SimpleLogger = SimpleLogger;

#[tokio::main]
async fn main() {
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Info));

    let response_cache = Cache::builder()
        .weigher(|_key, value: &ResponseCache| -> u32 {
            value.response.len().try_into().unwrap_or(u32::MAX)
        })
        // This cache will hold up to 64MiB of values.
        .max_capacity(64 * 1024 * 1024)
        .build();
    // cache cover art data for 1 day
    let cover_art_cache = Cache::builder()
        .time_to_live(Duration::from_secs(1 * 24 * 60 * 60))
        // A weigher closure takes &K and &V and returns a u32 representing the
        // relative size of the entry. Here, we use the byte length of the value
        // String as the size.
        .weigher(|_key, value: &String| -> u32 { value.len().try_into().unwrap_or(u32::MAX) })
        // This cache will hold up to 64MiB of values.
        .max_capacity(64 * 1024 * 1024)
        .build();

    let tera = match Tera::new("templates/**/*.html") {
        Ok(t) => t,
        Err(e) => {
            error!("Parsing error(s): {}", e);
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

    info!("App started!");
}

async fn get_playing_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let color_mode: Option<String> = params.get("color_mode").cloned();
    let fill = params.get("fill").cloned();
    let transparent = params.get("transparent").cloned();

    if let Some(val) = state
        .response_cache
        .get(&format!(
            "{}-{}-{}-{}",
            id,
            color_mode.clone().unwrap_or("default".to_string()),
            fill.clone().unwrap_or("default".to_string()),
            transparent.clone().unwrap_or("default".to_string())
        ))
        .await
    {
        info!("Cache HIT, user {}", id);

        if Instant::now().duration_since(val.timestamp).as_secs() > (60 * 1) {
            tokio::spawn(async move {
                info!("Revalidating listen of user {}", id);
                let result = render_listen(
                    &state,
                    &id,
                    color_mode.as_ref(),
                    fill.as_ref(),
                    transparent.as_ref(),
                )
                .await
                .unwrap();

                state
                    .response_cache
                    .insert(
                        format!(
                            "{}-{}-{}-{}",
                            id,
                            color_mode.unwrap_or("default".to_string()),
                            fill.unwrap_or("default".to_string()),
                            transparent.unwrap_or("default".to_string())
                        ),
                        ResponseCache {
                            timestamp: Instant::now(),
                            response: result.clone(),
                        },
                    )
                    .await;
            });
        }

        return Ok((
            [
                (header::CONTENT_TYPE, "image/png"),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"now-playing.png\"",
                ),
            ],
            val.response,
        ));
    };

    let result = render_listen(
        &state,
        &id,
        color_mode.as_ref(),
        fill.as_ref(),
        transparent.as_ref(),
    )
    .await
    .unwrap();

    state
        .response_cache
        .insert(
            format!(
                "{}-{}-{}-{}",
                id,
                color_mode.unwrap_or("default".to_string()),
                fill.unwrap_or("default".to_string()),
                transparent.unwrap_or("default".to_string())
            ),
            ResponseCache {
                timestamp: Instant::now(),
                response: result.clone(),
            },
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

pub async fn render_listen(
    state: &AppState,
    id: &String,
    color_mode: Option<&String>,
    fill: Option<&String>,
    transparent: Option<&String>,
) -> Result<Vec<u8>, String> {
    const WIDTH: i32 = 1000;
    const HEIGHT: i32 = 200;

    info!("Getting listen of user {}", id);

    let client = Client::new();

    let (listen, listening) = match now_playing(&client, &id).await {
        Ok(val) => (val, true),
        Err(_) => match previous_listen(&client, &id).await {
            Ok(val) => (val, false),
            Err(err) => return Err(format!("User has no listen history, error: {:#?}", err)),
        },
    };

    let image = if let Some(metadata) = listen.metadata {
        if let Ok(result) = get_cover_art(&metadata, &state.cover_art_cache).await {
            result
        } else {
            None
        }
    } else {
        None
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

    Ok(result)
}

async fn get_cover_art(
    metadata: &ListenMetadata,
    cache: &Cache<String, String>,
) -> Result<Option<String>, String> {
    let result = if let Some(release_group) = &metadata.release_group {
        match cache.get(release_group).await {
            Some(val) => {
                info!(
                    "Cache HIT, getting cover art of release group #{}",
                    &release_group
                );
                Some(val)
            }
            None => match cover_art_by_release_group(&release_group).await {
                Ok(val) => {
                    info!(
                        "Cache MISS, inserting cover art of release group #{}",
                        &release_group
                    );
                    cache.insert(release_group.clone(), val.clone()).await;
                    Some(val)
                }
                Err(_err) => None,
            },
        }
    } else if let Some(spotify_path) = &metadata.spotify_path.clone() {
        match cache.get(spotify_path).await {
            Some(val) => {
                info!(
                    "Cache HIT, getting cover art of Spotify path [{}]",
                    &spotify_path
                );
                Some(val)
            }
            None => match cover_art_by_spotify_path(&spotify_path).await {
                Ok(val) => {
                    info!(
                        "Cache MISS, inserting cover art of Spotify path [{}]",
                        &spotify_path
                    );
                    cache.insert(spotify_path.clone(), val.clone()).await;
                    Some(val)
                }
                Err(_err) => None,
            },
        }
    } else {
        None
    };

    Ok(result)
}
