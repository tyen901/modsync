// src/ui/torrent_file_tree.rs
use eframe::egui::{self, Ui};
use std::collections::BTreeMap;
use std::path::{Component, Path};

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    file_size: Option<u64>, // Store file size if it's a file node
}

impl TreeNode {
    fn insert(&mut self, path: &Path, size: u64) {
        let mut current_node = self;
        for component in path.components() {
            if let Component::Normal(name_osstr) = component {
                let name = name_osstr.to_string_lossy().to_string();
                current_node = current_node.children.entry(name).or_default();
            } else {
                // Handle other Component types if necessary (RootDir, CurDir, ParentDir)
                // For typical torrent paths, Normal should suffice.
                eprintln!("Warning: Unexpected path component type: {:?}", component);
                return; // Skip this file if path is unusual
            }
        }
        // If we've traversed all components, this node represents the file itself
        current_node.file_size = Some(size);
    }

    fn build_tree(files: &[(String, u64)]) -> TreeNode {
        let mut root = TreeNode::default();
        for (name, size) in files {
            root.insert(Path::new(name), *size);
        }
        root
    }
}

#[derive(Default, Debug, Clone)]
pub struct TorrentFileTree {
    // Potentially add state here if needed, e.g., expanded state cache
    // For now, egui's collapsing header state should be sufficient.
}

impl TorrentFileTree {
    pub fn ui(&mut self, ui: &mut Ui, files: &[(String, u64)]) {
        let root_node = TreeNode::build_tree(files);

        // Add a scroll area
        egui::ScrollArea::vertical()
            .auto_shrink([false, false]) // Prevent shrinking
            .show(ui, |ui| {
                // Iterate through the top-level children directly
                // Sort top-level items alphabetically
                let mut top_level_children: Vec<_> = root_node.children.iter().collect();
                top_level_children.sort_by_key(|(k, _)| *k);

                if top_level_children.is_empty() && root_node.file_size.is_some() {
                    // Handle the case of a single-file torrent
                     ui.label(format!(
                        "{} ({})",
                        // Extract filename from the first (and only) file entry if possible
                        files.first().map_or("File", |(name, _)| Path::new(name).file_name().map_or(name.as_str(), |os| os.to_str().unwrap_or(name.as_str()))),
                        format_bytes(root_node.file_size.unwrap_or(0))
                    ));
                } else {
                    for (name, node) in top_level_children {
                         self.render_tree_node(ui, node, name);
                    }
                }
            });
    }

    fn render_tree_node(&mut self, ui: &mut Ui, node: &TreeNode, name: &str) {
        // Check if it's a file node (has size, no children)
        if node.file_size.is_some() && node.children.is_empty() {
            ui.label(format!(
                "{} ({})",
                name,
                format_bytes(node.file_size.unwrap_or(0))
            ));
        } 
        // Check if it's a directory node (has children)
        else if !node.children.is_empty() {
            // It's a directory
            let default_open = false; // Keep directories closed by default
            egui::CollapsingHeader::new(name)
                .default_open(default_open)
                .show(ui, |ui| {
                    // Sort children alphabetically for consistent display
                    let mut children: Vec<_> = node.children.iter().collect();
                    children.sort_by_key(|(k, _)| *k);

                    for (child_name, child_node) in children {
                        // Render child node recursively
                        self.render_tree_node(ui, child_node, child_name);
                    }
                });
        }
        // else: Node might be an intermediate path component without a size 
        // and potentially no children listed explicitly if structure is weird.
        // In standard torrents, this shouldn't happen for leaf nodes.
    }
}

// Helper function (consider moving to a utility module if reused)
fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let sizes = ["B", "KiB", "MiB", "GiB", "TiB"];
    let i = (bytes as f64).log(1024.0).floor() as i32;
    if i == 0 {
        format!("{} {}", bytes, sizes[i as usize])
    } else {
        format!("{:.1} {}", (bytes as f64) / (1024.0_f64.powi(i)), sizes[i as usize])
    }
} 