#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::models::{Terrain, City, Loc, MoveResult, Owner, Piece, PieceType, UnitActivity};

    #[test]
    fn test_dynamic_city_scaling() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();
        
        let expected_target = (120 * 240) / 85;
        // Like the original, the full number of cities is always placed (spacing relaxes if needed).
        assert_eq!(game.cities.len(), expected_target);
    }

    #[test]
    fn test_cities_never_share_a_square() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();
        for (i, a) in game.cities.iter().enumerate() {
            for b in game.cities.iter().skip(i + 1) {
                assert_ne!(a.loc, b.loc, "two cities on one square");
                assert!(game.map[a.loc.r][a.loc.c] == Terrain::Land);
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
            default_orders: Vec::new(), seen_owner: None,
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
        game.map[5][4] = Terrain::Land; // the armies wait on the shore

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
    fn test_waking_a_transport_brings_its_armies_up() {
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
    fn test_fill_order_waits_until_full() {
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
    fn test_turn_production_cycle() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 },
            owner: Owner::Player,
            prod: PieceType::Army, 
            work: 5,               
            default_orders: Vec::new(), seen_owner: None,
        });
        
        let initial_pieces = game.pieces.len();
        game.process_turn();
        
        assert_eq!(game.pieces.len(), initial_pieces + 1, "New piece should spawn after work threshold is met");
        // The original's `work++ >= build_time` then `work -= build_time`: 5 + 1 - 5.
        assert_eq!(game.cities[0].work, 1, "Surplus work should carry over after production");
        
        let new_piece = game.pieces.last().unwrap();
        assert_eq!(new_piece.ptype, PieceType::Army, "Produced unit should match city production target");
        assert_eq!(new_piece.loc, Loc { r: 5, c: 5 }, "Unit should spawn on the city tile");
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
    fn test_sentry_wakes_when_an_enemy_is_next_to_it() {
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
    fn test_exploring_army_goes_around_water_to_the_unknown() {
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

        // The only unexplored square, (2,4), is beyond a wall of water at column 2: the
        // army has to go round it, by row 0 or row 4, until it can see (2,4), and never
        // steps into the sea.
        let mut path = vec![game.pieces[0].loc];
        for _ in 0..8 {
            if game.player_map[2][4] == 1 {
                break;
            }
            game.pieces[0].moves_left = 1;
            game.explore_piece(0);
            path.push(game.pieces[0].loc);
        }
        assert!(path.iter().all(|l| game.map[l.r][l.c] == Terrain::Land), "stepped into the sea: {:?}", path);
        assert_eq!(game.player_map[2][4], 1, "never got round the water to see it: {:?}", path);
        assert!(path.iter().any(|l| (l.r == 0 || l.r == 4) && l.c == 2), "went round the end of the wall: {:?}", path);
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
                    loc: Loc { r: 5, c: 6 }, owner, prod: PieceType::Army, work: 5, default_orders: Vec::new(), seen_owner: None,
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
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
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
                loc: Loc { r: 5, c: 6 }, owner, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
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
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
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
            loc: Loc { r: 5, c: 6 }, owner: Owner::Comp, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
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
            loc: Loc { r: 5, c: 6 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
        });
        game.pieces.push(test_piece(1, PieceType::Destroyer, Owner::Comp, 5, 5));
        game.pieces.push(test_piece(2, PieceType::Fighter, Owner::Comp, 5, 4));
        for _ in 0..20 {
            for p in game.pieces.iter_mut() { p.moves_left = p.ptype.max_moves(); p.fuel = 32; }
            game.comp_move();
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
        game.map[5][5] = Terrain::Land; // a port city

        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });

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
    fn test_waking_a_carrier_brings_its_fighters_up() {
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
    fn test_full_computer_transport_lands_its_armies_at_a_city() {
        // A full transport next to a coast with a neutral city: the armies come up from
        // the hold and go for the city.
        let mut game = Game::with_seed(12, 12, 50, 5);
        game.map = vec![vec![Terrain::Sea; 12]; 12];
        for r in 1..11 { game.map[r][8] = Terrain::Land; game.map[r][9] = Terrain::Land; }
        game.cities.push(City {
            loc: Loc { r: 5, c: 8 }, owner: Owner::Neutral, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
        });
        let mut transport = test_piece(1, PieceType::Transport, Owner::Comp, 5, 6);
        for i in 0..6 {
            transport.cargo.push(test_piece(10 + i, PieceType::Army, Owner::Comp, 5, 6));
        }
        game.pieces.push(transport);
        for _ in 0..3 {
            game.comp_move();
            for p in game.pieces.iter_mut() { p.moves_left = p.ptype.max_moves(); }
        }
        let aboard = game.pieces.iter().find(|p| p.ptype == PieceType::Transport).map_or(0, |t| {
            t.cargo.len() + game.pieces.iter().filter(|a| a.ptype == PieceType::Army && a.hits > 0 && a.loc == t.loc).count()
        });
        assert!(aboard < 6 || game.cities[0].owner == Owner::Comp, "the armies should have gone ashore");
    }

    #[test]
    fn test_order_inheritance_global_vs_local() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        
        game.cities.push(City { 
            loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, 
            default_orders: vec![(PieceType::Army, UnitActivity::Sentry)], seen_owner: None
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
    fn test_transport_carries_deck_and_hold_units() {
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

        // 1. Armies on deck sail with the transport
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
    fn test_transport_leaving_port_takes_at_most_six_armies() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[5][5] = Terrain::Land;
        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });

        // Transport in port at (5,5)
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

        assert_eq!(armies_at_sea, 6, "the transport sails with a full load of 6");
        assert_eq!(armies_on_land, 4, "the other 4 stay in the city");
    }

    #[test]
    fn test_army_waiting_for_a_transport_boards_one_alongside() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        game.map[6][6] = Terrain::Land; // the army waits on the shore

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
    fn test_computer_army_heads_for_a_known_enemy_city() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(City {
            loc: Loc { r: 1, c: 1 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
        });
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Comp, 8, 8));
        // The computer has seen the whole map (it only acts on what it has seen).
        for r in 0..10 { for c in 0..10 { game.comp_map[r][c] = b'+'; } }
        game.comp_map[1][1] = b'O';

        let before = game.dist(game.pieces[0].loc, Loc { r: 1, c: 1 });
        game.comp_move();
        let after = game.dist(game.pieces[0].loc, Loc { r: 1, c: 1 });
        assert_eq!(after, before - 1, "the army should step toward the city");
    }

    #[test]
    fn test_game_over_conditions() {
        let mut game = Game::new(10, 10, 50);

        game.cities.push(City { loc: Loc { r: 0, c: 0 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });
        game.cities.push(City { loc: Loc { r: 0, c: 1 }, owner: Owner::Comp, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });

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
            default_orders: Vec::new(), seen_owner: None,
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
            default_orders: Vec::new(), seen_owner: None,
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
            work: PieceType::Army.build_cost(),
            default_orders: Vec::new(), seen_owner: None,
        });
        game.process_turn();
        let first_id = game.pieces.last().unwrap().id;
        game.pieces.retain(|p| p.id != first_id); // that piece dies
        game.cities[0].work = PieceType::Army.build_cost();
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
            default_orders: vec![(PieceType::Army, UnitActivity::Sentry)], seen_owner: None,
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
        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });
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
        let game = Game::new(10, 10, 50);
        let a = Loc { r: 2, c: 3 };
        assert_eq!(game.dist(a, Loc { r: 2, c: 3 }), 0);
        assert_eq!(game.dist(a, Loc { r: 5, c: 4 }), 3);
        assert_eq!(game.dist(a, Loc { r: 0, c: 9 }), 6);
        assert_eq!(game.dist(Loc { r: 9, c: 0 }, Loc { r: 0, c: 9 }), 9, "symmetric, diagonals cost one");
    }

    /// Two own cities where Euclidean and move-distance disagree about which is nearer:
    /// (0,5) is 5 moves from the origin (distance^2 25); (4,4) is 4 moves (distance^2 32).
    fn two_city_world() -> Game {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![Terrain::Sea; 10]; 10];
        for loc in [Loc { r: 0, c: 5 }, Loc { r: 4, c: 4 }] {
            game.map[loc.r][loc.c] = Terrain::Land;
            game.cities.push(City { loc, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None });
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

    // ---- sentries ----

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

    // ---- save format: version, UI state, validation ----

    fn save_to_json(game: &Game, name: &str) -> (std::path::PathBuf, serde_json::Value) {
        let path = scratch_path(name);
        game.save_game(path.to_str().unwrap()).unwrap();
        let v = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        (path, v)
    }

    fn write_json(path: &std::path::Path, v: &serde_json::Value) {
        std::fs::write(path, serde_json::to_string(v).unwrap()).unwrap();
    }

    #[test]
    fn test_saves_carry_the_current_version() {
        let game = Game::new(10, 10, 50);
        let (path, v) = save_to_json(&game, "version.sav");
        assert_eq!(v["save_version"], crate::game::SAVE_VERSION);
        assert_eq!(Game::load_game(path.to_str().unwrap()).unwrap().save_version, crate::game::SAVE_VERSION);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_saves_from_before_versioning_still_load() {
        let (path, mut v) = save_to_json(&Game::new(10, 10, 50), "legacy.sav");
        v.as_object_mut().unwrap().remove("save_version");
        write_json(&path, &v);
        let loaded = Game::load_game(path.to_str().unwrap()).expect("legacy save should load");
        assert_eq!(loaded.save_version, crate::game::SAVE_VERSION, "migrated to the current version");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_saves_from_before_scoring_still_load() {
        // The first builds that could save had no score fields at all.
        let (path, mut v) = save_to_json(&Game::new(10, 10, 50), "unscored.sav");
        for key in ["save_version", "player_score", "comp_score", "next_id", "torus", "moved_this_turn", "player_view"] {
            v.as_object_mut().unwrap().remove(key);
        }
        write_json(&path, &v);
        let loaded = Game::load_game(path.to_str().unwrap()).expect("the oldest saves should load");
        assert_eq!((loaded.player_score, loaded.comp_score), (0, 0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_save_from_a_newer_build_is_refused_with_a_reason() {
        let (path, mut v) = save_to_json(&Game::new(10, 10, 50), "newer.sav");
        v["save_version"] = serde_json::json!(crate::game::SAVE_VERSION + 1);
        write_json(&path, &v);
        let err = Game::load_game(path.to_str().unwrap()).unwrap_err();
        assert!(err.contains("newer version"), "{}", err);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_saves_hold_no_view_state_and_old_saves_with_it_still_load() {
        let game = Game::new(10, 10, 50);
        let (path, mut v) = save_to_json(&game, "ui.sav");
        for key in ["viewport_center", "last_mouse_coord", "edit_cursor", "msg_scroll", "mode", "messages_logged"] {
            assert!(v.get(key).is_none(), "{} should not be written", key);
        }
        // a save written by an older build still contains view fields: they are ignored
        v["viewport_center"] = serde_json::json!({"r": 3, "c": 3});
        v["last_mouse_coord"] = serde_json::json!([4, 4]);
        v["edit_cursor"] = serde_json::json!({"r": 5, "c": 5});
        v["msg_scroll"] = serde_json::json!(9);
        v["mode"] = serde_json::json!({"PickDest": 3});
        write_json(&path, &v);
        assert!(Game::load_game(path.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_new_messages_scroll_the_view_back_down() {
        let mut game = Game::new(10, 10, 50);
        let mut view = crate::view::View::default();
        game.log(String::from("one"));
        view.follow_messages(&game);
        view.msg_scroll = 4;
        view.follow_messages(&game);
        assert_eq!(view.msg_scroll, 4, "nothing new: stays scrolled back");
        for i in 0..150 {
            game.log(format!("{}", i)); // the log is capped, the count is not
        }
        view.follow_messages(&game);
        assert_eq!(view.msg_scroll, 0);
    }

    #[test]
    fn test_game_state_survives_the_round_trip() {
        let mut game = Game::with_seed(60, 100, 50, 3);
        game.setup_initial_states();
        game.turn = 17;
        game.log(String::from("hello"));
        let path = scratch_path("state.sav");
        game.save_game(path.to_str().unwrap()).unwrap();
        let loaded = Game::load_game(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.turn, 17);
        assert_eq!(loaded.map, game.map);
        assert_eq!(loaded.cities.len(), game.cities.len());
        assert_eq!(loaded.messages, game.messages);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_bad_saves_fail_with_a_reason_instead_of_panicking() {
        let missing = scratch_path("does_not_exist.sav");
        assert!(Game::load_game(missing.to_str().unwrap()).unwrap_err().contains("could not read"));

        let garbage = scratch_path("garbage.sav");
        std::fs::write(&garbage, "this is not json").unwrap();
        assert!(Game::load_game(garbage.to_str().unwrap()).unwrap_err().contains("not a valid save"));
        let _ = std::fs::remove_file(garbage);

        // map shorter than the stated dimensions
        let (path, mut v) = save_to_json(&Game::new(10, 10, 50), "damaged.sav");
        v["map"].as_array_mut().unwrap().pop();
        write_json(&path, &v);
        assert!(Game::load_game(path.to_str().unwrap()).unwrap_err().contains("damaged"));

        // a unit placed off the map
        let mut game = Game::new(10, 10, 50);
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Player, 50, 50));
        game.save_game(path.to_str().unwrap()).unwrap();
        assert!(Game::load_game(path.to_str().unwrap()).unwrap_err().contains("outside the map"));
        let _ = std::fs::remove_file(path);
    }

    // ---- map generation: original algorithm ----

    fn land_masses(map: &Vec<Vec<Terrain>>) -> Vec<usize> {
        let (rows, cols) = (map.len(), map[0].len());
        let mut seen = vec![vec![false; cols]; rows];
        let mut sizes = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                if map[r][c] != Terrain::Land || seen[r][c] { continue; }
                let (mut stack, mut n) = (vec![(r, c)], 0);
                seen[r][c] = true;
                while let Some((a, b)) = stack.pop() {
                    n += 1;
                    for dr in -1i32..=1 {
                        for dc in -1i32..=1 {
                            let (x, y) = (a as i32 + dr, b as i32 + dc);
                            if x < 0 || y < 0 || x as usize >= rows || y as usize >= cols { continue; }
                            let (x, y) = (x as usize, y as usize);
                            if map[x][y] == Terrain::Land && !seen[x][y] {
                                seen[x][y] = true;
                                stack.push((x, y));
                            }
                        }
                    }
                }
                sizes.push(n);
            }
        }
        sizes.sort_by(|a, b| b.cmp(a));
        sizes
    }

    #[test]
    fn test_map_is_exactly_thirty_percent_land_with_a_sea_border() {
        for seed in 0..12 {
            for (rows, cols) in [(60, 100), (40, 80), (120, 240)] {
                let mut game = Game::with_seed(rows, cols, 50, seed);
                game.generate_map();
                let playable = (rows - 2) * (cols - 2);
                let land = (1..rows - 1).flat_map(|r| (1..cols - 1).map(move |c| (r, c)))
                    .filter(|&(r, c)| game.map[r][c] == Terrain::Land).count();
                assert_eq!(land, playable - playable * 70 / 100, "seed {} {}x{}", seed, rows, cols);
                assert!((0..rows).all(|r| game.map[r][0] == Terrain::Sea && game.map[r][cols - 1] == Terrain::Sea));
                assert!((0..cols).all(|c| game.map[0][c] == Terrain::Sea && game.map[rows - 1][c] == Terrain::Sea));
            }
        }
    }

    #[test]
    fn test_maps_have_several_land_masses_not_one_blob() {
        for seed in 0..25 {
            let mut game = Game::with_seed(60, 100, 50, seed);
            game.generate_map();
            let sizes = land_masses(&game.map);
            let total: usize = sizes.iter().sum();
            assert!(sizes[0] * 100 / total < 70, "seed {}: one land mass holds {}% of the land", seed, sizes[0] * 100 / total);
            assert!(sizes.iter().filter(|&&s| s >= 50).count() >= 2, "seed {}: expected at least two real land masses", seed);
        }
    }

    #[test]
    fn test_same_seed_same_map() {
        let (mut a, mut b) = (Game::with_seed(60, 100, 50, 9), Game::with_seed(60, 100, 50, 9));
        a.generate_map();
        b.generate_map();
        assert_eq!(a.map, b.map);
    }

    #[test]
    fn test_city_sites_respect_the_derived_minimum_distance() {
        let mut game = Game::with_seed(60, 100, 50, 1);
        let land: Vec<Loc> = (1..59).flat_map(|r| (1..99).map(move |c| Loc { r, c })).collect();
        // expected land per city = (58*98*30/100)/20 = 85 -> minimum distance isqrt(85) = 9
        let sites = game.place_cities(&land, 20);
        assert_eq!(sites.len(), 20);
        for (i, a) in sites.iter().enumerate() {
            for b in sites.iter().skip(i + 1) {
                assert!(game.dist(*a, *b) >= 9, "{:?} and {:?}", a, b);
            }
        }
    }

    #[test]
    fn test_city_spacing_relaxes_when_land_is_scarce_but_all_cities_are_placed() {
        let mut game = Game::with_seed(60, 100, 50, 1);
        // a 7x7 patch of land cannot hold 30 cities 4+ apart, so the spacing must loosen
        let land: Vec<Loc> = (10..17).flat_map(|r| (10..17).map(move |c| Loc { r, c })).collect();
        let sites = game.place_cities(&land, 30);
        assert_eq!(sites.len(), 30);
        let mut unique = sites.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 30, "no square used twice");
    }

    // ---- -t: torus maps ----

    fn torus_game(rows: usize, cols: usize) -> Game {
        let mut game = Game::new(rows, cols, 50);
        game.torus = true;
        game.map = vec![vec![Terrain::Sea; cols]; rows];
        game
    }

    #[test]
    fn test_torus_steps_wrap_across_the_seams_but_flat_maps_do_not() {
        let game = torus_game(10, 12);
        // playable squares are rows 1..=8 and columns 1..=10
        assert_eq!(game.step(Loc { r: 5, c: 10 }, (0, 1)), Some(Loc { r: 5, c: 1 }));
        assert_eq!(game.step(Loc { r: 5, c: 1 }, (0, -1)), Some(Loc { r: 5, c: 10 }));
        assert_eq!(game.step(Loc { r: 1, c: 4 }, (-1, 0)), Some(Loc { r: 8, c: 4 }));
        assert_eq!(game.step(Loc { r: 8, c: 4 }, (1, 0)), Some(Loc { r: 1, c: 4 }));
        assert_eq!(game.step(Loc { r: 1, c: 1 }, (-1, -1)), Some(Loc { r: 8, c: 10 }));
        assert_eq!(game.step(Loc { r: 5, c: 5 }, (1, 1)), Some(Loc { r: 6, c: 6 }));
        let flat = Game::new(10, 12, 50);
        assert_eq!(flat.step(Loc { r: 5, c: 10 }, (0, 1)), Some(Loc { r: 5, c: 11 }));
    }

    #[test]
    fn test_torus_distance_takes_the_short_way_round() {
        let mut game = torus_game(20, 30);
        assert_eq!(game.dist(Loc { r: 5, c: 1 }, Loc { r: 5, c: 28 }), 1);
        assert_eq!(game.dist(Loc { r: 1, c: 5 }, Loc { r: 18, c: 5 }), 1);
        assert_eq!(game.dist(Loc { r: 2, c: 2 }, Loc { r: 5, c: 6 }), 4);
        game.torus = false;
        assert_eq!(game.dist(Loc { r: 5, c: 1 }, Loc { r: 5, c: 28 }), 27);
    }

    #[test]
    fn test_torus_neighbours_cross_the_seams() {
        let mut game = torus_game(10, 12);
        let around: Vec<Loc> = game.neighbors(Loc { r: 1, c: 1 }).collect();
        assert_eq!(around.len(), 8);
        assert!(around.contains(&Loc { r: 8, c: 10 }), "diagonal across the corner seam");
        assert!(around.contains(&Loc { r: 8, c: 1 }) && around.contains(&Loc { r: 1, c: 10 }));
        assert!(around.iter().all(|l| l.r >= 1 && l.r <= 8 && l.c >= 1 && l.c <= 10), "never the border ring");
        // coastline is seen across a seam
        game.map = vec![vec![Terrain::Land; 12]; 10];
        assert!(!game.is_coastal(Loc { r: 5, c: 1 }));
        game.map[5][10] = Terrain::Sea;
        assert!(game.is_coastal(Loc { r: 5, c: 1 }));
    }

    #[test]
    fn test_ship_sails_across_the_seam() {
        let mut game = torus_game(10, 12);
        game.pieces.push(test_piece(1, PieceType::Destroyer, Owner::Player, 5, 10));
        game.pieces[0].moves_left = 2;
        let target = game.step(Loc { r: 5, c: 10 }, (0, 1)).unwrap();
        let (result, _) = game.attempt_move(0, target);
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 1 });
    }

    #[test]
    fn test_directed_ship_keeps_going_around_the_world() {
        let mut game = torus_game(10, 12);
        let mut ship = test_piece(1, PieceType::PatrolBoat, Owner::Player, 5, 9);
        ship.activity = UnitActivity::Dir(0, 1);
        ship.moves_left = 4;
        game.pieces.push(ship);
        game.dir_piece(0);
        // 9 -> 10 -> (seam) 1 -> 2 -> 3
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 3 });
    }

    #[test]
    fn test_satellites_bounce_rather_than_wrap_on_a_torus() {
        let mut game = torus_game(10, 12);
        let mut sat = test_piece(1, PieceType::Satellite, Owner::Player, 5, 9);
        sat.activity = UnitActivity::Dir(-1, 1);
        sat.fuel = 20;
        game.pieces.push(sat);
        game.move_satellites();
        let sat = &game.pieces[0];
        assert!(sat.hits > 0);
        assert!((1..=8).contains(&sat.loc.r) && (1..=10).contains(&sat.loc.c), "left the playable map: {:?}", sat.loc);
        assert!(matches!(sat.activity, UnitActivity::Dir(_, -1)), "should have bounced off the east edge: {:?}", sat.activity);
    }

    #[test]
    fn test_torus_view_wraps_and_never_shows_the_border() {
        use crate::disp::view_axes;
        let game = torus_game(20, 30);
        let (rows, cols) = view_axes(&game, Loc { r: 1, c: 1 }, 6, 8);
        assert_eq!((rows.len(), cols.len()), (6, 8));
        assert_eq!(rows, vec![16, 17, 18, 1, 2, 3]);
        assert_eq!(cols, vec![25, 26, 27, 28, 1, 2, 3, 4]);
        assert!(rows.iter().all(|&r| (1..=18).contains(&r)) && cols.iter().all(|&c| (1..=28).contains(&c)));
        // a view larger than the world is limited to the playable size
        let (rows, cols) = view_axes(&game, Loc { r: 5, c: 5 }, 100, 100);
        assert_eq!((rows.len(), cols.len()), (18, 28));
        let mut unique = rows.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 18, "no row is drawn twice");
    }

    #[test]
    fn test_flat_view_is_unchanged() {
        use crate::disp::view_axes;
        let game = Game::new(20, 30, 50);
        let (rows, cols) = view_axes(&game, Loc { r: 0, c: 0 }, 6, 8);
        assert_eq!(rows, (0..6).collect::<Vec<_>>());
        assert_eq!(cols, (0..8).collect::<Vec<_>>());
        let (rows, _) = view_axes(&game, Loc { r: 19, c: 5 }, 6, 8);
        assert_eq!(rows, (14..20).collect::<Vec<_>>());
    }

    #[test]
    fn test_cursor_wraps_on_a_torus_and_clamps_otherwise() {
        let mut game = torus_game(10, 12);
        assert_eq!(game.shift_cursor(Loc { r: 1, c: 1 }, -1, -1), Loc { r: 8, c: 10 });
        game.torus = false;
        assert_eq!(game.shift_cursor(Loc { r: 0, c: 0 }, -1, -1), Loc { r: 0, c: 0 });
    }

    #[test]
    fn test_torus_world_gets_all_its_cities_with_wrapped_spacing() {
        let mut game = Game::with_seed(60, 100, 50, 4);
        game.torus = true;
        game.setup_initial_states();
        assert_eq!(game.cities.len(), 6000 / 85);
        let mut unique: Vec<_> = game.cities.iter().map(|c| c.loc).collect();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), game.cities.len());
        assert!(game.cities.iter().all(|c| (1..=58).contains(&c.loc.r) && (1..=98).contains(&c.loc.c)));
    }

    #[test]
    fn test_torus_flag_is_saved_and_restored() {
        let mut game = torus_game(10, 12);
        game.torus = true;
        let path = scratch_path("torus.sav");
        game.save_game(path.to_str().unwrap()).unwrap();
        let loaded = Game::load_game(path.to_str().unwrap()).unwrap();
        assert!(loaded.torus);
        let _ = std::fs::remove_file(path);
    }

    fn unit(id: usize, ptype: PieceType, owner: Owner, r: usize, c: usize) -> Piece {
        Piece {
            id, ptype, owner, loc: Loc { r, c },
            hits: ptype.max_hits(),
            moves_left: ptype.max_moves(),
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: match ptype { PieceType::Fighter => 32, PieceType::Satellite => 20, _ => 0 },
        }
    }

    fn city(loc: Loc, owner: Owner) -> City {
        City { loc, owner, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None }
    }

    #[test]
    fn test_satellite_flies_over_land() {
        let mut game = Game::with_seed(10, 10, 50, 1);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.pieces.push(unit(1, PieceType::Satellite, Owner::Player, 5, 5));
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 });

        // Standing orders work over land too.
        game.pieces[0].activity = UnitActivity::Random;
        game.pieces[0].moves_left = 10;
        game.random_piece(0);
        assert_eq!(game.pieces[0].moves_left, 0, "satellite should spend all its moves over land");
    }

    #[test]
    fn test_transport_deck_counts_toward_capacity() {
        let mut game = Game::with_seed(10, 10, 50, 1);
        game.map[5][4] = Terrain::Land;
        let mut transport = unit(1, PieceType::Transport, Owner::Player, 5, 5);
        for i in 0..6 {
            transport.cargo.push(unit(10 + i, PieceType::Army, Owner::Player, 5, 5));
        }
        game.pieces.push(transport);
        game.wake_cargo(0); // all six now stand on deck
        assert_eq!(game.free_berths(&game.pieces[0], None), 0);

        // A seventh army can't squeeze aboard: like a full hold, it goes into the sea.
        game.pieces.push(unit(20, PieceType::Army, Owner::Player, 5, 4));
        let seventh = game.pieces.len() - 1;
        let (result, _) = game.attempt_move(seventh, Loc { r: 5, c: 5 });
        assert_eq!(result, MoveResult::CombatLost);
        game.pieces.retain(|p| p.hits > 0);

        // One of the six on deck can still go below into the hold.
        let deck_army = game.pieces.iter().position(|p| p.ptype == PieceType::Army).unwrap();
        let (result, msg) = game.attempt_move(deck_army, Loc { r: 5, c: 5 });
        assert_eq!(result, MoveResult::Moved, "{}", msg);
        assert!(msg.contains("boarded"));
        game.pieces.retain(|p| p.hits > 0);

        // When the transport sails, nobody is left behind on the open sea.
        let t = game.pieces.iter().position(|p| p.ptype == PieceType::Transport).unwrap();
        game.attempt_move(t, Loc { r: 5, c: 6 });
        assert!(!game.pieces.iter().any(|p| p.ptype == PieceType::Army && p.hits > 0 && p.loc == Loc { r: 5, c: 5 }));
        let t = &game.pieces[t];
        let on_deck = game.pieces.iter().filter(|p| p.ptype == PieceType::Army && p.hits > 0 && p.loc == t.loc).count();
        assert_eq!(t.cargo.len() + on_deck, 6);
    }

    #[test]
    fn test_armies_on_deck_drown_when_their_transport_sinks() {
        let mut game = Game::with_seed(10, 10, 50, 3);
        let mut transport = unit(1, PieceType::Transport, Owner::Player, 5, 5);
        transport.cargo.push(unit(10, PieceType::Army, Owner::Player, 5, 5));
        transport.cargo.push(unit(11, PieceType::Army, Owner::Player, 5, 5));
        game.pieces.push(transport);
        game.wake_cargo(0);
        // A battleship keeps attacking until the one-hit transport goes down.
        game.pieces.push(unit(2, PieceType::Battleship, Owner::Comp, 5, 6));
        let bs = game.pieces.len() - 1;
        while game.pieces[0].hits > 0 {
            game.pieces[bs].hits = 10;
            game.attempt_move(bs, Loc { r: 5, c: 5 });
        }
        assert!(!game.pieces.iter().any(|p| p.ptype == PieceType::Army && p.hits > 0),
            "armies standing on a sunk transport's deck must not be left floating");
    }

    #[test]
    fn test_fighter_on_attack_does_not_circle_an_enemy_city() {
        let mut game = Game::with_seed(20, 20, 50, 1);
        game.map = vec![vec![Terrain::Land; 20]; 20];
        for row in game.player_map.iter_mut() { for v in row.iter_mut() { *v = 1; } }
        game.cities.push(city(Loc { r: 10, c: 10 }, Owner::Comp));
        game.cities.push(city(Loc { r: 10, c: 2 }, Owner::Player));
        let mut fighter = unit(1, PieceType::Fighter, Owner::Player, 10, 6);
        fighter.activity = UnitActivity::Attack;
        game.pieces.push(fighter);
        game.attack_piece(0);
        // Nothing it can attack: it falls back to exploring (and, with the whole map known,
        // wakes up) instead of flying to the city and burning fuel beside it.
        assert_ne!(game.pieces[0].activity, UnitActivity::Attack);
        assert!(game.pieces[0].fuel >= 31, "fuel burned: {}", game.pieces[0].fuel);
    }

    #[test]
    fn test_fighter_on_attack_still_hunts_enemy_units() {
        let mut game = Game::with_seed(20, 20, 50, 1);
        game.map = vec![vec![Terrain::Land; 20]; 20];
        for row in game.player_map.iter_mut() { for v in row.iter_mut() { *v = 1; } }
        game.cities.push(city(Loc { r: 10, c: 10 }, Owner::Comp));
        game.cities.push(city(Loc { r: 10, c: 2 }, Owner::Player));
        game.pieces.push(unit(1, PieceType::Fighter, Owner::Player, 10, 6));
        game.pieces[0].activity = UnitActivity::Attack;
        game.pieces.push(unit(2, PieceType::Army, Owner::Comp, 4, 6));
        game.attack_piece(0);
        assert!(game.pieces[0].hits == 0 || game.pieces[1].hits == 0, "the fighter should have engaged the army");
    }

    #[test]
    fn test_fighter_landing_in_city_ends_its_turn() {
        let mut game = Game::with_seed(10, 10, 50, 1);
        game.cities.push(city(Loc { r: 5, c: 6 }, Owner::Player));
        let mut fighter = unit(1, PieceType::Fighter, Owner::Player, 5, 5);
        fighter.fuel = 3;
        game.pieces.push(fighter);
        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[0].fuel, 32);
        assert_eq!(game.pieces[0].moves_left, 0, "no touch-and-go: landing ends the turn");
    }

    #[test]
    fn test_fighter_over_full_carrier_refuels_and_stops() {
        let mut game = Game::with_seed(10, 10, 50, 1);
        let mut carrier = unit(1, PieceType::Carrier, Owner::Player, 5, 6);
        carrier.hits = 1; // badly damaged: room for one fighter
        carrier.cargo.push(unit(10, PieceType::Fighter, Owner::Player, 5, 6));
        game.pieces.push(carrier);
        let mut fighter = unit(2, PieceType::Fighter, Owner::Player, 5, 5);
        fighter.fuel = 3;
        game.pieces.push(fighter);
        game.attempt_move(1, Loc { r: 5, c: 6 });
        assert_eq!(game.pieces[1].fuel, 32);
        assert_eq!(game.pieces[1].moves_left, 0);
    }

    // ---- fog of war ----

    /// A 12x12 sea world with a player destroyer at (5,2) (sight 4) and everything explored.
    fn fog_world() -> Game {
        let mut game = Game::with_seed(12, 12, 50, 1);
        for row in game.player_map.iter_mut() { for v in row.iter_mut() { *v = 1; } }
        game.pieces.push(unit(1, PieceType::Destroyer, Owner::Player, 5, 2));
        game
    }

    #[test]
    fn test_enemies_on_explored_squares_out_of_sight_are_hidden() {
        let mut game = fog_world();
        game.pieces.push(unit(2, PieceType::Battleship, Owner::Comp, 5, 10)); // 8 away
        assert!(!game.is_visible_to(1, Owner::Player), "explored is not the same as in sight");
        game.pieces[1].loc = Loc { r: 5, c: 6 }; // 4 away
        assert!(game.is_visible_to(1, Owner::Player));
    }

    #[test]
    fn test_last_seen_enemy_is_remembered_until_looked_at_again() {
        let mut game = fog_world();
        game.pieces.push(unit(2, PieceType::Battleship, Owner::Comp, 5, 5));
        game.refresh_player_view();
        assert_eq!(game.player_view[5][5], b'b');

        // The destroyer leaves, then the battleship sails off unseen: the player still
        // remembers it where it was.
        game.pieces[0].loc = Loc { r: 5, c: 0 };
        game.pieces[1].loc = Loc { r: 5, c: 10 };
        game.refresh_player_view();
        assert_eq!(game.player_view[5][5], b'b', "out-of-date memory, as in the original");
        assert_eq!(game.player_view[5][10], 0, "never seen there");

        // The destroyer comes over for a look: the square is seen empty and forgotten.
        game.pieces[0].loc = Loc { r: 5, c: 7 };
        game.refresh_player_view();
        assert_eq!(game.player_view[5][5], 0);
        assert_eq!(game.player_view[5][10], b'b');
    }

    #[test]
    fn test_city_owner_out_of_sight_is_as_last_seen() {
        let mut game = fog_world();
        game.map[5][9] = Terrain::Land;
        game.cities.push(city(Loc { r: 5, c: 9 }, Owner::Neutral));
        game.pieces[0].loc = Loc { r: 5, c: 6 };
        game.refresh_player_view();
        game.pieces[0].loc = Loc { r: 5, c: 1 };
        game.cities[0].owner = Owner::Comp; // taken while nobody was watching
        game.refresh_player_view();
        assert_eq!(game.known_city_owner(&game.cities[0], None), Some(Owner::Neutral));
        game.pieces[0].loc = Loc { r: 5, c: 6 };
        game.refresh_player_view();
        assert_eq!(game.known_city_owner(&game.cities[0], None), Some(Owner::Comp));
    }

    #[test]
    fn test_losing_a_city_is_known_even_out_of_sight() {
        let mut game = Game::with_seed(10, 10, 50, 2);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Player));
        game.pieces.push(unit(1, PieceType::Army, Owner::Comp, 5, 6));
        // Keep attacking until the coin flip goes the computer's way.
        while game.cities[0].owner == Owner::Player {
            game.pieces[0].hits = 1;
            game.attempt_move(0, Loc { r: 5, c: 5 });
        }
        assert_eq!(game.known_city_owner(&game.cities[0], None), Some(Owner::Comp));
    }

    #[test]
    fn test_old_saves_get_a_remembered_view() {
        let mut game = fog_world();
        game.map[5][3] = Terrain::Land;
        game.cities.push(city(Loc { r: 5, c: 3 }, Owner::Comp));
        let (path, mut v) = save_to_json(&game, "fog_v1.sav");
        v["save_version"] = serde_json::json!(1);
        v.as_object_mut().unwrap().remove("player_view");
        v["cities"][0].as_object_mut().unwrap().remove("seen_owner");
        write_json(&path, &v);
        let loaded = Game::load_game(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.player_view.len(), 12);
        assert_eq!(loaded.cities[0].seen_owner, Some(Owner::Comp));
        let _ = std::fs::remove_file(path);
    }

    // ---- satellites ----

    #[test]
    fn test_new_satellite_flies_a_diagonal_by_itself() {
        let mut game = Game::with_seed(30, 30, 50, 4);
        game.cities.push(city(Loc { r: 15, c: 15 }, Owner::Player));
        game.cities[0].prod = PieceType::Satellite;
        game.cities[0].work = PieceType::Satellite.build_cost();
        game.process_turn();
        let sat = game.pieces.iter().find(|p| p.ptype == PieceType::Satellite).expect("satellite built");
        assert!(crate::game::SAT_DIRS.contains(&sat.activity), "{:?}", sat.activity);
        assert_eq!(sat.fuel, crate::game::SATELLITE_LIFE - 1, "one turn of its life used");
        assert_eq!(game.dist(sat.loc, Loc { r: 15, c: 15 }), 10, "moved its full 10 squares");
        assert_eq!(sat.moves_left, 0, "nothing left for the player to order");
    }

    #[test]
    fn test_satellite_bounces_off_the_map_edge() {
        let mut game = Game::with_seed(40, 40, 50, 1);
        let mut sat = unit(1, PieceType::Satellite, Owner::Player, 2, 10);
        sat.activity = UnitActivity::Dir(-1, 1); // NE, about to hit the top edge
        game.pieces.push(sat);
        game.move_satellites();
        let sat = &game.pieces[0];
        // (2,10) -> (1,11); then NE is off the map and so is NW, so it turns SE for 9 more.
        assert_eq!(sat.activity, UnitActivity::Dir(1, 1), "NE off the top bounces to SE");
        assert_eq!(sat.loc, Loc { r: 10, c: 20 });
        assert!(sat.hits > 0);
    }

    #[test]
    fn test_satellite_in_a_corner_turns_straight_back() {
        let mut game = Game::with_seed(20, 20, 50, 1);
        let mut sat = unit(1, PieceType::Satellite, Owner::Player, 1, 18);
        sat.activity = UnitActivity::Dir(-1, 1);
        game.pieces.push(sat);
        game.move_satellites();
        assert_eq!(game.pieces[0].activity, UnitActivity::Dir(1, -1));
        assert_eq!(game.pieces[0].loc, Loc { r: 11, c: 8 });
    }

    // ---- production ----

    #[test]
    fn test_changing_production_costs_a_fifth_of_the_build_time() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Player));
        game.cities[0].work = 4;
        game.set_production(0, PieceType::Transport, false);
        assert_eq!(game.cities[0].work, -6, "progress lost and 30/5 turns of retooling");
        game.cities[0].work = 3;
        game.set_production(0, PieceType::Transport, false);
        assert_eq!(game.cities[0].work, 3, "keeping the same production costs nothing");
        game.set_production(0, PieceType::Transport, true);
        assert_eq!(game.cities[0].work, -6, "a newly won city always starts by retooling");
    }

    #[test]
    fn test_first_army_after_retooling_takes_seven_turns_then_five() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Player));
        game.set_production(0, PieceType::Army, true);
        let mut built_on = Vec::new();
        for turn in 1..=12 {
            game.process_turn();
            if game.pieces.len() > built_on.len() {
                built_on.push(turn);
            }
        }
        assert_eq!(built_on, vec![7, 12], "as in the original: -1 work, then work++ >= 5");
    }

    #[test]
    fn test_computer_capture_pays_retooling() {
        let mut game = Game::with_seed(10, 10, 50, 2);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Neutral));
        game.pieces.push(unit(1, PieceType::Army, Owner::Comp, 5, 6));
        while game.cities[0].owner == Owner::Neutral {
            game.pieces[0].hits = 1;
            game.attempt_move(0, Loc { r: 5, c: 5 });
        }
        assert_eq!(game.cities[0].prod, PieceType::Army);
        assert_eq!(game.cities[0].work, -1);
    }

    // ---- the computer player (ai.rs) ----

    /// Lets the computer see the whole map, as if it had explored everything.
    fn comp_knows_all(game: &mut Game) {
        let (r, c) = (game.rows / 2, game.cols / 2);
        game.comp_scan(Loc { r, c }, (game.rows.max(game.cols)) as i32);
    }

    #[test]
    fn test_computer_only_sees_what_its_units_see() {
        let mut game = Game::with_seed(20, 20, 50, 1);
        game.map = vec![vec![Terrain::Land; 20]; 20];
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Comp, 5, 5));
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Player, 5, 15));
        game.comp_scan_all();
        assert!(matches!(game.comp_map[5][15], 0 | b' '), "too far to see");
        assert_eq!(game.comp_map[5][6], b'+');
        game.pieces[1].loc = Loc { r: 5, c: 7 }; // within an army's sight of 2
        game.comp_scan_all();
        assert_eq!(game.comp_map[5][7], b'A');
    }

    #[test]
    fn test_computer_hides_nothing_but_detects_subs_only_up_close() {
        let mut game = Game::with_seed(20, 20, 50, 1);
        game.pieces.push(test_piece(1, PieceType::Battleship, Owner::Comp, 5, 5));
        game.pieces.push(test_piece(2, PieceType::Submarine, Owner::Player, 5, 8));
        game.comp_scan_all();
        assert_eq!(game.comp_map[5][8], b'.', "a submarine 3 away goes unseen");
        game.pieces[1].loc = Loc { r: 5, c: 6 };
        game.comp_scan_all();
        assert_eq!(game.comp_map[5][6], b'S');
    }

    /// Two islands: the computer's (two coastal cities) and one with a coastal neutral city.
    fn two_islands() -> Game {
        let mut game = Game::with_seed(20, 30, 50, 3);
        for r in 4..9 { for c in 3..9 { game.map[r][c] = Terrain::Land; } }
        for r in 4..9 { for c in 20..26 { game.map[r][c] = Terrain::Land; } }
        game.cities.push(city(Loc { r: 4, c: 4 }, Owner::Comp));
        game.cities.push(city(Loc { r: 8, c: 8 }, Owner::Comp));
        game.cities.push(city(Loc { r: 4, c: 20 }, Owner::Neutral));
        game
    }

    #[test]
    fn test_computer_sets_up_a_transport_producer() {
        let mut game = two_islands();
        comp_knows_all(&mut game);
        // The home island is fully known and has nothing left to take, so a newly won
        // city there becomes the computer's transport producer.
        game.comp_new_cities.push(1);
        game.comp_move();
        assert_eq!(game.cities[1].prod, PieceType::Transport);
        // retooling (-6), then this turn's work (+1)
        assert_eq!(game.cities[1].work, -(PieceType::Transport.build_cost() / 5) + 1);
        assert!(game.comp_new_cities.is_empty());
    }

    #[test]
    fn test_new_computer_city_next_to_an_enemy_city_builds_armies() {
        let mut game = two_islands();
        game.cities[0].owner = Owner::Player;
        comp_knows_all(&mut game);
        game.cities[1].prod = PieceType::Fighter;
        game.comp_new_cities.push(1);
        game.comp_move();
        assert_eq!(game.cities[1].prod, PieceType::Army);
    }

    #[test]
    fn test_army_with_nothing_to_do_boards_a_loading_transport() {
        let mut game = two_islands();
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Comp, 6, 8));
        game.pieces.push(test_piece(2, PieceType::Transport, Owner::Comp, 6, 9));
        game.pieces[1].activity = UnitActivity::Fill; // loading
        comp_knows_all(&mut game);
        game.comp_move();
        let t = game.pieces.iter().find(|p| p.ptype == PieceType::Transport).unwrap();
        let aboard = t.cargo.len() + game.pieces.iter().filter(|a| a.ptype == PieceType::Army && a.loc == t.loc).count();
        assert_eq!(aboard, 1, "the army should have gone aboard");
    }

    #[test]
    fn test_computer_fighter_low_on_fuel_flies_home() {
        let mut game = Game::with_seed(30, 30, 50, 1);
        game.map[5][5] = Terrain::Land;
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Comp));
        let mut f = test_piece(1, PieceType::Fighter, Owner::Comp, 5, 13);
        f.fuel = 10; // 8 from home: time to turn back (fuel <= distance + 2)
        game.pieces.push(f);
        comp_knows_all(&mut game);
        game.comp_move();
        let f = game.pieces.iter().find(|p| p.ptype == PieceType::Fighter).expect("the fighter must not crash");
        assert_eq!(f.loc, Loc { r: 5, c: 5 }, "it should have flown its 8 moves home... ");
        assert_eq!(f.fuel, crate::game::FIGHTER_FUEL, "...and refuelled on landing");
    }

    #[test]
    fn test_damaged_computer_ship_heads_for_port() {
        let mut game = Game::with_seed(30, 30, 50, 1);
        game.map[5][5] = Terrain::Land;
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Comp));
        let mut d = test_piece(1, PieceType::Destroyer, Owner::Comp, 5, 12);
        d.hits = 1;
        game.pieces.push(d);
        comp_knows_all(&mut game);
        // One hit of three left: one move a turn.
        for _ in 0..7 {
            game.comp_move();
        }
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 }, "it should be in port");
    }

    #[test]
    fn test_computer_capture_is_queued_for_production() {
        let mut game = Game::with_seed(10, 10, 50, 2);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Neutral));
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Comp, 5, 6));
        while game.cities[0].owner == Owner::Neutral {
            game.pieces[0].hits = 1;
            game.attempt_move(0, Loc { r: 5, c: 5 });
        }
        assert_eq!(game.comp_new_cities, vec![0]);
    }

    #[test]
    fn test_old_saves_give_the_computer_a_full_view() {
        let mut game = Game::with_seed(12, 12, 50, 1);
        game.map[5][5] = Terrain::Land;
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Player));
        let (path, mut v) = save_to_json(&game, "compview_v2.sav");
        v["save_version"] = serde_json::json!(2);
        write_json(&path, &v);
        let loaded = Game::load_game(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.comp_map[5][5], b'O');
        assert_eq!(loaded.comp_map[1][1], b'.');
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_computer_turns_are_reproducible_with_a_seed() {
        let play = || {
            let mut g = Game::with_seed(40, 60, 50, 11);
            g.setup_initial_states();
            for _ in 0..60 { g.process_turn(); }
            (g.pieces.iter().map(|p| (p.id, p.loc)).collect::<Vec<_>>(), g.cities.iter().map(|c| c.owner).collect::<Vec<_>>())
        };
        assert_eq!(play(), play());
    }

    #[test]
    fn test_fighter_statistics_are_counted() {
        let mut game = Game::with_seed(10, 10, 50, 1);
        game.cities.push(city(Loc { r: 5, c: 6 }, Owner::Comp));
        let mut f = test_piece(1, PieceType::Fighter, Owner::Comp, 5, 5);
        f.fuel = 2;
        game.pieces.push(f);
        game.attempt_move(0, Loc { r: 5, c: 6 }); // lands with 1 to spare
        let mut f = test_piece(2, PieceType::Fighter, Owner::Comp, 2, 2);
        f.fuel = 1;
        game.pieces.push(f);
        game.attempt_move(1, Loc { r: 2, c: 3 }); // out of fuel
        let st = game.fighter_stats[1];
        assert_eq!((st.landings, st.low_fuel_landings, st.out_of_fuel), (1, 1, 1));
        assert_eq!(game.fighter_stats[0], Default::default());
    }

    // ---- starting cities and difficulty (the original's select_cities) ----

    /// Three islands: A (rows 2-6, cols 2-8) with three shore cities, B (rows 2-6,
    /// cols 15-20) with two, and C (rows 10-14, cols 2-9) with only one.
    fn start_world() -> Game {
        let mut game = Game::with_seed(20, 30, 0, 9);
        game.map = vec![vec![Terrain::Sea; 30]; 20];
        let mut land = |r0: usize, r1: usize, c0: usize, c1: usize| {
            for r in r0..=r1 { for c in c0..=c1 { game.map[r][c] = Terrain::Land; } }
        };
        land(2, 6, 2, 8);
        land(2, 6, 15, 20);
        land(10, 14, 2, 9);
        for (r, c) in [(2, 2), (6, 8), (2, 8), (2, 15), (6, 20), (12, 5)] {
            game.cities.push(city(Loc { r, c }, Owner::Neutral));
        }
        game
    }

    #[test]
    fn test_start_continents_need_two_cities_and_a_shore_city() {
        let game = start_world();
        let conts = game.start_continents();
        assert_eq!(conts.len(), 2, "island C has only one city");
        // A: (3 shore - 2) * 3 * 1000 + 35 squares; B: 0 + 30 squares
        assert_eq!(conts[0].value, 3035);
        assert_eq!(conts[1].value, 30);
        assert_eq!(conts[0].cities.len(), 3);
    }

    #[test]
    fn test_difficulty_levels_rank_continent_pairs_easiest_first() {
        let game = start_world();
        let pairs = game.start_pairs();
        assert_eq!(pairs.len(), 4, "two continents: 2 x 2 pairs");
        assert_eq!(pairs[0], (0, 1), "easiest: the player gets the better continent");
        assert_eq!(pairs[3], (1, 0), "hardest: the computer does");
        assert!(pairs[1..3].contains(&(0, 0)) && pairs[1..3].contains(&(1, 1)), "sharing a continent is in the middle");
    }

    #[test]
    fn test_choose_start_gives_each_side_a_city_on_its_continent() {
        for level in 0..4 {
            let mut game = start_world();
            game.choose_start(level);
            let player = game.cities.iter().position(|c| c.owner == Owner::Player).unwrap();
            let comp = game.cities.iter().position(|c| c.owner == Owner::Comp).unwrap();
            assert_ne!(player, comp);
            let on_a = |i: usize| i < 3;
            let (want_p, want_c) = [(true, false), (true, true), (false, false), (false, true)][level];
            if level == 0 || level == 3 {
                assert_eq!((on_a(player), on_a(comp)), (want_p, want_c), "level {}", level);
            } else {
                assert_eq!(on_a(player), on_a(comp), "level {}: both on one continent", level);
            }
            assert_eq!(game.difficulty, level as u32);
            assert_eq!(game.cities[comp].prod, PieceType::Army);
        }
    }

    #[test]
    fn test_generated_worlds_offer_a_square_number_of_levels() {
        for seed in 1..=5 {
            let mut game = Game::with_seed(60, 100, 0, seed);
            let levels = game.generate_world();
            let n = game.start_continents().len();
            assert!(n >= 1 && n <= 10);
            assert_eq!(levels, n * n);
            game.choose_start(levels - 1);
            assert_eq!(game.cities.iter().filter(|c| c.owner == Owner::Player).count(), 1);
            assert_eq!(game.cities.iter().filter(|c| c.owner == Owner::Comp).count(), 1);
        }
    }

    // ---- the computer acknowledging defeat (the original's check_endgame) ----

    /// A world with `pc`/`cc` player/computer cities and `pa`/`ca` armies.
    fn endgame_world(pc: usize, cc: usize, pa: usize, ca: usize) -> Game {
        let mut game = Game::new(20, 20, 0);
        let mut n = 0;
        for (count, owner) in [(pc, Owner::Player), (cc, Owner::Comp)] {
            for _ in 0..count {
                game.cities.push(city(Loc { r: 1 + n / 18, c: 1 + n % 18 }, owner));
                n += 1;
            }
        }
        for (count, owner) in [(pa, Owner::Player), (ca, Owner::Comp)] {
            for i in 0..count {
                game.pieces.push(test_piece(100 + n + i, PieceType::Army, owner, 10, 10));
            }
            n += count;
        }
        game
    }

    #[test]
    fn test_computer_resigns_when_down_to_a_third() {
        assert!(endgame_world(9, 2, 9, 2).comp_should_resign());
        assert!(!endgame_world(9, 3, 9, 2).comp_should_resign(), "3 cities is not under 9/3");
        assert!(!endgame_world(9, 2, 9, 3).comp_should_resign(), "armies count too");
        assert!(!endgame_world(8, 2, 8, 2).comp_should_resign(), "8/3 is 2 in whole numbers");
        assert!(endgame_world(3, 0, 3, 0).comp_should_resign());
    }

    #[test]
    fn test_armies_aboard_transports_count_for_resigning() {
        let mut game = endgame_world(9, 2, 0, 0);
        let mut t = test_piece(1, PieceType::Transport, Owner::Player, 5, 5);
        for i in 0..6 { t.cargo.push(test_piece(10 + i, PieceType::Army, Owner::Player, 5, 5)); }
        game.pieces.push(t);
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Comp, 8, 8));
        assert!(game.comp_should_resign(), "1 is under 6/3");
    }

    #[test]
    fn test_computer_resigns_only_once() {
        let mut game = endgame_world(9, 2, 9, 2);
        game.comp_resigned = true;
        assert!(!game.comp_should_resign());
        let (path, _) = save_to_json(&game, "resigned.sav");
        assert!(Game::load_game(path.to_str().unwrap()).unwrap().comp_resigned, "remembered in saves");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_a_lake_shore_is_not_a_port_for_starting() {
        let mut game = Game::with_seed(20, 30, 0, 9);
        game.map = vec![vec![Terrain::Sea; 30]; 20];
        // Island A (rows 2-10, cols 2-12) around a lake (rows 5-7, cols 5-8); island B.
        for r in 2..=10 { for c in 2..=12 { game.map[r][c] = Terrain::Land; } }
        for r in 5..=7 { for c in 5..=8 { game.map[r][c] = Terrain::Sea; } }
        for r in 2..=6 { for c in 18..=23 { game.map[r][c] = Terrain::Land; } }
        game.cities.push(city(Loc { r: 4, c: 6 }, Owner::Neutral)); // on the lake
        game.cities.push(city(Loc { r: 8, c: 7 }, Owner::Neutral)); // on the lake
        game.cities.push(city(Loc { r: 2, c: 18 }, Owner::Neutral));
        game.cities.push(city(Loc { r: 6, c: 23 }, Owner::Neutral));

        let ocean = game.open_ocean();
        assert!(ocean[0 + 1][1] && !ocean[6][6], "the lake is not the ocean");
        let conts = game.start_continents();
        assert_eq!(conts.len(), 1, "island A's cities are only on its lake");
        assert_eq!(conts[0].cities, vec![2, 3]);

        // Give A a city on the open ocean: now it qualifies, and its lake cities count
        // as inland ones: (1 - 1) * 3 + (3 - 1 - 1) * 2 = 2, times 1000, plus land.
        game.cities.push(city(Loc { r: 2, c: 2 }, Owner::Neutral));
        let conts = game.start_continents();
        assert_eq!(conts.len(), 2);
        let land_a = 9 * 11 - 3 * 4;
        assert_eq!(conts[0].value, 2000 + land_a);
    }

    // ---- the player's order commands (what the interface calls) ----

    #[test]
    fn test_global_standing_order_reaches_waiting_units_but_not_cities_with_their_own() {
        let mut game = Game::with_seed(10, 10, 0, 1);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(city(Loc { r: 2, c: 2 }, Owner::Player));
        game.cities.push(city(Loc { r: 7, c: 7 }, Owner::Player));
        game.cities[1].default_orders.push((PieceType::Army, UnitActivity::Sentry));
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Player, 2, 2));
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Player, 7, 7));
        game.pieces[0].activity = UnitActivity::Idle;
        game.pieces[1].activity = UnitActivity::Idle;
        let n = game.set_standing_order(crate::game::OrderScope::Global, PieceType::Army, UnitActivity::Sentry);
        assert_eq!(n, 1, "the city with its own army order is left alone");
        assert_eq!(game.default_orders.iter().find(|(p, _)| *p == PieceType::Army).unwrap().1, UnitActivity::Sentry);
        assert_eq!(game.pieces[0].activity, UnitActivity::Sentry);
        assert_eq!(game.pieces[1].activity, UnitActivity::Idle);
    }

    #[test]
    fn test_city_standing_order_is_carried_out_at_once() {
        let mut game = Game::with_seed(10, 10, 0, 1);
        game.map = vec![vec![Terrain::Land; 10]; 10];
        game.cities.push(city(Loc { r: 5, c: 5 }, Owner::Player));
        game.pieces.push(test_piece(1, PieceType::Army, Owner::Player, 5, 5));
        game.pieces[0].activity = UnitActivity::Idle;
        let n = game.set_standing_order(crate::game::OrderScope::City(0), PieceType::Army, UnitActivity::Dir(0, 1));
        assert_eq!(n, 1);
        assert_eq!(game.cities[0].default_orders, vec![(PieceType::Army, UnitActivity::Dir(0, 1))]);
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 }, "it marched straight away");
    }

    #[test]
    fn test_wake_commands() {
        let mut game = Game::with_seed(10, 10, 0, 1);
        let mut t = test_piece(1, PieceType::Transport, Owner::Player, 5, 5);
        t.cargo.push(test_piece(10, PieceType::Army, Owner::Player, 5, 5));
        game.pieces.push(t);
        game.pieces.push(test_piece(2, PieceType::Destroyer, Owner::Player, 5, 5));
        game.pieces[1].activity = UnitActivity::Sentry;
        assert_eq!(game.wake_units_at(Loc { r: 5, c: 5 }, Some(PieceType::Destroyer)), 1);
        assert_eq!(game.pieces[0].cargo.len(), 1, "only destroyers were asked for");
        game.pieces[0].activity = UnitActivity::Sentry;
        assert_eq!(game.wake_units_at(Loc { r: 5, c: 5 }, None), 1, "* wakes the transport...");
        assert!(game.pieces[0].cargo.is_empty(), "...and brings up its hold");
        assert!(game.pieces.iter().any(|p| p.id == 10 && p.activity == UnitActivity::Idle));
    }

    /// A strip of sea along row 5 with land to the north (rows 0-3) and a transport at
    /// (5, `col`) carrying two sleeping armies.
    fn coast_world(col: usize) -> Game {
        let mut game = Game::with_seed(10, 20, 0, 1);
        game.map = vec![vec![Terrain::Sea; 20]; 10];
        for r in 1..4 {
            for c in 1..19 {
                game.map[r][c] = Terrain::Land;
            }
        }
        game.player_map = vec![vec![1; 20]; 10];
        let mut t = test_piece(1, PieceType::Transport, Owner::Player, 5, col);
        for id in [10, 11] {
            let mut a = test_piece(id, PieceType::Army, Owner::Player, 5, col);
            a.activity = UnitActivity::Sentry;
            t.cargo.push(a);
        }
        game.pieces.push(t);
        game
    }

    #[test]
    fn test_transport_arriving_next_to_land_brings_its_armies_up() {
        let mut game = coast_world(2);
        // destination (4, 6): next to the land at (3, 6)
        game.order_unit(0, UnitActivity::Dest(Loc { r: 4, c: 6 }));
        let t = game.pieces.iter().find(|p| p.id == 1).unwrap();
        assert_eq!(t.loc, Loc { r: 4, c: 4 }, "two moves along: still on the way");
        assert_eq!(t.cargo.len(), 2, "the armies sleep while it sails, even along the coast");

        game.process_turn();
        let t = game.pieces.iter().find(|p| p.id == 1).unwrap();
        assert_eq!(t.loc, Loc { r: 4, c: 6 });
        assert!(t.cargo.is_empty(), "arrived: the hold is emptied");
        let armies: Vec<_> = game.pieces.iter().filter(|p| p.ptype == PieceType::Army).collect();
        assert_eq!(armies.len(), 2);
        assert!(armies.iter().all(|a| a.loc == Loc { r: 4, c: 6 } && a.activity == UnitActivity::Idle && a.moves_left > 0),
            "they are on deck, awake and ready to go ashore");
        assert!(game.messages.iter().any(|m| m.contains("Transport arrived at (4,6)")));
    }

    #[test]
    fn test_transport_arriving_at_open_sea_keeps_its_armies_asleep() {
        let mut game = coast_world(2);
        game.order_unit(0, UnitActivity::Dest(Loc { r: 8, c: 3 }));
        game.process_turn();
        let t = game.pieces.iter().find(|p| p.id == 1).unwrap();
        assert_eq!(t.loc, Loc { r: 8, c: 3 });
        assert_eq!(t.cargo.len(), 2, "nowhere to land, so nobody is woken");
    }

    #[test]
    fn test_transport_moved_by_hand_along_the_coast_keeps_its_armies_asleep() {
        let mut game = coast_world(2);
        for c in 3..8 {
            let i = game.pieces.iter().position(|p| p.id == 1).unwrap();
            game.attempt_move(i, Loc { r: 4, c });
            game.pieces[i].moves_left = 2;
        }
        let t = game.pieces.iter().find(|p| p.id == 1).unwrap();
        assert_eq!(t.cargo.len(), 2, "moving by hand never wakes the hold");
    }

    #[test]
    fn test_transport_stuck_next_to_land_brings_its_armies_up() {
        let mut game = coast_world(2);
        game.map[4][3] = Terrain::Land; // a headland blocks the way east along row 4
        game.pieces[0].loc = Loc { r: 4, c: 2 };
        game.order_unit(0, UnitActivity::Dir(0, 1));
        let t = game.pieces.iter().find(|p| p.id == 1).unwrap();
        assert!(t.cargo.is_empty(), "stuck with moves to spare counts as arriving");
    }

    #[test]
    fn test_carriers_keep_their_fighters_until_woken() {
        let mut game = coast_world(2);
        game.pieces[0].ptype = PieceType::Carrier;
        game.pieces[0].hits = PieceType::Carrier.max_hits();
        for a in game.pieces[0].cargo.iter_mut() {
            a.ptype = PieceType::Fighter;
            a.fuel = crate::game::FIGHTER_FUEL;
        }
        game.order_unit(0, UnitActivity::Dest(Loc { r: 4, c: 4 }));
        assert_eq!(game.pieces[0].cargo.len(), 2, "fighters are launched on purpose, not on arrival");
        game.wake_unit(0);
        assert!(game.pieces[0].cargo.is_empty(), "W on the carrier launches them");
        assert_eq!(game.pieces.iter().filter(|p| p.ptype == PieceType::Fighter && p.activity == UnitActivity::Idle).count(), 2);
    }

    #[test]
    fn test_sentry_on_a_transport_goes_below_deck() {
        let mut game = Game::with_seed(10, 10, 0, 1);
        game.pieces.push(test_piece(1, PieceType::Transport, Owner::Player, 5, 5));
        game.pieces.push(test_piece(2, PieceType::Army, Owner::Player, 5, 5));
        game.sentry_unit(1);
        assert_eq!(game.pieces.len(), 1);
        assert_eq!(game.pieces[0].cargo.len(), 1);
        // with nothing to board, it just stands sentry
        game.pieces.push(test_piece(3, PieceType::Army, Owner::Player, 2, 2));
        game.sentry_unit(1);
        assert_eq!(game.pieces[1].activity, UnitActivity::Sentry);
        assert_eq!(game.pieces[1].moves_left, 0);
    }

    #[test]
    fn test_skip_and_order_unit() {
        let mut game = Game::with_seed(10, 10, 0, 1);
        game.pieces.push(test_piece(1, PieceType::Destroyer, Owner::Player, 5, 5));
        game.pieces[0].activity = UnitActivity::Idle;
        game.skip_unit(0);
        assert_eq!(game.pieces[0].moves_left, 0);
        game.pieces[0].moves_left = 2;
        game.order_unit(0, UnitActivity::Dir(1, 0));
        assert_eq!(game.pieces[0].loc, Loc { r: 7, c: 5 });
        assert!(game.messages.last().unwrap().contains("South"), "{:?}", game.messages.last());
    }
}
