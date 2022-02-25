pub const EMP_TIMEOUT: i32 = 20;
pub const HEALTH: i32 = 20;
pub const NO_OF_ROBOTS: i32 = 1000;
pub const GAME_TIME_MINUTES: i32 = 420;
pub const GAME_MINUTES_PER_FRAME: i32 = 2;
pub const ATTACKER_RESTRICTED_FRAMES: i32 = 30;
pub const GAME_START_HOUR: i32 = 9;
pub const NO_OF_FRAMES: i32 = GAME_TIME_MINUTES / GAME_MINUTES_PER_FRAME;
pub const MAX_STAY_IN_TIME: i32 = 10;
pub const MAP_SIZE: usize = 40;
pub const ATTACK_START_TIME: &str = "20:00:00";
pub const ATTACK_END_TIME: &str = "23:59:59";
pub const DEFENSE_START_TIME: &str = "00:00:00";
pub const DEFENSE_END_TIME: &str = "19:00:00";
pub const TOTAL_ATTACKS_PER_LEVEL: i64 = 2;
pub const TOTAL_ATTACKS_ON_A_BASE: i64 = 2;
pub const ROAD_ID: i32 = 4;
pub const INITIAL_RATING: f32 = 1000.0;
pub const K_FACTOR: f32 = 200.0;
pub const EMP_PENALTY: i32 = 200;
pub const MAX_SCORE: i32 = 2 * HEALTH * NO_OF_ROBOTS - EMP_PENALTY;
