use super::*;

#[test]
fn test_invalid_paths() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // Invalid start node:
    {
        let start = Node::new(Cell::new(-1, -1));
        let goal  = Node::new(Cell::new(0, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        assert!(path.not_found());
    }

    // Invalid goal node:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(8, 8));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        assert!(path.not_found());
    }

    // Non traversable nodes:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(3, 3));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        assert!(path.not_found());
    }
}

#[test]
fn test_straight_line_paths() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // Two vertical and horizontal crossing road paths.
    let nodes = vec![
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        R,R,R,R,R,R,R,R,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
    ];

    // Expected paths:
    let vertical_path: Vec<Node> = (0..8).map(|i| Node::new(Cell::new(3, i))).collect();
    let horizontal_path: Vec<Node> = (0..8).map(|i| Node::new(Cell::new(i, 3))).collect();

    let top_to_right_path: Vec<Node> = [&vertical_path[0..3], &horizontal_path[3..8]].concat();
    let left_to_bottom_path: Vec<Node> = [&horizontal_path[0..3], &vertical_path[3..8]].concat();

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // Vertical path across the grid:
    {
        let start = Node::new(Cell::new(3, 0));
        let goal  = Node::new(Cell::new(3, 7));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &vertical_path);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Horizontal path across the grid:
    {
        let start = Node::new(Cell::new(0, 3));
        let goal  = Node::new(Cell::new(7, 3));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &horizontal_path);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Crossing path from top to right:
    {
        let start = Node::new(Cell::new(3, 0));
        let goal  = Node::new(Cell::new(7, 3));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &top_to_right_path);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Crossing path from left to bottom:
    {
        let start = Node::new(Cell::new(0, 3));
        let goal  = Node::new(Cell::new(3, 7));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &left_to_bottom_path);
            }
            _ => panic!("Expected a path!"),
        }
    }
}

#[test]
fn test_diagonal_paths() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // One single diagonal road path.
    let nodes = vec![
        R,R,W,W,W,W,W,W,
        W,R,R,W,W,W,W,W,
        W,W,R,R,W,W,W,W,
        W,W,W,R,R,W,W,W,
        W,W,W,W,R,R,W,W,
        W,W,W,W,W,R,R,W,
        W,W,W,W,W,W,R,R,
        W,W,W,W,W,W,W,R,
    ];

    // Expected path:
    let diagonal_path: Vec<Node> = (0..8).flat_map(|i| {
        let mut nodes = Vec::new();
        if i > 0 {
            nodes.push(Node::new(Cell::new(i, i - 1)));
        }
        nodes.push(Node::new(Cell::new(i, i)));
        nodes
    }).collect();

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // start=goal:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(0, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &[goal]);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Diagonal path across the grid:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(7, 7));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &diagonal_path);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Diagonal path across the grid (reverse):
    {
        let start = Node::new(Cell::new(7, 7));
        let goal  = Node::new(Cell::new(0, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                let reverse_diagonal_path: Vec<Node> =
                    diagonal_path.iter().rev().cloned().collect();
                assert_eq!(path, &reverse_diagonal_path);
            }
            _ => panic!("Expected a path!"),
        }
    }
}

