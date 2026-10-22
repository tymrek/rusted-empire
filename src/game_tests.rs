#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::models::{Terrain, City, Loc, MoveResult, Owner, Piece, PieceType, UnitActivity};

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
        game.map = vec![vec![Terrain::Sea; 10]; 10]; 
        
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
        game.map = vec![vec![Terrain::Land; 10]; 10]; 
        
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
        game.map = vec![vec![Terrain::Sea; 10]; 10]; 

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
            // As in the original game, an army that insists on a full transport drowns.
            else if res == MoveResult::CombatLost { army_fail += 1; }
        }

        assert_eq!(army_success, 6, "Transport should accept exactly 6 armies");
        assert_eq!(army_fail, 1, "The 7th army should drown (no room on the transport)");
        assert!(game.pieces[7].hits <= 0, "the 7th army is lost");
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
        game.map = vec![vec![Terrain::Land; 10]; 10]; 
        
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
                game.map[r][c] = Terrain::Land;
                game.player_map[r][c] = 1; 
            }
        }
        
        game.map[1][2] = Terrain::Sea;
        game.map[2][2] = Terrain::Sea;
        game.map[3][2] = Terrain::Sea;

        game.player_map[2][4] = 0;

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 2, c: 1 }, hits: 1, moves_left: 1, activity: UnitActivity::Explore,
            cargo: Vec::new(), fuel: 0,
        });

        game.explore_piece(0);

        let new_loc = game.pieces[0].loc;
        assert_ne!(new_loc, Loc { r: 2, c: 2 }, "Army must not step into the water wall");
        assert_eq!(game.map[new_loc.r][new_loc.c], Terrain::Land, "Army must only step on land");
        assert!(new_loc.c >= 1, "Army should not move backward away from the fog");
    }

    #[test]
    fn test_combat_unit_engagement() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10]; 
        
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
    fn test_ship_entering_enemy_city_breaks_up_on_shore() {
        // Original VMS-Empire: ships can't take cities; trying destroys the ship.
        for owner in [Owner::Comp, Owner::Neutral] {
            for ptype in [PieceType::PatrolBoat, PieceType::Destroyer, PieceType::Submarine, PieceType::Carrier, PieceType::Battleship, PieceType::Transport] {
                let mut game = Game::new(10, 10, 50);
                game.map = vec![vec![Terrain::Sea; 10]; 10];
                game.map[5][6] = Terrain::Land;
                game.pieces.push(Piece {
                    id: 1, ptype, owner: Owner::Player,
                    loc: Loc { r: 5, c: 5 }, hits: ptype.max_hits(), moves_left: 2, activity: UnitActivity::Idle,
                    cargo: Vec::new(), fuel: 0,
                });
                game.cities.push(City {
                    loc: Loc { r: 5, c: 6 }, owner, prod: PieceType::Army, work: 5, default_orders: Vec::new(),
                });

                let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });

                assert_eq!(result, MoveResult::CombatLost, "{:?} should be destroyed", ptype);
                assert!(game.pieces[0].hits <= 0);
                assert_eq!(game.cities[0].work, 5, "no bombardment: production is untouched");
                assert_eq!(game.cities[0].owner, owner);
            }
        }
    }

    #[test]
    fn test_ship_may_still_enter_its_own_city() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][6] = Terrain::Land;
        game.pieces.push(test_piece(1, PieceType::Destroyer, Owner::Player, 5, 5));
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::Moved);
        assert!(game.pieces[0].hits > 0);
    }

    #[test]
    fn test_fighter_entering_enemy_city_is_shot_down() {
        for owner in [Owner::Comp, Owner::Neutral] {
            let mut game = Game::new(10, 10, 50);
            game.map = vec![vec![Terrain::Sea; 10]; 10];
            game.map[5][6] = Terrain::Land;
            game.pieces.push(test_piece(1, PieceType::Fighter, Owner::Player, 5, 5));
            game.cities.push(City {
                loc: Loc { r: 5, c: 6 }, owner, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
            });
            let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
            assert_eq!(result, MoveResult::CombatLost);
            assert!(game.pieces[0].hits <= 0);
        }
    }

    #[test]
    fn test_fighter_may_land_in_its_own_city() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][6] = Terrain::Land;
        game.pieces.push(test_piece(1, PieceType::Fighter, Owner::Player, 5, 5));
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::Moved);
        assert!(game.pieces[0].hits > 0);
    }

    #[test]
    fn test_automated_movers_never_fly_into_enemy_cities() {
        // Standing orders must not throw a fighter away: marching in a direction stops short.
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][6] = Terrain::Land;
        let mut f = test_piece(1, PieceType::Fighter, Owner::Player, 5, 5);
        f.activity = UnitActivity::Dir(0, 1);
        game.pieces.push(f);
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Comp, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });
        game.dir_piece(0);
        assert!(game.pieces[0].hits > 0, "fighter must survive");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 });
    }

    #[test]
    fn test_computer_units_never_suicide_into_player_cities() {
        // A computer ship and fighter heading for a player city must hold short of it.
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][6] = Terrain::Land;
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });
        game.pieces.push(test_piece(1, PieceType::Destroyer, Owner::Comp, 5, 5));
        game.pieces.push(test_piece(2, PieceType::Fighter, Owner::Comp, 5, 4));
        for _ in 0..20 {
            for p in game.pieces.iter_mut() { p.moves_left = p.ptype.max_moves(); p.fuel = 32; }
            game.process_comp_ai();
        }
        assert!(game.pieces.iter().all(|p| p.hits > 0), "computer units were thrown away on the city");
    }

    #[test]
    fn test_combat_submarine_stealth() {
        let mut game = Game::new(10, 15, 50);
        game.map = vec![vec![Terrain::Sea; 15]; 10];

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
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        
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
        game.map = vec![vec![Terrain::Sea; 10]; 10]; 
        game.map[5][6] = Terrain::Land; 

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
        game.map = vec![vec![Terrain::Land; 10]; 10];
        
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
        game.map = vec![vec![Terrain::Sea; 10]; 10];

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
        game.map = vec![vec![Terrain::Sea; 10]; 10]; 

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

    #[test]
    fn test_mobile_sector_drag_exploit_limit() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][5] = Terrain::Land;

        // Transport at (5,5)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // 10 Armies stacked at (5,5)
        for i in 0..10 {
            game.pieces.push(Piece {
                id: i + 2, ptype: PieceType::Army, owner: Owner::Player,
                loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
                cargo: Vec::new(), fuel: 0,
            });
        }

        // Move transport out to sea
        let (res, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(res, MoveResult::Moved);

        let armies_at_sea = game.pieces.iter().filter(|p| p.ptype == PieceType::Army && p.loc == Loc { r: 5, c: 6 }).count();
        let armies_on_land = game.pieces.iter().filter(|p| p.ptype == PieceType::Army && p.loc == Loc { r: 5, c: 5 }).count();

        assert_eq!(armies_at_sea, 6, "Transport should drag exactly 6 armies out to sea");
        assert_eq!(armies_on_land, 4, "4 armies should be left behind on the original land tile");
    }

    #[test]
    fn test_logistics_transport_wait_boarding() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];

        // Player Transport at (5,5)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // Player Army at (6,6) waiting for transport
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 6, c: 6 }, hits: 1, moves_left: 0, activity: UnitActivity::TransportWait,
            cargo: Vec::new(), fuel: 0,
        });

        let did_board = game.process_transport_wait(1);
        
        assert!(did_board, "Army should successfully find and board the adjacent transport");
        assert_eq!(game.pieces[0].cargo.len(), 1, "Transport should have 1 unit in cargo");
        assert_eq!(game.pieces[1].hits, 0, "Original Army entity should be marked for cleanup");
    }

    #[test]
    fn test_ai_flow_field_pathfinding() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10]; // All land

        // Player City at (1,1)
        game.cities.push(City {
            loc: Loc { r: 1, c: 1 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });

        // Comp Army at (8,8)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Comp,
            loc: Loc { r: 8, c: 8 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        game.process_comp_ai();

        let new_loc = game.pieces[0].loc;
        assert_ne!(new_loc, Loc { r: 8, c: 8 }, "Comp Army should have moved");
        
        let old_dist = (8isize - 1).pow(2) + (8isize - 1).pow(2);
        let new_dist = (new_loc.r as isize - 1).pow(2) + (new_loc.c as isize - 1).pow(2);
        
        assert!(new_dist < old_dist, "Comp Army should move strictly closer to the Player target via gradient descent");
    }

    #[test]
    fn test_game_over_conditions() {
        let mut game = Game::new(10, 10, 50);

        game.cities.push(City { loc: Loc { r: 0, c: 0 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() });
        game.cities.push(City { loc: Loc { r: 0, c: 1 }, owner: Owner::Comp, prod: PieceType::Army, work: 0, default_orders: Vec::new() });

        game.pieces.push(Piece { id: 1, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 1, c: 0 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0 });
        game.pieces.push(Piece { id: 2, ptype: PieceType::Army, owner: Owner::Comp, loc: Loc { r: 1, c: 1 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0 });

        assert!(game.check_game_over().is_none(), "Game should continue with forces on both sides");

        game.cities[0].owner = Owner::Comp;
        game.pieces[0].hits = 0;
        
        let defeat = game.check_game_over();
        assert!(defeat.is_some() && defeat.unwrap().contains("DEFEAT"), "Player defeat should trigger when losing all cities and units");

        game.cities[0].owner = Owner::Player;
        game.cities[1].owner = Owner::Player;
        game.pieces[0].hits = 1;
        game.pieces[1].hits = 1; // Leave an enemy unit alive to bypass the 'eradicated' check

        let victory = game.check_game_over();
        assert!(victory.is_some() && victory.unwrap().contains("TOTAL DOMINATION"), "Total Domination should trigger when holding all cities");
    }

    #[test]
    fn test_satellite_orbital_immunity() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10]; 

        // Enemy Satellite
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Satellite, owner: Owner::Comp,
            loc: Loc { r: 5, c: 6 }, hits: 1, moves_left: 10, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 20,
        });

        // Player Army
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        let (result, _) = game.attempt_move(1, Loc { r: 5, c: 6 });

        assert_eq!(result, MoveResult::Moved, "Army should simply move under the satellite");
        assert_eq!(game.pieces[0].hits, 1, "Satellite should not be damaged or engaged in combat");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 6 }, "Army should share the tile with the enemy satellite");
    }

    #[test]
    fn test_fighter_grope_bingo_fuel_return() {
        let mut game = Game::new(10, 20, 50);
        game.map = vec![vec![Terrain::Sea; 20]; 10];
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 },
            owner: Owner::Player,
            prod: PieceType::Army,
            work: 0,
            default_orders: Vec::new(),
        });
        
        // Fighter exploring far away. Distance to base = 6.
        // Bingo fuel threshold: dist (6) + 2 = 8
        let fighter = Piece {
            id: 1,
            ptype: PieceType::Fighter,
            owner: Owner::Player,
            loc: Loc { r: 5, c: 11 },
            hits: 1,
            moves_left: 8,
            activity: UnitActivity::Explore,
            cargo: Vec::new(),
            fuel: 8, 
        };
        game.pieces.push(fighter);
        
        game.explore_piece(0);
        
        assert_eq!(game.pieces[0].activity, UnitActivity::Land, "Fighter should abort grope and switch to Land when hitting bingo fuel");
        assert_eq!(game.pieces[0].moves_left, 0, "Fighter should halt its current movement to process the return order");
        
        game.process_land_mode(0);
        
        if let UnitActivity::Dest(dest_loc) = game.pieces[0].activity {
            assert_eq!(dest_loc, Loc { r: 5, c: 5 }, "Fighter should target the nearest base to land");
        } else {
            panic!("Fighter activity should be Dest(base)");
        }
    }

    fn test_piece(id: usize, ptype: PieceType, owner: Owner, r: usize, c: usize) -> Piece {
        Piece {
            id,
            ptype,
            owner,
            loc: Loc { r, c },
            hits: ptype.max_hits(),
            moves_left: ptype.max_moves(),
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: 32,
        }
    }

    #[test]
    fn test_turn_refreshes_full_moves() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        let mut fighter = test_piece(1, PieceType::Fighter, Owner::Player, 5, 5);
        fighter.moves_left = 5; // spent 3 of 8 last turn
        let mut idle_army = test_piece(2, PieceType::Army, Owner::Player, 1, 1);
        idle_army.moves_left = 0;
        game.pieces.push(fighter);
        game.pieces.push(idle_army);

        game.process_turn();

        assert_eq!(game.pieces[0].moves_left, PieceType::Fighter.max_moves());
        assert_eq!(game.pieces[1].moves_left, PieceType::Army.max_moves());
    }

    #[test]
    fn test_new_piece_ids_skip_cargo_ids() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[2][2] = Terrain::Land;
        let mut transport = test_piece(1, PieceType::Transport, Owner::Player, 5, 5);
        transport.cargo.push(test_piece(2, PieceType::Army, Owner::Player, 5, 5));
        game.pieces.push(transport);
        game.cities.push(City {
            loc: Loc { r: 2, c: 2 },
            owner: Owner::Player,
            prod: PieceType::Army,
            work: PieceType::Army.build_cost() - 1,
            default_orders: Vec::new(),
        });

        game.process_turn(); // city completes an Army
        game.wake_cargo(0); // cargo army comes back onto the deck

        let mut ids: Vec<usize> = game.pieces.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), game.pieces.len(), "duplicate piece ids: {:?}", game.pieces.iter().map(|p| p.id).collect::<Vec<_>>());
    }

    #[test]
    fn test_piece_ids_not_reused_after_death() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[2][2] = Terrain::Land;
        game.cities.push(City {
            loc: Loc { r: 2, c: 2 },
            owner: Owner::Player,
            prod: PieceType::Army,
            work: PieceType::Army.build_cost() - 1,
            default_orders: Vec::new(),
        });
        game.process_turn();
        let first_id = game.pieces.last().unwrap().id;
        game.pieces.retain(|p| p.id != first_id); // that piece dies
        game.cities[0].work = PieceType::Army.build_cost() - 1;
        game.process_turn();
        let second_id = game.pieces.last().unwrap().id;
        assert!(second_id > first_id, "id {} was reused", second_id);
    }

    #[test]
    fn test_init_message_reports_actual_city_count() {
        let mut game = Game::new(30, 30, 50);
        game.setup_initial_states();
        let expected = format!("{} cities scattered", game.cities.len());
        assert!(
            game.messages.iter().any(|m| m.contains(&expected)),
            "init log should mention the real city count ({}); got {:?}",
            game.cities.len(),
            game.messages
        );
    }

    // ---- combat model and stacked defenders ----

    /// 10x10 map: land on row 5 columns 4..=7, water elsewhere.
    fn combat_world() -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        for c in 4..=7 {
            game.map[5][c] = Terrain::Land;
        }
        game
    }

    #[test]
    fn test_combat_is_fifty_fifty_per_round_like_the_original() {
        // 1-hit army vs 1-hit army: exactly one blow decides it, so ~50% each way.
        // (The old "both sides roll each round" model gave the attacker about 2/3.)
        let trials = 20_000;
        let mut attacker_wins = 0;
        for _ in 0..trials {
            let mut game = combat_world();
            game.pieces.push(test_piece(1, PieceType::Army, Owner::Player, 5, 5));
            game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, 5, 6));
            if game.attempt_move(0, Loc { r: 5, c: 6 }).0 == MoveResult::CombatWon {
                attacker_wins += 1;
            }
        }
        let rate = attacker_wins as f64 / trials as f64;
        assert!((0.48..=0.52).contains(&rate), "attacker win rate {}", rate);
    }

    #[test]
    fn test_ground_attacker_fights_ground_unit_before_flyover_fighter() {
        let mut game = combat_world();
        let mut attacker = test_piece(1, PieceType::Army, Owner::Player, 5, 5);
        attacker.hits = 1000; // cannot realistically lose
        attacker.moves_left = 2;
        game.pieces.push(attacker);
        game.pieces.push(test_piece(2, PieceType::Fighter, Owner::Comp, 5, 6)); // listed first...
        game.pieces.push(test_piece(3, PieceType::Army, Owner::Comp, 5, 6)); // ...army must still defend

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatWon);
        assert!(game.pieces[2].hits <= 0, "enemy army should have defended and died");
        assert!(game.pieces[1].hits > 0, "enemy fighter should be untouched");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 }, "attacker must not occupy a sector that is still held");

        // Second attack takes on the fighter and now the attacker advances.
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatWon);
        assert!(game.pieces[1].hits <= 0);
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 });
    }

    #[test]
    fn test_fighter_attacker_fights_enemy_fighter_before_ground_unit() {
        let mut game = combat_world();
        let mut attacker = test_piece(1, PieceType::Fighter, Owner::Player, 5, 5);
        attacker.hits = 1000;
        attacker.moves_left = 2;
        game.pieces.push(attacker);
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, 5, 6)); // listed first...
        game.pieces.push(test_piece(3, PieceType::Fighter, Owner::Comp, 5, 6)); // ...fighter must defend

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatWon);
        assert!(game.pieces[2].hits <= 0, "enemy fighter should have defended and died");
        assert!(game.pieces[1].hits > 0, "enemy army should be untouched");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 });
    }

    #[test]
    fn test_same_plane_defender_prefers_highest_original_type() {
        // Two enemy surface units in one tile (as in a city): the destroyer outranks the patrol boat.
        let mut game = combat_world();
        game.map[5][6] = Terrain::Sea;
        let mut attacker = test_piece(1, PieceType::Battleship, Owner::Player, 5, 5);
        attacker.hits = 1000;
        game.map[5][5] = Terrain::Sea;
        game.pieces.push(attacker);
        game.pieces.push(test_piece(2, PieceType::Destroyer, Owner::Comp, 5, 6)); // listed first...
        game.pieces.push(test_piece(3, PieceType::PatrolBoat, Owner::Comp, 5, 6));
        game.attempt_move(0, Loc { r: 5, c: 6 });
        assert!(game.pieces[1].hits <= 0, "destroyer outranks patrol boat and should defend");
        assert!(game.pieces[2].hits > 0);
    }

    // ---- saving ----

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rs_empire_test_{}_{}", std::process::id(), name))
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let path = scratch_path("roundtrip.sav");
        let mut game = Game::new(10, 10, 50);
        game.turn = 42;
        game.pieces.push(test_piece(7, PieceType::Army, Owner::Player, 1, 1));
        game.save_game(path.to_str().unwrap()).expect("save should succeed");
        let loaded = Game::load_game(path.to_str().unwrap()).expect("load should succeed");
        assert_eq!(loaded.turn, 42);
        assert_eq!(loaded.pieces.len(), 1);
        assert!(!std::path::Path::new(&format!("{}.tmp", path.display())).exists(), "temp file must not linger");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_failure_is_reported() {
        let game = Game::new(10, 10, 50);
        let path = scratch_path("no_such_dir").join("empire.sav");
        assert!(game.save_game(path.to_str().unwrap()).is_err());
    }

    #[test]
    fn test_failed_save_leaves_existing_save_intact() {
        let path = scratch_path("keep.sav");
        std::fs::write(&path, "PREVIOUS GOOD SAVE").unwrap();
        // Make the temp file impossible to create by putting a directory in its place.
        let tmp = format!("{}.tmp", path.display());
        std::fs::create_dir_all(&tmp).unwrap();

        let game = Game::new(10, 10, 50);
        assert!(game.save_game(path.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "PREVIOUS GOOD SAVE");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&path);
    }

    // ---- city assault (original VMS-Empire rules) ----

    /// Player army at (5,4) next to a Comp city at (5,5) full of defenders.
    fn city_assault_world() -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 },
            owner: Owner::Comp,
            prod: PieceType::Destroyer,
            work: 7,
            default_orders: vec![(PieceType::Army, UnitActivity::Sentry)],
        });
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Player, 5, 4)); // attacker
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, 5, 5));
        game.pieces.push(test_piece(3, PieceType::Fighter, Owner::Comp, 5, 5));
        let mut transport = test_piece(4, PieceType::Transport, Owner::Comp, 5, 5);
        transport.cargo.push(test_piece(5, PieceType::Army, Owner::Comp, 5, 5));
        game.pieces.push(transport);
        game.pieces.push(test_piece(6, PieceType::Satellite, Owner::Comp, 5, 5));
        game
    }

    /// Re-runs the assault on a fresh world until it gets the wanted outcome.
    fn assault_until(want_capture: bool) -> Game {
        for _ in 0..500 {
            let mut game = city_assault_world();
            let (result, _) = game.attempt_move(0, Loc { r: 5, c: 5 });
            if matches!(result, MoveResult::CityCaptured(_)) == want_capture {
                return game;
            }
        }
        panic!("never saw the wanted outcome in 500 tries");
    }

    #[test]
    fn test_city_assault_is_a_coin_flip_regardless_of_defenders() {
        let trials = 20_000;
        let captured = (0..trials)
            .filter(|_| {
                let mut game = city_assault_world();
                matches!(game.attempt_move(0, Loc { r: 5, c: 5 }).0, MoveResult::CityCaptured(_))
            })
            .count();
        let rate = captured as f64 / trials as f64;
        assert!((0.48..=0.52).contains(&rate), "capture rate {}", rate);
    }

    #[test]
    fn test_failed_city_assault_leaves_city_and_defenders_alone() {
        let game = assault_until(false);
        assert_eq!(game.pieces[0].hits, 0, "attacking army is spent");
        assert_eq!(game.cities[0].owner, Owner::Comp);
        assert_eq!(game.cities[0].work, 7);
        for p in &game.pieces[1..] {
            assert!(p.hits > 0 && p.owner == Owner::Comp, "defender {:?} must be untouched", p.ptype);
        }
        assert_eq!(game.pieces[3].cargo.len(), 1);
    }

    #[test]
    fn test_captured_city_resets_and_converts_its_contents() {
        let game = assault_until(true);
        assert_eq!(game.pieces[0].hits, 0, "attacking army is dispersed");
        let city = &game.cities[0];
        assert_eq!(city.owner, Owner::Player);
        assert_eq!(city.work, 0, "production progress resets");
        assert!(city.default_orders.is_empty(), "standing orders reset");

        assert!(game.pieces[1].hits <= 0, "defending army dies when the city falls");
        assert_eq!(game.pieces[2].owner, Owner::Player, "fighter changes sides");
        assert_eq!(game.pieces[2].activity, UnitActivity::Idle);
        assert_eq!(game.pieces[3].owner, Owner::Player, "transport changes sides");
        assert!(game.pieces[3].cargo.is_empty(), "transport cargo is lost");
        assert_eq!(game.pieces[4].owner, Owner::Comp, "satellite is unaffected");
        assert!(game.pieces[4].hits > 0);
    }

    // ---- ships attacking ashore (original: the attacking ship always breaks up) ----

    fn shore_world(attacker: PieceType, attacker_hits: i32) -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][6] = Terrain::Land;
        let mut ship = test_piece(1, attacker, Owner::Player, 5, 5);
        ship.hits = attacker_hits;
        game.pieces.push(ship);
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, 5, 6));
        game
    }

    #[test]
    fn test_ship_that_wins_a_shore_assault_still_breaks_up() {
        let mut game = shore_world(PieceType::Battleship, 1000); // cannot realistically lose
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatWon);
        assert!(game.pieces[1].hits <= 0, "the army ashore is destroyed");
        assert!(game.pieces[0].hits <= 0, "the victorious ship breaks up on shore");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 });
    }

    #[test]
    fn test_ship_that_loses_a_shore_assault_is_destroyed() {
        let mut game = shore_world(PieceType::Destroyer, 1);
        game.pieces[1].hits = 1000;
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
        assert!(game.pieces[0].hits <= 0);
        assert!(game.pieces[1].hits > 0);
    }

    #[test]
    fn test_ship_defending_at_sea_is_not_broken_up() {
        // The break-up rule is for a ship that initiates the attack, not one that defends.
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        let mut fighter = test_piece(1, PieceType::Fighter, Owner::Comp, 5, 5);
        fighter.hits = 1;
        let mut ship = test_piece(2, PieceType::Battleship, Owner::Player, 5, 6);
        ship.hits = 1000;
        game.pieces.push(fighter);
        game.pieces.push(ship);
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
        assert!(game.pieces[1].hits > 0, "the defending ship survives");
    }

    #[test]
    fn test_automated_attack_never_launches_a_shore_assault() {
        let mut game = shore_world(PieceType::Battleship, 10);
        game.pieces[0].activity = UnitActivity::Attack;
        game.reveal_entire_map();
        game.attack_piece(0);
        assert!(game.pieces[0].hits > 0, "standing orders must not make a ship break itself up");
        assert!(game.pieces[1].hits > 0);
    }

    // ---- ships ashore and armies at sea: original terrain rules ----

    #[test]
    fn test_every_ship_type_may_attack_ashore_and_breaks_up() {
        for ptype in [PieceType::PatrolBoat, PieceType::Destroyer, PieceType::Submarine,
                      PieceType::Carrier, PieceType::Battleship, PieceType::Transport] {
            let mut game = shore_world(ptype, 1000);
            let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
            assert_eq!(result, MoveResult::CombatWon, "{:?} should fight the army ashore", ptype);
            assert!(game.pieces[1].hits <= 0, "{:?}: the army ashore dies", ptype);
            assert!(game.pieces[0].hits <= 0, "{:?} breaks up on shore after winning", ptype);
        }
    }

    #[test]
    fn test_ship_going_ashore_with_nothing_there_breaks_up() {
        let mut game = shore_world(PieceType::Destroyer, 3);
        game.pieces.pop(); // remove the army: just empty land
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
        assert!(game.pieces[0].hits <= 0);
    }

    #[test]
    fn test_ship_cannot_go_ashore_onto_a_friendly_unit() {
        let mut game = shore_world(PieceType::Destroyer, 3);
        game.pieces[1].owner = Owner::Player; // our own army is there
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::InvalidMove);
        assert!(game.pieces[0].hits > 0);
    }

    fn sea_world(army_hits: i32) -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][5] = Terrain::Land; // the army stands on land, the sea is to its east
        let mut army = test_piece(1, PieceType::Army, Owner::Player, 5, 5);
        army.hits = army_hits;
        game.pieces.push(army);
        game
    }

    #[test]
    fn test_army_walking_into_empty_sea_drowns() {
        let mut game = sea_world(1);
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
        assert!(game.pieces[0].hits <= 0);
    }

    #[test]
    fn test_army_that_wins_at_sea_still_drowns() {
        let mut game = sea_world(1000);
        game.pieces.push(test_piece(2, PieceType::PatrolBoat, Owner::Comp, 5, 6));
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatWon);
        assert!(game.pieces[1].hits <= 0, "the enemy ship is sunk");
        assert!(game.pieces[0].hits <= 0, "the army drowns after its assault");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 });
    }

    #[test]
    fn test_army_that_loses_at_sea_is_destroyed() {
        let mut game = sea_world(1);
        let mut ship = test_piece(2, PieceType::Battleship, Owner::Comp, 5, 6);
        ship.hits = 1000;
        game.pieces.push(ship);
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
        assert!(game.pieces[1].hits > 0, "the defending ship is unharmed in the sense that it survives");
    }

    #[test]
    fn test_army_boards_a_transport_with_room_without_drowning() {
        let mut game = sea_world(1);
        game.pieces.push(test_piece(2, PieceType::Transport, Owner::Player, 5, 6));
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[1].cargo.len(), 1);
    }

    #[test]
    fn test_army_stepping_off_a_transport_into_open_sea_drowns() {
        let mut game = sea_world(1);
        game.map[5][5] = Terrain::Sea; // the army is on a transport at sea
        game.pieces.push(test_piece(2, PieceType::Transport, Owner::Player, 5, 5));
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::CombatLost);
    }

    #[test]
    fn test_suicidal_moves_are_flagged_for_confirmation() {
        let mut game = sea_world(1);
        let east = Loc { r: 5, c: 6 };
        assert!(game.suicidal_move_prompt(PieceType::Army, Owner::Player, east).unwrap().contains("go to sea"));
        assert!(game.suicidal_move_prompt(PieceType::Army, Owner::Player, Loc { r: 5, c: 4 }).is_some()); // more sea
        game.map[5][4] = Terrain::Land;
        assert!(game.suicidal_move_prompt(PieceType::Army, Owner::Player, Loc { r: 5, c: 4 }).is_none(), "land is safe");
        assert!(game.suicidal_move_prompt(PieceType::Destroyer, Owner::Player, Loc { r: 5, c: 4 }).unwrap().contains("ashore"));
        assert!(game.suicidal_move_prompt(PieceType::Destroyer, Owner::Player, east).is_none(), "sea is safe for ships");
    }

    // ---- seeded RNG ----

    fn snapshot(game: &Game) -> (Vec<Vec<Terrain>>, Vec<(usize, usize, Owner)>) {
        (game.map.clone(), game.cities.iter().map(|c| (c.loc.r, c.loc.c, c.owner)).collect())
    }

    #[test]
    fn test_same_seed_generates_identical_world() {
        let mut a = Game::with_seed(60, 120, 50, 42);
        let mut b = Game::with_seed(60, 120, 50, 42);
        a.setup_initial_states();
        b.setup_initial_states();
        assert_eq!(snapshot(&a), snapshot(&b));
    }

    #[test]
    fn test_different_seeds_generate_different_worlds() {
        let mut a = Game::with_seed(60, 120, 50, 1);
        let mut b = Game::with_seed(60, 120, 50, 2);
        a.setup_initial_states();
        b.setup_initial_states();
        assert_ne!(snapshot(&a).0, snapshot(&b).0);
    }

    #[test]
    fn test_same_seed_gives_identical_combat() {
        let outcome = |seed: u64| {
            let mut game = Game::with_seed(10, 10, 50, seed);
            game.map = vec![vec![Terrain::Land; 10]; 10];
            game.pieces.push(test_piece(0, PieceType::Army, Owner::Player, 5, 5));
            game.pieces.push(test_piece(1, PieceType::Army, Owner::Comp, 5, 6));
            game.pieces[0].hits = 50;
            game.pieces[1].hits = 50;
            game.attempt_move(0, Loc { r: 5, c: 6 });
            (game.pieces[0].hits, game.pieces[1].hits)
        };
        assert_eq!(outcome(7), outcome(7));
        assert!((0..20).any(|s| outcome(s) != outcome(7)), "different seeds should vary");
    }

    // ---- Terrain / neighbors ----

    #[test]
    fn test_neighbors_clamps_to_the_map() {
        let game = Game::new(5, 6, 50);
        let mut corner: Vec<_> = game.neighbors(Loc { r: 0, c: 0 }).map(|l| (l.r, l.c)).collect();
        corner.sort();
        assert_eq!(corner, vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!(game.neighbors(Loc { r: 4, c: 5 }).count(), 3);
        assert_eq!(game.neighbors(Loc { r: 0, c: 3 }).count(), 5);
        let middle: Vec<_> = game.neighbors(Loc { r: 2, c: 2 }).collect();
        assert_eq!(middle.len(), 8);
        assert!(!middle.contains(&Loc { r: 2, c: 2 }), "a tile is not its own neighbor");
    }

    #[test]
    fn test_is_coastal_uses_neighbors() {
        let mut game = Game::new(7, 7, 50);
        game.map = vec![vec![Terrain::Land; 7]; 7];
        assert!(!game.is_coastal(Loc { r: 3, c: 3 }));
        game.map[2][2] = Terrain::Sea; // diagonal neighbor
        assert!(game.is_coastal(Loc { r: 3, c: 3 }));
        assert!(!game.is_coastal(Loc { r: 5, c: 5 }));
    }

    #[test]
    fn test_terrain_saves_as_the_original_bytes() {
        let json = serde_json::to_string(&vec![Terrain::Land, Terrain::Sea]).unwrap();
        assert_eq!(json, "[35,46]");
        let back: Vec<Terrain> = serde_json::from_str("[35,46]").unwrap();
        assert_eq!(back, vec![Terrain::Land, Terrain::Sea]);
        assert!(serde_json::from_str::<Vec<Terrain>>("[7]").is_err());
    }

    // ---- damaged ships: speed and repair, as in the original ----

    #[test]
    fn test_moves_for_hits_matches_original_formula() {
        // ceil(speed * hits / max_hits), minimum 1 while alive
        let bb = PieceType::Battleship; // speed 2, 10 hits
        assert_eq!((bb.moves_for_hits(10), bb.moves_for_hits(6), bb.moves_for_hits(5), bb.moves_for_hits(1)), (2, 2, 1, 1));
        let car = PieceType::Carrier; // speed 2, 8 hits
        assert_eq!((car.moves_for_hits(8), car.moves_for_hits(5), car.moves_for_hits(4), car.moves_for_hits(1)), (2, 2, 1, 1));
        let dd = PieceType::Destroyer; // speed 2, 3 hits
        assert_eq!((dd.moves_for_hits(3), dd.moves_for_hits(2), dd.moves_for_hits(1)), (2, 2, 1));
        let sub = PieceType::Submarine; // speed 2, 2 hits
        assert_eq!((sub.moves_for_hits(2), sub.moves_for_hits(1)), (2, 1));
        assert_eq!(PieceType::Fighter.moves_for_hits(1), 8);
        assert_eq!(PieceType::PatrolBoat.moves_for_hits(1), 4);
        assert_eq!(bb.moves_for_hits(0), 0);
    }

    fn port_world(ptype: PieceType, hits: i32, owner: Owner) -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner, prod: PieceType::Army, work: 0, default_orders: Vec::new() });
        let mut ship = test_piece(1, ptype, owner, 5, 5);
        ship.hits = hits;
        ship.activity = UnitActivity::Idle;
        game.pieces.push(ship);
        game
    }

    #[test]
    fn test_damaged_ship_gets_reduced_moves_each_turn() {
        for (ptype, hits, expected) in [
            (PieceType::Battleship, 10, 2), (PieceType::Battleship, 6, 2), (PieceType::Battleship, 5, 1),
            (PieceType::Carrier, 5, 2), (PieceType::Carrier, 4, 1),
            (PieceType::Destroyer, 1, 1), (PieceType::Submarine, 2, 2),
        ] {
            let mut game = port_world(ptype, hits, Owner::Player);
            game.cities[0].owner = Owner::Neutral; // no repair interferes
            game.process_turn();
            assert_eq!(game.pieces[0].moves_left, expected, "{:?} with {} hits", ptype, hits);
        }
    }

    #[test]
    fn test_ship_damaged_in_combat_loses_the_rest_of_its_moves() {
        let (mut saw_slowed, mut saw_hurt_but_fast) = (false, false);
        for seed in 0..400 {
            let mut game = Game::with_seed(10, 10, 50, seed);
            game.map = vec![vec![Terrain::Sea; 10]; 10];
            let mut bb = test_piece(1, PieceType::Battleship, Owner::Player, 5, 5);
            bb.moves_left = 2;
            game.pieces.push(bb);
            game.pieces.push(test_piece(2, PieceType::Battleship, Owner::Comp, 5, 6));
            let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
            let hits = game.pieces[0].hits;
            if result == MoveResult::CombatWon && hits < 10 {
                // allowance is recomputed from current hits; one move was spent attacking
                let expected = (PieceType::Battleship.moves_for_hits(hits) - 1).max(0);
                assert_eq!(game.pieces[0].moves_left, expected, "hits {}", hits);
                if hits <= 5 { saw_slowed = true; assert_eq!(expected, 0); } else { saw_hurt_but_fast = true; }
            }
        }
        assert!(saw_slowed, "needed a seed where the attacker won but dropped to 5 hits or fewer");
        assert!(saw_hurt_but_fast);
    }

    #[test]
    fn test_unhurt_attacker_keeps_its_remaining_move() {
        let mut found = false;
        for seed in 0..200 {
            let mut game = Game::with_seed(10, 10, 50, seed);
            game.map = vec![vec![Terrain::Sea; 10]; 10];
            let mut bb = test_piece(1, PieceType::Battleship, Owner::Player, 5, 5);
            bb.moves_left = 2;
            game.pieces.push(bb);
            game.pieces.push(test_piece(2, PieceType::PatrolBoat, Owner::Comp, 5, 6));
            let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
            if result == MoveResult::CombatWon && game.pieces[0].hits == 10 {
                found = true;
                assert_eq!(game.pieces[0].moves_left, 1);
            }
        }
        assert!(found);
    }

    #[test]
    fn test_idle_damaged_boat_in_port_repairs_one_hit_per_turn() {
        let mut game = port_world(PieceType::Destroyer, 1, Owner::Player);
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 2);
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 3);
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 3, "never past the maximum");
    }

    #[test]
    fn test_computer_boats_repair_in_port_too() {
        let mut game = port_world(PieceType::Battleship, 5, Owner::Comp);
        game.process_turn();
        game.process_turn();
        assert!(game.pieces.iter().find(|p| p.id == 1).map_or(false, |p| p.hits >= 6));
    }

    #[test]
    fn test_boat_that_moved_does_not_repair_even_if_it_came_back() {
        let mut game = port_world(PieceType::Destroyer, 1, Owner::Player);
        game.pieces[0].moves_left = 2;
        game.attempt_move(0, Loc { r: 5, c: 6 }); // out...
        game.attempt_move(0, Loc { r: 5, c: 5 }); // ...and back into port
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 });
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 1, "it changed location this turn, so no repair");
        game.process_turn(); // a full turn without moving
        assert_eq!(game.pieces[0].hits, 2);
    }

    #[test]
    fn test_only_boats_in_their_own_port_repair() {
        let mut game = port_world(PieceType::Destroyer, 1, Owner::Player);
        game.cities[0].owner = Owner::Comp; // enemy port
        game.process_turn();
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 1);

        let mut game = port_world(PieceType::Army, 1, Owner::Player);
        game.pieces[0].hits = 1;
        game.process_turn();
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 1, "armies are not boats");
    }

    #[test]
    fn test_upgrade_order_repairs_exactly_one_hit_per_turn() {
        let mut game = port_world(PieceType::Battleship, 5, Owner::Player);
        game.pieces[0].activity = UnitActivity::Upgrade;
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 6, "one hit per turn, not two");
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 7);
        assert_eq!(game.pieces[0].activity, UnitActivity::Upgrade);
    }

    #[test]
    fn test_moved_this_turn_survives_a_save() {
        let mut game = port_world(PieceType::Destroyer, 1, Owner::Player);
        game.pieces[0].moves_left = 2;
        game.attempt_move(0, Loc { r: 5, c: 6 });
        let json = serde_json::to_string(&game).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert!(back.moved_this_turn.contains(&1));
    }

    // ---- one distance metric everywhere ----

    #[test]
    fn test_loc_dist_is_the_larger_axis_difference() {
        let a = Loc { r: 2, c: 3 };
        assert_eq!(a.dist(&Loc { r: 2, c: 3 }), 0);
        assert_eq!(a.dist(&Loc { r: 5, c: 4 }), 3);
        assert_eq!(a.dist(&Loc { r: 0, c: 9 }), 6);
        assert_eq!(Loc { r: 9, c: 0 }.dist(&Loc { r: 0, c: 9 }), 9, "symmetric, diagonals cost one");
    }

    /// Two own cities where Euclidean and move-distance disagree about which is nearer:
    /// (0,5) is 5 moves from the origin (distance^2 25); (4,4) is 4 moves (distance^2 32).
    fn two_city_world() -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        for loc in [Loc { r: 0, c: 5 }, Loc { r: 4, c: 4 }] {
            game.map[loc.r][loc.c] = Terrain::Land;
            game.cities.push(City { loc, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() });
        }
        game
    }

    #[test]
    fn test_fighter_landing_picks_the_city_nearest_in_moves() {
        let mut game = two_city_world();
        let mut fighter = test_piece(1, PieceType::Fighter, Owner::Player, 0, 0);
        fighter.fuel = 20;
        game.pieces.push(fighter);
        game.process_land_mode(0);
        assert_eq!(game.pieces[0].activity, UnitActivity::Dest(Loc { r: 4, c: 4 }));
    }

    #[test]
    fn test_damaged_ship_heads_for_the_city_nearest_in_moves() {
        let mut game = two_city_world();
        let mut ship = test_piece(1, PieceType::Destroyer, Owner::Player, 0, 0);
        ship.hits = 1;
        ship.moves_left = 2;
        game.pieces.push(ship);
        game.process_upgrade_mode(0);
        assert!(game.pieces[0].loc.r >= 1, "sailed toward (4,4), not along the top edge toward (0,5): {:?}", game.pieces[0].loc);
    }

    // ---- one distance metric ----

    fn sentry_world(enemy_r: usize, enemy_c: usize) -> Game {
        let mut game = Game::new(15, 15, 50);
        game.map = vec![vec![Terrain::Land; 15]; 15];
        let mut sentry = test_piece(1, PieceType::Army, Owner::Player, 5, 5);
        sentry.activity = UnitActivity::Sentry;
        game.pieces.push(sentry);
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, enemy_r, enemy_c));
        game.player_map[enemy_r][enemy_c] = 1; // the enemy is in view
        game
    }

    #[test]
    fn test_sentry_wakes_on_the_diagonal_at_sight_range() {
        // Army sight is 2. A diagonal enemy two squares away is at dist() == 2
        // (Chebyshev), even though its straight-line distance is 2.83.
        let mut game = sentry_world(7, 7);
        game.process_turn();
        assert_ne!(game.pieces[0].activity, UnitActivity::Sentry);
    }

    #[test]
    fn test_sentry_ignores_enemies_beyond_sight_range() {
        let mut game = sentry_world(8, 5);
        game.process_turn();
        assert_eq!(game.pieces[0].activity, UnitActivity::Sentry);
        let mut game = sentry_world(8, 8);
        game.process_turn();
        assert_eq!(game.pieces[0].activity, UnitActivity::Sentry);
    }
}
