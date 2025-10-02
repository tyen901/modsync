//! File system -> graph visualizer.
//!
//! This module constructs a persistent `egui_graphs::Graph` representing the
//! directory tree for a selected root folder. The expensive filesystem walk
//! and conversion to the interactive graph happen only once per folder
//! selection. Subsequent UI frames simply render the already-built graph,
//! avoiding per-frame cloning / transformation overhead from the previous
//! implementation.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use walkdir::WalkDir;

use eframe::egui::{Ui, Pos2};
use petgraph::{
    stable_graph::{DefaultIx, NodeIndex, StableGraph},
    Directed,
};
use egui_graphs::{
    default_edge_transform, default_node_transform, to_graph_custom,
    DefaultEdgeShape, DefaultNodeShape, Graph, GraphView,
    SettingsNavigation,
    LayoutForceDirected, FruchtermanReingold, FruchtermanReingoldState,
};

/// Payload stored for each file/directory node.
#[derive(Clone, Debug)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

impl FileNode {
    fn label(&self) -> String { self.name.clone() }
}

pub struct FileGraph {
    /// The interactive graph shown by egui_graphs (built once per folder).
    g: Option<Graph<FileNode, (), Directed, DefaultIx, DefaultNodeShape>>,
    /// Root path we last built for (used to prevent redundant rebuilds).
    built_root: Option<PathBuf>,
    /// Indicates the current graph was freshly built and needs an initial
    /// burst of force simulation so the first rendered frame is already
    /// partially stabilized.
    graph_fresh: bool,
}

impl FileGraph {
    pub fn new() -> Self { Self { g: None, built_root: None, graph_fresh: false } }

    /// Clears current graph.
    pub fn clear(&mut self) { self.g = None; self.built_root = None; self.graph_fresh = false; }

    /// Build (or rebuild) the graph from a folder path. Only direct parent->child edges are created.
    /// This is intentionally synchronous; caller should avoid invoking it on performance sensitive paths.
    pub fn build_from_path(&mut self, folder: &Path) {
        if !folder.exists() { self.clear(); return; }
        if self.built_root.as_ref().map(|p| p == folder).unwrap_or(false) { return; } // already built

        let mut pet_g: StableGraph<FileNode, (), Directed, DefaultIx> = StableGraph::new();
        let mut map: HashMap<String, NodeIndex<DefaultIx>> = HashMap::new();

        // root node
        let root_fn = FileNode { name: folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string_lossy().into_owned()), path: folder.to_string_lossy().into_owned(), is_dir: true, depth: 0 };
        let root_idx = pet_g.add_node(root_fn.clone());
        map.insert(root_fn.path.clone(), root_idx);

