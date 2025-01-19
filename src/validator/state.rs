use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::constants::{BOMB_DAMAGE_MULTIPLIER, LIVES, PERCENTANGE_ARTIFACTS_OBTAINABLE};
use crate::{
    api::attack::socket::{BuildingResponse, BulletHit, DefenderResponse},
    validator::util::{
        Attacker, BuildingDetails, Coords, DefenderDetails, DefenderReturnType, InValidation,
        MineDetails, SourceDestXY,
    },
};

use serde::{Deserialize, Serialize};

use super::util::BombType;

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    pub frame_no: i32,
    pub attacker_user_id: i32,
    pub defender_user_id: i32,
    pub attacker: Option<Attacker>,
    pub attacker_death_count: i32,
    pub bombs: BombType,
    pub damage_percentage: f32,
    pub artifacts: i32,
    pub defenders: Vec<DefenderDetails>,
    pub mines: Vec<MineDetails>,
    pub buildings: Vec<BuildingDetails>,
    pub total_hp_buildings: i32,
    pub in_validation: InValidation,
    pub uav_revealed: bool,
    pub uav_checkpoints: Vec<f32>,
    pub revealed_mines: Vec<MineDetails>,
    pub last_processed_frame: i32,
    pub revealed_checkpoints: Vec<f32>,
}

impl State {
    pub fn new(
        attacker_user_id: i32,
        defender_user_id: i32,
        defenders: Vec<DefenderDetails>,
        mines: Vec<MineDetails>,
        buildings: Vec<BuildingDetails>,
    ) -> State {
        State {
            frame_no: 0,
            attacker_user_id,
            defender_user_id,
            attacker: None,
            attacker_death_count: 0,
            bombs: BombType {
                id: -1,
                radius: 0,
                damage: 0,
                total_count: 0,
            },
            damage_percentage: 0.0,
            artifacts: 0,
            defenders,
            mines,
            buildings,
            total_hp_buildings: 0,
            in_validation: InValidation {
                message: "".to_string(),
                is_invalidated: false,
            },
            uav_revealed: false,
            uav_checkpoints: vec![20.0, 50.0, 70.0],
            revealed_checkpoints: Vec::new(),
            revealed_mines: Vec::new(),
            last_processed_frame: -1,
        }
    }

    pub fn check_uav_reveal(&mut self) -> Option<Vec<MineDetails>> {
        let mut revealed: Vec<MineDetails> = Vec::new();
        for checkpoint in &self.uav_checkpoints {
            if self.damage_percentage >= *checkpoint
                && !self.revealed_checkpoints.contains(checkpoint)
            {
                self.revealed_checkpoints.push(*checkpoint);
                let mines: Vec<MineDetails> = self
                    .mines
                    .iter()
                    .map(|mine| MineDetails {
                        id: mine.id,
                        position: mine.position.clone(),
                        radius: mine.radius,
                        damage: mine.damage,
                    })
                    .collect();
                revealed.extend(mines);
            }
        }

        if !revealed.is_empty() {
            self.revealed_mines = revealed.clone();
            return Some(revealed);
        }

        None
    }

    pub fn self_destruct(&mut self) {
        self.attacker_death_count += 1;
        self.attacker.as_mut().unwrap().attacker_health = 0;
        for defender in self.defenders.iter_mut() {
            defender.target_id = None;
        }
    }

    pub fn set_total_hp_buildings(&mut self) {
        let mut total_hp = 0;
        for building in self.buildings.iter() {
            total_hp += building.total_hp;
        }
        self.total_hp_buildings = total_hp;
    }

    pub fn set_bombs(&mut self, bomb_type: BombType, bombs: i32) {
        self.bombs = BombType {
            id: bomb_type.id,
            radius: bomb_type.radius,
            damage: bomb_type.damage,
            total_count: bombs,
        };
    }
    pub fn place_attacker(&mut self, attacker: Attacker) {
        self.attacker = Some(attacker);
        // println!("defnders: {:?}",self.defenders);
    }

    pub fn mine_blast_update(&mut self, _id: i32, damage_to_attacker: i32) {
        let attacker = self.attacker.as_mut().unwrap();

        if attacker.attacker_health > 0 {
            attacker.attacker_health =
                std::cmp::max(0, attacker.attacker_health - damage_to_attacker);
            if attacker.attacker_health == 0 {
                self.attacker_death_count += 1;
                for defender in self.defenders.iter_mut() {
                    defender.target_id = None;
                }
                attacker.attacker_pos = Coords { x: -1, y: -1 };
            }
        }

        self.mines.retain(|mine| mine.id != _id);
    }

