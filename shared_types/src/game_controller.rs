
use super::*;

pub struct GameStats {

}

pub struct PlayerStats {

}

pub trait GameController {
    fn get_game_objects(&self) -> &[RemoteObject];

    fn get_game_time(&self) -> std::time::Duration;

    fn get_game_stats(&self) -> &GameStats;

    fn get_player_stats(&self) -> &PlayerStats;

    fn update_local_objects(&mut self, objects: &[RemoteObject]);
}