#[test]
fn test_find_waypoint() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // Two vertical and horizontal crossing road paths.
    let nodes = vec![
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        R,R,R,R,R,R,R,R,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    // Vertical path:
    {
        let start = Node::new(Cell::new(3, 0));
        let max_distance = 5;

        let path = search.find_waypoints(&graph,
                                         &heuristic,
                                         &bias,
                                         &mut filter,
                                         NodeKind::Road,
                                         start,
                                         max_distance);

        match path {
            SearchResult::PathFound(path) => {
                let expected_path: Vec<Node> =
                    (0..=5).map(|i| Node::new(Cell::new(3, i))).collect();
                assert_eq!(path, &expected_path); // goal=[3,5]
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Horizontal path:
    {
        let start = Node::new(Cell::new(0, 3));
        let max_distance = 7;

        let path = search.find_waypoints(&graph,
                                         &heuristic,
                                         &bias,
                                         &mut filter,
                                         NodeKind::Road,
                                         start,
                                         max_distance);

        match path {
            SearchResult::PathFound(path) => {
                let expected_path: Vec<Node> =
                    (0..=7).map(|i| Node::new(Cell::new(i, 3))).collect();
                assert_eq!(path, &expected_path); // goal=[7,3]
            }
            _ => panic!("Expected a path!"),
        }
    }
}

#[test]
fn test_find_waypoint_shorter_distance() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    let nodes = vec![
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,R,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(3, 0));
    let max_distance = 7; // Max distance is bigger than length of only available path.

    let path = search.find_waypoints(&graph,
                                     &heuristic,
                                     &bias,
                                     &mut filter,
                                     NodeKind::Road,
                                     start,
                                     max_distance);

    match path {
        SearchResult::PathFound(path) => {
            // We only have traversable nodes up to distance=4.
            let expected_path: Vec<Node> = (0..=4).map(|i| Node::new(Cell::new(3, i))).collect();
            assert_eq!(path, &expected_path); // goal=[3,4]
        }
        _ => panic!("Expected a path!"),
    }
}

#[test]
fn test_find_path_to_node() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;
    const V: NodeKind = NodeKind::VacantLot;

    // Row 3 is a road; cell (5, 3) is also flagged as a vacant lot.
    let nodes = vec![
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        R,R,R,R,R,V,R,R,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    let traversable = NodeKind::Road | NodeKind::VacantLot;

    // Path from road start to the vacant lot.
    {
        let start = Node::new(Cell::new(0, 3));
        let path = search.find_path_to_node(&graph,
                                            &heuristic,
                                            &bias,
                                            &mut filter,
                                            traversable,
                                            start,
                                            NodeKind::VacantLot);
        match path {
            SearchResult::PathFound(path) => {
                let expected: Vec<Node> = (0..=5).map(|i| Node::new(Cell::new(i, 3))).collect();
                assert_eq!(path, &expected); // goal=[5,3]
            }
            _ => panic!("Expected a path!"),
        }
    }

    // start == goal: unit already standing on a vacant lot.
    {
        let start = Node::new(Cell::new(5, 3));
        let path = search.find_path_to_node(&graph,
                                            &heuristic,
                                            &bias,
                                            &mut filter,
                                            traversable,
                                            start,
                                            NodeKind::VacantLot);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &[start]);
            }
            _ => panic!("Expected a 1-node path!"),
        }
    }

    // No reachable goal of the requested kind (grid has no EmptyLand).
    {
        let start = Node::new(Cell::new(0, 3));
        let traversable_with_empty = NodeKind::Road | NodeKind::EmptyLand;
        let path = search.find_path_to_node(&graph,
                                            &heuristic,
                                            &bias,
                                            &mut filter,
                                            traversable_with_empty,
                                            start,
                                            NodeKind::EmptyLand);
        assert!(path.not_found());
    }
}

#[test]
fn test_find_buildings() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;
    let l = NodeKind::Road | NodeKind::BuildingRoadLink;

    // Row 3 is a road; cell (5, 3) is also flagged as a building road link.
    let nodes = vec![
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        R,R,R,R,R,l,R,R,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    // Path from a road start to the nearest road link.
    {
        let start = Node::new(Cell::new(0, 3));
        let max_distance = 100;
        let path = search.find_buildings(&graph,
                                         &heuristic,
                                         &bias,
                                         &mut filter,
                                         NodeKind::Road,
                                         start,
                                         max_distance);
        match path {
            SearchResult::PathFound(path) => {
                let expected: Vec<Node> = (0..=5).map(|i| Node::new(Cell::new(i, 3))).collect();
                assert_eq!(path, &expected); // goal=[5,3]
            }
            _ => panic!("Expected a path!"),
        }
    }

    // start == goal: unit already standing on a building road link.
    {
        let start = Node::new(Cell::new(5, 3));
        let max_distance = 100;
        let path = search.find_buildings(&graph,
                                         &heuristic,
                                         &bias,
                                         &mut filter,
                                         NodeKind::Road,
                                         start,
                                         max_distance);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &[start]);
            }
            _ => panic!("Expected a 1-node path!"),
        }
    }

    // No road link within the allowed distance.
    {
        let start = Node::new(Cell::new(0, 3));
        let max_distance = 3; // Not enough to reach (5, 3).
        let path = search.find_buildings(&graph,
                                         &heuristic,
                                         &bias,
                                         &mut filter,
                                         NodeKind::Road,
                                         start,
                                         max_distance);
        assert!(path.not_found());
    }
}

// ----------------------------------------------
// find_path: correctness
// ----------------------------------------------