    pub fn update_frame_number(&mut self, frame_no: i32) {
        self.frame_no = frame_no;
    }

    pub fn attacker_movement(
        &mut self,
        frame_no: i32,
        roads: &HashSet<(i32, i32)>,
        attacker_current: Attacker,
    ) -> Option<Attacker> {
        if (frame_no - self.frame_no) != 1 {
            self.in_validation = InValidation {
                message: "Frame number mismatch".to_string(),
                is_invalidated: true,
            };
            // GAME_OVER
        }

        if self.attacker_death_count == LIVES {
            self.in_validation = InValidation {
                message: "Attacker Lives forged!".to_string(),
                is_invalidated: true,
            };
        }

        for coord in attacker_current.path_in_current_frame.clone().into_iter() {
            if !roads.contains(&(coord.x, coord.y)) {
                // GAME_OVER

                println!("attacker out of road at {} frame", frame_no);
            }
        }

        let mut attacker = attacker_current.clone();

        if attacker.attacker_speed + 1 != attacker.path_in_current_frame.len() as i32 {
            println!(
                "attacker speed abuse at {} frame --- speed  :{}, length: {}",
                frame_no,
                attacker.attacker_speed,
                attacker.path_in_current_frame.len()
            );
        }

        let mut coord_temp: Coords = Coords {
            x: attacker_current.path_in_current_frame[0].x,
            y: attacker_current.path_in_current_frame[0].y,
        };

        for (i, coord) in attacker_current
            .path_in_current_frame
            .into_iter()
            .enumerate()
        {
            if (coord_temp.x - coord.x > 1)
                || (coord_temp.y - coord.y > 1)
                || ((coord_temp.x - coord.x).abs() == 1 && coord_temp.y != coord.y)
                || ((coord_temp.y - coord.y).abs() == 1 && coord_temp.x != coord.x)
            {
                // GAME_OVER
                // println!("attacker skipped a tile at {} frame", frame_no);
                self.in_validation = InValidation {
                    message: "attacker skipped a tile".to_string(),
                    is_invalidated: true,
                };
            }

            let new_pos = coord;

            for defender in self.defenders.iter_mut() {
                if defender.target_id.is_none()
                    && defender.is_alive
                    && (((defender.defender_pos.x - new_pos.x).abs()
                        + (defender.defender_pos.y - new_pos.y).abs())
                        <= defender.radius)
                {
                    // println!(
                    //     "defender triggered when attacker was at ---- x:{}, y:{} and defender id: {}",
                    //     new_pos.x, new_pos.y, defender.id
                    // );
                    defender.target_id = Some((i) as f32 / attacker.attacker_speed as f32);
                    attacker.trigger_defender = true;
                }
            }

            coord_temp = coord;
        }

        self.frame_no += 1;

        let attacker_result = Attacker {
            id: attacker.id,
            attacker_pos: *attacker.path_in_current_frame.last().unwrap(),
            attacker_health: attacker.attacker_health,
            attacker_speed: attacker.attacker_speed,
            path_in_current_frame: attacker.path_in_current_frame.clone(),
            bombs: attacker.bombs.clone(),
            trigger_defender: attacker.trigger_defender,
            bomb_count: attacker.bomb_count,
        };
        Some(attacker_result)
    }

    pub fn place_bombs(
        &mut self,
        current_pos: Coords,
        bomb_position: Coords,
    ) -> Vec<BuildingResponse> {
        // if attacker_current.bombs.len() - attacker.bombs.len() > 1 {

        // }

        if self.bombs.total_count <= 0 {
            self.in_validation = InValidation {
                message: "Bomb Count forged".to_string(),
                is_invalidated: true,
            };
        }

        if let Some(attacker) = &mut self.attacker {
            attacker.bomb_count -= 1;
        }

        if current_pos.x != bomb_position.x || current_pos.y != bomb_position.y {
            //GAME_OVER
            println!("Bomb placed out of path");
            self.in_validation = InValidation {
                message: "Bomb placed out of path".to_string(),
                is_invalidated: true,
            };
        }

        self.bomb_blast(bomb_position)
    }

