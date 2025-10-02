use eframe::egui::Ui;
use petgraph::stable_graph::StableGraph;
use petgraph::Directed;
use walkdir::WalkDir;
use std::path::Path;

pub struct FileGraph {
    pub graph: StableGraph<String, ()>,
}

impl FileGraph {
    pub fn new() -> Self {
        Self { graph: StableGraph::new() }
    }

    /// Build the graph from a folder path. Only direct parent->child edges are created.
    pub fn build_from_path(&mut self, folder: &Path) {
        self.graph = StableGraph::new();

        if !folder.exists() {
            return;
        }

        let folder_str = folder.to_string_lossy().to_string();
        let mut idx_map: std::collections::HashMap<String, petgraph::stable_graph::NodeIndex<u32>> = std::collections::HashMap::new();

        // create root node
        let root_idx = self.graph.add_node(folder_str.clone());
        idx_map.insert(folder_str.clone(), root_idx);

        // Walk entries in increasing depth order so parents appear before children
        let mut entries: Vec<_> = WalkDir::new(folder).into_iter().filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.depth());

        for entry in entries {
            let p = entry.path().to_path_buf();
            if p == folder { continue; }
            let p_str = p.to_string_lossy().to_string();

            // create node for this path if not exists
            if !idx_map.contains_key(&p_str) {
                let label = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p_str.clone());
                let n = self.graph.add_node(label);
                idx_map.insert(p_str.clone(), n);
            }

            // ensure parent exists and add edge parent -> child only
            if let Some(parent) = p.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                // create parent nodes on-demand (should mostly exist due to sorting)
                if !idx_map.contains_key(&parent_str) {
                    let label = parent.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| parent_str.clone());
                    let pn = self.graph.add_node(label);
                    idx_map.insert(parent_str.clone(), pn);
                }

                if let (Some(&p_idx), Some(&c_idx)) = (idx_map.get(&parent_str), idx_map.get(&p_str)) {
                    // Add only one edge from the parent to this child
                    self.graph.add_edge(p_idx, c_idx, ());
                }
            } else {
                // no parent, attach to root
                if let Some(&c_idx) = idx_map.get(&p_str) {
                    self.graph.add_edge(root_idx, c_idx, ());
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        // reserve all available space for the graph widget
        let avail = ui.available_size();
        if self.graph.node_count() == 0 {
            ui.set_min_height(avail.y.max(180.0));
            ui.centered_and_justified(|ui| {
                ui.label("No files to display. Pick a folder to visualize the file tree.");
            });
            return;
        }

        // convert and render with egui_graphs
        let mut sg = self.graph.clone();
        let mut g = egui_graphs::to_graph::<String, (), Directed, u32, egui_graphs::DefaultNodeShape, egui_graphs::DefaultEdgeShape>(&mut sg);

        // Create a GraphView with concrete layout types
        let mut view = egui_graphs::GraphView::<
            String,
            (),
            Directed,
            u32,
            egui_graphs::DefaultNodeShape,
            egui_graphs::DefaultEdgeShape,
            egui_graphs::LayoutStateHierarchical,
            egui_graphs::LayoutHierarchical,
        >::new(&mut g)
        .with_navigations(&egui_graphs::SettingsNavigation::default());

        // allocate the full available area to the widget
        ui.add_sized(avail, &mut view);
    }
}
