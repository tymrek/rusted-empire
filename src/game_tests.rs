#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::models::{City, Loc, MoveResult, Owner, Piece, PieceType, UnitActivity};

    #[test]
    fn test_map_generation_dimensions() {
        let game = Game::new(120, 240, 50);
        assert_eq!(game.map.len(), 120);
        assert_eq!(game.map[0].len(), 240);
    }

    #[test]
    fn test_dynamic_city_scaling() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();
        
        let expected_target = (120 * 240) / 85;
        assert!(game.cities.len() <= expected_target);
        assert!(game.cities.len() > expected_target - 40, "City count was lower than expected target constraints");
    }

    #[test]
    fn test_city_distance_enforcement() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();

        for (i, city_a) in game.cities.iter().enumerate() {
            for (j, city_b) in game.cities.iter().enumerate() {
                if i != j {
                    let dr = (city_a.loc.r as isize - city_b.loc.r as isize).abs();
                    let dc = (city_a.loc.c as isize - city_b.loc.c as isize).abs();
                    let dist = dr.max(dc);
                    assert!(dist >= 6, "Cities spawned too close: {:?} and {:?}", city_a.loc, city_b.loc);
                }
            }
        }
    }

    #[test]
    fn test_fighter_fuel_depletion_and_crash() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; 
        
        let fighter = Piece {
            id: 1,
            ptype: PieceType::Fighter,
            owner: Owner::Player,
            loc: Loc { r: 5, c: 5 },
            hits: 1,
            moves_left: 8,
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: 1, 
        };
        game.pieces.push(fighter);

        let target_loc = Loc { r: 5, c: 6 };
        let (result, _) = game.attempt_move(0, target_loc);
        
        assert_eq!(result, MoveResult::CombatLost, "Fighter should crash when fuel reaches 0 without a base");
        assert_eq!(game.pieces[0].hits, 0, "Fighter hits should be 0 after crashing");
    }

    #[test]
    fn test_fighter_refuel_at_city() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; 
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 },
            owner: Owner::Player,
            prod: PieceType::Army,
            work: 0,
            default_orders: Vec::new(),
        });

        let fighter = Piece {
            id: 1,
            ptype: PieceType::Fighter,
            owner: Owner::Player,
            loc: Loc { r: 5, c: 5 },
            hits: 1,
            moves_left: 8,
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: 5, 
        };
        game.pieces.push(fighter);

        let target_loc = Loc { r: 5, c: 6 };
        let (result, _) = game.attempt_move(0, target_loc);
        
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[0].fuel, 32, "Fighter fuel should instantly reset to 32 upon entering friendly city");
    }

    #[test]
    fn test_cargo_boarding_limits() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; 

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        for i in 0..7 {
            game.pieces.push(Piece {
                id: i + 2, ptype: PieceType::Army, owner: Owner::Player,
                loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
                cargo: Vec::new(), fuel: 0,
            });
        }

        let mut army_success = 0;
        let mut army_fail = 0;
        for i in 1..=7 {
            let (res, _) = game.attempt_move(i, Loc { r: 5, c: 5 });
            if res == MoveResult::Moved { army_success += 1; }
            else if res == MoveResult::InvalidMove { army_fail += 1; }
        }

        assert_eq!(army_success, 6, "Transport should accept exactly 6 armies");
        assert_eq!(army_fail, 1, "Transport should reject the 7th army");
        assert_eq!(game.pieces[0].cargo.len(), 6, "Transport cargo array should hold 6 items");

        let carrier_idx = game.pieces.len();
        game.pieces.push(Piece {
            id: 10, ptype: PieceType::Carrier, owner: Owner::Player,
            loc: Loc { r: 8, c: 8 }, hits: 2, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        for i in 0..3 {
            game.pieces.push(Piece {
                id: i + 11, ptype: PieceType::Fighter, owner: Owner::Player,
                loc: Loc { r: 8, c: 7 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle,
                cargo: Vec::new(), fuel: 32,
            });
        }

        let mut f_boarded = 0;
        let mut f_hovering = 0;
        for i in 0..3 {
            let fighter_idx = carrier_idx + 1 + i;
            let (res, _) = game.attempt_move(fighter_idx, Loc { r: 8, c: 8 });
            if res == MoveResult::Moved { 
                if game.pieces[fighter_idx].hits == 0 {
                    f_boarded += 1;
                } else {
                    f_hovering += 1;
                }
            }
        }

        assert_eq!(f_boarded, 2, "Damaged Carrier should accept exactly 2 fighters");
        assert_eq!(f_hovering, 1, "The 3rd fighter should perform a flyover on the full carrier");
        assert_eq!(game.pieces[carrier_idx].cargo.len(), 2, "Carrier cargo array should hold exactly 2 items");
    }

    #[test]
    fn test_logistics_wake_cargo() {
        let mut game = Game::new(10, 10, 50);
        let mut transport = Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        };
        
        let army1 = Piece { id: 2, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 };
        let army2 = Piece { id: 3, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 };
        
        transport.cargo.push(army1);
        transport.cargo.push(army2);
        game.pieces.push(transport);

        game.wake_cargo(0);

        assert_eq!(game.pieces[0].cargo.len(), 0, "Transport cargo should be emptied onto the deck");
        assert_eq!(game.pieces.len(), 3, "Awakened armies should be appended to the global pieces array");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 5 }, "Army 1 should spawn on the transport's exact tile");
        assert_eq!(game.pieces[1].activity, UnitActivity::Idle, "Army 1 should be awake (Idle)");
        assert_eq!(game.pieces[1].moves_left, 1, "Army 1 should have its movement points restored to walk ashore");
    }

    #[test]
    fn test_logistics_process_fill_mode() {
        let mut game = Game::new(10, 10, 50);
        let mut transport = Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Fill,
            cargo: Vec::new(), fuel: 0,
        };
        
        for i in 0..6 {
            transport.cargo.push(Piece { id: i+2, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 });
        }
        game.pieces.push(transport);

        game.process_fill_mode(0);

        assert_eq!(game.pieces[0].activity, UnitActivity::Idle, "Full transport should wake up to Idle");
        assert_eq!(game.pieces[0].moves_left, 2, "Full transport should regain its movement points");

        game.pieces[0].cargo.pop(); 
        game.pieces[0].activity = UnitActivity::Fill;
        game.process_fill_mode(0);

        assert_eq!(game.pieces[0].activity, UnitActivity::Fill, "Partially filled transport should remain asleep in Fill mode");
        assert_eq!(game.pieces[0].moves_left, 0, "Partially filled transport should have its moves drained to wait for more cargo");
    }

    #[test]
    fn test_combat_city_assault_consumption() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; 
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 }, owner: Owner::Neutral, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 5 });
        
        assert!(matches!(result, MoveResult::CityCaptured(_) | MoveResult::CityAssaultFailed), "Result should be capture or fail");
        assert_eq!(game.pieces[0].hits, 0, "Attacking army must be consumed (hits = 0) regardless of RNG success or failure");
        
        if let MoveResult::CityCaptured(idx) = result {
            assert_eq!(game.cities[idx].owner, Owner::Player, "City owner should update to player on successful assault");
        } else {
            assert_eq!(game.cities[0].owner, Owner::Neutral, "City owner should remain neutral on failed assault");
        }
    }

    #[test]
    fn test_turn_production_cycle() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 },
            owner: Owner::Player,
            prod: PieceType::Army, 
            work: 4,               
            default_orders: Vec::new(),
        });
        
        let initial_pieces = game.pieces.len();
        game.process_turn();
        
        assert_eq!(game.pieces.len(), initial_pieces + 1, "New piece should spawn after work threshold is met");
        assert_eq!(game.cities[0].work, 0, "City work should reset to 0 after production");
        
        let new_piece = game.pieces.last().unwrap();
        assert_eq!(new_piece.ptype, PieceType::Army, "Produced unit should match city production target");
        assert_eq!(new_piece.loc, Loc { r: 5, c: 5 }, "Unit should spawn on the city tile");
    }

    #[test]
    fn test_turn_port_healing() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(City { 
            loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() 
        });
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Battleship, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 5, moves_left: 0, activity: UnitActivity::Sentry,
            cargo: Vec::new(), fuel: 0,
        });

        game.process_turn();
        assert_eq!(game.pieces[0].hits, 6, "Battleship should heal 1 hit point while on Sentry in a friendly city");
        
        game.pieces[0].hits = 10;
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 10, "Battleship should not heal past its maximum hits threshold");
    }

    #[test]
    fn test_turn_satellite_decay() {
        let mut game = Game::new(10, 10, 50);
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Satellite, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 0, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 2, 
        });

        game.process_turn();
        assert_eq!(game.pieces.len(), 1, "Satellite should remain in orbit");
        assert_eq!(game.pieces[0].fuel, 1, "Satellite fuel should decrease by 1 per turn");

        game.process_turn();
        assert_eq!(game.pieces.len(), 0, "Satellite should be destroyed and removed from pieces array when fuel reaches 0");
    }

    #[test]
    fn test_ai_sentry_wake_logic() {
        let mut game = Game::new(10, 10, 50);
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry,
            cargo: Vec::new(), fuel: 0,
        });

        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Comp,
            loc: Loc { r: 5, c: 6 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        game.player_map[5][6] = 1;

        game.process_turn();

        assert_eq!(game.pieces[0].activity, UnitActivity::Idle, "Player Army should wake up from Sentry to Idle when an enemy is spotted");
    }

    #[test]
    fn test_ai_explore_bfs_obstacle_avoidance() {
        let mut game = Game::new(5, 5, 50);
        
        for r in 0..5 {
            for c in 0..5 {
                game.map[r][c] = b'#';
                game.player_map[r][c] = 1; 
            }
        }
        
        game.map[1][2] = b'.';
        game.map[2][2] = b'.';
        game.map[3][2] = b'.';

        game.player_map[2][4] = 0;

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 2, c: 1 }, hits: 1, moves_left: 1, activity: UnitActivity::Explore,
            cargo: Vec::new(), fuel: 0,
        });

        game.explore_piece(0);

        let new_loc = game.pieces[0].loc;
        assert_ne!(new_loc, Loc { r: 2, c: 2 }, "Army must not step into the water wall");
        assert_eq!(game.map[new_loc.r][new_loc.c], b'#', "Army must only step on land");
        assert!(new_loc.c >= 1, "Army should not move backward away from the fog");
    }

    #[test]
    fn test_combat_unit_engagement() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; 
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Comp,
            loc: Loc { r: 5, c: 6 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        
        assert!(matches!(result, MoveResult::CombatWon | MoveResult::CombatLost), "Result must be CombatWon or CombatLost");
        
        if result == MoveResult::CombatWon {
            assert_eq!(game.pieces[1].hits, 0, "Enemy should be destroyed");
            assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 }, "Victor should occupy the tile");
        } else {
            assert_eq!(game.pieces[0].hits, 0, "Attacker should be destroyed");
        }
    }

    #[test]
    fn test_combat_naval_bombardment() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10];
        game.map[5][6] = b'#'; 

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Battleship, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 10, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Comp, prod: PieceType::Army, work: 5, default_orders: Vec::new(),
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        
        assert_eq!(result, MoveResult::Moved, "Bombardment evaluates as Moved for the shelling action");
        assert_eq!(game.cities[0].work, 0, "Bombardment should reset city production work to 0");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 }, "Battleship must not enter the land tile");
    }

    #[test]
    fn test_combat_submarine_stealth() {
        let mut game = Game::new(10, 15, 50);
        game.map = vec![vec![b'.'; 15]; 10];

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Submarine, owner: Owner::Comp,
            loc: Loc { r: 5, c: 5 }, hits: 2, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 7 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        assert!(!game.is_visible_to(0, Owner::Player), "Submarine should be hidden from Army at distance 2");

        game.pieces[1].loc = Loc { r: 5, c: 6 };
        assert!(game.is_visible_to(0, Owner::Player), "Submarine should be visible to adjacent Army");

        game.pieces[1].hits = 0;
        game.pieces.push(Piece {
            id: 3, ptype: PieceType::Destroyer, owner: Owner::Player,
            loc: Loc { r: 5, c: 9 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        assert!(game.is_visible_to(0, Owner::Player), "Submarine should be visible to Destroyer at distance 4");

        game.pieces[2].loc = Loc { r: 5, c: 10 };
        assert!(!game.is_visible_to(0, Owner::Player), "Submarine should be hidden from Destroyer at distance 5");
    }

    #[test]
    fn test_city_stacking_and_fighter_flyover() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10];
        
        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() });

        for i in 0..6 {
            game.pieces.push(Piece {
                id: i+1, ptype: PieceType::Destroyer, owner: Owner::Player,
                loc: Loc { r: 5, c: 5 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
            });
        }
        
        game.pieces.push(Piece {
            id: 7, ptype: PieceType::Destroyer, owner: Owner::Player,
            loc: Loc { r: 5, c: 6 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
        });
        
        let (res_city, _) = game.attempt_move(6, Loc { r: 5, c: 5 });
        assert_eq!(res_city, MoveResult::Moved, "Friendly units should stack infinitely inside friendly cities");

        game.pieces.push(Piece {
            id: 8, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 8, c: 8 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
        });

        game.pieces.push(Piece {
            id: 9, ptype: PieceType::Fighter, owner: Owner::Player,
            loc: Loc { r: 8, c: 9 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 32,
        });

        let (res_flyover, _) = game.attempt_move(8, Loc { r: 8, c: 8 });
        assert_eq!(res_flyover, MoveResult::Moved, "Fighter should be able to fly over a friendly Transport");
        
        game.pieces.push(Piece {
            id: 10, ptype: PieceType::Fighter, owner: Owner::Player,
            loc: Loc { r: 8, c: 7 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 32,
        });
        let (res_fighter_crash, _) = game.attempt_move(9, Loc { r: 8, c: 8 });
        assert_eq!(res_fighter_crash, MoveResult::InvalidMove, "Fighters cannot stack with other Fighters outside of cities/carriers");
    }

    #[test]
    fn test_logistics_wake_fighter_from_carrier() {
        let mut game = Game::new(10, 10, 50);
        let mut carrier = Piece {
            id: 1, ptype: PieceType::Carrier, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 8, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        };
        
        let fighter1 = Piece { 
            id: 2, ptype: PieceType::Fighter, owner: Owner::Player, 
            loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, 
            cargo: Vec::new(), fuel: 10 
        };
        
        carrier.cargo.push(fighter1);
        game.pieces.push(carrier);

        game.wake_cargo(0);

        assert_eq!(game.pieces[0].cargo.len(), 0, "Carrier cargo should be emptied onto the deck");
        assert_eq!(game.pieces.len(), 2, "Awakened fighter should be appended to the global pieces array");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 5 }, "Fighter 1 should spawn on the carrier's exact tile");
        assert_eq!(game.pieces[1].activity, UnitActivity::Idle, "Fighter 1 should be awake (Idle)");
        assert_eq!(game.pieces[1].moves_left, 8, "Fighter 1 should have its movement points restored to launch");
    }

    #[test]
    fn test_logistics_auto_unload_on_coast() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; 
        game.map[5][6] = b'#'; 

        let transport = Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Comp,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: vec![Piece { 
                id: 2, ptype: PieceType::Army, owner: Owner::Comp, 
                loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, 
                cargo: Vec::new(), fuel: 0 
            }],
            fuel: 0,
        };
        game.pieces.push(transport);

        game.check_adjacent_unloads();

        assert_eq!(game.pieces[0].cargo.len(), 0, "Transport should automatically empty its cargo when adjacent to land");
        assert_eq!(game.pieces.len(), 2, "Cargo should be moved to the active pieces array");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 5 }, "Unloaded army should spawn on the transport's tile");
        assert_eq!(game.pieces[1].activity, UnitActivity::Idle, "Unloaded army should be woken up");
    }

    #[test]
    fn test_order_inheritance_global_vs_local() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10];
        
        game.cities.push(City { 
            loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, 
            default_orders: vec![(PieceType::Army, UnitActivity::Sentry)] 
        });
        
        game.default_orders = vec![(PieceType::Army, UnitActivity::Explore)];

        game.pieces.push(Piece { 
            id: 1, ptype: PieceType::Army, owner: Owner::Player, 
            loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle, 
            cargo: Vec::new(), fuel: 0 
        });

        let (res, _) = game.attempt_move(0, Loc { r: 5, c: 5 });
        
        assert_eq!(res, MoveResult::Moved);
        assert_eq!(game.pieces[0].activity, UnitActivity::Sentry, "Local city order should take precedence over the global fallback order");
    }

    #[test]
    fn test_carrier_fighter_landing_same_tile() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10];

        game.pieces.push(Piece { 
            id: 1, ptype: PieceType::Carrier, owner: Owner::Player, 
            loc: Loc { r: 5, c: 5 }, hits: 8, moves_left: 2, activity: UnitActivity::Idle, 
            cargo: Vec::new(), fuel: 0 
        });
        
        game.pieces.push(Piece { 
            id: 2, ptype: PieceType::Fighter, owner: Owner::Player, 
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle, 
            cargo: Vec::new(), fuel: 10 
        });

        game.process_land_mode(1);

        assert_eq!(game.pieces[0].cargo.len(), 1, "Fighter should instantly board the carrier directly beneath it");
        assert_eq!(game.pieces[0].cargo[0].fuel, 32, "Fighter should refuel upon landing in the carrier");
        assert_eq!(game.pieces[1].hits, 0, "Original hovering fighter entity should be marked for deletion");
    }

    #[test]
    fn test_mobile_sector_load_move_unload_logistics() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; 

        // 0: Transport at (5,5)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // 1: Army (Awake on Deck) at (5,5)
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // 2: Army (On adjacent tile) at (5,4)
        game.pieces.push(Piece {
            id: 3, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // 1. Mobile Sector Drag
        // Move Transport from (5,5) to (5,6)
        let (res, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(res, MoveResult::Moved, "Transport should move successfully");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 }, "Transport loc updated");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 6 }, "Awake Army on deck should be dragged to the new tile");

        // 2. Load (Boarding into Cargo)
        // Move adjacent Army into the Transport
        let (res_board, _) = game.attempt_move(2, Loc { r: 5, c: 6 });
        assert_eq!(res_board, MoveResult::Moved, "Army should successfully board the Transport");
        assert_eq!(game.pieces[2].hits, 0, "Original Army entity should be marked for deletion");
        assert_eq!(game.pieces[0].cargo.len(), 1, "Transport should securely hold 1 Army in cargo");

        // 3. Move again with Cargo + Deck units
        let (res2, _) = game.attempt_move(0, Loc { r: 6, c: 6 });
        assert_eq!(res2, MoveResult::Moved);
        assert_eq!(game.pieces[0].loc, Loc { r: 6, c: 6 }, "Transport moved south");
        assert_eq!(game.pieces[1].loc, Loc { r: 6, c: 6 }, "Awake Army on deck dragged south again");

        // 4. Unload (Wake)
        game.wake_cargo(0);
        assert_eq!(game.pieces[0].cargo.len(), 0, "Cargo should be empty after waking");
        let awakened_army = game.pieces.last().unwrap();
        assert_eq!(awakened_army.ptype, PieceType::Army, "Awakened unit is an Army");
        assert_eq!(awakened_army.loc, Loc { r: 6, c: 6 }, "Awakened Army should spawn on the Transport's current tile");
        assert_eq!(awakened_army.activity, UnitActivity::Idle, "Awakened Army should be ready for orders");
    }
}
