use base64::{engine::general_purpose, Engine};
use listenbrainz::raw::Client;
use log::{info, warn};
use musicbrainz_rs::{
    entity::{release::Release, release_group::ReleaseGroup},
    Browse,
};
use regex::Regex;

#[derive(Clone)]
pub struct ListenMetadata {
    pub release_group: Option<String>,
    pub spotify_path: Option<String>,
}

#[derive(Clone)]
pub struct ListenData {
    pub title: String,
    pub artist: String,
    pub metadata: Option<ListenMetadata>,
}

pub async fn now_playing(client: &Client, user: &String) -> Result<ListenData, String> {
    let now_playing = match client.user_playing_now(&user) {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting user now playing: {}", err)),
    };

    let listen = match now_playing.payload.listens.first() {
        Some(val) => val,
        None => return Err(format!("User [{}] does not have any listen", user)),
    };

    let title = listen.track_metadata.track_name.clone();
    let artist = listen.track_metadata.artist_name.clone();

    let release_group =
        if let Some(release_id_value) = listen.track_metadata.additional_info.get("release_mbid") {
            if let Some(release_id) = release_id_value.as_str() {
                if let Ok(release_group) = release_group_by_release(&release_id.to_string()).await {
                    Some(release_group.id)
                } else {
                    warn!("Cannot get release group for release {release_id}");
                    None
                }
            } else {
                warn!("Cannot parse release ID");
                None
            }
        } else {
            warn!("Cannot get release for track");
            None
        };

    let spotify_path = if let Some(spotify_id_value) = listen
        .track_metadata
        .additional_info
        .get("spotify_album_id")
    {
        if let Some(spotify_id) = spotify_id_value.as_str() {
            Some(
                spotify_id
                    .to_string()
                    .replace("https://open.spotify.com/", ""),
            )
        } else {
            warn!("Cannot Spotify album ID");
            None
        }
    } else {
        None
    };

    Ok(ListenData {
        title,
        artist,
        metadata: Some(ListenMetadata {
            release_group,
            spotify_path,
        }),
    })
}

pub async fn previous_listens(
    client: &Client,
    user: &String,
    count: u64,
) -> Result<Vec<ListenData>, String> {
    let listens = match client.user_listens(&user, None, None, Some(count)) {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting user listens: {}", err)),
    };

    Ok(listens
        .payload
        .listens
        .iter()
        .map(|listen| ListenData {
            title: listen.track_metadata.track_name.clone(),
            artist: listen.track_metadata.artist_name.clone(),
            metadata: None,
        })
        .collect())
}

pub async fn previous_listen(client: &Client, user: &String) -> Result<ListenData, String> {
    let listens = match client.user_listens(&user, None, None, Some(1)) {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting user listens: {}", err)),
    };

    let listen = listens.payload.listens.first().unwrap().clone();

    let title = listen.track_metadata.track_name.clone();
    let artist = listen.track_metadata.artist_name.clone();

    let release_group = if let Some(mapping) = listen.track_metadata.mbid_mapping {
        let recording_id = mapping.recording_mbid;
        if let Ok(release) = release_by_recording(&recording_id).await {
            let release_id = release.id;
            info!("Getting release group of release #{release_id}");
            if let Ok(release_group) = release_group_by_release(&release_id).await {
                Some(release_group.id)
            } else {
                warn!("Cannot get release group for release #{release_id}");
                None
            }
        } else {
            warn!("Cannot get release for recording #{recording_id}");
            None
        }
    } else {
        warn!("Cannot get release for track [{title}]");
        None
    };

    let spotify_path = if let Some(spotify_id_value) = listen
        .track_metadata
        .additional_info
        .get("spotify_album_id")
    {
        if let Some(spotify_id) = spotify_id_value.as_str() {
            Some(
                spotify_id
                    .to_string()
                    .replace("https://open.spotify.com/", ""),
            )
        } else {
            warn!("Cannot Spotify album ID");
            None
        }
    } else {
        None
    };

    Ok(ListenData {
        title,
        artist,
        metadata: Some(ListenMetadata {
            release_group,
            spotify_path,
        }),
    })
}

pub async fn cover_art_by_release_group(release_group: &String) -> Result<String, String> {
    let image = format!(
        "https://coverartarchive.org/release-group/{}/front-250.jpg",
        release_group,
    );

    let response = match reqwest::get(&image).await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting image data: {:#?}", err)),
    };

    let data = match response.bytes().await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while parsing image bytes: {:#?}", err)),
    };

    let encoded = general_purpose::STANDARD.encode(&data);

    Ok(encoded)
}

pub async fn cover_art_by_spotify_path(path: &String) -> Result<String, String> {
    let url = format!("https://open.spotify.com/embed/{}", path);

    let response = match reqwest::get(&url).await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting Spotify data: {:#?}", err)),
    };

    let data = match response.text().await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while parsing Spotify data: {:#?}", err)),
    };

    let re = Regex::new(r#"(https:\/\/image[\w-]+\.spotifycdn\.com\/image\/[\w\d]+)(\",\"((maxHeight)|(maxWidth))\":300)"#).unwrap();

    let urls: Vec<&str> = re
        .captures_iter(data.as_str())
        .map(|c| {
            let (_, [url, _, _, _]) = c.extract();
            url
        })
        .collect();

    if let Some(url) = urls.first() {
        let response = match reqwest::get(*url).await {
            Ok(val) => val,
            Err(err) => return Err(format!("Error while getting image data: {:#?}", err)),
        };

        let data = match response.bytes().await {
            Ok(val) => val,
            Err(err) => return Err(format!("Error while parsing image bytes: {:#?}", err)),
        };

        let encoded = general_purpose::STANDARD.encode(&data);

        return Ok(encoded);
    }

    Err("Cannot get image from Spotify".to_string())
}

pub async fn release_by_recording(recording_id: &String) -> Result<Release, String> {
    let results = match Release::browse().by_recording(recording_id).execute().await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while browsing release: {}", err)),
    };

    let release = match results.entities.first() {
        Some(val) => val,
        None => return Err(format!("No release found!")),
    };

    Ok(release.clone())
}

pub async fn release_group_by_release(release_id: &String) -> Result<ReleaseGroup, String> {
    let results = match ReleaseGroup::browse()
        .by_release(&release_id)
        .execute()
        .await
    {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while browsing release group: {}", err)),
    };

    let release_group = match results.entities.first() {
        Some(val) => val,
        None => return Err(format!("No release group found!")),
    };

    Ok(release_group.clone())
}
