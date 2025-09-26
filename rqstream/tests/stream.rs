use std::time::Duration;

use librqbit::AddTorrentOptions;
use rqstream::Rqstream;
use tokio::time::sleep;

#[tokio::test]
#[ignore = "does not terminate on success"]
async fn stream() {
    const BIG_BUCK_BUNNY_MAGNET: &str = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969&tr=udp%3A%2F%2Ftracker.coppersurfer.tk%3A6969&tr=udp%3A%2F%2Ftracker.empire-js.us%3A1337&tr=udp%3A%2F%2Ftracker.leechers-paradise.org%3A6969&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337&tr=wss%3A%2F%2Ftracker.btorrent.xyz&tr=wss%3A%2F%2Ftracker.fastcast.nz&tr=wss%3A%2F%2Ftracker.openwebtorrent.com&ws=https%3A%2F%2Fwebtorrent.io%2Ftorrents%2F&xs=https%3A%2F%2Fwebtorrent.io%2Ftorrents%2Fbig-buck-bunny.torrent";
    let s = Rqstream::create("127.0.0.1:9000").await.unwrap();
    dbg!();
    // stream the Big Buck Bunny torrent to 127.0.0.1:9000/stream/test
    let t = s
        .session
        .add_torrent(
            librqbit::AddTorrent::Url(BIG_BUCK_BUNNY_MAGNET.into()),
            Some(AddTorrentOptions {
                only_files: Some(Vec::new()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    let h = t.into_handle().unwrap();
    h.wait_until_initialized().await.unwrap();
    let id = s.stream_file(&h, 1, "test".to_string()).await.unwrap();
    dbg!(id);
    loop {
        sleep(Duration::from_secs(5)).await;
        dbg!(h.stats());
    }
}