#[test]
fn test_find_path_adjacent_cells() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(1, 0));
    let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
    match path {
        SearchResult::PathFound(path) => {
            assert_eq!(path, &[start, goal]);
        }
        _ => panic!("Expected a path!"),
    }
}

#[test]
fn test_find_path_detour_around_obstacle() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // Row 3 is a road with a two-cell Water wall at (3,3) and (4,3).
    // Rows above/below are also road, so the search must detour around.
    let nodes = vec![
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,W,W,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 3));
    let goal  = Node::new(Cell::new(7, 3));
    let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
    match path {
        SearchResult::PathFound(path) => {
            // Manhattan distance is 7 (8 nodes straight). The 2-wide wall
            // forces +2 detour cells (one step out, one step back).
            assert_eq!(path.len(), 10);
            assert_eq!(*path.first().unwrap(), start);
            assert_eq!(*path.last().unwrap(), goal);
            assert!(!path.iter().any(|n| n.cell == Cell::new(3, 3) || n.cell == Cell::new(4, 3)),
                    "Path must not cross the Water wall: {:?}", path);
        }
        _ => panic!("Expected a path!"),
    }
}

#[test]
fn test_find_path_unreachable_surrounded_goal() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // (4,4) is Road but fully boxed in by Water on all four 4-neighbors.
    let nodes = vec![
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,W,R,R,R,
        R,R,R,W,R,W,R,R,
        R,R,R,R,W,R,R,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(4, 4));
    let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
    assert!(path.not_found());
}

#[test]
fn test_find_path_disconnected_components() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // Two Road rows separated by a solid Water sea — no bridge.
    let nodes = vec![
        R,R,R,R,R,R,R,R,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        W,W,W,W,W,W,W,W,
        R,R,R,R,R,R,R,R,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(0, 7));
    let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
    assert!(path.not_found());
}

#[test]
fn test_find_path_boundary_hugging() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // Path along the top edge (y=0):
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(7, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                let expected: Vec<Node> = (0..8).map(|i| Node::new(Cell::new(i, 0))).collect();
                assert_eq!(path, &expected);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // Opposite corners — just verify length and endpoints (many equal-cost routes).
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(7, 7));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                // Manhattan distance = 14, so optimal path = 15 nodes.
                assert_eq!(path.len(), 15);
                assert_eq!(*path.first().unwrap(), start);
                assert_eq!(*path.last().unwrap(), goal);
            }
            _ => panic!("Expected a path!"),
        }
    }
}

#[test]
fn test_find_path_optimality() {
    // Fully open grid: many equal-cost paths exist between start and goal.
    // Whichever one the tie-breaker picks, its length must equal the
    // optimal Manhattan distance + 1.
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(1, 2));
    let goal  = Node::new(Cell::new(6, 5));
    let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
    match path {
        SearchResult::PathFound(path) => {
            let manhattan = start.manhattan_distance(goal);
            assert_eq!(path.len() as i32, manhattan + 1);
            assert_eq!(*path.first().unwrap(), start);
            assert_eq!(*path.last().unwrap(), goal);
        }
        _ => panic!("Expected a path!"),
    }
}

// ----------------------------------------------
// Search reuse and determinism
// ----------------------------------------------

#[test]
fn test_search_reuse_across_queries() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // 1st query: success.
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(3, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                let expected: Vec<Node> = (0..=3).map(|i| Node::new(Cell::new(i, 0))).collect();
                assert_eq!(path, &expected);
            }
            _ => panic!("Expected a path!"),
        }
    }

    // 2nd query: non-traversable kind -> PathNotFound, must not leak state.
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(3, 0));
        let path  = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        assert!(path.not_found());
    }

    // 3rd query: reset() must clear came_from / cost_so_far from the 1st run.
    {
        let start = Node::new(Cell::new(7, 7));
        let goal  = Node::new(Cell::new(7, 4));
        let path  = search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                let expected: Vec<Node> = (4..=7).rev().map(|i| Node::new(Cell::new(7, i))).collect();
                assert_eq!(path, &expected);
            }
            _ => panic!("Expected a path!"),
        }
    }
}

