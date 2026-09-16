use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use sha2::Digest;
use std::sync::Arc;
use tracing::{error, warn};

use onlinerpg_terrain::coords;

use super::io::TerrainIO;
use crate::game_state::GameState;

/// Routes that update runtime collision along with terrain files.
#[derive(Clone)]
struct ObjectsState {
    terrain: Arc<TerrainIO>,
    game_state: Arc<GameState>,
}

#[derive(Deserialize)]
struct MinimapQuery {
    size: Option<u32>,
}

pub fn terrain_router(
    terrain_io: Arc<TerrainIO>,
    game_state: Arc<GameState>,
    auth: Arc<crate::auth::AuthService>,
) -> Router {
    let ownership_router = Router::new()
        .route("/api/terrain/land-ownership", get(get_land_ownership))
        .with_state(auth);
    let objects_router = Router::new()
        .route(
            "/api/terrain/height/{x}/{z}",
            get(get_heightmap).put(put_heightmap),
        )
        .route(
            "/api/terrain/splat/{x}/{z}",
            get(get_splatmap).put(put_splatmap),
        )
        .route(
            "/api/terrain/objects/{rx}/{rz}",
            get(get_object).put(put_object),
        )
        .route(
            "/api/terrain/grass/{x}/{z}",
            get(get_grass)
                .put(put_grass)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/api/terrain/weather-sectors", get(get_weather_sectors))
        .route(
            "/api/terrain/snapshot/{profile}/{x}/{z}/{version}",
            get(get_snapshot),
        )
        .with_state(ObjectsState {
            terrain: Arc::clone(&terrain_io),
            game_state,
        });
    Router::new()
        .route(
            "/api/terrain/height-original/{x}/{z}",
            get(get_original_heightmap).put(put_original_heightmap),
        )
        .route(
            "/api/terrain/height-original/{x}/{z}/ensure",
            post(ensure_original_heightmap),
        )
        .route(
            "/api/terrain/grass-original/{x}/{z}",
            get(get_original_grass)
                .put(put_original_grass)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/terrain/grass-original/{x}/{z}/ensure",
            post(ensure_original_grass),
        )
        .route(
            "/api/terrain/minimap/{rx}/{rz}",
            get(get_minimap).put(put_minimap),
        )
        .route("/api/terrain/zones/{rx}/{rz}", get(get_zone).put(put_zone))
        .route(
            "/api/terrain/land-grades/{rx}/{rz}",
            get(get_land_grades).put(put_land_grades),
        )
        .route("/api/terrain/climate/{rx}/{rz}", get(get_climate))
        .route("/api/terrain/trees/{x}/{z}", get(get_trees))
        .route("/api/terrain/river-field/{x}/{z}", get(get_river_field))
        .route("/api/terrain/water-field/{x}/{z}", get(get_water_field))
        .route(
            "/api/terrain/region/{rx}/{rz}",
            delete(delete_region_handler),
        )
        .with_state(terrain_io)
        .merge(objects_router)
        .merge(ownership_router)
}

async fn get_snapshot(
    Path((profile, x, z, version)): Path<(String, i32, i32, String)>,
    State(state): State<ObjectsState>,
    headers: axum::http::HeaderMap,
) -> Response {
    use onlinerpg_terrain::snapshot::{content_version, encode_snapshot};
    let visuals = match profile.as_str() {
        "full" => true,
        "ground" => false,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if version.len() != 64 || !version.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let result = async {
        let _guard = state.game_state.world_edit_guard().await;
        let message = state.terrain.read_snapshot(x, z, visuals).await?;
        encode_snapshot(&message)
    }
    .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!(%error, x, z, "Failed to read terrain snapshot");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CACHE_CONTROL, "no-store")],
            )
                .into_response();
        }
    };
    if content_version(&bytes) != version {
        return (StatusCode::CONFLICT, [(header::CACHE_CONTROL, "no-store")]).into_response();
    }
    let etag = format!("\"{version}\"");
    let metadata = [
        (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
        (
            header::CACHE_CONTROL,
            "public, max-age=31536000, immutable".to_owned(),
        ),
        (header::ETAG, etag.clone()),
    ];
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        == Some(etag.as_str())
    {
        return (StatusCode::NOT_MODIFIED, metadata).into_response();
    }
    (metadata, bytes).into_response()
}

