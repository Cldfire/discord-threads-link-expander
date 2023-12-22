use std::fs;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub bot_token: String,
}

impl Config {
    pub fn load_from_disk() -> Result<Config, anyhow::Error> {
        let config_string = fs::read_to_string("./discord-threads-link-expander-config.toml")?;
        Ok(toml::from_str(&config_string)?)
    }
}