#[test]
fn test_find_path_deterministic() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    // Non-trivial grid with multiple equal-cost detour options around obstacles.
    let nodes = vec![
        R,R,R,R,R,R,R,R,
        R,W,W,W,W,W,W,R,
        R,R,R,R,R,R,W,R,
        W,W,W,W,W,R,W,R,
        R,R,R,R,W,R,W,R,
        R,W,W,R,W,R,W,R,
        R,R,R,R,R,R,R,R,
        R,R,R,R,R,R,R,R,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(7, 6));

    let path_a: Path = {
        let mut search = Search::with_graph(&graph);
        match search.find_path(&graph, &heuristic, NodeKind::Road, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a path!"),
        }
    };

    let path_b: Path = {
        let mut search = Search::with_graph(&graph);
        match search.find_path(&graph, &heuristic, NodeKind::Road, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a path!"),
        }
    };

    // Assert both runs produce the same deterministic path.
    assert_eq!(path_a, path_b);
}

// ----------------------------------------------
// find_paths + PathFilter
// ----------------------------------------------

#[test]
fn test_find_paths_accepts_first_matches_find_path() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(5, 3));

    let single: Path = {
        let mut search = Search::with_graph(&graph);
        match search.find_path(&graph, &heuristic, NodeKind::EmptyLand, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a path!"),
        }
    };

    let multi: Path = {
        let mut filter = DefaultPathFilter::new();
        let mut search = Search::with_graph(&graph);
        match search.find_paths(&graph, &heuristic, &mut filter, 8, NodeKind::EmptyLand, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a path!"),
        }
    };

    // DefaultPathFilter accepts the first candidate, which is the shortest.
    assert_eq!(single.len(), multi.len());
    assert_eq!(single.first(), multi.first());
    assert_eq!(single.last(), multi.last());
}

struct RejectAllFilter;
impl PathFilter for RejectAllFilter {
    fn accepts(&mut self, _index: usize, _path: &Path, _goal: Node) -> bool {
        false
    }
}

struct RejectAllWithFallbackFilter;
impl PathFilter for RejectAllWithFallbackFilter {
    fn accepts(&mut self, _index: usize, _path: &Path, _goal: Node) -> bool {
        false
    }
    const TAKE_FALLBACK_PATH: bool = true;
}

#[test]
fn test_find_paths_filter_rejects_all_returns_not_found() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut filter = RejectAllFilter;
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(3, 3));
    let path  = search.find_paths(&graph, &heuristic, &mut filter, 4, NodeKind::EmptyLand, start, goal);
    assert!(path.not_found());
}

#[test]
fn test_find_paths_filter_fallback() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut filter = RejectAllWithFallbackFilter;
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(0, 0));
    let goal  = Node::new(Cell::new(3, 3));
    let path  = search.find_paths(&graph, &heuristic, &mut filter, 4, NodeKind::EmptyLand, start, goal);
    match path {
        SearchResult::PathFound(path) => {
            // Fallback path may not be the shortest, but must still connect start->goal.
            assert_eq!(*path.first().unwrap(), start);
            assert_eq!(*path.last().unwrap(), goal);
        }
        _ => panic!("Expected a fallback path!"),
    }
}

// ----------------------------------------------
// find_waypoints + Bias
// ----------------------------------------------

#[test]
fn test_find_waypoints_blocked_start() {
    const R: NodeKind = NodeKind::Road;
    const W: NodeKind = NodeKind::Water;

    let nodes = vec![
        W,W,W,W,W,W,W,W,
        W,R,R,R,R,R,R,W,
        W,R,R,R,R,R,R,W,
        W,R,R,R,R,R,R,W,
        W,R,R,R,R,R,R,W,
        W,R,R,R,R,R,R,W,
        W,R,R,R,R,R,R,W,
        W,W,W,W,W,W,W,W,
    ];

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    // Start is on Water — not traversable as Road.
    let start = Node::new(Cell::new(0, 0));
    let path  = search.find_waypoints(&graph, &heuristic, &bias, &mut filter, NodeKind::Road, start, 5);
    assert!(path.not_found());
}

#[test]
fn test_find_waypoints_max_distance_one() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let bias = Unbiased::new();
    let mut filter = DefaultPathFilter::new();
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(3, 3));
    let path  = search.find_waypoints(&graph, &heuristic, &bias, &mut filter, NodeKind::EmptyLand, start, 1);
    match path {
        SearchResult::PathFound(path) => {
            assert_eq!(path.len(), 2, "Expected [start, neighbor]: {:?}", path);
            assert_eq!(*path.first().unwrap(), start);
            let endpoint = *path.last().unwrap();
            assert_eq!(start.manhattan_distance(endpoint), 1);
        }
        _ => panic!("Expected a path!"),
    }
}