async fn get_land_ownership(
    State(auth): State<Arc<crate::auth::AuthService>>,
) -> Result<Json<Vec<crate::auth::OwnedLandPlot>>, StatusCode> {
    tokio::task::spawn_blocking(move || auth.owned_land_plots())
        .await
        .map_err(|e| {
            error!("Land ownership task failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            error!("Failed to read land ownership: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
) -> Result<Response, StatusCode> {
    let data = state.terrain.read_heightmap(x, z).await.map_err(|e| {
        error!("Failed to read heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        data,
    )
        .into_response())
}

async fn put_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .game_state
        .save_terrain_heightmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write heightmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_original_heightmap(x, z).await.map_err(|e| {
        error!("Failed to read original heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => {
            Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_original_heightmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write original heightmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_original_grass(x, z).await.map_err(|e| {
        error!("Failed to read original grass ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match data {
        Some(bytes) => {
            Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn put_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_original_grass(x, z, &body)
        .await
        .map_err(|e| {
            error!("Failed to write original grass ({}, {}): {}", x, z, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_original_heightmap(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    let created = terrain.ensure_original_heightmap(x, z).await.map_err(|e| {
        error!("Failed to ensure original heightmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if created {
        Ok(StatusCode::CREATED)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn ensure_original_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    let created = terrain.ensure_original_grass(x, z).await.map_err(|e| {
        error!("Failed to ensure original grass ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if created {
        Ok(StatusCode::CREATED)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn get_splatmap(
    Path((x, z)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
) -> Result<Response, StatusCode> {
    let data = state.terrain.read_splatmap(x, z).await.map_err(|e| {
        error!("Failed to read splatmap ({}, {}): {}", x, z, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        data,
    )
        .into_response())
}

async fn put_splatmap(
    Path((x, z)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .game_state
        .save_terrain_splatmap(x, z, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write splatmap ({}, {}): {}", x, z, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_grass(
    Path((x, z)): Path<(i32, i32)>,
    request_headers: axum::http::HeaderMap,
    State(state): State<ObjectsState>,
) -> Result<Response, StatusCode> {
    serve_vegetation(
        &state.terrain,
        x,
        z,
        coords::grass_path(state.terrain.base_dir(), x, z),
        &request_headers,
    )
    .await
}

async fn put_grass(
    Path((x, z)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    let _guard = state.game_state.world_edit_guard().await;
    let result = async {
        state.terrain.write_grass(x, z, &body).await?;
        state
            .game_state
            .publish_terrain_tiles(&[(coords::wrap_tile_x(x), z)])
            .await
    }
    .await;
    result.map_err(|e| {
        error!("Failed to write grass ({}, {}): {}", x, z, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_minimap(
    Path((rx, rz)): Path<(i32, i32)>,
    Query(query): Query<MinimapQuery>,
    request_headers: axum::http::HeaderMap,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let size = query.size.unwrap_or(coords::MINIMAP_BASE_SIZE);
    if size != coords::MINIMAP_BASE_SIZE && !coords::MINIMAP_LOD_SIZES.contains(&size) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let found = terrain.stat_minimap_lod(rx, rz, size).await.map_err(|e| {
        error!("Failed to stat minimap ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // The client's ?v= only changes on client deploys, so a server-side rebake
    // must reach players through revalidation: cache briefly, then let the
    // ETag turn repeat fetches into bodyless 304s. The tag comes from the
    // file's identity (path/mtime/len) so revalidation never reads the body.
    // 404s cache briefly too, so clients near unbaked regions don't hammer
    // the route.
    const MINIMAP_CACHE: (header::HeaderName, &str) =
        (header::CACHE_CONTROL, "public, max-age=300");
    let Some((tile, meta)) = found else {
        return Ok((StatusCode::NOT_FOUND, [MINIMAP_CACHE]).into_response());
    };
    let etag = file_etag(&tile.path, &meta);
    let revalidated = request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag);
    if revalidated {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag)],
            [MINIMAP_CACHE],
        )
            .into_response());
    }
    let bytes = tokio::fs::read(&tile.path).await.map_err(|e| {
        error!("Failed to read minimap ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [(header::ETAG, etag)],
        [(header::CONTENT_TYPE, tile.family.content_type())],
        [MINIMAP_CACHE],
        bytes,
    )
        .into_response())
}

/// Terrain files a client refetches as it walks: serve them with an ETag so a
/// repeat fetch costs headers instead of the body. Missing files cache
/// briefly too, so clients near unbaked tiles stop hammering the route.
async fn serve_revalidated(
    path: std::path::PathBuf,
    request_headers: &axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    const CACHE: (header::HeaderName, &str) = (header::CACHE_CONTROL, "public, max-age=300");
    let meta = match tokio::fs::metadata(&path).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((StatusCode::NOT_FOUND, [CACHE]).into_response());
        }
        Err(e) => {
            error!("Failed to stat {:?}: {}", path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let etag = file_etag(&path, &meta);
    let revalidated = request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag);
    if revalidated {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, etag)], [CACHE]).into_response());
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        error!("Failed to read {:?}: {}", path, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        [(header::ETAG, etag)],
        [(header::CONTENT_TYPE, "application/octet-stream")],
        [CACHE],
        bytes,
    )
        .into_response())
}

/// Cache tag for a terrain file: its path, mtime and size. A rebake or an
/// editor save changes all three sources cheaply, and none of them read the
/// body.
fn file_etag(path: &std::path::Path, meta: &std::fs::Metadata) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(meta.len().to_le_bytes());
    if let Ok(modified) = meta.modified() {
        if let Ok(since_epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
            hasher.update(since_epoch.as_nanos().to_le_bytes());
        }
    }
    let digest = hasher.finalize();
    let tag = digest
        .iter()
        .take(8)
        .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
    format!("\"{tag:016x}\"")
}

async fn put_minimap(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    terrain.write_minimap(rx, rz, &body).await.map_err(|e| {
        error!("Failed to write minimap ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_land_grades(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let data = terrain.read_land_grades(rx, rz).await.map_err(|e| {
        error!("Failed to read land grades ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let data = data.unwrap_or_else(|| crate::land_grades::default_grades(rx, rz));
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        data,
    )
        .into_response())
}

async fn put_land_grades(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain
        .write_land_grades(rx, rz, &body)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => (StatusCode::BAD_REQUEST, e.to_string()),
            _ => {
                error!("Failed to write land grades ({}, {}): {}", rx, rz, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_zone(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    let zone = terrain.read_zone(rx, rz).await.map_err(|e| {
        error!("Failed to read zone ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(zone).into_response())
}

async fn put_zone(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    terrain.write_zone(rx, rz, &body).await.map_err(|e| {
        error!("Failed to write zone ({}, {}): {}", rx, rz, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_object(
    Path((rx, rz)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
) -> Result<Response, StatusCode> {
    let data = state.terrain.read_object(rx, rz).await.map_err(|e| {
        error!("Failed to read object ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(data).into_response())
}

async fn put_object(
    Path((rx, rz)): Path<(i32, i32)>,
    State(state): State<ObjectsState>,
    Json(body): Json<serde_json::Value>,
) -> Result<StatusCode, (StatusCode, String)> {
    let placements = GameState::parse_region_furniture(&body).map_err(|e| {
        warn!("Invalid region objects ({rx},{rz}): {e}");
        (
            StatusCode::BAD_REQUEST,
            "Invalid region object data".to_string(),
        )
    })?;
    state
        .terrain
        .write_object(rx, rz, &body)
        .await
        .map_err(|e| {
            error!("Failed to write object ({}, {}): {}", rx, rz, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
    state.game_state.sync_region_furniture(rx, rz, &placements);
    Ok(StatusCode::NO_CONTENT)
}

async fn get_climate(
    Path((rx, rz)): Path<(i32, i32)>,
    request_headers: axum::http::HeaderMap,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    serve_revalidated(
        coords::climate_path(terrain.base_dir(), rx, rz),
        &request_headers,
    )
    .await
}

/// The list the server loaded at boot, not the file on disk: the client asks
/// with `?v=<sectors_tag>` from `WeatherSync`, so the body is immutable per
/// URL and a re-bake reaches clients only through a restart.
async fn get_weather_sectors(State(state): State<ObjectsState>) -> Response {
    match state.game_state.weather_sectors_json() {
        Some(json) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            json,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, [(header::CACHE_CONTROL, "no-cache")]).into_response(),
    }
}

async fn get_trees(
    Path((x, z)): Path<(i32, i32)>,
    request_headers: axum::http::HeaderMap,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    serve_vegetation(
        &terrain,
        x,
        z,
        coords::tree_path(terrain.base_dir(), x, z),
        &request_headers,
    )
    .await
}

async fn serve_vegetation(
    terrain: &TerrainIO,
    x: i32,
    z: i32,
    path: std::path::PathBuf,
    headers: &axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    let tile = terrain.read_landscaping_tile(x, z).await.map_err(|error| {
        error!(%error, "Failed to read landscaping tile");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut response = if let Some(tile) = tile {
        let raw = match tokio::fs::read(&path).await {
            Ok(raw) => Some(raw),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                error!(%error, "Failed to read vegetation tile");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
        if let Some(raw) = raw {
            let bytes = onlinerpg_terrain::landscaping::filter_vegetation(raw, &tile.cleared)
                .map_err(|error| {
                    error!(%error, "Failed to filter landscaping vegetation");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let etag = format!("\"{:x}\"", sha2::Sha256::digest(&bytes));
            if headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                == Some(etag.as_str())
            {
                (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response()
            } else {
                (
                    [(header::ETAG, etag)],
                    [(header::CONTENT_TYPE, "application/octet-stream")],
                    bytes,
                )
                    .into_response()
            }
        } else {
            StatusCode::NOT_FOUND.into_response()
        }
    } else {
        serve_revalidated(path, headers).await?
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache"),
    );
    Ok(response)
}

async fn get_river_field(
    Path((x, z)): Path<(i32, i32)>,
    request_headers: axum::http::HeaderMap,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    serve_revalidated(
        coords::river_field_path(terrain.base_dir(), x, z),
        &request_headers,
    )
    .await
}

async fn get_water_field(
    Path((x, z)): Path<(i32, i32)>,
    request_headers: axum::http::HeaderMap,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<Response, StatusCode> {
    serve_revalidated(
        coords::water_field_path(terrain.base_dir(), x, z),
        &request_headers,
    )
    .await
}

async fn delete_region_handler(
    Path((rx, rz)): Path<(i32, i32)>,
    State(terrain): State<Arc<TerrainIO>>,
) -> Result<StatusCode, StatusCode> {
    terrain.delete_region(rx, rz).await.map_err(|e| {
        error!("Failed to delete region ({}, {}): {}", rx, rz, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::unique_temp_dir;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn snapshot_versions_are_immutable_and_ground_omits_visuals() {
        use onlinerpg_shared::{deserialize_server_msg, ServerMessage};
        use onlinerpg_terrain::snapshot::{content_version, encode_snapshot};
        let game = Arc::new(crate::game_state::tests::make_test_game_state(
            "http_snapshot",
        ));
        let terrain = Arc::new(TerrainIO::new(unique_temp_dir("http_snapshot_tiles")));
        terrain
            .write_grass(0, 0, &vec![0; 256 * 1024])
            .await
            .unwrap();
        let ServerMessage::TerrainTileVersion {
            version,
            ground_version,
            ..
        } = terrain.snapshot_versions(0, 0).await.unwrap()
        else {
            panic!()
        };
        let notice = terrain.snapshot_versions(0, 0).await.unwrap();
        assert!(encode_snapshot(&notice).unwrap().len() < 256);
        let state = ObjectsState {
            terrain: terrain.clone(),
            game_state: game,
        };
        for (profile, hash) in [("full", &version), ("ground", &ground_version)] {
            let request = || Path((profile.to_owned(), 0, 0, hash.clone()));
            let response = get_snapshot(request(), State(state.clone()), HeaderMap::new()).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "public, max-age=31536000, immutable"
            );
            let etag = etag_of(&response);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(content_version(&bytes), *hash);
            let ServerMessage::TerrainTileSnapshot {
                height,
                splat,
                trees,
                grass,
                landscape,
                ..
            } = deserialize_server_msg(&bytes).unwrap()
            else {
                panic!()
            };
            assert_eq!(height.len(), 8450);
            assert_eq!(splat.len(), 16384);
            if profile == "ground" {
                assert!(trees.is_none() && grass.is_none() && landscape.is_none());
                assert!(bytes.len() < 26000);
            } else {
                assert_eq!(grass.unwrap().len(), 256 * 1024);
            }
            let mut headers = HeaderMap::new();
            headers.insert(header::IF_NONE_MATCH, etag.parse().unwrap());
            assert_eq!(
                get_snapshot(request(), State(state.clone()), headers)
                    .await
                    .status(),
                StatusCode::NOT_MODIFIED
            );
        }
        let reopened = TerrainIO::new(terrain.base_dir().to_path_buf());
        assert_eq!(
            encode_snapshot(&notice).unwrap(),
            encode_snapshot(&reopened.snapshot_versions(0, 0).await.unwrap()).unwrap()
        );
        terrain.write_grass(0, 0, &[1; 64]).await.unwrap();
        let ServerMessage::TerrainTileVersion {
            version: next,
            ground_version: ground_next,
            ..
        } = terrain.snapshot_versions(0, 0).await.unwrap()
        else {
            panic!()
        };
        assert_ne!(next, version);
        assert_eq!(ground_next, ground_version);
        let stale = get_snapshot(
            Path(("full".into(), 0, 0, version)),
            State(state.clone()),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        assert_eq!(stale.headers()[header::CACHE_CONTROL], "no-store");
        let mut splat = terrain.read_splatmap(0, 0).await.unwrap();
        splat[0] = 5;
        terrain.write_splatmap(0, 0, &splat).await.unwrap();
        let stale = get_snapshot(
            Path(("ground".into(), 0, 0, ground_version)),
            State(state.clone()),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        let path = onlinerpg_terrain::coords::heightmap_path(terrain.base_dir(), 0, 0);
        tokio::fs::create_dir_all(path).await.unwrap();
        let failure = get_snapshot(
            Path(("full".into(), 0, 0, next)),
            State(state),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(failure.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failure.headers()[header::CACHE_CONTROL], "no-store");
    }

    fn etag_of(response: &Response) -> String {
        response
            .headers()
            .get(header::ETAG)
            .expect("served terrain files carry an ETag")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn revalidation_turns_a_repeat_fetch_into_a_304() {
        let dir = unique_temp_dir("terrain_revalidate");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tile.bin");
        std::fs::write(&path, b"first").unwrap();

        let first = serve_revalidated(path.clone(), &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let tag = etag_of(&first);

        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, tag.parse().unwrap());
        let repeat = serve_revalidated(path.clone(), &headers).await.unwrap();
        assert_eq!(repeat.status(), StatusCode::NOT_MODIFIED);

        // A rewrite changes size and mtime, so the stale tag must miss.
        std::fs::write(&path, b"second body").unwrap();
        let after_edit = serve_revalidated(path.clone(), &headers).await.unwrap();
        assert_eq!(after_edit.status(), StatusCode::OK);
        assert_ne!(etag_of(&after_edit), tag);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_missing_tile_is_a_cacheable_404() {
        let dir = unique_temp_dir("terrain_missing");
        let response = serve_revalidated(dir.join("absent.bin"), &HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=300"
        );
    }
}