    pub fn defender_movement(
        &mut self,
        attacker_delta: Vec<Coords>,
        shortest_path: &HashMap<SourceDestXY, Coords>,
    ) -> DefenderReturnType {
        let attacker = self.attacker.as_mut().unwrap();
        let mut defenders_damaged: Vec<DefenderResponse> = Vec::new();
        let bullet_hits: Vec<BulletHit> = Vec::new();

        // if attacker is dead, no need to move the defenders
        if attacker.attacker_health == 0 {
            return DefenderReturnType {
                attacker_health: attacker.attacker_health,
                defender_response: defenders_damaged,
                bullet_hits,
                state: self.clone(),
            };
        }

        let mut collision_array: Vec<(usize, f32)> = Vec::new();

        for (index, defender) in self.defenders.iter_mut().enumerate() {
            if !defender.is_alive || defender.target_id.is_none() {
                continue;
            }

            let attacker_ratio = attacker.attacker_speed as f32 / defender.speed as f32;
            let mut attacker_float_coords = (
                attacker.attacker_pos.x as f32,
                attacker.attacker_pos.y as f32,
            );
            let mut attacker_delta_index = 1;

            defender.path_in_current_frame.clear();
            defender.path_in_current_frame.push(defender.defender_pos);

            // for every tile of defender's movement
            for i in 1..=defender.speed {
                let next_hop = shortest_path
                    .get(&SourceDestXY {
                        source_x: defender.defender_pos.x,
                        source_y: defender.defender_pos.y,
                        dest_x: attacker.attacker_pos.x,
                        dest_y: attacker.attacker_pos.y,
                    })
                    .unwrap_or(&defender.defender_pos);

                let mut attacker_tiles_covered_fract = (((i - 1) as f32) * attacker_ratio).fract();

                let mut attacker_mov_x = 0.0;
                let mut attacker_mov_y = 0.0;

                let mut attacker_tiles_left = attacker_ratio;
                while attacker_tiles_left > 1e-6 {
                    let attacker_tiles_fract_left = attacker_tiles_left
                        .min(1.0)
                        .min(1.0 - attacker_tiles_covered_fract);

                    attacker_mov_x += attacker_tiles_fract_left
                        * ((attacker_delta[attacker_delta_index].x
                            - attacker_delta[attacker_delta_index - 1].x)
                            as f32);
                    attacker_mov_y += attacker_tiles_fract_left
                        * ((attacker_delta[attacker_delta_index].y
                            - attacker_delta[attacker_delta_index - 1].y)
                            as f32);

                    attacker_tiles_left -= attacker_tiles_fract_left;
                    attacker_tiles_covered_fract =
                        (attacker_tiles_covered_fract + attacker_tiles_fract_left).fract();
                    if attacker_tiles_covered_fract == 0.0 {
                        attacker_delta_index += 1;
                    }
                }

                attacker_float_coords.0 += attacker_mov_x;
                attacker_float_coords.1 += attacker_mov_y;

                attacker.attacker_pos = Coords {
                    x: attacker_float_coords.0.round() as i32,
                    y: attacker_float_coords.1.round() as i32,
                };

                // if defender lags
                if defender.target_id.unwrap() >= ((i as f32) / (defender.speed as f32)) {
                    defender.path_in_current_frame.push(defender.defender_pos);
                    continue;
                }
                defender.defender_pos = *next_hop;
                defender.path_in_current_frame.push(defender.defender_pos);

                // if defender and attacker are on the same tile, add the defender to the collision_array
                if (defender.defender_pos == attacker.attacker_pos)
                    || (defender.path_in_current_frame[(i - 1) as usize] == attacker.attacker_pos)
                {
                    collision_array.push((index, (i as f32) / (defender.speed as f32)));
                    defender.damage_dealt = true;
                    break;
                }
            }
            defender.target_id = Some(0.0);
            if !defender.damage_dealt {
                collision_array.push((index, 2.0));
            }
            attacker.attacker_pos = *attacker_delta.first().unwrap();
        }

        attacker.attacker_pos = *attacker_delta.last().unwrap();
        // sort the collision_array by the time of collision
        collision_array.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut attacker_death_time = 0.0; // frame fraction at which attacker dies
        for (index, time) in collision_array {
            self.defenders[index].target_id = None;
            if time > 1.0 {
                break;
            }
            if attacker.attacker_health == 0 {
                self.defenders[index].defender_pos = self.defenders[index].path_in_current_frame
                    [(attacker_death_time * (self.defenders[index].speed as f32)) as usize];
                continue;
            }
            defenders_damaged.push(DefenderResponse {
                id: self.defenders[index].id,
                position: self.defenders[index].defender_pos,
                damage: self.defenders[index].damage,
            });
            self.defenders[index].damage_dealt = true;
            attacker.trigger_defender = true;
            attacker.attacker_health =
                max(0, attacker.attacker_health - self.defenders[index].damage);
            self.defenders[index].is_alive = false;

            if attacker.attacker_health == 0 {
                attacker_death_time = time;
                self.attacker_death_count += 1;
            }
        }

        DefenderReturnType {
            attacker_health: attacker.attacker_health,
            defender_response: defenders_damaged,
            bullet_hits,
            state: self.clone(),
        }
    }