// Deterministic directional bias used to assert that a change in preferred
// direction actually steers find_waypoints to a different endpoint.
struct FixedDirectionalBias {
    dir_x: f32,
    dir_y: f32,
    strength: f32,
}

impl Bias for FixedDirectionalBias {
    fn cost_for(&self, start: Node, node: Node) -> f32 {
        let dx = (node.cell.x - start.cell.x) as f32;
        let dy = (node.cell.y - start.cell.y) as f32;
        let alignment = dx * self.dir_x + dy * self.dir_y;
        -alignment * self.strength
    }
}

#[test]
fn test_find_waypoints_random_directional_bias() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::EmptyLand);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut filter = DefaultPathFilter::new();
    let start = Node::new(Cell::new(4, 4));
    let max_distance = 3;

    // Same seed -> same endpoint (determinism under seed).
    let endpoint_a = {
        let mut rng = Pcg64::seed_from_u64(42);
        let bias = RandomDirectionalBias::new(&mut rng, 1.0, 2.0);
        let mut search = Search::with_graph(&graph);
        match search.find_waypoints(&graph, &heuristic, &bias, &mut filter,
                                    NodeKind::EmptyLand, start, max_distance) {
            SearchResult::PathFound(p) => *p.last().unwrap(),
            _ => panic!("Expected a path!"),
        }
    };
    let endpoint_b = {
        let mut rng = Pcg64::seed_from_u64(42);
        let bias = RandomDirectionalBias::new(&mut rng, 1.0, 2.0);
        let mut search = Search::with_graph(&graph);
        match search.find_waypoints(&graph, &heuristic, &bias, &mut filter,
                                    NodeKind::EmptyLand, start, max_distance) {
            SearchResult::PathFound(p) => *p.last().unwrap(),
            _ => panic!("Expected a path!"),
        }
    };
    assert_eq!(endpoint_a, endpoint_b, "Same seed must yield identical endpoint");

    // Opposite fixed biases must steer to distinct endpoints on an open grid.
    let east_endpoint = {
        let bias = FixedDirectionalBias { dir_x: 1.0, dir_y: 0.0, strength: 2.0 };
        let mut search = Search::with_graph(&graph);
        match search.find_waypoints(&graph, &heuristic, &bias, &mut filter,
                                    NodeKind::EmptyLand, start, max_distance) {
            SearchResult::PathFound(p) => *p.last().unwrap(),
            _ => panic!("Expected a path!"),
        }
    };
    let west_endpoint = {
        let bias = FixedDirectionalBias { dir_x: -1.0, dir_y: 0.0, strength: 2.0 };
        let mut search = Search::with_graph(&graph);
        match search.find_waypoints(&graph, &heuristic, &bias, &mut filter,
                                    NodeKind::EmptyLand, start, max_distance) {
            SearchResult::PathFound(p) => *p.last().unwrap(),
            _ => panic!("Expected a path!"),
        }
    };
    assert_ne!(east_endpoint, west_endpoint, "Opposite biases must steer to distinct endpoints");

    assert!(east_endpoint.cell.x > start.cell.x,
            "East bias endpoint should be east of start: {:?}", east_endpoint);

    assert!(west_endpoint.cell.x < start.cell.x,
            "West bias endpoint should be west of start: {:?}", west_endpoint);
}

// ----------------------------------------------
// Graph mutation and state accessors
// ----------------------------------------------

#[test]
fn test_graph_set_node_kind_bounds() {
    let mut graph = Graph::with_node_kind(Size::new(4, 4), NodeKind::EmptyLand);

    // Out-of-bounds writes are silently ignored (must not panic).
    graph.set_node_kind(Node::new(Cell::new(-1, 0)), NodeKind::Road);
    graph.set_node_kind(Node::new(Cell::new(0, -1)), NodeKind::Road);
    graph.set_node_kind(Node::new(Cell::new(4, 0)), NodeKind::Road);
    graph.set_node_kind(Node::new(Cell::new(0, 4)), NodeKind::Road);

    // In-bounds cells are unaffected.
    for y in 0..4 {
        for x in 0..4 {
            let node = Node::new(Cell::new(x, y));
            assert!(graph.node_kind(node) == Some(NodeKind::EmptyLand));
        }
    }
}

