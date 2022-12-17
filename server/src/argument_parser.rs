use core::fmt::Display;
use shared_types::game_map;
use std::env;
use std::error::Error;

pub const DEFAULT_PORT: u16 = 33200;

#[derive(Debug)]
pub enum MapType {
    AsteroidMap,
}

impl MapType {
    pub fn get_game_map(&self) -> Box<dyn game_map::Map> {
        match self {
            Self::AsteroidMap => Box::new(game_map::AsteroidMap {}),
        }
    }
}

impl TryFrom<&str> for MapType {
    type Error = String;
    fn try_from(val: &str) -> Result<Self, Self::Error> {
        match val {
            "asteroid" => Ok(Self::AsteroidMap),
            _ => Err(format!("Invalid map type: {}", val)),
        }
    }
}

pub const DEFAULT_MAP: MapType = MapType::AsteroidMap;

#[derive(Debug)]
pub struct ServerConfiguration {
    pub port: u16,
    pub map: MapType,
}

impl Default for ServerConfiguration {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            map: DEFAULT_MAP,
        }
    }
}

impl Display for ServerConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "ServerConfiguration {{\n\
                \tport: {},\n\
                \tmap: {:?},\n\
            }}",
            self.port, self.map
        )
    }
}

fn parse_args_helper(
    mut args: env::Args,
    mut config: ServerConfiguration,
) -> Result<ServerConfiguration, Box<dyn Error>> {
    match args.next() {
        None => Ok(config),
        Some(x) if x == "-p" || x == "--port" => {
            let port = args.next().ok_or("--port requires an argument")?;
            config.port = port.parse::<u16>()?;
            parse_args_helper(args, config)
        }
        Some(x) if x == "-m" || x == "--map" => {
            let map_name = args.next().ok_or("--map requires an argument")?;
            config.map = MapType::try_from(map_name.as_str())?;
            parse_args_helper(args, config)
        }
        Some(x) => Err(format!("Unknown argument \"{}\"", x))?,
    }
}

#[inline]
pub fn parse_args(
    mut args: env::Args,
) -> Result<ServerConfiguration, Box<dyn Error>> {
    args.next(); // skip the program name
    parse_args_helper(args, ServerConfiguration::default())
}
