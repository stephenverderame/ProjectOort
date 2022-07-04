use std::env;
use std::error::Error;
use core::fmt::Display;

#[derive(Debug)]
pub struct ServerConfiguration {
    pub port: u16,
}

impl Default for ServerConfiguration {
    fn default() -> Self {
        ServerConfiguration {
            port: 33200,
        }
    }
}

impl Display for ServerConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, 
            "ServerConfiguration {{\n\
                \tport: {},\n\
            }}", self.port)
    }
}

fn parse_args_helper(mut args: env::Args, mut config: ServerConfiguration) 
    -> Result<ServerConfiguration, Box<dyn Error>>
{
    match args.next() {
        None => Ok(config),
        Some(x) if x == "-p" || x == "--port" => {
            let port = args.next().ok_or("--port requires an argument")?;
            config.port = port.parse::<u16>()?;
            parse_args_helper(args, config)
        },
        Some(x) => Err(format!("Unknown argument \"{}\"", x))?,

    }
}

#[inline(always)]
pub fn parse_args(mut args: env::Args) 
    -> Result<ServerConfiguration, Box<dyn Error>> 
{
    args.next(); // skip the program name
    parse_args_helper(args, Default::default())
}