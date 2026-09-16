use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Weak},
};

use onlinerpg_shared::{deserialize_server_msg, ServerMessage};
use onlinerpg_terrain::{
    io::TerrainIO,
    snapshot::{content_version, encode_snapshot},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTerrain {
    pub epoch: String,
    pub generation: u64,
    pub revision: u64,
    pub x: i32,
    pub z: i32,
    pub version: String,
}

#[derive(Default)]
pub struct AppliedTerrain {
    pub epoch: String,
    pub revisions: HashMap<(i32, i32), u64>,
}

pub struct TerrainSnapshots {
    source: String,
    cache_dir: PathBuf,
    loading: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    pub applied: Mutex<AppliedTerrain>,
}

impl TerrainSnapshots {
    pub fn new(source: &str, cache_dir: &str) -> Self {
        Self {
            source: source.trim_end_matches('/').to_owned(),
            cache_dir: PathBuf::from(cache_dir).join("snapshots"),
            loading: Mutex::default(),
            applied: Mutex::default(),
        }
    }

    pub async fn load(&self, tile: &PendingTerrain) -> anyhow::Result<ServerMessage> {
        anyhow::ensure!(
            tile.version.len() == 64 && tile.version.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid terrain version"
        );
        let lock = {
            let mut loading = self.loading.lock().await;
            loading.retain(|_, lock| lock.strong_count() > 0);
            let entry = loading.entry(tile.version.clone()).or_default();
            match entry.upgrade() {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    *entry = Arc::downgrade(&lock);
                    lock
                }
            }
        };
        let _loading = lock.lock().await;
        let path = self.cache_dir.join(format!("{}.bin", tile.version));
        if let Ok(bytes) = tokio::fs::read(&path).await {
            if let Ok(message) = decode(&bytes, tile) {
                return Ok(message);
            }
        }
        let bytes = if crate::is_http_source(&self.source) {
            crate::terrain_http::http_client()
                .get(format!(
                    "{}/api/terrain/snapshot/ground/{}/{}/{}",
                    self.source, tile.x, tile.z, tile.version
                ))
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec()
        } else {
            encode_snapshot(
                &TerrainIO::new(PathBuf::from(&self.source))
                    .read_snapshot(tile.x, tile.z, false)
                    .await?,
            )?
        };
        let message = decode(&bytes, tile)?;
        let write = async {
            tokio::fs::create_dir_all(&self.cache_dir).await?;
            onlinerpg_terrain::io::atomic_write(&path, &bytes).await
        }
        .await;
        if let Err(error) = write {
            tracing::warn!(%error, "Could not cache terrain snapshot");
        }
        Ok(message)
    }
}

fn decode(bytes: &[u8], tile: &PendingTerrain) -> anyhow::Result<ServerMessage> {
    anyhow::ensure!(
        content_version(bytes) == tile.version,
        "Terrain content version changed"
    );
    let message = deserialize_server_msg(bytes)?;
    anyhow::ensure!(
        matches!(&message, ServerMessage::TerrainTileSnapshot {
        tile_x, tile_z, height, splat, trees: None, grass: None, landscape: None
    } if *tile_x == tile.x && *tile_z == tile.z && height.len() == 65 * 65 * 2 && splat.len() == 64 * 64 * 4),
        "Invalid ground snapshot"
    );
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn downloads_are_shared_and_verified_cache_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("ground_snapshot_{}", rand::random::<u64>()));
        let terrain = TerrainIO::new(dir.join("terrain"));
        let message = terrain.read_snapshot(0, 0, false).await.unwrap();
        let bytes = encode_snapshot(&message).unwrap();
        let tile = PendingTerrain {
            epoch: "first".into(),
            generation: 1,
            revision: 1,
            x: 0,
            z: 0,
            version: content_version(&bytes),
        };
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let app = axum::Router::new().route(
            "/api/terrain/snapshot/ground/{x}/{z}/{version}",
            axum::routing::get(move || {
                let bytes = bytes.clone();
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    bytes
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let cache = dir.join("cache");
        let source = TerrainSnapshots::new(&origin, cache.to_str().unwrap());
        let (a, b) = tokio::join!(source.load(&tile), source.load(&tile));
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        let restarted = TerrainSnapshots::new(&origin, cache.to_str().unwrap());
        let tile = PendingTerrain {
            epoch: "second".into(),
            generation: 5,
            revision: 10,
            ..tile
        };
        assert!(restarted.load(&tile).await.is_ok());
        let path = restarted.cache_dir.join(format!("{}.bin", tile.version));
        tokio::fs::write(path, b"corrupt").await.unwrap();
        assert!(restarted.load(&tile).await.is_err());
        let local = TerrainSnapshots::new(
            terrain.base_dir().to_str().unwrap(),
            cache.to_str().unwrap(),
        );
        assert!(local.load(&tile).await.is_ok());
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }
}
