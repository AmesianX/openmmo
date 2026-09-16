use crate::{
    coords,
    io::{write_terrain_file, TerrainIO},
};
use onlinerpg_shared::{serialize_server_msg, ServerMessage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const SNAPSHOT_FORMAT: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceStamp {
    size: u64,
    seconds: u64,
    nanos: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SnapshotEntry {
    format: u32,
    x: i32,
    z: i32,
    version: String,
    ground_version: String,
    full_size: u64,
    ground_size: u64,
    sources: [Option<SourceStamp>; 5],
}

impl SnapshotEntry {
    fn message(&self) -> ServerMessage {
        ServerMessage::TerrainTileVersion {
            tile_x: self.x,
            tile_z: self.z,
            version: self.version.clone(),
            ground_version: self.ground_version.clone(),
        }
    }
}

pub fn valid_version(version: &str) -> bool {
    version.len() == 64
        && version
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

async fn stamp(path: &Path) -> io::Result<Option<SourceStamp>> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => {
            let time = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?;
            Ok(Some(SourceStamp {
                size: metadata.len(),
                seconds: time.as_secs(),
                nanos: time.subsec_nanos(),
            }))
        }
        Ok(_) => Err(io::Error::other(format!(
            "Not a terrain file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn content_version(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn encode_snapshot(message: &ServerMessage) -> io::Result<Vec<u8>> {
    serialize_server_msg(message).map_err(io::Error::other)
}

pub fn ground_snapshot(message: &mut ServerMessage) {
    if let ServerMessage::TerrainTileSnapshot {
        trees,
        grass,
        landscape,
        ..
    } = message
    {
        *trees = None;
        *grass = None;
        *landscape = None;
    }
}

impl TerrainIO {
    pub async fn read_snapshot(&self, x: i32, z: i32, visuals: bool) -> io::Result<ServerMessage> {
        Ok(ServerMessage::TerrainTileSnapshot {
            tile_x: coords::wrap_tile_x(x),
            tile_z: z,
            height: self.read_heightmap(x, z).await?,
            splat: self.read_splatmap(x, z).await?,
            trees: if visuals {
                self.read_trees(x, z).await?
            } else {
                None
            },
            grass: if visuals {
                self.read_grass(x, z).await?
            } else {
                None
            },
            landscape: if visuals {
                self.read_landscaping_tile(x, z).await?
            } else {
                None
            },
        })
    }

    pub fn snapshot_dir(&self) -> PathBuf {
        self.base_dir().join("snapshots")
    }

    pub fn snapshot_path(
        &self,
        profile: &str,
        x: i32,
        z: i32,
        version: &str,
    ) -> io::Result<PathBuf> {
        if !matches!(profile, "full" | "ground") || !valid_version(version) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid snapshot address",
            ));
        }
        Ok(self
            .snapshot_dir()
            .join(profile)
            .join(coords::wrap_tile_x(x).to_string())
            .join(z.to_string())
            .join(version))
    }

    fn snapshot_index_path(&self, x: i32, z: i32) -> PathBuf {
        self.snapshot_dir()
            .join("index")
            .join(x.to_string())
            .join(format!("{z}.json"))
    }

    async fn snapshot_sources(&self, x: i32, z: i32) -> io::Result<[Option<SourceStamp>; 5]> {
        Ok([
            stamp(&coords::heightmap_path(self.base_dir(), x, z)).await?,
            stamp(&coords::splatmap_path(self.base_dir(), x, z)).await?,
            stamp(&coords::tree_path(self.base_dir(), x, z)).await?,
            stamp(&coords::grass_path(self.base_dir(), x, z)).await?,
            stamp(&coords::landscaping_path(self.base_dir(), x, z)).await?,
        ])
    }

    pub async fn snapshot_versions(&self, x: i32, z: i32) -> io::Result<ServerMessage> {
        self.ensure_snapshot(x, z, false).await
    }

    pub async fn rebuild_snapshot(&self, x: i32, z: i32) -> io::Result<ServerMessage> {
        self.ensure_snapshot(x, z, true).await
    }

    async fn ensure_snapshot(&self, x: i32, z: i32, rebuild: bool) -> io::Result<ServerMessage> {
        let x = coords::wrap_tile_x(x);
        let mut cache = self.snapshots.lock().await;
        if !rebuild {
            if let Some(entry) = cache.get(&(x, z)) {
                return Ok(entry.message());
            }
        }
        cache.remove(&(x, z));
        let sources = self.snapshot_sources(x, z).await?;
        let index_path = self.snapshot_index_path(x, z);
        if !rebuild {
            let saved = match tokio::fs::read(&index_path).await {
                Ok(bytes) => serde_json::from_slice::<SnapshotEntry>(&bytes).ok(),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if let Some(entry) = saved {
                if entry.format == SNAPSHOT_FORMAT
                    && entry.x == x
                    && entry.z == z
                    && valid_version(&entry.version)
                    && valid_version(&entry.ground_version)
                    && entry.sources == sources
                    && file_size(&self.snapshot_path("full", x, z, &entry.version)?).await?
                        == Some(entry.full_size)
                    && file_size(&self.snapshot_path("ground", x, z, &entry.ground_version)?)
                        .await?
                        == Some(entry.ground_size)
                {
                    let message = entry.message();
                    cache.insert((x, z), entry);
                    return Ok(message);
                }
            }
        }
        let mut message = self.read_snapshot(x, z, true).await?;
        let full = encode_snapshot(&message)?;
        ground_snapshot(&mut message);
        let ground = encode_snapshot(&message)?;
        if self.snapshot_sources(x, z).await? != sources {
            return Err(io::Error::other(
                "Terrain changed while preparing a snapshot; retry",
            ));
        }
        let entry = SnapshotEntry {
            format: SNAPSHOT_FORMAT,
            x,
            z,
            version: content_version(&full),
            ground_version: content_version(&ground),
            full_size: full.len() as u64,
            ground_size: ground.len() as u64,
            sources,
        };
        save_body(&self.snapshot_path("full", x, z, &entry.version)?, &full).await?;
        save_body(
            &self.snapshot_path("ground", x, z, &entry.ground_version)?,
            &ground,
        )
        .await?;
        write_terrain_file(&index_path, &serde_json::to_vec(&entry)?).await?;
        let message = entry.message();
        cache.insert((x, z), entry);
        Ok(message)
    }

    pub async fn prepare_snapshots(&self) -> io::Result<usize> {
        let base = self.base_dir().clone();
        let tiles = tokio::task::spawn_blocking(move || discover_tiles(&base))
            .await
            .map_err(io::Error::other)??;
        for &(x, z) in &tiles {
            self.snapshot_versions(x, z).await?;
        }
        Ok(tiles.len())
    }
}

async fn file_size(path: &Path) -> io::Result<Option<u64>> {
    Ok(stamp(path).await?.map(|stamp| stamp.size))
}

async fn save_body(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if file_size(path).await? != Some(bytes.len() as u64) {
        write_terrain_file(path, bytes).await?;
    }
    Ok(())
}

fn entries(path: &Path) -> io::Result<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error),
    }
}