#[test]
fn test_graph_node_kind_out_of_bounds_returns_none() {
    let graph = Graph::with_node_kind(Size::new(4, 4), NodeKind::EmptyLand);

    assert!(graph.node_kind(Node::new(Cell::new(-1, 0))).is_none());
    assert!(graph.node_kind(Node::new(Cell::new(0, -1))).is_none());
    assert!(graph.node_kind(Node::new(Cell::new(4, 0))).is_none());
    assert!(graph.node_kind(Node::new(Cell::new(0, 4))).is_none());
    assert!(graph.node_kind(Node::new(Cell::new(0, 0))) == Some(NodeKind::EmptyLand));
    assert!(graph.node_kind(Node::new(Cell::new(3, 3))) == Some(NodeKind::EmptyLand));
}

#[test]
fn test_graph_vacant_lot_counters() {
    const R: NodeKind = NodeKind::Road;
    const V: NodeKind = NodeKind::VacantLot;

    let nodes = vec![
        R,R,R,R,
        R,V,V,R,
        R,R,R,R,
        R,R,R,R,
    ];
    let mut graph = Graph::with_node_grid(Size::new(4, 4), nodes);

    assert!(graph.has_vacant_lot_nodes());
    assert_eq!(graph.vacant_lot_nodes_count(), 2);

    // Remove one VacantLot by overwriting with Road.
    graph.set_node_kind(Node::new(Cell::new(1, 1)), NodeKind::Road);
    assert_eq!(graph.vacant_lot_nodes_count(), 1);

    // Add a new VacantLot elsewhere.
    graph.set_node_kind(Node::new(Cell::new(3, 3)), NodeKind::VacantLot);
    assert_eq!(graph.vacant_lot_nodes_count(), 2);

    // Remove all VacantLots.
    graph.set_node_kind(Node::new(Cell::new(2, 1)), NodeKind::Road);
    graph.set_node_kind(Node::new(Cell::new(3, 3)), NodeKind::Road);
    assert!(!graph.has_vacant_lot_nodes());
    assert_eq!(graph.vacant_lot_nodes_count(), 0);
}

#[test]
fn test_graph_settlers_spawn_point() {
    const R: NodeKind = NodeKind::Road;
    const S: NodeKind = NodeKind::SettlersSpawnPoint;

    let nodes = vec![
        R,R,R,R,
        R,R,R,R,
        R,R,S,R,
        R,R,R,R,
    ];
    let mut graph = Graph::with_node_grid(Size::new(4, 4), nodes);

    assert_eq!(graph.settlers_spawn_point(), Some(Node::new(Cell::new(2, 2))));

    // Graph::clear resets the cached spawn point.
    graph.clear();
    assert_eq!(graph.settlers_spawn_point(), None);
}

#[test]
fn test_graph_dynamic_obstacle_reroutes() {
    let mut graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::Road);
    let heuristic = AStarUniformCostHeuristic::new();

    let start = Node::new(Cell::new(0, 3));
    let goal  = Node::new(Cell::new(7, 3));

    // 1st search on the pristine grid: straight-line path (8 nodes).
    let path_before: Path = {
        let mut search = Search::with_graph(&graph);
        match search.find_path(&graph, &heuristic, NodeKind::Road, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a path!"),
        }
    };
    assert_eq!(path_before.len(), 8);

    // Drop a Water tile directly in the previous path's middle.
    let blocker = Node::new(Cell::new(4, 3));
    graph.set_node_kind(blocker, NodeKind::Water);

    // 2nd search must detour around the new obstacle.
    let path_after: Path = {
        let mut search = Search::with_graph(&graph);
        match search.find_path(&graph, &heuristic, NodeKind::Road, start, goal) {
            SearchResult::PathFound(p) => p.clone(),
            _ => panic!("Expected a detour path!"),
        }
    };

    assert_ne!(path_before, path_after);
    assert!(path_after.len() > path_before.len());
    assert!(!path_after.contains(&blocker), "Detour path must not cross the blocker: {:?}", path_after);
}

// ----------------------------------------------
// PathHistory
// ----------------------------------------------

