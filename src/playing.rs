use listenbrainz::raw::Client;
use musicbrainz_rs::{
    entity::{release::Release, release_group::ReleaseGroup},
    Browse,
};

pub struct ListenData {
    pub title: String,
    pub artist: String,
    pub release_group: String,
}

pub async fn now_playing(client: &Client, user: &String) -> Result<ListenData, String> {
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
            Some(val) => val.to_string(),
            None => return Err(format!("Cannot parse track release ID")),
        },
        None => {
            return Err(format!(
                "Track [{}] does not have a release ID",
                listen.track_metadata.track_name
            ))
        }
    };

    let release_group = release_group_by_release(&release_id).await.unwrap();

    Ok(ListenData {
        title: listen.track_metadata.track_name.clone(),
        artist: listen.track_metadata.artist_name.clone(),
        release_group: release_group.id.clone(),
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

    println!("{:#?}", listens.payload.listens);

    Ok(listens
        .payload
        .listens
        .iter()
        .map(|listen| ListenData {
            title: listen.track_metadata.track_name.clone(),
            artist: listen.track_metadata.artist_name.clone(),
            release_group: listen
                .track_metadata
                .additional_info
                .get("release_group_mbid")
                .unwrap()
                .to_string(),
        })
        .collect())
}

pub async fn previous_listen(client: &Client, user: &String) -> Result<ListenData, String> {
    let listens = match client.user_listens(&user, None, None, Some(1)) {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while getting user listens: {}", err)),
    };

    println!("{:#?}", listens.payload.listens);

    let listen = listens.payload.listens.first().unwrap().clone();

    let release = release_by_recording(&listen.track_metadata.mbid_mapping.unwrap().recording_mbid)
        .await
        .unwrap();
    let release_group = release_group_by_release(&release.id).await.unwrap();

    Ok(ListenData {
        title: listen.track_metadata.track_name.clone(),
        artist: listen.track_metadata.artist_name.clone(),
        release_group: release_group.id.clone(),
    })
}

pub async fn release_by_recording(recording_id: &String) -> Result<Release, String> {
    let results = match Release::browse().by_recording(recording_id).execute().await {
        Ok(val) => val,
        Err(err) => return Err(format!("Error while browsing release: {}", err)),
    };

    let release_group = match results.entities.first() {
        Some(val) => val,
        None => return Err(format!("Error while getting release")),
    };

    Ok(release_group.clone())
}

pub async fn release_group_by_release(release_id: &String) -> Result<ReleaseGroup, String> {
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

    Ok(release_group.clone())
}