fn discover_tiles(base: &Path) -> io::Result<BTreeSet<(i32, i32)>> {
    let mut tiles = BTreeSet::new();
    for (kind, prefix) in [
        ("height", "h_"),
        ("splat", "s_"),
        ("trees", "t_"),
        ("grass", "g_"),
        ("landscaping", "l_"),
    ] {
        for region in entries(&base.join(kind))? {
            if !region.file_type()?.is_dir() {
                continue;
            }
            for file in entries(&region.path())? {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let Some(coords) = name
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(".bin"))
                else {
                    continue;
                };
                let (x, z) = coords
                    .split_once('_')
                    .and_then(|(x, z)| Some((x.parse::<i32>().ok()?, z.parse::<i32>().ok()?)))
                    .ok_or_else(|| io::Error::other(format!("Invalid terrain tile: {name}")))?;
                tiles.insert((coords::wrap_tile_x(x), z));
            }
        }
    }
    for column in entries(&base.join("snapshots/index"))? {
        if !column.file_type()?.is_dir() {
            continue;
        }
        let Ok(x) = column.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        for file in entries(&column.path())? {
            if let Some(z) = file
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<i32>().ok())
            {
                tiles.insert((coords::wrap_tile_x(x), z));
            }
        }
    }
    Ok(tiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn grass(count: u8) -> Vec<u8> {
        let mut data = onlinerpg_shared::grass_format::empty_grass();
        data[4] = count;
        data
    }

    fn terrain() -> TerrainIO {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        TerrainIO::new(std::env::temp_dir().join(format!(
            "terrain_snapshots_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn versions(message: ServerMessage) -> (String, String) {
        let ServerMessage::TerrainTileVersion {
            version,
            ground_version,
            ..
        } = message
        else {
            panic!()
        };
        (version, ground_version)
    }

    #[tokio::test]
    async fn runtime_lookup_prepares_only_the_requested_tile() {
        let io = terrain();
        let unrelated = coords::heightmap_path(io.base_dir(), 100, 100);
        tokio::fs::create_dir_all(&unrelated).await.unwrap();
        let (version, ground_version) = versions(io.snapshot_versions(0, 0).await.unwrap());
        assert!(io.snapshot_path("full", 0, 0, &version).unwrap().is_file());
        assert!(io
            .snapshot_path("ground", 0, 0, &ground_version)
            .unwrap()
            .is_file());
        assert!(!io.snapshot_index_path(100, 100).exists());
        assert_eq!(io.snapshots.lock().await.len(), 1);
        assert!(io.prepare_snapshots().await.is_err());
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn prepared_files_survive_restart_and_only_changed_tiles_are_rebuilt() {
        let io = terrain();
        io.write_heightmap(0, 0, &crate::defaults::default_heightmap())
            .await
            .unwrap();
        io.write_grass(0, 0, &grass(1)).await.unwrap();
        io.write_splatmap(1, 0, &crate::defaults::default_splatmap())
            .await
            .unwrap();
        assert_eq!(io.prepare_snapshots().await.unwrap(), 2);
        let first = versions(io.snapshot_versions(0, 0).await.unwrap());
        let other = versions(io.snapshot_versions(1, 0).await.unwrap());
        let index = io.snapshot_index_path(0, 0);
        let unchanged = stamp(&index).await.unwrap();
        let restarted = TerrainIO::new(io.base_dir().clone());
        assert_eq!(restarted.prepare_snapshots().await.unwrap(), 2);
        assert_eq!(stamp(&index).await.unwrap(), unchanged);
        assert_eq!(
            versions(restarted.snapshot_versions(0, 0).await.unwrap()),
            first
        );
        io.write_grass(0, 0, &grass(2)).await.unwrap();
        let deployed = TerrainIO::new(io.base_dir().clone());
        assert_eq!(deployed.prepare_snapshots().await.unwrap(), 2);
        let changed = versions(deployed.snapshot_versions(0, 0).await.unwrap());
        assert_ne!(first.0, changed.0);
        assert_eq!(first.1, changed.1);
        assert_eq!(
            versions(deployed.snapshot_versions(1, 0).await.unwrap()),
            other
        );
        for (profile, hash) in [
            ("full", &first.0),
            ("full", &changed.0),
            ("ground", &first.1),
        ] {
            let bytes = tokio::fs::read(io.snapshot_path(profile, 0, 0, hash).unwrap())
                .await
                .unwrap();
            assert_eq!(content_version(&bytes), *hash);
        }
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn cached_versions_do_not_read_sources_and_failed_rebuilds_are_not_published() {
        let io = terrain();
        let original = versions(io.snapshot_versions(0, 0).await.unwrap());
        let path = coords::heightmap_path(io.base_dir(), 0, 0);
        tokio::fs::create_dir_all(&path).await.unwrap();
        assert_eq!(
            versions(io.snapshot_versions(0, 0).await.unwrap()),
            original
        );
        assert!(io.rebuild_snapshot(0, 0).await.is_err());
        let saved: SnapshotEntry =
            serde_json::from_slice(&tokio::fs::read(io.snapshot_index_path(0, 0)).await.unwrap())
                .unwrap();
        assert_eq!(saved.version, original.0);
        tokio::fs::remove_dir(&path).await.unwrap();
        assert_eq!(versions(io.rebuild_snapshot(0, 0).await.unwrap()), original);
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn preparation_repairs_missing_bodies_and_detects_deleted_source_tiles() {
        let io = terrain();
        io.write_grass(-1, -1, &grass(3)).await.unwrap();
        io.prepare_snapshots().await.unwrap();
        let first = versions(io.snapshot_versions(-1, -1).await.unwrap());
        let ground = io.snapshot_path("ground", -1, -1, &first.1).unwrap();
        tokio::fs::remove_file(&ground).await.unwrap();
        let restarted = TerrainIO::new(io.base_dir().clone());
        assert_eq!(restarted.prepare_snapshots().await.unwrap(), 1);
        assert!(ground.is_file());
        io.delete_region(-1, -1).await.unwrap();
        let restarted = TerrainIO::new(io.base_dir().clone());
        assert_eq!(restarted.prepare_snapshots().await.unwrap(), 1);
        let deleted = versions(restarted.snapshot_versions(-1, -1).await.unwrap());
        assert_ne!(deleted.0, first.0);
        assert_eq!(deleted.1, first.1);
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn a_partial_body_write_cannot_replace_the_version_index() {
        let io = terrain();
        let original = versions(io.snapshot_versions(0, 0).await.unwrap());
        let ground_dir = io.snapshot_dir().join("ground/0/0");
        tokio::fs::remove_dir_all(&ground_dir).await.unwrap();
        tokio::fs::write(&ground_dir, b"blocked").await.unwrap();
        io.write_grass(0, 0, &grass(5)).await.unwrap();
        assert!(io.rebuild_snapshot(0, 0).await.is_err());
        let entry: SnapshotEntry =
            serde_json::from_slice(&tokio::fs::read(io.snapshot_index_path(0, 0)).await.unwrap())
                .unwrap();
        assert_eq!(entry.version, original.0);
        tokio::fs::remove_file(ground_dir).await.unwrap();
        assert_ne!(
            versions(io.rebuild_snapshot(0, 0).await.unwrap()).0,
            original.0
        );
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }
}