#[test]
fn test_path_history_dedup_and_eviction() {
    let p1: Path = vec![Node::new(Cell::new(0, 0)), Node::new(Cell::new(1, 0))];
    let p2: Path = vec![Node::new(Cell::new(0, 0)), Node::new(Cell::new(0, 1))];
    let p3: Path = vec![Node::new(Cell::new(2, 0)), Node::new(Cell::new(2, 1))];
    let p4: Path = vec![Node::new(Cell::new(3, 3)), Node::new(Cell::new(4, 3))];
    let p5: Path = vec![Node::new(Cell::new(5, 5)), Node::new(Cell::new(6, 5))];

    let mut history = PathHistory::default();
    assert!(history.is_empty());

    history.add_path(&p1);
    assert!(!history.is_empty());
    assert!(history.has_path(&p1));
    assert!(history.is_last_path_hash(PathHistory::hash_path(&p1)));

    // Duplicate insert: still only contains p1.
    history.add_path(&p1);
    assert!(history.is_last_path_hash(PathHistory::hash_path(&p1)));

    // Fill up to PATH_HISTORY_MAX_SIZE (= 4).
    history.add_path(&p2);
    history.add_path(&p3);
    history.add_path(&p4);
    for p in [&p1, &p2, &p3, &p4] {
        assert!(history.has_path(p));
    }
    assert!(history.is_last_path_hash(PathHistory::hash_path(&p4)));

    // p5 triggers eviction of the oldest entry (p1).
    history.add_path(&p5);
    assert!(!history.has_path(&p1), "p1 should have been evicted");
    for p in [&p2, &p3, &p4, &p5] {
        assert!(history.has_path(p));
    }

    // hash_path_reverse(p) equals hash_path(p_reversed) — the reverse hasher
    // must match Vec::hash on the reversed sequence.
    let reversed: Path = p1.iter().rev().cloned().collect();
    assert_eq!(PathHistory::hash_path_reverse(&p1), PathHistory::hash_path(&reversed));
}

// ----------------------------------------------
// Free helpers
// ----------------------------------------------

#[test]
fn test_find_nearest_road_link() {
    const R: NodeKind = NodeKind::Road;
    const E: NodeKind = NodeKind::EmptyLand;

    // Road cardinally adjacent to the target cell -> Some.
    {
        let nodes = vec![
            E,E,E,E,E,
            E,E,R,E,E,
            E,E,E,E,E,
            E,E,E,E,E,
            E,E,E,E,E,
        ];
        let graph = Graph::with_node_grid(Size::new(5, 5), nodes);
        let range = CellRange::new(Cell::new(2, 2), Cell::new(2, 2));
        assert_eq!(find_nearest_road_link(&graph, range), Some(Cell::new(2, 1)));
    }

    // Only diagonal Roads around the target -> corners are skipped, no link found.
    {
        let nodes = vec![
            E,E,E,E,E,
            E,R,E,R,E,
            E,E,E,E,E,
            E,R,E,R,E,
            E,E,E,E,E,
        ];
        let graph = Graph::with_node_grid(Size::new(5, 5), nodes);
        let range = CellRange::new(Cell::new(2, 2), Cell::new(2, 2));
        assert_eq!(find_nearest_road_link(&graph, range), None);
    }
}

#[test]
fn test_for_each_surrounding_cell_skips_corners() {
    let range = CellRange::new(Cell::new(2, 2), Cell::new(3, 3));
    let mut visited: Vec<Cell> = Vec::new();
    for_each_surrounding_cell(range, |cell| {
        visited.push(cell);
        true
    });

    // Expanded range (1,1)..=(4,4) contains 16 cells; 4 corners are skipped -> 12.
    assert_eq!(visited.len(), 12);

    let corners = [
        Cell::new(1, 1), Cell::new(4, 1),
        Cell::new(1, 4), Cell::new(4, 4),
    ];
    for corner in corners {
        assert!(!visited.contains(&corner), "Corner {:?} must not be visited", corner);
    }

    // The 8 edge cells of the expanded ring must all be visited.
    let edges = [
        Cell::new(2, 1), Cell::new(3, 1), // top
        Cell::new(2, 4), Cell::new(3, 4), // bottom
        Cell::new(1, 2), Cell::new(1, 3), // left
        Cell::new(4, 2), Cell::new(4, 3), // right
    ];
    for edge in edges {
        assert!(visited.contains(&edge), "Edge {:?} must be visited", edge);
    }

    // The inner block (original range) is also visited (the helper iterates
    // the full expanded rectangle, skipping only the 4 outer corners).
    for y in 2..=3 {
        for x in 2..=3 {
            assert!(visited.contains(&Cell::new(x, y)));
        }
    }
}