        // Collect entries and sort by depth so parents come first.
        let mut entries: Vec<_> = WalkDir::new(folder)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.depth());

        for entry in entries {
            let path = entry.path();
            let is_dir = entry.file_type().is_dir();
            let depth = entry.depth();
            let path_str = path.to_string_lossy().to_string();

            // create node if missing
            let idx = if let Some(idx) = map.get(&path_str).copied() { idx } else {
                let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path_str.clone());
                let n = pet_g.add_node(FileNode { name, path: path_str.clone(), is_dir, depth });
                map.insert(path_str.clone(), n);
                n
            };

            // parent relation
            if let Some(parent) = path.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                // ensure parent node exists (could happen if traversal order produces child first in odd FS; depth sort should prevent, but keep safe)
                let p_idx = if let Some(p_idx) = map.get(&parent_str).copied() { p_idx } else {
                    let name = parent.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| parent_str.clone());
                    let pn = pet_g.add_node(FileNode { name, path: parent_str.clone(), is_dir: true, depth: depth.saturating_sub(1) });
                    map.insert(parent_str.clone(), pn);
                    pn
                };

                // Only one edge parent -> child (avoid duplicates)
                let duplicate = pet_g.edges_connecting(p_idx, idx).next().is_some();
                if !duplicate { pet_g.add_edge(p_idx, idx, ()); }
            } else {
                // attach to root if somehow missing parent (should not happen except root itself excluded by min_depth)
                if idx != root_idx {
                    let duplicate = pet_g.edges_connecting(root_idx, idx).next().is_some();
                    if !duplicate { pet_g.add_edge(root_idx, idx, ()); }
                }
            }
        }

        // Deterministic circular layout (no guessing, no jitter):
        let total = pet_g.node_count().max(1);
        let radius = (total as f32).sqrt() * 35.0 + 60.0;
        // Precompute positions keyed by path for lookup in transform closure.
        let mut positions: HashMap<String, Pos2> = HashMap::with_capacity(total);
        for (i, idx) in pet_g.node_indices().enumerate() {
            if let Some(node) = pet_g.node_weight(idx) {
                let angle = 2.0 * std::f32::consts::PI * (i as f32 / total as f32);
                let x = radius * angle.cos();
                let y = radius * angle.sin();
                positions.insert(node.path.clone(), Pos2 { x, y });
            }
        }
        self.g = Some(to_graph_custom(&pet_g, |n| {
            default_node_transform(n);
            if let Some(pos) = positions.get(&n.payload().path) {
                n.set_location(*pos);
            }
            if n.payload().is_dir { n.set_label(format!("{}/", n.payload().label())); }
            else { n.set_label(n.payload().label()); }
        }, default_edge_transform));
        self.graph_fresh = true;
        self.built_root = Some(folder.to_path_buf());
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        // Always query current available size so resizing the window/frame
        // immediately updates the render surface for the graph.
        let avail = ui.available_size();
        if self.g.as_ref().map(|g| g.node_count()).unwrap_or(0) == 0 {
            ui.set_min_height(avail.y.max(180.0));
            ui.centered_and_justified(|ui| { ui.label("No files to display. Pick a folder to visualize the file tree."); });
            return;
        }

        // Render existing graph (no rebuild here). Use hierarchical layout for a tree-like appearance.
        if let Some(g) = &mut self.g {
            // Ensure simulation is running.
            let mut state = GraphView::<
                FileNode,
                (),
                Directed,
                DefaultIx,
                DefaultNodeShape,
                DefaultEdgeShape,
                FruchtermanReingoldState,
                LayoutForceDirected<FruchtermanReingold>,
            >::get_layout_state(ui);
            if !state.is_running {
                state.is_running = true;
                GraphView::<
                    FileNode,
                    (),
                    Directed,
                    DefaultIx,
                    DefaultNodeShape,
                    DefaultEdgeShape,
                    FruchtermanReingoldState,
                    LayoutForceDirected<FruchtermanReingold>,
                >::set_layout_state(ui, state);
            }

            // If freshly built, fast-forward a bit so initial frame is stabilized.
            if self.graph_fresh {
                GraphView::<
                    FileNode,
                    (),
                    Directed,
                    DefaultIx,
                    DefaultNodeShape,
                    DefaultEdgeShape,
                    FruchtermanReingoldState,
                    LayoutForceDirected<FruchtermanReingold>,
                >::fast_forward_budgeted_force_run(ui, g, 300, 8); // up to 300 steps or 8ms
                self.graph_fresh = false;
            }

            // Render with force-directed layout. Interactions intentionally minimal (no selection/dragging).
            let mut view = GraphView::<
                FileNode,
                (),
                Directed,
                DefaultIx,
                DefaultNodeShape,
                DefaultEdgeShape,
                FruchtermanReingoldState,
                LayoutForceDirected<FruchtermanReingold>,
            >::new(g)
                .with_navigations(&SettingsNavigation::default().with_zoom_and_pan_enabled(true));
            // Force the graph viewport to claim all available space.
            let _response = ui.add_sized(avail, &mut view);
            // Hover overlay disabled until node iteration API is confirmed.
        }
    }
}
