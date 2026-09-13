//! Regional weather sync and admin overrides (doc/WEATHER_SYSTEM.md).

use bytes::Bytes;
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::weather::{WeatherSectors, WEATHER_SECTORS_VERSION};
use onlinerpg_shared::PlayerId;
use sha2::Digest;
use tracing::{info, warn};

use super::{chat::parse_bounded, GameState};

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherState {
    pub seed: u64,
    /// Multiplier on every zone's rain chance; 1.0 is the baked schedule.
    pub bias: f32,
    pub rain_override: Option<f32>,
    /// Sector bytes loaded at boot and served to clients.
    pub sectors_json: Bytes,
    /// Content hash of `sectors_json`; clients re-fetch only when it changes.
    pub sectors_tag: String,
}

impl WeatherState {
    pub fn new(seed: u64, bias: f32, sectors_json: Vec<u8>) -> Self {
        let sectors_tag = sectors_tag(&sectors_json);
        Self {
            seed,
            bias,
            rain_override: None,
            sectors_json: Bytes::from(sectors_json),
            sectors_tag,
        }
    }
}

pub fn sectors_tag(json: &[u8]) -> String {
    let digest = sha2::Sha256::digest(json);
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

impl GameState {
    /// Weather stays off until a bake has produced `weather-sectors.json`.
    pub async fn load_weather(&self, bias: f32) {
        let json = match self.terrain_io.read_weather_sectors_bytes().await {
            Ok(Some(json)) => json,
            Ok(None) => {
                warn!("weather: no weather-sectors.json in the terrain dir; weather disabled");
                return;
            }
            Err(err) => {
                warn!("weather: failed to read weather-sectors.json; weather disabled: {err}");
                return;
            }
        };
        match serde_json::from_slice::<WeatherSectors>(&json) {
            Ok(sectors) if sectors.version != WEATHER_SECTORS_VERSION => {
                warn!(
                    "weather: weather-sectors.json is version {} (expected {WEATHER_SECTORS_VERSION}); weather disabled",
                    sectors.version
                )
            }
            Ok(sectors) => {
                let state = WeatherState::new(sectors.seed, bias, json);
                info!(
                    "weather: {} rain sectors, seed {}, bias {bias}, tag {}",
                    sectors.sectors.len(),
                    sectors.seed,
                    state.sectors_tag
                );
                self.set_weather(state);
            }
            Err(err) => {
                warn!("weather: invalid weather-sectors.json; weather disabled: {err}")
            }
        }
    }

    pub fn set_weather(&self, state: WeatherState) {
        *self.weather.write().expect("weather lock poisoned") = Some(state);
    }

    pub fn weather_sync_message(&self) -> Option<ServerMessage> {
        self.weather
            .read()
            .expect("weather lock poisoned")
            .as_ref()
            .map(|w| ServerMessage::WeatherSync {
                seed: w.seed,
                bias: w.bias,
                sectors_tag: w.sectors_tag.clone(),
                rain_override: w.rain_override,
            })
    }

    pub fn weather_sectors_json(&self) -> Option<Bytes> {
        self.weather
            .read()
            .expect("weather lock poisoned")
            .as_ref()
            .map(|w| w.sectors_json.clone())
    }

    pub fn broadcast_weather(&self) {
        if let Some(msg) = self.weather_sync_message() {
            self.broadcast(msg);
        }
    }

    pub(super) fn weather_command(
        &self,
        admin_id: &PlayerId,
        args: &str,
    ) -> Result<String, String> {
        let mut args = args.split_whitespace();
        let rain_override = match (args.next(), args.next(), args.next()) {
            (Some("rain"), raw, None) => Some(parse_bounded(
                raw.unwrap_or("1"),
                0.0..=1.0,
                "Weather: intensity",
            )?),
            (Some("clear"), None, None) => Some(0.0),
            (Some("auto"), None, None) => None,
            _ => return Err("Weather: /weather rain [0..1], /weather clear, /weather auto".into()),
        };
        {
            let mut weather = self.weather.write().expect("weather lock poisoned");
            weather
                .as_mut()
                .ok_or("Weather: weather data is not loaded.")?
                .rain_override = rain_override;
        }
        info!(admin = ?admin_id, ?rain_override, "admin weather override");
        self.broadcast_weather();
        Ok(match rain_override {
            None => "Weather: automatic regional weather restored server-wide.".into(),
            Some(0.0) => "Weather: clear skies server-wide. /weather auto restores automatic weather.".into(),
            Some(rain) => format!("Weather: rain intensity {rain} server-wide. /weather auto restores automatic weather."),
        })
    }
}