    pub fn mine_blast(&mut self, start_pos: Option<Coords>) -> Vec<MineDetails> {
        let mut damage_to_attacker;
        let attack_current_pos = start_pos.unwrap();

        let mut triggered_mines: Vec<MineDetails> = Vec::new();

        for mine in self.mines.clone().iter_mut() {
            if attack_current_pos.x == mine.position.x && attack_current_pos.y == mine.position.y {
                damage_to_attacker = mine.damage;
                triggered_mines.push(MineDetails {
                    id: mine.id,
                    position: mine.position,
                    radius: mine.radius,
                    damage: mine.damage,
                });
                self.mine_blast_update(mine.id, damage_to_attacker);
            }
        }

        triggered_mines
    }

    pub fn bomb_blast(&mut self, bomb_position: Coords) -> Vec<BuildingResponse> {
        let bomb = &mut self.bombs;
        let mut buildings_damaged: Vec<BuildingResponse> = Vec::new();
        for building in self.buildings.iter_mut() {
            if building.current_hp > 0 {
                let mut artifacts_taken_by_destroying_building: i32 = 0;

                let building_matrix: HashSet<Coords> = (building.tile.y
                    ..building.tile.y + building.width)
                    .flat_map(|y| {
                        (building.tile.x..building.tile.x + building.width)
                            .map(move |x| Coords { x, y })
                    })
                    .collect();

                let bomb_matrix: HashSet<Coords> = (bomb_position.y - bomb.radius
                    ..bomb_position.y + bomb.radius + 1)
                    .flat_map(|y| {
                        (bomb_position.x - bomb.radius..bomb_position.x + bomb.radius + 1)
                            .map(move |x| Coords { x, y })
                    })
                    .collect();

                let coinciding_coords_damage = building_matrix.intersection(&bomb_matrix).count();

                let damage_buildings: f32 =
                    coinciding_coords_damage as f32 / building_matrix.len() as f32;

                if damage_buildings != 0.0 {
                    let old_hp = building.current_hp;
                    let mut current_damage = (damage_buildings
                        * (bomb.damage as f32 * BOMB_DAMAGE_MULTIPLIER))
                        .round() as i32;

                    building.current_hp -= current_damage;

                    if building.current_hp <= 0 {
                        building.current_hp = 0;
                        current_damage = old_hp;
                        artifacts_taken_by_destroying_building =
                            (building.artifacts_obtained as f32 * PERCENTANGE_ARTIFACTS_OBTAINABLE)
                                .floor() as i32;
                        self.artifacts += artifacts_taken_by_destroying_building;
                        self.damage_percentage +=
                            (current_damage as f32 / self.total_hp_buildings as f32) * 100.0_f32;
                    } else {
                        self.damage_percentage +=
                            (current_damage as f32 / self.total_hp_buildings as f32) * 100.0_f32;
                    }

                    buildings_damaged.push(BuildingResponse {
                        id: building.id,
                        position: building.tile,
                        hp: building.current_hp,
                        artifacts_if_damaged: artifacts_taken_by_destroying_building,
                    });
                }
            } else {
                continue;
            }
        }

        self.bombs.total_count -= 1;

        buildings_damaged
    }

