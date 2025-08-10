use super::*;

#[test]
fn test_invalid_paths() {
    let graph = Graph::with_node_kind(Size::new(8, 8), NodeKind::Ground);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // Invalid start node:
    {
        let start = Node::new(Cell::new(-1, -1));
        let goal  = Node::new(Cell::new(0, 0));
        let path = search.find_path(&graph, &heuristic, NodeKind::Ground, start, goal);
        assert!(path.not_found());
    }
    
    // Invalid goal node:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(8, 8));
        let path = search.find_path(&graph, &heuristic, NodeKind::Ground, start, goal);
        assert!(path.not_found());
    }

    // Non traversable nodes:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(3, 3));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
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
    let vertical_path:   Vec<Node> = (0..8).map(|i| Node::new(Cell::new(3, i))).collect();
    let horizontal_path: Vec<Node> = (0..8).map(|i| Node::new(Cell::new(i, 3))).collect();

    let top_to_right_path:   Vec<Node> = [ &vertical_path[0..3],   &horizontal_path[3..8] ].concat();
    let left_to_bottom_path: Vec<Node> = [ &horizontal_path[0..3], &vertical_path[3..8]   ].concat();

    let graph = Graph::with_node_grid(Size::new(8, 8), nodes);
    let heuristic = AStarUniformCostHeuristic::new();
    let mut search = Search::with_graph(&graph);

    // Vertical path across the grid:
    {
        let start = Node::new(Cell::new(3, 0));
        let goal  = Node::new(Cell::new(3, 7));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &vertical_path);
            },
            _ => panic!("Expected a path!")
        }
    }

    // Horizontal path across the grid:
    {
        let start = Node::new(Cell::new(0, 3));
        let goal  = Node::new(Cell::new(7, 3));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &horizontal_path);
            },
            _ => panic!("Expected a path!")
        }
    }

    // Crossing path from top to right:
    {
        let start = Node::new(Cell::new(3, 0));
        let goal  = Node::new(Cell::new(7, 3));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &top_to_right_path);
            },
            _ => panic!("Expected a path!")
        }
    }

    // Crossing path from left to bottom:
    {
        let start = Node::new(Cell::new(0, 3));
        let goal  = Node::new(Cell::new(3, 7));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &left_to_bottom_path);
            },
            _ => panic!("Expected a path!")
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
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &[goal]);
            },
            _ => panic!("Expected a path!")
        }
    }

    // Diagonal path across the grid:
    {
        let start = Node::new(Cell::new(0, 0));
        let goal  = Node::new(Cell::new(7, 7));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                assert_eq!(path, &diagonal_path);
            },
            _ => panic!("Expected a path!")
        }
    }

    // Diagonal path across the grid (reverse):
    {
        let start = Node::new(Cell::new(7, 7));
        let goal  = Node::new(Cell::new(0, 0));
        let path = search.find_path(&graph, &heuristic, NodeKind::Road, start, goal);
        match path {
            SearchResult::PathFound(path) => {
                let reverse_diagonal_path: Vec<Node> = diagonal_path.iter().rev().cloned().collect();
                assert_eq!(path, &reverse_diagonal_path);
            },
            _ => panic!("Expected a path!")
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
    let mut search = Search::with_graph(&graph);

    // Vertical path:
    {
        let start = Node::new(Cell::new(3, 0));
        let max_distance = 5;

        let path = search.find_waypoint(&graph, &heuristic, NodeKind::Road, start, max_distance);
        match path {
            SearchResult::PathFound(path) => {
                let expected_path: Vec<Node> = (0..=5).map(|i| Node::new(Cell::new(3, i))).collect();
                assert_eq!(path, &expected_path); // goal=[3,5]
            },
            _ => panic!("Expected a path!")
        }
    }

    // Horizontal path:
    {
        let start = Node::new(Cell::new(0, 3));
        let max_distance = 7;

        let path = search.find_waypoint(&graph, &heuristic, NodeKind::Road, start, max_distance);
        match path {
            SearchResult::PathFound(path) => {
                let expected_path: Vec<Node> = (0..=7).map(|i| Node::new(Cell::new(i, 3))).collect();
                assert_eq!(path, &expected_path); // goal=[7,3]
            },
            _ => panic!("Expected a path!")
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
    let mut search = Search::with_graph(&graph);

    let start = Node::new(Cell::new(3, 0));
    let max_distance = 7; // Max distance is bigger than length of only available path.

    let path = search.find_waypoint(&graph, &heuristic, NodeKind::Road, start, max_distance);
    match path {
        SearchResult::PathFound(path) => {
            // We only have traversable nodes up to distance=4.
            let expected_path: Vec<Node> = (0..=4).map(|i| Node::new(Cell::new(3, i))).collect();
            assert_eq!(path, &expected_path); // goal=[3,4]
        },
        _ => panic!("Expected a path!")
    }
}
