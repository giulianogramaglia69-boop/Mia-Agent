//! Pure graph layout for the canvas: no widgets, no drawing, just
//! positions — so the layout is unit-testable without a window.
//!
//! Entities sit on a circle, spread evenly; each relation becomes a
//! straight edge between the two node positions. Relations that name an
//! entity the graph does not know are skipped (the graph only draws what
//! it can place).

use mia_core::{Entity, Relation};

/// One entity placed on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeLayout {
    /// Entity name (label drawn on the canvas).
    pub name: String,
    /// Canvas x, in pixels from the canvas origin.
    pub x: f32,
    /// Canvas y, in pixels from the canvas origin.
    pub y: f32,
}

/// One relation drawn as an edge between two nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeLayout {
    /// Index of the source node in [`GraphLayout::nodes`].
    pub from: usize,
    /// Index of the target node in [`GraphLayout::nodes`].
    pub to: usize,
    /// Relation label drawn along the edge.
    pub relation: String,
}

/// The whole placed graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphLayout {
    /// Nodes, in the same order as the input entities.
    pub nodes: Vec<NodeLayout>,
    /// Edges, in the same order as the input relations (minus unplaceable).
    pub edges: Vec<EdgeLayout>,
}

/// Place `entities` on a circle inside a `width` x `height` canvas and
/// resolve `relations` to edges between them.
///
/// A single entity goes in the middle; an empty graph lays out nothing.
/// The margin keeps labels inside the canvas.
pub fn layout_graph(
    entities: &[Entity],
    relations: &[Relation],
    width: f32,
    height: f32,
) -> GraphLayout {
    let mut nodes = Vec::with_capacity(entities.len());
    if entities.is_empty() {
        return GraphLayout::default();
    }
    let cx = width / 2.0;
    let cy = height / 2.0;
    if entities.len() == 1 {
        nodes.push(NodeLayout {
            name: entities[0].name.clone(),
            x: cx,
            y: cy,
        });
    } else {
        let radius = (width.min(height) / 2.0 - 48.0).max(20.0);
        for (i, entity) in entities.iter().enumerate() {
            // Start at the top (-90°) and go clockwise.
            let angle = -std::f32::consts::FRAC_PI_2
                + (i as f32) * 2.0 * std::f32::consts::PI / (entities.len() as f32);
            nodes.push(NodeLayout {
                name: entity.name.clone(),
                x: cx + radius * angle.cos(),
                y: cy + radius * angle.sin(),
            });
        }
    }
    let index_of = |name: &str| nodes.iter().position(|n| n.name == name);
    let mut edges = Vec::new();
    for relation in relations {
        if let (Some(from), Some(to)) = (
            index_of(&relation.source),
            index_of(&relation.target),
        ) {
            edges.push(EdgeLayout {
                from,
                to,
                relation: relation.relation.clone(),
            });
        }
    }
    GraphLayout { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mia_core::EntityKind;

    fn entity(name: &str) -> Entity {
        Entity {
            id: 0,
            name: name.to_string(),
            kind: EntityKind::Unknown,
            first_seen: String::new(),
            last_seen: String::new(),
        }
    }

    fn relation(source: &str, rel: &str, target: &str) -> Relation {
        Relation {
            source: source.to_string(),
            relation: rel.to_string(),
            target: target.to_string(),
            weight: 1.0,
        }
    }

    #[test]
    fn empty_graph_lays_out_nothing() {
        let layout = layout_graph(&[], &[], 800.0, 600.0);
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
    }

    #[test]
    fn single_entity_goes_in_the_middle() {
        let layout = layout_graph(&[entity("Mario")], &[], 800.0, 600.0);
        assert_eq!(layout.nodes.len(), 1);
        assert_eq!(layout.nodes[0].x, 400.0);
        assert_eq!(layout.nodes[0].y, 300.0);
    }

    #[test]
    fn four_entities_spread_around_the_circle() {
        let entities = vec![
            entity("A"),
            entity("B"),
            entity("C"),
            entity("D"),
        ];
        let layout = layout_graph(&entities, &[], 800.0, 600.0);
        assert_eq!(layout.nodes.len(), 4);
        // All on the same circle around the centre (radius 252).
        for node in &layout.nodes {
            let dx = node.x - 400.0;
            let dy = node.y - 300.0;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!((dist - 252.0).abs() < 0.01, "dist {dist}");
        }
        // First node at the top, third at the bottom.
        assert!((layout.nodes[0].y - 48.0).abs() < 0.01);
        assert!((layout.nodes[2].y - 552.0).abs() < 0.01);
    }

    #[test]
    fn edges_resolve_by_name_and_skip_unknown_endpoints() {
        let entities = vec![entity("Mario"), entity("Luigi")];
        let relations = vec![
            relation("Mario", "lavora_con", "Luigi"),
            relation("Mario", "conosce", "Peach"),
        ];
        let layout = layout_graph(&entities, &relations, 800.0, 600.0);
        assert_eq!(layout.edges.len(), 1);
        assert_eq!(layout.edges[0].from, 0);
        assert_eq!(layout.edges[0].to, 1);
        assert_eq!(layout.edges[0].relation, "lavora_con");
    }
}
