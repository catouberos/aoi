use listenbrainz::raw::{response::UserPlayingNowListen, Client};
use musicbrainz_rs::{entity::release_group::ReleaseGroup, Browse};

pub async fn now_playing(
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
