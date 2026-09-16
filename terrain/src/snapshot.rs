use crate::{coords, io::TerrainIO};
use onlinerpg_shared::{serialize_server_msg, ServerMessage};
use sha2::{Digest, Sha256};
use std::io;

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

    pub async fn snapshot_versions(&self, x: i32, z: i32) -> io::Result<ServerMessage> {
        let mut message = self.read_snapshot(x, z, true).await?;
        let version = content_version(&encode_snapshot(&message)?);
        ground_snapshot(&mut message);
        let ground_version = content_version(&encode_snapshot(&message)?);
        Ok(ServerMessage::TerrainTileVersion {
            tile_x: coords::wrap_tile_x(x),
            tile_z: z,
            version,
            ground_version,
        })
    }
}