    pub fn defender_ranged_attack(&mut self, frame_number: i32) -> DefenderReturnType {
        log::info!("Starting defender_ranged_attack for frame_number: {}", frame_number);
    
        if frame_number <= self.last_processed_frame {
            log::info!("Frame number {} is less than or equal to last processed frame {}", frame_number, self.last_processed_frame);
            return DefenderReturnType {
                attacker_health: self.attacker.as_ref().unwrap().attacker_health,
                defender_response: Vec::new(),
                bullet_hits: Vec::new(),
                state: self.clone(),
            };
        }
    
        let attacker = self.attacker.as_mut().unwrap();
        let mut defenders_damaged: Vec<DefenderResponse> = Vec::new();
        let mut bullet_hits: Vec<BulletHit> = Vec::new();
        // if attacker.attacker_health == 0 {
        //     log::info!("Attacker health is 0, returning early");
        //     return DefenderReturnType {
        //         attacker_health: attacker.attacker_health,
        //         defender_response: defenders_damaged,
        //         bullet_hits,
        //         state: self.clone(),
        //     };
        // }
    
        for defender in self.defenders.iter_mut() {
            if !defender.is_alive {
                log::info!("Defender {} is not alive, skipping", defender.id);
                continue;
            }
    
            let start = SystemTime::now();
            let now = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
            let time_interval = defender.frequency as u128;
    
            // Initialize defender.last_attack if it hasn't been set
            if defender.last_attack == 0 {
                defender.last_attack = now.as_millis() as i32;
                log::info!("Initializing defender {} last_attack to {}", defender.id, defender.last_attack);
            }
    
            let time_elapsed = now.as_millis() >= defender.last_attack as u128 + time_interval;
    
            if time_elapsed {
                log::info!("Time elapsed for defender {}. Updating last attack time.", defender.id);
                log::info!("last_attack: {}", defender.last_attack);
                defender.last_attack = now.as_millis() as i32;
                log::info!("last_attack update: {}", defender.last_attack);
                log::info!("nowmillis: {}", now.as_millis());
                log::info!("timeinterval: {}", time_interval);
    
                // Check if attacker is within defender's range
                let distance = (((defender.defender_pos.x - attacker.attacker_pos.x).pow(2)
                    + (defender.defender_pos.y - attacker.attacker_pos.y).pow(2))
                    as f32)
                    .sqrt();
                log::info!("Distance between defender {} and attacker: {}", defender.id, distance);
    
                if distance <= defender.range as f32 {
                    log::info!("Defender {} is within range of attacker", defender.id);
                    if defender.defender_pos.x == attacker.attacker_pos.x
                        || defender.defender_pos.y == attacker.attacker_pos.y
                    {
                        // Check if there are any buildings between the defender and the attacker
                        let mut blocked = false;
                        if defender.defender_pos.x == attacker.attacker_pos.x {
                            let min_y = defender.defender_pos.y.min(attacker.attacker_pos.y);
                            let max_y = defender.defender_pos.y.max(attacker.attacker_pos.y);
                            for building in &self.buildings {
                                if building.tile.x == defender.defender_pos.x
                                    && building.tile.y > min_y
                                    && building.tile.y < max_y
                                {
                                    blocked = true;
                                    log::info!("Defender {} attack is blocked by building at ({}, {})", defender.id, building.tile.x, building.tile.y);
                                    break;
                                }
                            }
                        } else if defender.defender_pos.y == attacker.attacker_pos.y {
                            let min_x = defender.defender_pos.x.min(attacker.attacker_pos.x);
                            let max_x = defender.defender_pos.x.max(attacker.attacker_pos.x);
                            for building in &self.buildings {
                                if building.tile.y == defender.defender_pos.y
                                    && building.tile.x > min_x
                                    && building.tile.x < max_x
                                {
                                    blocked = true;
                                    log::info!("Defender {} attack is blocked by building at ({}, {})", defender.id, building.tile.x, building.tile.y);
                                    break;
                                }
                            }
                        }
    
                        if !blocked {
                            log::info!("Defender {} successfully attacks attacker", defender.id);
                            attacker.attacker_health =
                                attacker.attacker_health.saturating_sub(defender.damage);
                            defenders_damaged.push(DefenderResponse {
                                id: defender.id,
                                position: defender.defender_pos,
                                damage: defender.damage,
                            });
                            bullet_hits.push(BulletHit {
                                defender_id: defender.id,
                                target_id: attacker.id,
                                damage: defender.damage,
                                position: defender.defender_pos,
                            });
                        } else {
                            log::info!("Defender {} attack is blocked", defender.id);
                        }
                    } else {
                        log::info!(
                            "Defender {} is not aligned with attacker. Defender position: ({}, {}), Attacker position: ({}, {})",
                            defender.id,
                            defender.defender_pos.x,
                            defender.defender_pos.y,
                            attacker.attacker_pos.x,
                            attacker.attacker_pos.y
                        );
                    }
                } else {
                    log::info!("Defender {} is out of range of attacker", defender.id);
                }
            } else {
                log::info!("Time not elapsed for defender {}. Skipping attack.", defender.id);
            }
        }
    
        log::info!("Defender ranged attack completed for frame_number: {}", frame_number);
    
        DefenderReturnType {
            attacker_health: attacker.attacker_health,
            defender_response: defenders_damaged,
            bullet_hits,
            state: self.clone(),
        }
    }
}
